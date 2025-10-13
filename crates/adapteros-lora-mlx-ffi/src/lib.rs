//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, Result};
use std::path::Path;

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod backend;
pub mod lora;
pub mod routing;
pub mod tensor;

#[cfg(test)]
pub mod mock;

pub use backend::MLXFFIBackend;
pub use lora::{LoRAAdapter, LoRAConfig};
pub use routing::apply_multi_lora;
pub use tensor::MLXFFITensor;

/// MLX model wrapper for inference using FFI
pub struct MLXFFIModel {
    /// C++ MLX model object
    model: *mut mlx_model_t,
    /// Model configuration
    pub config: ModelConfig,
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

impl MLXFFIModel {
    /// Load a model from MLX format using FFI
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

        // Convert path to C string
        let path_str = model_path
            .to_str()
            .ok_or_else(|| AosError::Internal("Invalid model path".to_string()))?;
        let path_cstr = std::ffi::CString::new(path_str)
            .map_err(|e| AosError::Internal(format!("Invalid path string: {}", e)))?;

        // Clear any previous errors
        unsafe {
            mlx_clear_error();
        }

        // Load model via FFI
        let model = unsafe { mlx_model_load(path_cstr.as_ptr()) };
        if model.is_null() {
            let error_msg = unsafe { mlx_get_last_error() };
            let error_str = if error_msg.is_null() {
                "Unknown MLX error".to_string()
            } else {
                unsafe {
                    std::ffi::CStr::from_ptr(error_msg)
                        .to_string_lossy()
                        .to_string()
                }
            };
            return Err(AosError::Mlx(format!(
                "Failed to load MLX model: {}",
                error_str
            )));
        }

        tracing::info!("MLX model loaded via FFI: {}", path_str);

        Ok(Self { model, config })
    }

    /// Run forward pass for a single token using FFI
    ///
    /// # Arguments
    /// * `token_ids` - Input token IDs
    /// * `position` - Current position in sequence
    ///
    /// # Returns
    /// Logits for next token prediction
    pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        // Convert token_ids to C array
        let token_ints: Vec<i32> = token_ids.iter().map(|&x| x as i32).collect();

        // Create MLX array from token IDs
        let input_array =
            unsafe { mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32) };
        if input_array.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        // Run forward pass
        let output_array = unsafe { mlx_model_forward(self.model, input_array) };
        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(AosError::Mlx("Failed to run model forward".to_string()));
        }

        // Extract output data
        let output_size = unsafe { mlx_array_size(output_array) };
        let output_data = unsafe { mlx_array_data(output_array) };

        let result: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Clean up
        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

        tracing::debug!(
            "MLX FFI forward pass complete: {} tokens -> {} logits",
            token_ids.len(),
            result.len()
        );

        Ok(result)
    }

    /// Generate text from a prompt using FFI
    ///
    /// # Arguments
    /// * `prompt` - Input text prompt
    /// * `max_tokens` - Maximum tokens to generate
    ///
    /// # Returns
    /// Generated text
    pub fn generate(&self, prompt: &str, _max_tokens: usize) -> Result<String> {
        // For now, return a placeholder implementation
        // Full implementation would require tokenization and generation loop
        tracing::warn!("MLX FFI generate() not yet implemented, returning placeholder");
        Ok(format!("[MLX FFI placeholder for: {}]", prompt))
    }

    /// Run forward pass with hidden states using FFI
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
        // For now, just run forward pass and return empty hidden states
        // Full implementation would require model modifications to expose intermediate activations
        let logits = self.forward(token_ids, 0)?;
        let hidden_states = std::collections::HashMap::new();

        tracing::debug!(
            "MLX FFI forward with hidden states: {} tokens -> {} logits, {} hidden state modules",
            token_ids.len(),
            logits.len(),
            hidden_states.len()
        );

        Ok((logits, hidden_states))
    }

    /// Get model configuration
    pub fn config(&self) -> &ModelConfig {
        &self.config
    }
}

impl Drop for MLXFFIModel {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe {
                mlx_model_free(self.model);
            }
        }
    }
}

// Safety: MLX FFI model is thread-safe
unsafe impl Send for MLXFFIModel {}
unsafe impl Sync for MLXFFIModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_parsing() {
        let config_json = r#"
        {
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "vocab_size": 32000,
            "max_position_embeddings": 32768,
            "rope_theta": 10000.0
        }
        "#;

        let config: ModelConfig = serde_json::from_str(config_json).unwrap();
        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_hidden_layers, 32);
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    #[ignore] // Requires MLX model
    fn test_model_loading() {
        // This test would require a real MLX model
        // Skipped for now
    }
}
