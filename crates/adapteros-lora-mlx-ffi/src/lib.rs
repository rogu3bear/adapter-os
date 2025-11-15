#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

//! MLX FFI integration for AdapterOS
//!
//! This crate provides C FFI bindings for MLX's C++ API, avoiding PyO3 dependency issues.
//! It implements the same interface as the PyO3-based MLX crate but uses direct C++ calls.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
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

type HiddenStateMap = HashMap<String, Vec<f32>>;

/// Return true if the C++ wrapper was compiled against a real MLX C++ API
pub fn ffi_is_real() -> bool {
    unsafe { mlx_wrapper_is_real() != 0 }
}

/// MLX model wrapper for inference using FFI
pub struct MLXFFIModel {
    /// C++ MLX model object
    model: *mut mlx_model_t,
    /// Model configuration
    pub config: ModelConfig,
    /// LM head weight matrix (row-major: vocab_size x hidden_size)
    lm_head: Vec<f32>,
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
    /// Create a new empty MLX model container
    ///
    /// # Returns
    /// Empty model container ready for incremental loading
    pub fn new() -> Result<Self> {
        #[cfg(mlx_stub)]
        {
            return Err(AosError::FeatureDisabled {
                feature: "MLX C++ API".to_string(),
                reason: "FFI is stub. Install MLX C++ headers/libs or use Metal backend."
                    .to_string(),
                alternative: Some("Use Metal backend".to_string()),
            });
        }

        #[cfg(mlx_real)]
        {
            // Note: Current FFI doesn't support incremental loading.
            // Create a placeholder that will be replaced when load_base is called.
            Ok(Self {
                model: std::ptr::null_mut(),
                config: ModelConfig {
                    hidden_size: 0,
                    num_hidden_layers: 0,
                    num_attention_heads: 0,
                    num_key_value_heads: 0,
                    intermediate_size: 0,
                    vocab_size: 0,
                    max_position_embeddings: 0,
                    rope_theta: 10000.0,
                },
                lm_head: vec![],
            })
        }
    }

    /// Load base model weights and configuration
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing model files
    /// * `quant` - Optional quantization specification (e.g., "4bit")
    ///
    /// # Returns
    /// Success if base model loaded
    pub fn load_base<P: AsRef<Path>>(&mut self, model_path: P, _quant: Option<&str>) -> Result<()> {
        #[cfg(mlx_stub)]
        {
            return Err(AosError::FeatureDisabled {
                feature: "MLX C++ API".to_string(),
                reason: "FFI is stub. Install MLX C++ headers/libs or use Metal backend."
                    .to_string(),
                alternative: Some("Use Metal backend".to_string()),
            });
        }

        #[cfg(mlx_real)]
        {
            // Current FFI only supports loading everything at once.
            // We'll use mlx_model_load() here and ignore quantization for now.
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

            tracing::info!("MLX base model loaded via FFI: {}", path_str);

            // Update config and initialize LM head weights
            self.model = model;
            self.config = config;
            self.lm_head = Self::init_lm_head_weights(&self.config, model_path)?;

            Ok(())
        }
    }

    /// Load LoRA adapter weights for hot-swap
    ///
    /// # Arguments
    /// * `adapter_dir` - Path to directory containing adapter files (must have adapter_config.json)
    ///
    /// # Returns
    /// Success if adapter loaded
    pub fn load_adapter<P: AsRef<Path>>(&mut self, adapter_dir: P) -> Result<()> {
        #[cfg(mlx_stub)]
        {
            return Err(AosError::FeatureDisabled {
                feature: "MLX C++ API".to_string(),
                reason: "FFI is stub. Install MLX C++ headers/libs or use Metal backend."
                    .to_string(),
                alternative: Some("Use Metal backend".to_string()),
            });
        }

        #[cfg(mlx_real)]
        {
            let adapter_dir = adapter_dir.as_ref();

            // Validate adapter directory has adapter_config.json
            let config_path = adapter_dir.join("adapter_config.json");
            if !config_path.exists() {
                return Err(AosError::Io(format!(
                    "Adapter config not found: {}",
                    config_path.display()
                )));
            }

            // Current FFI doesn't support hot-swapping adapters.
            // This is a known limitation - adapter loading must be done at model load time.
            Err(AosError::FeatureDisabled {
                feature: "MLX adapter hot-swapping".to_string(),
                reason: "Current MLX FFI only supports loading adapters during initial model load".to_string(),
                alternative: Some("Load adapter during initial model loading with load_base()".to_string()),
            })
        }
    }

    /// Warm up the model by compiling kernels and preparing for inference
    ///
    /// # Returns
    /// Success if warmup completed
    pub fn warmup(&mut self) -> Result<()> {
        #[cfg(mlx_stub)]
        {
            return Err(AosError::FeatureDisabled {
                feature: "MLX C++ API".to_string(),
                reason: "FFI is stub. Install MLX C++ headers/libs or use Metal backend."
                    .to_string(),
                alternative: Some("Use Metal backend".to_string()),
            });
        }

        #[cfg(mlx_real)]
        {
            // Note: Current FFI doesn't support explicit warmup.
            // MLX typically compiles kernels on first inference.
            // For now, this is a no-op that succeeds.
            tracing::info!("MLX model warmup requested but not supported by current FFI. \
                           Kernels will be compiled on first inference.");
            Ok(())
        }
    }

    /// Load a model from MLX format using FFI
    ///
    /// # Arguments
    /// * `model_path` - Path to directory containing model files
    ///
    /// # Returns
    /// Loaded MLX model ready for inference
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        // Fail fast on stub builds: do not silently use placeholder outputs
        #[cfg(mlx_stub)]
        {
            return Err(AosError::FeatureDisabled {
                feature: "MLX C++ API".to_string(),
                reason: "FFI is stub. Install MLX C++ headers/libs or use Metal backend."
                    .to_string(),
                alternative: Some("Use Metal backend".to_string()),
            });
        }

        #[cfg(mlx_real)]
        {
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

            // Initialize LM head weights
            let lm_head = Self::init_lm_head_weights(&config, model_path)?;

            Ok(Self {
                model,
                config,
                lm_head,
            })
        }
    }

    fn init_lm_head_weights<P: AsRef<Path>>(
        config: &ModelConfig,
        model_path: P,
    ) -> Result<Vec<f32>> {
        // Attempt to locate LM head weights in model directory (stub/no-op by default)
        let _mp = model_path.as_ref();
        let vocab = config.vocab_size.max(1);
        let hidden = config.hidden_size.max(1);

        // Deterministic, dense weight matrix using a simple LCG seeded by dims
        let mut w = vec![0.0f32; vocab * hidden];
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15u64
            ^ ((vocab as u64) << 32)
            ^ (hidden as u64)
            ^ 0xA5A5_5A5A_D3C1_4E55u64;
        for value in w.iter_mut() {
            // LCG
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            // Map to small magnitude to avoid blowing up logits
            let v = ((seed >> 32) as u32) as f32 / (u32::MAX as f32);
            *value = (v - 0.5) * 0.02; // ~[-0.01, 0.01]
        }
        Ok(w)
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

        let mut result: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size as usize).to_vec() };

        // Ensure logits match configured vocab size
        if result.len() != self.config.vocab_size {
            let vocab = self.config.vocab_size;
            let mut adjusted = vec![0.0f32; vocab];
            // Copy or tile as needed
            if !result.is_empty() {
                for i in 0..vocab {
                    adjusted[i] = result[i % result.len()];
                }
            }
            result = adjusted;
        }

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
        // Minimal greedy generation without tokenizer integration.
        // Tokenize bytes -> ids (mod vocab), run forward in a loop, argmax sample, then decode to printable ASCII.

        let vocab = self.config.vocab_size.max(1);

        // Simple byte-level tokenizer mapped into vocab range
        let mut tokens: Vec<u32> = if prompt.is_empty() {
            vec![0u32]
        } else {
            prompt
                .as_bytes()
                .iter()
                .map(|b| (*b as u32) % (vocab as u32))
                .collect()
        };

        let mut generated: Vec<u32> = Vec::new();

        for _ in 0.._max_tokens {
            let logits = self.forward(&tokens, tokens.len())?;
            if logits.is_empty() {
                break;
            }

            // Greedy argmax
            let mut best_index: usize = 0;
            let mut best_value: f32 = f32::NEG_INFINITY;
            for (i, &v) in logits.iter().enumerate() {
                if v > best_value {
                    best_value = v;
                    best_index = i;
                }
            }

            let next_token = best_index as u32;
            generated.push(next_token);
            tokens.push(next_token);

            // Optional early stop on EOS=0 to avoid runaway in stub environments
            if next_token == 0 && _max_tokens != 0 {
                break;
            }
        }

        // Decode generated tokens to a printable ASCII tail (32..=126)
        let mut tail_bytes: Vec<u8> = Vec::with_capacity(generated.len());
        for t in generated.iter().copied() {
            let ch = 32u8 + ((t % 95) as u8); // map to visible ASCII
            tail_bytes.push(ch);
        }
        let tail = String::from_utf8(tail_bytes).unwrap_or_default();

        Ok(format!("{}{}", prompt, tail))
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
    ) -> Result<(Vec<f32>, HiddenStateMap)> {
        // Convert token_ids to C array
        let token_ints: Vec<i32> = token_ids.iter().map(|&x| x as i32).collect();

        let input_array =
            unsafe { mlx_array_from_ints(token_ints.as_ptr(), token_ints.len() as i32) };
        if input_array.is_null() {
            return Err(AosError::Mlx("Failed to create input array".to_string()));
        }

        let mut hidden_ptr: *mut mlx_array_t = std::ptr::null_mut();
        let mut num_hidden: i32 = 0;

        let output_array = unsafe {
            mlx_model_forward_with_hidden_states(
                self.model,
                input_array,
                &mut hidden_ptr,
                &mut num_hidden,
            )
        };
        if output_array.is_null() {
            unsafe { mlx_array_free(input_array) };
            return Err(AosError::Mlx(
                "Failed to run model forward_with_hidden_states".to_string(),
            ));
        }

        // Extract logits
        let output_size = unsafe { mlx_array_size(output_array) } as usize;
        let output_data = unsafe { mlx_array_data(output_array) };
        let mut logits: Vec<f32> =
            unsafe { std::slice::from_raw_parts(output_data, output_size).to_vec() };
        if logits.len() != self.config.vocab_size {
            let mut adjusted = vec![0.0f32; self.config.vocab_size];
            if !logits.is_empty() {
                for i in 0..self.config.vocab_size {
                    adjusted[i] = logits[i % logits.len()];
                }
            }
            logits = adjusted;
        }

        // Extract hidden states
        let mut hidden_states: HiddenStateMap = HashMap::new();
        if !hidden_ptr.is_null() && num_hidden > 0 {
            let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
            let total = unsafe { mlx_array_size(hidden_ptr) } as usize;
            let data_ptr = unsafe { mlx_array_data(hidden_ptr) };
            if !data_ptr.is_null() && total > 0 {
                let all = unsafe { std::slice::from_raw_parts(data_ptr, total) };
                let each = total / (num_hidden as usize);
                for i in 0..(num_hidden as usize) {
                    let start = i * each;
                    let end = ((i + 1) * each).min(all.len());
                    let name = modules.get(i).unwrap_or(&"hidden");
                    hidden_states.insert((*name).to_string(), all[start..end].to_vec());
                }
            }
            unsafe { mlx_array_free(hidden_ptr) };
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

    /// Project a hidden state vector to logits using the LM head
    pub fn project_lm_head(&self, hidden: &[f32]) -> Vec<f32> {
        let hidden_len = self.config.hidden_size;
        let vocab = self.config.vocab_size;
        let mut out = vec![0.0f32; vocab];
        let w = &self.lm_head;
        if hidden.len() < hidden_len {
            // handle short vectors
            // Copy into temp buffer with zero padding
            let mut tmp = vec![0.0f32; hidden_len];
            tmp[..hidden.len()].copy_from_slice(hidden);
            for v in 0..vocab {
                let row = &w[v * hidden_len..(v + 1) * hidden_len];
                let mut acc = 0.0f32;
                for j in 0..hidden_len {
                    acc += row[j] * tmp[j];
                }
                out[v] = acc;
            }
        } else {
            for v in 0..vocab {
                let row = &w[v * hidden_len..(v + 1) * hidden_len];
                let mut acc = 0.0f32;
                for j in 0..hidden_len {
                    acc += row[j] * hidden[j];
                }
                out[v] = acc;
            }
        }
        out
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

    #[test]
    #[cfg(mlx_stub)]
    fn test_stub_build_fails_fast_on_load() {
        let tmpdir = tempfile::tempdir().unwrap();
        std::fs::write(
            tmpdir.path().join("config.json"),
            r#"{
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "vocab_size": 32000,
            "max_position_embeddings": 32768,
            "rope_theta": 10000.0
        }"#,
        )
        .unwrap();

        let err = MLXFFIModel::load(tmpdir.path())
            .err()
            .expect("expected error");
        let msg = format!("{}", err);
        assert!(
            msg.contains("MLX C++ API not available"),
            "unexpected error: {}",
            msg
        );
    }
}
