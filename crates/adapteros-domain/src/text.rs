//! Text domain adapter with deterministic tokenization and LoRA merging

use adapteros_core::B3Hash;
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_numerics::noise::{EpsilonStats, Tensor};
use adapteros_trace::{Event, EventMetadata, LogicalTimestamp};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::{AdapterMetadata, DomainAdapter, TensorData};
use crate::error::{DomainAdapterError, Result};
use crate::manifest::{load_manifest, AdapterManifest};
use serde::{Deserialize, Serialize};

/// LoRA merge visualization data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAMergeVisualization {
    pub adapter_name: String,
    pub rank: usize,
    pub alpha: f32,
    pub scaling_factor: f32,
    pub delta_norm: f32,
    pub merge_timestamp: u64,
    pub delta_stats: DeltaStats,
}

/// Delta statistics for LoRA merge visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaStats {
    pub mean: f32,
    pub std_dev: f32,
    pub min: f32,
    pub max: f32,
}

/// Text adapter for deterministic text processing
///
/// This adapter handles:
/// - Canonical UTF-8 normalization
/// - Deterministic BPE tokenization
/// - LoRA weight merging
/// - Text-to-tensor conversion
pub struct TextAdapter {
    /// Adapter metadata
    metadata: AdapterMetadata,
    /// Internal state
    state: Arc<RwLock<TextAdapterState>>,
    /// Manifest configuration
    manifest: AdapterManifest,
}

#[derive(Debug)]
struct TextAdapterState {
    /// Whether adapter is initialized
    initialized: bool,
    /// Vocabulary size
    vocab_size: usize,
    /// Maximum sequence length
    max_sequence_length: usize,
    /// Current epsilon statistics
    epsilon_stats: Option<EpsilonStats>,
    /// Token counter for deterministic generation
    token_counter: u64,
}

impl TextAdapter {
    /// Load text adapter from manifest
    pub fn load<P: AsRef<std::path::Path>>(manifest_path: P) -> Result<Self> {
        let manifest = load_manifest(manifest_path)?;

        // Extract configuration
        let vocab_size = manifest.get_parameter_i64("vocab_size").unwrap_or(32000) as usize;

        let max_sequence_length = manifest
            .get_parameter_i64("max_sequence_length")
            .unwrap_or(2048) as usize;

        let model_hash = manifest.parse_hash()?;

        let metadata = AdapterMetadata {
            name: manifest.adapter.name.clone(),
            version: manifest.adapter.version.clone(),
            model_hash,
            input_format: manifest.adapter.input_format.clone(),
            output_format: manifest.adapter.output_format.clone(),
            epsilon_threshold: manifest.adapter.epsilon_threshold,
            deterministic: manifest.adapter.deterministic,
            custom: HashMap::new(),
        };

        let state = TextAdapterState {
            initialized: false,
            vocab_size,
            max_sequence_length,
            epsilon_stats: None,
            token_counter: 0,
        };

        tracing::info!(
            "Created TextAdapter '{}' v{} (vocab_size={}, max_len={})",
            metadata.name,
            metadata.version,
            vocab_size,
            max_sequence_length
        );

        Ok(Self {
            metadata,
            state: Arc::new(RwLock::new(state)),
            manifest,
        })
    }

    /// Normalize text to canonical UTF-8 form
    fn normalize_text(&self, text: &str) -> String {
        // Apply Unicode NFC normalization for deterministic text processing
        // This ensures that visually identical strings are byte-identical
        use unicode_normalization::UnicodeNormalization;
        text.nfc().collect::<String>()
    }

    /// Tokenize text deterministically
    ///
    /// This is a simplified tokenization that splits on whitespace
    /// and converts to token IDs. In production, this would use a
    /// proper BPE tokenizer with deterministic ordering.
    fn tokenize(&self, text: &str) -> Vec<u32> {
        let normalized = self.normalize_text(text);
        let state = self.state.read();

        // Simple whitespace tokenization (deterministic)
        // In production, this would use a proper tokenizer
        let tokens: Vec<u32> = normalized
            .split_whitespace()
            .map(|word| {
                // Hash each word to get a deterministic token ID
                let hash = B3Hash::hash(word.as_bytes());
                // Use first 4 bytes as token ID, modulo vocab size
                let token_id = u32::from_le_bytes([
                    hash.as_bytes()[0],
                    hash.as_bytes()[1],
                    hash.as_bytes()[2],
                    hash.as_bytes()[3],
                ]) % (state.vocab_size as u32);
                token_id
            })
            .collect();

        tracing::debug!("Tokenized '{}' into {} tokens", normalized, tokens.len());
        tokens
    }

    /// Convert tokens to tensor
    fn tokens_to_tensor(&self, tokens: &[u32]) -> Tensor {
        let state = self.state.read();

        // Pad or truncate to max_sequence_length
        let mut padded = tokens.to_vec();
        padded.resize(state.max_sequence_length, 0);

        // Convert to f32 tensor
        let data: Vec<f32> = padded.iter().map(|&t| t as f32).collect();

        Tensor::new(data, vec![1, state.max_sequence_length])
    }

    /// Apply LoRA merge with visualization support
    ///
    /// This implements a simplified LoRA merge with visualization capabilities:
    /// 1. Load LoRA matrices A and B (simulated)
    /// 2. Compute delta = B @ A (deterministic matmul)
    /// 3. Apply scaling factor alpha/r
    /// 4. Merge with base weights: W' = W + delta
    /// 5. Generate visualization data for the merge process
    fn apply_lora_merge(&self, tensor: &Tensor) -> Tensor {
        let _state = self.state.read();

        // Simulate LoRA matrices A and B
        // In production, these would be loaded from adapter files
        let rank = 16; // LoRA rank
        let hidden_dim = tensor.shape[1];

        // Generate deterministic LoRA matrices based on adapter hash
        let adapter_hash = B3Hash::hash(self.metadata.name.as_bytes());
        let seed_bytes = adapter_hash.as_bytes();

        // Matrix A: [hidden_dim, rank]
        let mut matrix_a = Vec::with_capacity(hidden_dim * rank);
        for i in 0..hidden_dim {
            for j in 0..rank {
                let idx = (i * rank + j) % seed_bytes.len();
                let val = (seed_bytes[idx] as f32 - 128.0) / 128.0; // Normalize to [-1, 1]
                matrix_a.push(val);
            }
        }

        // Matrix B: [rank, hidden_dim]
        let mut matrix_b = Vec::with_capacity(rank * hidden_dim);
        for i in 0..rank {
            for j in 0..hidden_dim {
                let idx = (i * hidden_dim + j) % seed_bytes.len();
                let val = (seed_bytes[idx] as f32 - 128.0) / 128.0; // Normalize to [-1, 1]
                matrix_b.push(val);
            }
        }

        // Compute delta = B @ A
        let mut delta = Vec::with_capacity(hidden_dim * hidden_dim);
        for i in 0..hidden_dim {
            for j in 0..hidden_dim {
                let mut sum = 0.0f32;
                for k in 0..rank {
                    sum += matrix_b[k * hidden_dim + j] * matrix_a[i * rank + k];
                }
                delta.push(sum);
            }
        }

        // Apply scaling factor (alpha/r)
        let alpha = 32.0; // LoRA alpha parameter
        let scaling_factor = alpha / rank as f32;
        let scaled_delta: Vec<f32> = delta.iter().map(|&x| x * scaling_factor).collect();

        // Merge with base weights: W' = W + delta
        let mut merged_data = Vec::with_capacity(tensor.len());
        for (i, &base_val) in tensor.data.iter().enumerate() {
            let delta_val = scaled_delta[i % scaled_delta.len()];
            merged_data.push(base_val + delta_val);
        }

        // Generate visualization data
        let visualization_data = LoRAMergeVisualization {
            adapter_name: self.metadata.name.clone(),
            rank,
            alpha,
            scaling_factor,
            delta_norm: scaled_delta.iter().map(|&x| x * x).sum::<f32>().sqrt(),
            merge_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            delta_stats: DeltaStats {
                mean: scaled_delta.iter().sum::<f32>() / scaled_delta.len() as f32,
                std_dev: {
                    let mean = scaled_delta.iter().sum::<f32>() / scaled_delta.len() as f32;
                    let variance = scaled_delta
                        .iter()
                        .map(|&x| (x - mean).powi(2))
                        .sum::<f32>()
                        / scaled_delta.len() as f32;
                    variance.sqrt()
                },
                min: scaled_delta.iter().fold(f32::INFINITY, |a, &b| a.min(b)),
                max: scaled_delta
                    .iter()
                    .fold(f32::NEG_INFINITY, |a, &b| a.max(b)),
            },
        };

        // Store visualization data in adapter state
        // In production, this would be sent to a visualization service
        tracing::info!(
            "LoRA merge visualization: adapter={}, rank={}, alpha={}, delta_norm={:.4}, mean={:.4}, std={:.4}",
            visualization_data.adapter_name,
            visualization_data.rank,
            visualization_data.alpha,
            visualization_data.delta_norm,
            visualization_data.delta_stats.mean,
            visualization_data.delta_stats.std_dev
        );

        tracing::debug!("Applied LoRA merge with visualization data");

        Tensor::new(merged_data, tensor.shape.clone())
    }

    /// Compute epsilon statistics
    fn compute_epsilon(&self, reference: &Tensor, output: &Tensor) -> Result<EpsilonStats> {
        use adapteros_numerics::noise::measure_error;

        let stats = measure_error(reference, output, self.metadata.name.clone())?;

        if stats.exceeds_threshold(self.metadata.epsilon_threshold) {
            tracing::warn!(
                "Epsilon threshold exceeded: {} > {}",
                stats.l2_error,
                self.metadata.epsilon_threshold
            );
        }

        Ok(stats)
    }

    /// Get LoRA merge visualization data
    ///
    /// This function simulates a LoRA merge and returns visualization data
    /// that can be used by the UI to display merge statistics and graphs.
    pub fn get_lora_merge_visualization(&self, input_text: &str) -> Result<LoRAMergeVisualization> {
        // Tokenize input text
        let tokens = self.tokenize(input_text);
        let tensor = self.tokens_to_tensor(&tokens);

        // Apply LoRA merge (this generates the visualization data internally)
        let _merged_tensor = self.apply_lora_merge(&tensor);

        // In a real implementation, the visualization data would be stored
        // and retrieved here. For now, we'll generate it on-demand.
        let adapter_hash = B3Hash::hash(self.metadata.name.as_bytes());
        let seed_bytes = adapter_hash.as_bytes();

        // Generate deterministic visualization data
        let rank = 16;
        let alpha = 32.0;
        let scaling_factor = alpha / rank as f32;

        // Simulate delta statistics based on adapter hash
        let mut delta_values = Vec::new();
        for i in 0..100 {
            // Sample 100 values
            let idx = i % seed_bytes.len();
            let val = (seed_bytes[idx] as f32 - 128.0) / 128.0 * scaling_factor;
            delta_values.push(val);
        }

        let mean = delta_values.iter().sum::<f32>() / delta_values.len() as f32;
        let variance = delta_values
            .iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f32>()
            / delta_values.len() as f32;
        let std_dev = variance.sqrt();

        let visualization = LoRAMergeVisualization {
            adapter_name: self.metadata.name.clone(),
            rank,
            alpha,
            scaling_factor,
            delta_norm: delta_values.iter().map(|&x| x * x).sum::<f32>().sqrt(),
            merge_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            delta_stats: DeltaStats {
                mean,
                std_dev,
                min: delta_values.iter().fold(f32::INFINITY, |a, &b| a.min(b)),
                max: delta_values
                    .iter()
                    .fold(f32::NEG_INFINITY, |a, &b| a.max(b)),
            },
        };

        Ok(visualization)
    }
}

impl DomainAdapter for TextAdapter {
    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn metadata(&self) -> &AdapterMetadata {
        &self.metadata
    }

    fn prepare(&mut self, executor: &mut DeterministicExecutor) -> Result<()> {
        let mut state = self.state.write();

        if state.initialized {
            tracing::warn!("TextAdapter '{}' already initialized", self.metadata.name);
            return Ok(());
        }

        // Derive a deterministic seed for this adapter
        let adapter_seed = executor.derive_seed(&format!("text_adapter:{}", self.metadata.name));

        tracing::info!(
            "Initialized TextAdapter '{}' with seed: {:?}",
            self.metadata.name,
            &adapter_seed[..8]
        );

        state.initialized = true;
        Ok(())
    }

    fn forward(&mut self, input: &TensorData) -> Result<TensorData> {
        let state = self.state.read();

        if !state.initialized {
            return Err(DomainAdapterError::AdapterNotInitialized {
                adapter_name: self.metadata.name.clone(),
            });
        }

        // For text adapter, we expect the input tensor to represent text tokens
        // In a real implementation, this would perform the forward pass through
        // the language model with LoRA adapters applied

        let input_tensor = &input.tensor;

        // Apply LoRA merge
        let merged_tensor = self.apply_lora_merge(input_tensor);

        // Create output tensor (in production, this would be actual model output)
        let output_tensor = merged_tensor;

        let output_data = TensorData::new(output_tensor, "f32".to_string());

        tracing::debug!(
            "Forward pass completed for TextAdapter '{}'",
            self.metadata.name
        );

        Ok(output_data)
    }

    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData> {
        // Apply any final normalization or quantization
        // For now, this is a pass-through

        tracing::debug!(
            "Postprocessing output for TextAdapter '{}'",
            self.metadata.name
        );

        Ok(output.clone())
    }

    fn epsilon_stats(&self) -> Option<EpsilonStats> {
        self.state.read().epsilon_stats.clone()
    }

    fn reset(&mut self) {
        let mut state = self.state.write();
        state.token_counter = 0;
        state.epsilon_stats = None;

        tracing::info!("Reset TextAdapter '{}'", self.metadata.name);
    }

    fn create_trace_event(
        &self,
        tick_id: u64,
        op_id: String,
        inputs: &HashMap<String, serde_json::Value>,
        outputs: &HashMap<String, serde_json::Value>,
    ) -> Event {
        use adapteros_trace::schema::Event;

        let metadata = EventMetadata {
            global_seed: B3Hash::hash(b"text_adapter_seed"),
            plan_id: "text_adapter_plan".to_string(),
            cpid: "text_adapter_cpid".to_string(),
            tenant_id: "default".to_string(),
            session_id: "default".to_string(),
            adapter_ids: vec![self.metadata.name.clone()],
            memory_usage_mb: 0,
            gpu_utilization_pct: 0.0,
            custom: HashMap::new(),
        };

        let logical_timestamp = LogicalTimestamp::new(
            tick_id,                                            // global_tick
            0,                                                  // op_tick
            None,                                               // token_position
            B3Hash::hash(format!("text_{}", op_id).as_bytes()), // derivation_hash
        );

        Event::new(
            tick_id,
            op_id,
            "text.forward".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata,
            logical_timestamp,
        )
    }
}

/// Helper function to create a text tensor from string input
pub fn text_to_tensor(adapter: &TextAdapter, text: &str) -> Result<TensorData> {
    let tokens = adapter.tokenize(text);
    let tensor = adapter.tokens_to_tensor(&tokens);

    Ok(TensorData::new(tensor, "f32".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_manifest() -> (AdapterManifest, NamedTempFile) {
        use crate::manifest::{save_manifest, AdapterManifest};

        let mut manifest = AdapterManifest::new(
            "test_text_adapter".to_string(),
            "1.0.0".to_string(),
            "test_model".to_string(),
            "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605".to_string(),
        );

        manifest.adapter.input_format = "UTF8 canonical".to_string();
        manifest.adapter.output_format = "BPE deterministic".to_string();

        manifest.adapter.parameters.insert(
            "vocab_size".to_string(),
            serde_json::Value::Number(1000.into()),
        );

        manifest.adapter.parameters.insert(
            "max_sequence_length".to_string(),
            serde_json::Value::Number(128.into()),
        );

        let temp_file = NamedTempFile::new().unwrap();
        save_manifest(&manifest, temp_file.path()).unwrap();

        (manifest, temp_file)
    }

    #[test]
    fn test_text_adapter_load() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        assert_eq!(adapter.name(), "test_text_adapter");
        assert_eq!(adapter.state.read().vocab_size, 1000);
        assert_eq!(adapter.state.read().max_sequence_length, 128);
    }

    #[test]
    fn test_text_normalization() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        let text = "Hello World";
        let normalized = adapter.normalize_text(text);

        assert_eq!(normalized, text);
    }

    #[test]
    fn test_tokenization_deterministic() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        let text = "Hello World";
        let tokens1 = adapter.tokenize(text);
        let tokens2 = adapter.tokenize(text);

        assert_eq!(tokens1, tokens2);
    }

    #[test]
    fn test_tokens_to_tensor() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        let tokens = vec![1, 2, 3];
        let tensor = adapter.tokens_to_tensor(&tokens);

        assert_eq!(tensor.shape, vec![1, 128]); // Padded to max_sequence_length
    }

    #[test]
    fn test_lora_merge_visualization() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        let input_text = "Hello world";
        let visualization = adapter.get_lora_merge_visualization(input_text).unwrap();

        assert_eq!(visualization.adapter_name, "test_text_adapter");
        assert_eq!(visualization.rank, 16);
        assert_eq!(visualization.alpha, 32.0);
        assert!(visualization.scaling_factor > 0.0);
        assert!(visualization.delta_norm > 0.0);
        assert!(visualization.delta_stats.std_dev >= 0.0);
    }

    #[test]
    fn test_lora_merge_deterministic() {
        let (_manifest, temp_file) = create_test_manifest();
        let adapter = TextAdapter::load(temp_file.path()).unwrap();

        let input_text = "Test input";
        let viz1 = adapter.get_lora_merge_visualization(input_text).unwrap();
        let viz2 = adapter.get_lora_merge_visualization(input_text).unwrap();

        // Visualization should be deterministic for the same input
        assert_eq!(viz1.adapter_name, viz2.adapter_name);
        assert_eq!(viz1.rank, viz2.rank);
        assert_eq!(viz1.alpha, viz2.alpha);
        assert_eq!(viz1.scaling_factor, viz2.scaling_factor);
        assert_eq!(viz1.delta_stats.mean, viz2.delta_stats.mean);
        assert_eq!(viz1.delta_stats.std_dev, viz2.delta_stats.std_dev);
    }
}
