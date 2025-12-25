//! Separated LoRA trainer for positive/negative weight groups
//!
//! Trains separate LoRA weight sets for positive and negative examples,
//! enabling better control over reinforcement learning and adversarial training.

use super::dataset::TrainingExample;
use super::trainer::{LoRAWeights, TrainingConfig};
use adapteros_config::resolve_telemetry_dir;
use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use adapteros_single_file_adapter::{
    format::{AdapterWeights, CombinationStrategy, WeightGroup, WeightGroupType, WeightMetadata},
    weights::combine_weight_groups,
};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

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

    /// Separate examples into positive and negative groups
    fn separate_examples(
        &self,
        examples: &[TrainingExample],
    ) -> (Vec<TrainingExample>, Vec<TrainingExample>) {
        let mut positive = Vec::new();
        let mut negative = Vec::new();

        for example in examples {
            if example.weight > 0.0 {
                positive.push(example.clone());
            } else if example.weight < 0.0 {
                negative.push(example.clone());
            }
            // Skip zero-weight examples
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
        let mut weights = self.initialize_weights();

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
            let (output, hidden) = self.forward(weights, &example.input)?;

            // Compute loss
            let loss = self.compute_loss(&output, &example.target);
            total_loss += loss;
            example_count += 1;

            // Backward pass and weight update
            self.backward_and_update(weights, &hidden, &output, &example.target, loss)?;
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
        let mut batches = Vec::new();
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

    /// Compute loss (simplified cross-entropy)
    fn compute_loss(&self, output: &[f32], target: &[u32]) -> f32 {
        let n = output.len().min(target.len());
        if n == 0 {
            return 0.0;
        }

        let mut loss = 0.0;
        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            let diff = output[i] - target_val;
            loss += diff * diff;
        }

        loss / n as f32
    }

    /// Backward pass and weight update
    fn backward_and_update(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        loss: f32,
    ) -> Result<()> {
        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;

        // Compute gradient (simplified)
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            let target_val = (target[i] as f32) / 1000.0;
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32;
        }

        // Update LoRA_A
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                    weights.lora_a[r][h_idx] -= learning_rate * grad;
                }
            }
        }

        // Update LoRA_B
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx] * loss;
                        weights.lora_b[h_idx][r] -= learning_rate * grad;
                    }
                }
            }
        }

        Ok(())
    }

    /// Initialize LoRA weights with deterministic seeded RNG
    fn initialize_weights(&self) -> LoRAWeights {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        // Create deterministic RNG from training seed
        let mut rng = ChaCha20Rng::seed_from_u64(self.training_seed);

        let mut lora_a = Vec::new();
        let mut lora_b = Vec::new();

        // Initialize LoRA_A (rank × hidden_dim)
        for _ in 0..self.config.rank {
            let mut row = Vec::new();
            for _ in 0..self.config.hidden_dim {
                row.push(0.01 * (rng.gen::<f32>() - 0.5));
            }
            lora_a.push(row);
        }

        // Initialize LoRA_B (hidden_dim × rank)
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
    use std::collections::HashMap;

    #[test]
    fn test_separated_training() {
        let config = TrainingConfig {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 2,
            epochs: 2,
            hidden_dim: 128,
            vocab_size: 32000,
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
            determinism: None,
            moe_config: None,
        };

        let trainer = SeparatedLoRATrainer::new(config).unwrap();

        let examples = vec![
            TrainingExample {
                input: vec![1, 2, 3, 4],
                target: vec![5, 6, 7, 8],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![9, 10, 11, 12],
                target: vec![13, 14, 15, 16],
                metadata: HashMap::new(),
                weight: -1.0,
            },
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
}
