//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, Result};
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::ptr;
use std::time::Instant;
use tokenizers::Tokenizer;

// Include the generated bindings
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod backend;
pub mod lora;
pub mod routing;
pub mod tensor;
mod util;

#[cfg(test)]
pub mod mock;

pub use backend::MLXFFIBackend;
pub use lora::{LoRAAdapter, LoRAConfig};
pub use routing::apply_multi_lora;
pub use tensor::MLXFFITensor;
pub use util::HIDDEN_STATE_MODULES;

use util::{
    create_token_array, detect_eos_token, extract_array, last_mlx_error, normalize_logits,
    sanitize_logits, select_next_token,
};

/// MLX model wrapper for inference using FFI
pub struct MLXFFIModel {
    /// C++ MLX model object
    model: *mut mlx_model_t,
    /// Model configuration
    pub config: ModelConfig,
    /// Tokenizer loaded from tokenizer.json
    tokenizer: Tokenizer,
    /// Model identifier for telemetry reporting
    model_id: String,
    /// End-of-sequence token id
    eos_token_id: u32,
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

        // Load tokenizer
        let tokenizer_path = model_path.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(AosError::NotFound(format!(
                "Tokenizer not found at {}",
                tokenizer_path.display()
            )));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to load tokenizer from {}: {}",
                tokenizer_path.display(),
                e
            ))
        })?;
        let eos_token_id = detect_eos_token(&tokenizer)?;

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
            return Err(last_mlx_error("Failed to load MLX model"));
        }

        tracing::info!("MLX model loaded via FFI: {}", path_str);

        let model_id = model_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.to_string());

        Ok(Self {
            model,
            config,
            tokenizer,
            model_id,
            eos_token_id,
        })
    }

    fn ensure_sequence_length(&self, len: usize) -> Result<()> {
        if len == 0 {
            return Err(AosError::Validation(
                "Token sequence may not be empty".to_string(),
            ));
        }
        if len > self.config.max_position_embeddings {
            return Err(AosError::Validation(format!(
                "Token sequence length {} exceeds model limit {}",
                len, self.config.max_position_embeddings
            )));
        }
        Ok(())
    }

    fn emit_generation_telemetry(
        &self,
        input_tokens: usize,
        output_tokens: usize,
        duration_us: u64,
        success: bool,
        error: Option<&str>,
    ) {
        let level = if success {
            LogLevel::Info
        } else {
            LogLevel::Error
        };
        let metadata = json!({
            "model_id": self.model_id,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "duration_us": duration_us,
            "success": success,
            "error": error,
        });

        let event = TelemetryEventBuilder::new(
            EventType::Custom("mlx.generate".to_string()),
            level,
            "MLX FFI generation".to_string(),
        )
        .component("adapteros-lora-mlx-ffi".to_string())
        .metadata(metadata)
        .build();

        match serde_json::to_string(&event) {
            Ok(payload) => tracing::info!(target = "telemetry", "{}", payload),
            Err(err) => tracing::warn!("Failed to serialize telemetry event: {}", err),
        }
    }

    pub fn forward(&self, token_ids: &[u32], _position: usize) -> Result<Vec<f32>> {
        self.ensure_sequence_length(token_ids.len())?;

        unsafe {
            mlx_clear_error();
        }

        let input_array = create_token_array(token_ids)?;
        let output_array = unsafe { mlx_model_forward(self.model, input_array) };
        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(last_mlx_error("Failed to run model forward"));
        }

        let mut logits = normalize_logits(extract_array(output_array)?, self.config.vocab_size);
        sanitize_logits(&mut logits);

        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

        tracing::debug!(
            "MLX FFI forward pass complete: {} tokens -> {} logits",
            token_ids.len(),
            logits.len()
        );

        Ok(logits)
    }

    pub fn generate(&self, prompt: &str, max_tokens: usize) -> Result<String> {
        if prompt.trim().is_empty() {
            return Err(AosError::Validation("Prompt must not be empty".to_string()));
        }
        if max_tokens == 0 {
            return Err(AosError::Validation(
                "max_tokens must be greater than zero".to_string(),
            ));
        }

        let start = Instant::now();
        let encoding = self
            .tokenizer
            .encode(prompt, false)
            .map_err(|e| AosError::Mlx(format!("Failed to tokenize prompt: {}", e)))?;
        let mut tokens: Vec<u32> = encoding.get_ids().iter().copied().collect();
        if tokens.is_empty() {
            return Err(AosError::Validation(
                "Tokenizer produced no tokens for the provided prompt".to_string(),
            ));
        }

        self.ensure_sequence_length(tokens.len())?;
        let input_len = tokens.len();
        let mut generated_tokens: Vec<u32> = Vec::new();

        let generation_result: Result<()> = (|| {
            for _ in 0..max_tokens {
                if tokens.len() >= self.config.max_position_embeddings {
                    return Err(AosError::ResourceExhaustion(format!(
                        "Maximum position embeddings ({}) exceeded",
                        self.config.max_position_embeddings
                    )));
                }

                let logits = self.forward(&tokens, tokens.len().saturating_sub(1))?;
                let next_token = select_next_token(&logits)?;

                tokens.push(next_token);
                if next_token == self.eos_token_id {
                    break;
                }
                generated_tokens.push(next_token);
            }
            Ok(())
        })();

        let duration_us = start.elapsed().as_micros() as u64;

        match generation_result {
            Ok(()) => {
                let text = if generated_tokens.is_empty() {
                    String::new()
                } else {
                    self.tokenizer
                        .decode(&generated_tokens, true)
                        .map_err(|e| {
                            AosError::Mlx(format!("Failed to decode generated tokens: {}", e))
                        })?
                };
                self.emit_generation_telemetry(
                    input_len,
                    generated_tokens.len(),
                    duration_us,
                    true,
                    None,
                );
                tracing::debug!(
                    "MLX FFI generation complete: prompt_tokens={}, generated_tokens={}, duration_us={}",
                    input_len,
                    generated_tokens.len(),
                    duration_us
                );
                Ok(text)
            }
            Err(err) => {
                let err_string = err.to_string();
                self.emit_generation_telemetry(
                    input_len,
                    generated_tokens.len(),
                    duration_us,
                    false,
                    Some(err_string.as_str()),
                );
                Err(err)
            }
        }
    }

    pub fn forward_with_hidden_states(
        &self,
        token_ids: &[u32],
    ) -> Result<(Vec<f32>, HashMap<String, Vec<f32>>)> {
        self.ensure_sequence_length(token_ids.len())?;

        unsafe {
            mlx_clear_error();
        }

        let input_array = create_token_array(token_ids)?;
        let mut hidden_ptr: *mut mlx_array_t = ptr::null_mut();
        let mut hidden_count: i32 = 0;

        let output_array = unsafe {
            mlx_model_forward_with_hidden_states(
                self.model,
                input_array,
                &mut hidden_ptr,
                &mut hidden_count,
            )
        };

        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(last_mlx_error("Failed to run forward with hidden states"));
        }

        let mut logits = normalize_logits(extract_array(output_array)?, self.config.vocab_size);
        sanitize_logits(&mut logits);

        let mut hidden_states: HashMap<String, Vec<f32>> = HashMap::new();
        if hidden_count > 0 && !hidden_ptr.is_null() {
            let pointer_slice = unsafe {
                std::slice::from_raw_parts(
                    hidden_ptr as *mut *mut mlx_array_t,
                    hidden_count as usize,
                )
            };
            for (idx, &array_ptr) in pointer_slice.iter().enumerate() {
                if array_ptr.is_null() {
                    continue;
                }

                match extract_array(array_ptr) {
                    Ok(mut values) => {
                        if values.len() < self.config.hidden_size {
                            values.resize(self.config.hidden_size, 0.0);
                        } else if values.len() > self.config.hidden_size {
                            values.truncate(self.config.hidden_size);
                        }
                        let name = HIDDEN_STATE_MODULES
                            .get(idx)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("hidden_{}", idx));
                        hidden_states.insert(name, values);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to extract hidden state {}: {}", idx, err);
                    }
                }

                unsafe { mlx_array_free(array_ptr) };
            }
        }

        unsafe {
            mlx_array_free(input_array);
            mlx_array_free(output_array);
        }

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
mod tests;
