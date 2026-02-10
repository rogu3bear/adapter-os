//! Separated LoRA trainer for positive/negative weight groups
//!
//! Trains separate LoRA weight sets for positive and negative examples,
//! enabling better control over reinforcement learning and adversarial training.
//!
//! This trainer uses MLX-based cross-entropy loss computation when the `multi-backend`
//! feature is enabled, providing GPU-accelerated training. Without the feature,
//! it falls back to a CPU-based reference cross-entropy implementation.

use super::trainer::{LoRAWeights, TrainingConfig};
use adapteros_aos::single_file::{
    format::{AdapterWeights, CombinationStrategy, WeightGroup, WeightGroupType, WeightMetadata},
    weights::combine_weight_groups,
};
use adapteros_config::resolve_telemetry_dir;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use adapteros_telemetry::TelemetryWriter;
use adapteros_types::training::{sample_role_from_metadata, TrainingExampleV1 as TrainingExample};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Separated LoRA trainer that trains positive and negative weight groups independently
pub struct SeparatedLoRATrainer {
    config: TrainingConfig,
    training_seed: u64,
}

/// Result of separated training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeparatedTrainingResult {
    pub adapter_id: String,
    pub positive_result: WeightGroupResult,
    pub negative_result: WeightGroupResult,
    pub total_training_time_ms: u64,
    pub combination_strategy: CombinationStrategy,
    pub training_data: Vec<TrainingExample>,
}

/// Result for individual weight group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightGroupResult {
    pub group_type: WeightGroupType,
    pub final_loss: f32,
    pub training_time_ms: u64,
    pub example_count: usize,
    pub weights: LoRAWeights,
}

impl WeightGroupResult {
    pub fn to_weight_group(&self) -> WeightGroup {
        WeightGroup {
            lora_a: self.weights.lora_a.clone(),
            lora_b: self.weights.lora_b.clone(),
            metadata: WeightMetadata {
                example_count: self.example_count,
                avg_loss: self.final_loss,
                training_time_ms: self.training_time_ms,
                group_type: self.group_type.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        }
    }
}

impl SeparatedLoRATrainer {
    /// Create a new separated LoRA trainer
    ///
    /// Uses MLX-based cross-entropy loss when `multi-backend` feature is enabled,
    /// otherwise falls back to CPU-based reference cross-entropy implementation.
    pub fn new(config: TrainingConfig) -> Result<Self> {
        // Create a base hash for seed derivation (using config hash as base)
        let config_bytes = format!("{:?}", config).into_bytes();
        let base_hash = B3Hash::hash(&config_bytes);
        let seed_bytes = derive_seed(&base_hash, "separated_lora_training");

        // Convert seed bytes to u64 for RNG initialization
        let training_seed = u64::from_le_bytes([
            seed_bytes[0],
            seed_bytes[1],
            seed_bytes[2],
            seed_bytes[3],
            seed_bytes[4],
            seed_bytes[5],
            seed_bytes[6],
            seed_bytes[7],
        ]);

        // Initialize telemetry writer with default settings
        let telemetry_dir = resolve_telemetry_dir()?;
        let _ = std::fs::create_dir_all(&telemetry_dir.path);
        let _telemetry = TelemetryWriter::new(&telemetry_dir.path, 10_000, 10 * 1024 * 1024)?;
        info!(
            path = %telemetry_dir.path.display(),
            source = %telemetry_dir.source,
            "Initialized telemetry writer for separated trainer"
        );

        Ok(Self {
            config,
            training_seed,
        })
    }

    /// Train with separated positive/negative weight groups
    pub fn train_separated(
        &self,
        examples: &[TrainingExample],
        combination_strategy: CombinationStrategy,
    ) -> Result<SeparatedTrainingResult> {
        let start_time = Instant::now();

        // Separate examples by weight
        let (positive_examples, negative_examples) = self.separate_examples(examples);

        info!(
            "Starting separated training: {} positive, {} negative examples",
            positive_examples.len(),
            negative_examples.len()
        );

        // Train positive weight group
        let positive_result = if !positive_examples.is_empty() {
            self.train_weight_group(&positive_examples, WeightGroupType::Positive)?
        } else {
            return Err(AosError::Training(
                "No positive examples found for training".to_string(),
            ));
        };

        // Train negative weight group
        let negative_result = if !negative_examples.is_empty() {
            self.train_weight_group(&negative_examples, WeightGroupType::Negative)?
        } else {
            return Err(AosError::Training(
                "No negative examples found for training".to_string(),
            ));
        };

        let total_time = start_time.elapsed().as_millis() as u64;
        let total_time = total_time.max(1);

        info!(
            "Separated training completed: positive_loss={:.6}, negative_loss={:.6}, time_ms={}",
            positive_result.final_loss, negative_result.final_loss, total_time
        );

        Ok(SeparatedTrainingResult {
            adapter_id: Self::generate_adapter_id(),
            positive_result,
            negative_result,
            total_training_time_ms: total_time,
            combination_strategy,
            training_data: examples.to_vec(),
        })
    }

    /// Separate examples into positive and negative groups.
    ///
    /// Classification is based on explicit `sample_role` metadata:
    /// - "abstention" or "negative" -> negative group
    /// - All other values (including missing) -> positive group
    fn separate_examples(
        &self,
        examples: &[TrainingExample],
    ) -> (Vec<TrainingExample>, Vec<TrainingExample>) {
        let mut positive = Vec::new();
        let mut negative = Vec::new();

        for example in examples {
            let role = sample_role_from_metadata(&example.metadata);
            match role.as_deref() {
                Some("abstention") | Some("negative") => negative.push(example.clone()),
                _ => positive.push(example.clone()),
            }
        }

        (positive, negative)
    }

    /// Train a single weight group
    fn train_weight_group(
        &self,
        examples: &[TrainingExample],
        group_type: WeightGroupType,
    ) -> Result<WeightGroupResult> {
        let start_time = Instant::now();

        // Initialize weights
        let mut weights = self.init_weights();

        // Train for specified epochs
        let mut final_loss = 0.0;
        for epoch in 0..self.config.epochs {
            let epoch_loss = self.train_epoch(&mut weights, examples, epoch)?;
            final_loss = epoch_loss;

            debug!(
                "Epoch {} complete for {:?}: loss={:.6}",
                epoch + 1,
                group_type,
                epoch_loss
            );
        }

        let training_time = start_time.elapsed().as_millis() as u64;
        let training_time = training_time.max(1);

        Ok(WeightGroupResult {
            group_type,
            final_loss,
            training_time_ms: training_time,
            example_count: examples.len(),
            weights,
        })
    }

    /// Train one epoch
    fn train_epoch(
        &self,
        weights: &mut LoRAWeights,
        examples: &[TrainingExample],
        _epoch: usize,
    ) -> Result<f32> {
        let mut total_loss = 0.0;
        let mut batch_count = 0;

        // Create batches
        let batches = self.create_batches(examples);

        for batch in batches {
            let batch_loss = self.train_batch(weights, &batch)?;
            total_loss += batch_loss;
            batch_count += 1;
        }

        let avg_loss = if batch_count > 0 {
            total_loss / batch_count as f32
        } else {
            0.0
        };

        Ok(avg_loss)
    }

    /// Train one batch
    fn train_batch(&self, weights: &mut LoRAWeights, batch: &[TrainingExample]) -> Result<f32> {
        let mut total_loss = 0.0;
        let mut example_count = 0;

        for example in batch {
            // Forward pass
            let (output, hidden) = self.forward(weights, &example.input_tokens)?;

            // Compute loss using cross-entropy
            let loss = self.compute_loss(&output, &example.target_tokens);
            total_loss += loss;
            example_count += 1;

            // Backward pass and weight update
            self.backward_and_update(weights, &hidden, &output, &example.target_tokens, loss)?;
        }

        let avg_loss = if example_count > 0 {
            total_loss / example_count as f32
        } else {
            0.0
        };

        Ok(avg_loss)
    }

    /// Create batches from examples
    fn create_batches(&self, examples: &[TrainingExample]) -> Vec<Vec<TrainingExample>> {
        let mut batches: Vec<Vec<TrainingExample>> = Vec::new();
        let batch_size = self.config.batch_size;

        for chunk in examples.chunks(batch_size) {
            batches.push(chunk.to_vec());
        }

        batches
    }

    /// Forward pass with LoRA injection
    fn forward(&self, weights: &LoRAWeights, input: &[u32]) -> Result<(Vec<f32>, Vec<f32>)> {
        // Simplified forward pass (same as original trainer)
        let hidden: Vec<f32> = input
            .iter()
            .take(self.config.hidden_dim)
            .map(|&token_id| (token_id as f32) / 1000.0)
            .collect();

        let mut hidden = hidden;
        while hidden.len() < self.config.hidden_dim {
            hidden.push(0.0);
        }

        let lora_output = self.apply_lora(&hidden, weights);

        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// Apply LoRA transformation
    #[allow(clippy::needless_range_loop)]
    fn apply_lora(&self, hidden: &[f32], weights: &LoRAWeights) -> Vec<f32> {
        // Compute: hidden * LoRA_A^T * LoRA_B^T
        let mut intermediate = vec![0.0; self.config.rank];
        for r in 0..self.config.rank {
            for (h_idx, &h_val) in hidden.iter().enumerate() {
                if h_idx < weights.lora_a[r].len() {
                    intermediate[r] += h_val * weights.lora_a[r][h_idx];
                }
            }
        }

        let mut output = vec![0.0; self.config.hidden_dim];
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for (r, &inter_val) in intermediate.iter().enumerate() {
                    if r < weights.lora_b[h_idx].len() {
                        output[h_idx] += inter_val * weights.lora_b[h_idx][r];
                    }
                }
            }
        }

        output
    }

    /// Compute cross-entropy loss using MLX GPU acceleration when available.
    ///
    /// This replaces the deprecated MSE-based loss computation with proper
    /// cross-entropy loss that matches MicroLoRATrainer's behavior.
    #[cfg(feature = "multi-backend")]
    fn compute_loss(&self, logits: &[f32], target: &[u32]) -> f32 {
        use adapteros_lora_mlx_ffi::{training::mlx_cross_entropy_loss_gpu, MLXFFITensor};

        if logits.is_empty() || target.is_empty() {
            return 0.0;
        }

        // Build logits tensor [seq_len, vocab_size] - for simplified case, we treat
        // the output as [1, hidden_dim] logits over a "vocabulary" of hidden_dim
        let vocab_size = self.config.vocab_size.max(logits.len());
        let mut padded_logits = vec![0.0f32; vocab_size];
        for (i, &v) in logits.iter().enumerate() {
            if i < vocab_size {
                padded_logits[i] = v;
            }
        }

        let logits_tensor = match MLXFFITensor::from_data(&padded_logits, vec![1, vocab_size]) {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to create logits tensor: {}", e);
                return self.compute_loss_cpu_reference(logits, target);
            }
        };

        // Build targets tensor [1, seq_len] as i32 for indexing
        let targets_i32: Vec<i32> = target.iter().take(1).map(|&t| t as i32).collect();
        let targets_tensor = match MLXFFITensor::from_ints(&targets_i32, vec![1, 1]) {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to create targets tensor: {}", e);
                return self.compute_loss_cpu_reference(logits, target);
            }
        };

        // Compute cross-entropy loss on GPU
        match mlx_cross_entropy_loss_gpu(&logits_tensor, &targets_tensor, self.config.ignore_index)
        {
            Ok(loss_tensor) => match loss_tensor.to_float_vec() {
                Ok(loss_vec) => loss_vec.first().copied().unwrap_or(0.0),
                Err(e) => {
                    warn!("Failed to extract loss value: {}", e);
                    self.compute_loss_cpu_reference(logits, target)
                }
            },
            Err(e) => {
                warn!("MLX cross-entropy loss failed, falling back to CPU: {}", e);
                self.compute_loss_cpu_reference(logits, target)
            }
        }
    }

    /// Compute cross-entropy loss using CPU reference implementation.
    ///
    /// Used when multi-backend feature is disabled or as fallback on MLX errors.
    #[cfg(not(feature = "multi-backend"))]
    fn compute_loss(&self, logits: &[f32], target: &[u32]) -> f32 {
        self.compute_loss_cpu_reference(logits, target)
    }

    /// CPU reference implementation of cross-entropy loss.
    ///
    /// This implements the standard cross-entropy loss formula:
    /// CE = -sum(y_i * log(softmax(x)_i)) / N
    ///
    /// For numerical stability, uses the log-sum-exp trick.
    fn compute_loss_cpu_reference(&self, logits: &[f32], target: &[u32]) -> f32 {
        if logits.is_empty() || target.is_empty() {
            return 0.0;
        }

        let ignore_index = self.config.ignore_index;
        let mut total_loss = 0.0f32;
        let mut valid_count = 0usize;

        // For each target token, compute cross-entropy
        for &target_id in target.iter().take(1) {
            // Skip ignored indices
            if ignore_index >= 0 && target_id as i32 == ignore_index {
                continue;
            }

            let target_idx = target_id as usize;
            if target_idx >= logits.len() {
                // Target out of range - skip
                continue;
            }

            // Compute log-softmax for numerical stability
            // log_softmax(x)_i = x_i - log(sum(exp(x_j)))
            let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sum_exp: f32 = logits.iter().map(|&x| (x - max_logit).exp()).sum();
            let log_sum_exp = sum_exp.ln() + max_logit;

            // Cross-entropy for this position: -log(softmax(logits)[target])
            let log_prob = logits[target_idx] - log_sum_exp;
            total_loss -= log_prob;
            valid_count += 1;
        }

        if valid_count == 0 {
            0.0
        } else {
            total_loss / valid_count as f32
        }
    }

    /// Backward pass and weight update using cross-entropy gradients.
    ///
    /// Computes gradients of cross-entropy loss with respect to LoRA weights
    /// and applies SGD update.
    fn backward_and_update(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        logits: &[f32],
        target: &[u32],
        _loss: f32,
    ) -> Result<()> {
        let learning_rate = self.config.learning_rate;
        let ignore_index = self.config.ignore_index;

        // Compute softmax probabilities for gradient computation
        let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exp_logits: Vec<f32> = logits.iter().map(|&x| (x - max_logit).exp()).collect();
        let sum_exp: f32 = exp_logits.iter().sum();
        let softmax: Vec<f32> = exp_logits.iter().map(|&e| e / sum_exp).collect();

        // Compute gradient of cross-entropy: d_loss/d_logits = softmax - one_hot(target)
        let mut grad_logits = softmax;
        let mut valid_targets = 0usize;
        for &target_id in target.iter().take(1) {
            if ignore_index >= 0 && target_id as i32 == ignore_index {
                continue;
            }
            let target_idx = target_id as usize;
            if target_idx < grad_logits.len() {
                grad_logits[target_idx] -= 1.0;
                valid_targets += 1;
            }
        }

        // Normalize gradient by number of valid targets
        if valid_targets > 0 {
            let scale = 1.0 / valid_targets as f32;
            for g in &mut grad_logits {
                *g *= scale;
            }
        }

        // Chain rule: gradient flows through LoRA transformation
        // output = hidden + (hidden @ A^T @ B^T) * (alpha / rank)
        // d_loss/d_A and d_loss/d_B computed via chain rule
        let lora_scale = self.config.alpha / self.config.rank as f32;

        // Gradient for LoRA_B: d_loss/d_B = grad_logits^T @ intermediate
        // where intermediate = hidden @ A^T
        let mut intermediate = vec![0.0; self.config.rank];
        for (r, inter_val) in intermediate.iter_mut().enumerate().take(self.config.rank) {
            for (h_idx, &h_val) in hidden.iter().enumerate() {
                if h_idx < weights.lora_a[r].len() {
                    *inter_val += h_val * weights.lora_a[r][h_idx];
                }
            }
        }

        // Update LoRA_B: gradient = grad_logits (outer product) intermediate
        for (h_idx, &grad_logit) in grad_logits
            .iter()
            .enumerate()
            .take(self.config.hidden_dim.min(grad_logits.len()))
        {
            if h_idx < weights.lora_b.len() {
                for (r, &inter_val) in intermediate.iter().enumerate().take(self.config.rank) {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_logit * inter_val * lora_scale;
                        weights.lora_b[h_idx][r] -= learning_rate * grad;
                    }
                }
            }
        }

        // Gradient for LoRA_A: d_loss/d_A = B^T @ grad_logits^T @ hidden
        // First compute B^T @ grad_logits
        let mut grad_intermediate = vec![0.0; self.config.rank];
        for (r, grad_inter_val) in grad_intermediate
            .iter_mut()
            .enumerate()
            .take(self.config.rank)
        {
            for (h_idx, &grad_logit) in grad_logits
                .iter()
                .enumerate()
                .take(self.config.hidden_dim.min(grad_logits.len()))
            {
                if h_idx < weights.lora_b.len() && r < weights.lora_b[h_idx].len() {
                    *grad_inter_val += weights.lora_b[h_idx][r] * grad_logit;
                }
            }
        }

        // Update LoRA_A: gradient = grad_intermediate (outer product) hidden
        for (r, &grad_inter_val) in grad_intermediate.iter().enumerate().take(self.config.rank) {
            for (h_idx, &h_val) in hidden
                .iter()
                .enumerate()
                .take(self.config.hidden_dim.min(hidden.len()))
            {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_inter_val * h_val * lora_scale;
                    weights.lora_a[r][h_idx] -= learning_rate * grad;
                }
            }
        }

        Ok(())
    }

    /// Initialize LoRA weights with deterministic seeded RNG
    fn init_weights(&self) -> LoRAWeights {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        // Create deterministic RNG from training seed
        let mut rng = ChaCha20Rng::seed_from_u64(self.training_seed);

        let mut lora_a = Vec::new();
        let mut lora_b = Vec::new();

        // Initialize LoRA_A (rank x hidden_dim)
        for _ in 0..self.config.rank {
            let mut row = Vec::new();
            for _ in 0..self.config.hidden_dim {
                row.push(0.01 * (rng.gen::<f32>() - 0.5));
            }
            lora_a.push(row);
        }

        // Initialize LoRA_B (hidden_dim x rank)
        for _ in 0..self.config.hidden_dim {
            let mut row = Vec::new();
            for _ in 0..self.config.rank {
                row.push(0.01 * (rng.gen::<f32>() - 0.5));
            }
            lora_b.push(row);
        }

        LoRAWeights {
            lora_a,
            lora_b,
            modules: BTreeMap::new(),
            moe_config: None,
            precomputed_delta: None,
        }
    }

    /// Generate unique adapter ID
    fn generate_adapter_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("separated_lora_{}", timestamp)
    }
}

impl SeparatedTrainingResult {
    pub fn to_adapter_weights(&self) -> Result<AdapterWeights> {
        let positive_group = self.positive_result.to_weight_group();
        let negative_group = self.negative_result.to_weight_group();
        let combined = match &self.combination_strategy {
            CombinationStrategy::Separate => None,
            strategy => Some(combine_weight_groups(
                &positive_group,
                &negative_group,
                strategy,
            )?),
        };

        Ok(AdapterWeights {
            positive: positive_group,
            negative: negative_group,
            combined,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_types::training::{ExampleMetadataV1, TRAINING_DATA_CONTRACT_VERSION};

    fn mlx_device_available_for_tests() -> bool {
        #[cfg(feature = "multi-backend")]
        {
            #[cfg(target_os = "macos")]
            {
                if metal::Device::system_default().is_some() {
                    true
                } else {
                    eprintln!("SKIPPED: MLX tests require a Metal device");
                    false
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("SKIPPED: MLX tests require macOS Metal support");
                false
            }
        }
        #[cfg(not(feature = "multi-backend"))]
        {
            true
        }
    }

    fn make_metadata(row_id: u64, weight: f32) -> ExampleMetadataV1 {
        // Include sample_role for negative weights so separate_examples() can identify them
        let provenance = if weight < 0.0 {
            serde_json::json!({ "weight": weight, "sample_role": "negative" }).to_string()
        } else {
            serde_json::json!({ "weight": weight }).to_string()
        };
        ExampleMetadataV1::new("test_source", row_id, "row-hash", provenance, 0)
    }

    #[test]
    fn test_separated_training() {
        if !mlx_device_available_for_tests() {
            return;
        }

        let config = TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 2,
            epochs: 2,
            hidden_dim: 128,
            vocab_size: 32000,
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: -1,
            coreml_placement: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_fallback_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
            max_tokens_per_batch: None,
            device_policy: None,
            checkpoint_interval: None,
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            early_stopping: None,
            patience: None,
            min_delta: None,
            determinism: None,
            moe_config: None,
            use_gpu_backward: false,
            optimizer_config: Default::default(),
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: 0.0,
            preprocessing: None,
            targets: Vec::new(),
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
            mlx_version: None,
        };

        let trainer = SeparatedLoRATrainer::new(config).unwrap();

        let examples = vec![
            TrainingExample::with_pad_token(
                vec![1, 2, 3, 4],
                vec![5, 6, 7, 8],
                0,
                make_metadata(0, 1.0),
            ),
            TrainingExample::with_pad_token(
                vec![9, 10, 11, 12],
                vec![13, 14, 15, 16],
                0,
                make_metadata(1, -1.0),
            ),
        ];

        let result = trainer
            .train_separated(&examples, CombinationStrategy::Difference)
            .unwrap();

        assert_eq!(result.positive_result.example_count, 1);
        assert_eq!(result.negative_result.example_count, 1);
        assert!(
            result.total_training_time_ms > 0,
            "training time should be positive"
        );
    }

    #[test]
    fn test_cross_entropy_loss_computation() {
        if !mlx_device_available_for_tests() {
            return;
        }

        let config = TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 2,
            epochs: 1,
            hidden_dim: 8,
            vocab_size: 10,
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: -1,
            coreml_placement: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_fallback_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
            max_tokens_per_batch: None,
            device_policy: None,
            checkpoint_interval: None,
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            early_stopping: None,
            patience: None,
            min_delta: None,
            determinism: None,
            moe_config: None,
            use_gpu_backward: false,
            optimizer_config: Default::default(),
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: 0.0,
            preprocessing: None,
            targets: Vec::new(),
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
            mlx_version: None,
        };

        let trainer = SeparatedLoRATrainer::new(config).unwrap();

        // Test with uniform logits - should give ln(vocab_size) loss
        let logits = vec![0.0f32; 10];
        let target = vec![3u32];
        let loss = trainer.compute_loss_cpu_reference(&logits, &target);
        let expected = (10.0f32).ln(); // ln(10) for uniform distribution
        assert!(
            (loss - expected).abs() < 1e-5,
            "Expected loss ~{}, got {}",
            expected,
            loss
        );

        // Test with peaked logits at target - should give low loss
        let mut peaked_logits = vec![-10.0f32; 10];
        peaked_logits[3] = 10.0; // High probability for target=3
        let loss = trainer.compute_loss_cpu_reference(&peaked_logits, &target);
        assert!(
            loss < 0.1,
            "Expected low loss for peaked distribution, got {}",
            loss
        );
    }
}
