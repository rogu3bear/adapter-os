//! Text domain adapter with deterministic tokenization and LoRA merging

use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_numerics::noise::{EpsilonStats, Tensor};
use adapteros_trace::{Event, EventMetadata};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::adapter::{AdapterMetadata, DomainAdapter, TensorData};
use crate::error::{DomainAdapterError, Result};
use crate::manifest::{load_manifest, AdapterManifest};

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
        let vocab_size = manifest
            .get_parameter_i64("vocab_size")
            .unwrap_or(32000) as usize;
        
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
            .enumerate()
            .map(|(_idx, word)| {
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
    
    /// Apply LoRA merge (simplified)
    ///
    /// In production, this would merge LoRA weights with base model weights
    /// using deterministic matrix operations.
    fn apply_lora_merge(&self, tensor: &Tensor) -> Tensor {
        // For now, this is a no-op that returns the input tensor
        // In production, this would:
        // 1. Load LoRA matrices A and B
        // 2. Compute delta = B @ A (deterministic matmul)
        // 3. Apply scaling factor alpha/r
        // 4. Merge with base weights: W' = W + delta
        
        tracing::debug!("Applied LoRA merge (no-op in stub)");
        tensor.clone()
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
        
        tracing::debug!("Forward pass completed for TextAdapter '{}'", self.metadata.name);
        
        Ok(output_data)
    }
    
    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData> {
        // Apply any final normalization or quantization
        // For now, this is a pass-through
        
        tracing::debug!("Postprocessing output for TextAdapter '{}'", self.metadata.name);
        
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
        
        Event::new(
            tick_id,
            op_id,
            "text.forward".to_string(),
            inputs.clone(),
            outputs.clone(),
            metadata,
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
}

