//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, Result};
use generation::{run_generation_loop, SimpleTokenizer};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::path::Path;

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod backend;
mod generation;
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
        let path_cstr = CString::new(path_str)
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
        let result = unsafe { self.forward_raw(token_ids)? };

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
    pub fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String> {
        if max_tokens == 0 {
            return Err(AosError::Validation(
                "max_tokens must be greater than zero".to_string(),
            ));
        }

        let tokenizer = SimpleTokenizer::new(self.config.vocab_size);
        let mut tokens = tokenizer.encode(prompt)?;
        if tokens.is_empty() {
            return Err(AosError::Validation(
                "Prompt did not produce any tokens".to_string(),
            ));
        }

        if tokens.len() >= self.config.max_position_embeddings {
            return Err(AosError::Validation(format!(
                "Prompt exceeds maximum context window ({} >= {})",
                tokens.len(),
                self.config.max_position_embeddings
            )));
        }

        let prompt_len = tokens.len();
        let vocab_size = self.config.vocab_size;
        let max_context = self.config.max_position_embeddings;

        let generated_ids = run_generation_loop(
            &tokenizer,
            tokens,
            max_tokens,
            vocab_size,
            max_context,
            |current_tokens| {
                let (logits, hidden_states) = self.forward_with_hidden_states(current_tokens)?;
                tracing::trace!(
                    "generation_step",
                    tokens = current_tokens.len(),
                    logits = logits.len(),
                    hidden_modules = hidden_states.len()
                );
                Ok((logits, hidden_states.len()))
            },
        )?;

        let text = tokenizer.decode(&generated_ids)?;
        tracing::info!(
            "MLX FFI generation finished: {} prompt tokens -> {} generated tokens",
            prompt_len,
            generated_ids.len()
        );

        Ok(text)
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
    ) -> Result<(Vec<f32>, HashMap<String, Vec<f32>>)> {
        if token_ids.is_empty() {
            return Err(AosError::Validation(
                "forward_with_hidden_states requires at least one token".to_string(),
            ));
        }

        let mut hidden_states = HashMap::new();

        unsafe {
            mlx_clear_error();
            let token_ints: Vec<i32> = token_ids.iter().map(|&id| id as i32).collect();
            let input_array = mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32);
            if input_array.is_null() {
                return Err(AosError::Mlx(last_mlx_error(
                    "Failed to create input array",
                )));
            }

            let mut hidden_ptr: *mut *mut mlx_array_t = std::ptr::null_mut();
            let mut hidden_len: i32 = 0;
            let logits_array = mlx_model_forward_with_hidden_states(
                self.model,
                input_array,
                &mut hidden_ptr,
                &mut hidden_len,
            );

            if logits_array.is_null() {
                mlx_array_free(input_array);
                return Err(AosError::Mlx(last_mlx_error(
                    "Failed to run model forward with hidden states",
                )));
            }

            let logits_result = extract_array(logits_array);

            if !hidden_ptr.is_null() && hidden_len > 0 {
                let modules = module_names();
                let hidden_slices = std::slice::from_raw_parts(hidden_ptr, hidden_len as usize);
                for (idx, &array_ptr) in hidden_slices.iter().enumerate() {
                    if array_ptr.is_null() {
                        continue;
                    }

                    match extract_array(array_ptr) {
                        Ok(values) => {
                            let name = modules
                                .get(idx)
                                .cloned()
                                .unwrap_or_else(|| format!("hidden_{}", idx));
                            hidden_states.insert(name, values);
                        }
                        Err(err) => {
                            tracing::warn!("Failed to extract hidden state {}: {}", idx, err);
                        }
                    }
                }

                libc::free(hidden_ptr as *mut libc::c_void);
            }

            mlx_array_free(input_array);

            let logits = match logits_result {
                Ok(values) => values,
                Err(err) => return Err(err),
            };

            tracing::debug!(
                "MLX FFI forward with hidden states: {} tokens -> {} logits, {} hidden state modules",
                token_ids.len(),
                logits.len(),
                hidden_states.len()
            );

            return Ok((logits, hidden_states));
        }
    }

    unsafe fn forward_raw(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        mlx_clear_error();
        let token_ints: Vec<i32> = token_ids.iter().map(|&id| id as i32).collect();
        let input_array = mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32);
        if input_array.is_null() {
            return Err(AosError::Mlx(last_mlx_error(
                "Failed to create input array",
            )));
        }

        let output_array = mlx_model_forward(self.model, input_array);
        if output_array.is_null() {
            mlx_array_free(input_array);
            return Err(AosError::Mlx(last_mlx_error("Failed to run model forward")));
        }

        let result = match extract_array(output_array) {
            Ok(values) => values,
            Err(err) => {
                mlx_array_free(input_array);
                return Err(err);
            }
        };

        mlx_array_free(input_array);
        Ok(result)
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

fn last_mlx_error(default: &str) -> String {
    unsafe {
        let error_msg = mlx_get_last_error();
        if error_msg.is_null() {
            default.to_string()
        } else {
            CStr::from_ptr(error_msg).to_string_lossy().into_owned()
        }
    }
}

unsafe fn extract_array(array: *mut mlx_array_t) -> Result<Vec<f32>> {
    if array.is_null() {
        return Err(AosError::Mlx("MLX array pointer is null".to_string()));
    }

    let size = mlx_array_size(array);
    if size <= 0 {
        mlx_array_free(array);
        return Err(AosError::Mlx("MLX array has non-positive size".to_string()));
    }

    let data = mlx_array_data(array);
    if data.is_null() {
        mlx_array_free(array);
        return Err(AosError::Mlx("MLX array data pointer is null".to_string()));
    }

    let slice = std::slice::from_raw_parts(data, size as usize);
    let result = slice.to_vec();
    mlx_array_free(array);
    Ok(result)
}

fn module_names() -> &'static [&'static str] {
    &[
        "q_proj",
        "k_proj",
        "v_proj",
        "o_proj",
        "gate_proj",
        "up_proj",
        "down_proj",
    ]
}

#[cfg(test)]
mod tests;
