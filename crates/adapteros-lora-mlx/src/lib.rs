//! MLX integration for MPLoRA
//!
//! This crate provides Python/MLX bindings for running inference on Apple Silicon.
//! It uses PyO3 to call into MLX's Python API for model loading and execution.

use adapteros_core::{AosError, Result};
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::path::Path;
use std::sync::Arc;

pub mod backend;
pub mod lora;
pub mod routing;
pub mod tensor;

#[cfg(test)]
pub mod mock;

pub use backend::MLXBackend;
pub use lora::{LoRAAdapter, LoRAConfig};
pub use routing::apply_multi_lora;
pub use tensor::MLXTensor;

/// MLX model wrapper for inference
pub struct MLXModel {
    /// Python MLX model object
    model: PyObject,
    /// Model configuration
    config: ModelConfig,
    /// Python runtime guard
    _py: Arc<pyo3::Python<'static>>,
}

/// Model configuration parsed from config.json
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelConfig {
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub intermediate_size: usize,
    pub vocab_size: usize,
    pub max_position_embeddings: usize,
    #[serde(default = "default_rope_theta")]
    pub rope_theta: f32,
}

fn default_rope_theta() -> f32 {
    10000.0
}

impl MLXModel {
    /// Load a model from MLX format
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing model files
    ///
    /// # Returns
    /// Loaded MLX model ready for inference
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let model_path = model_path.as_ref();

        // Load config
        let config_path = model_path.join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;
        let config: ModelConfig = serde_json::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

        // Initialize Python and MLX
        Python::with_gil(|py| {
            // Import MLX
            let mlx = py
                .import_bound("mlx.core")
                .map_err(|e| AosError::Mlx(format!("Failed to import mlx.core: {}", e)))?;

            let _mlx_nn = py
                .import_bound("mlx.nn")
                .map_err(|e| AosError::Mlx(format!("Failed to import mlx.nn: {}", e)))?;

            // Load model weights
            let weights_path = model_path.join("model.safetensors");
            let weights_path_str = weights_path
                .to_str()
                .ok_or_else(|| AosError::Internal("Invalid weights path".to_string()))?;
            let weights = mlx
                .call_method1("load", (weights_path_str,))
                .map_err(|e| AosError::Mlx(format!("Failed to load weights: {}", e)))?;

            // Create model structure
            // For now, we'll store the weights dict as the model
            // In a full implementation, this would construct the actual model architecture
            let model = weights.to_object(py);

            // Store a static Python reference
            let py_static = unsafe { std::mem::transmute::<Python, Python<'static>>(py) };

            Ok(Self {
                model,
                config,
                _py: Arc::new(py_static),
            })
        })
    }

    /// Run forward pass for a single token
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Current position in sequence
    ///
    /// # Returns
    /// Logits for next token prediction
    pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        Python::with_gil(|py| {
            // Import MLX
            let mlx = py
                .import_bound("mlx.core")
                .map_err(|e| AosError::Mlx(format!("Failed to import mlx: {}", e)))?;

            // Get model from stored PyObject
            let model = self.model.bind(py);

            // Convert token_ids to MLX array
            let tokens_list = PyList::new_bound(py, token_ids);
            let tokens = mlx
                .call_method1("array", (tokens_list,))
                .map_err(|e| AosError::Mlx(format!("Failed to create token array: {}", e)))?;

            // Call model forward pass
            // The model should be a callable that takes tokens and returns logits
            let logits = model
                .call_method1("__call__", (tokens,))
                .map_err(|e| AosError::Mlx(format!("Failed to run model forward: {}", e)))?;

            // Convert to list and extract
            let logits_list = logits
                .call_method0("tolist")
                .map_err(|e| AosError::Mlx(format!("Failed to convert logits to list: {}", e)))?;

            let result: Vec<f32> = logits_list
                .extract()
                .map_err(|e| AosError::Mlx(format!("Failed to extract logits: {}", e)))?;

            tracing::debug!(
                "MLX forward pass complete: {} tokens -> {} logits",
                token_ids.len(),
                result.len()
            );

            Ok(result)
        })
    }

    /// Generate text from a prompt
    ///
    /// # Arguments
    /// * `prompt` - Input text prompt
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (higher = more random)
    /// * `top_p` - Nucleus sampling threshold
    ///
    /// # Returns
    /// Generated text
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<String> {
        // For now, return a stub implementation
        // In production, this would:
        // 1. Tokenize the prompt
        // 2. Run forward passes in a loop
        // 3. Sample from logits using temperature/top_p
        // 4. Detokenize and return text

        tracing::warn!("MLX generate() is a stub implementation");
        Ok(format!(
            "Generated response for prompt: {} (stub implementation, max_tokens={}, temp={}, top_p={})",
            &prompt[..prompt.len().min(50)],
            max_tokens,
            temperature,
            top_p
        ))
    }

    /// Run forward pass with hidden states for LoRA application
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    ///
    /// # Returns
    /// Tuple of (logits, hidden_states_by_module)
    pub fn forward_with_hidden_states(
        &self,
        token_ids: &[u32],
    ) -> Result<(Vec<f32>, std::collections::HashMap<String, Vec<f32>>)> {
        Python::with_gil(|py| {
            // Import MLX
            let mlx = py
                .import_bound("mlx.core")
                .map_err(|e| AosError::Mlx(format!("Failed to import mlx: {}", e)))?;

            // Get model
            let model = self.model.bind(py);

            // Convert tokens
            let tokens_list = PyList::new_bound(py, token_ids);
            let tokens = mlx
                .call_method1("array", (tokens_list,))
                .map_err(|e| AosError::Mlx(format!("Failed to create token array: {}", e)))?;

            // For now, just run forward pass and return empty hidden states
            // Full implementation would require model modifications to expose intermediate activations
            let logits = model
                .call_method1("__call__", (tokens,))
                .map_err(|e| AosError::Mlx(format!("Failed to run model forward: {}", e)))?;

            let logits_list = logits
                .call_method0("tolist")
                .map_err(|e| AosError::Mlx(format!("Failed to convert logits: {}", e)))?;

            let result: Vec<f32> = logits_list
                .extract()
                .map_err(|e| AosError::Mlx(format!("Failed to extract logits: {}", e)))?;

            // TODO: Extract hidden states from model
            // This requires the MLX model to return intermediate activations
            let hidden_states = std::collections::HashMap::new();

            tracing::debug!(
                "MLX forward with hidden states: {} tokens -> {} logits, {} hidden state modules",
                token_ids.len(),
                result.len(),
                hidden_states.len()
            );

            Ok((result, hidden_states))
        })
    }

    /// Get model configuration
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }

    /// Get hidden size
    pub fn hidden_size(&self) -> usize {
        self.config.hidden_size
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.config.vocab_size
    }
}

// Safety: MLXModel can be sent between threads as long as we use GIL protection
unsafe impl Send for MLXModel {}
unsafe impl Sync for MLXModel {}

impl Clone for MLXModel {
    fn clone(&self) -> Self {
        Python::with_gil(|py| Self {
            model: self.model.clone_ref(py),
            config: self.config.clone(),
            _py: Arc::clone(&self._py),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_parsing() {
        let config_json = r#"{
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 32,
            "intermediate_size": 11008,
            "vocab_size": 151936,
            "max_position_embeddings": 32768
        }"#;

        let config: ModelConfig =
            serde_json::from_str(config_json).expect("Test config should parse");
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.vocab_size, 151936);
        assert_eq!(config.rope_theta, 10000.0); // default value
    }
}
