//! Tiny-BERT ANE-pinned embedder for reasoning router
//!
//! This module provides a high-performance transformer-based embedder
//! that runs on the Apple Neural Engine (ANE) via CoreML.

use adapteros_core::io_utils::get_directory_size;
use adapteros_core::{AosError, Result};
#[cfg(feature = "coreml-backend")]
use adapteros_lora_kernel_coreml::ffi;
use serde::Deserialize;
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

/// Embedder using Tiny-BERT model pinned to ANE
pub struct TinyBertEmbedder {
    #[cfg(feature = "coreml-backend")]
    model_handle: *mut std::ffi::c_void,
    #[cfg(feature = "coreml-backend")]
    model_id: String,
    tokenizer: Tokenizer,
    dim: usize,
}

// SAFETY: `model_handle` is a pointer to a CoreML model that is thread-safe for inference operations:
// - CoreML's MLModel is documented as thread-safe for prediction calls
// - We never mutate the model after construction (load is single-threaded)
// - The tokenizer (HuggingFace Tokenizers) is explicitly Send+Sync
// - All inference via FFI is read-only (no mutable aliasing)
unsafe impl Send for TinyBertEmbedder {}
unsafe impl Sync for TinyBertEmbedder {}

impl TinyBertEmbedder {
    /// Load Tiny-BERT model from .mlpackage.
    /// If dim is None, it attempts to read 'hidden_size' from config.json.
    pub fn load<P: AsRef<Path>>(model_path: P, dim: Option<usize>) -> Result<Self> {
        let model_path = model_path.as_ref();

        let dim = match dim {
            Some(d) => d,
            None => Self::detect_dimension(model_path)?,
        };

        // Load tokenizer (expected in the same directory)
        let tokenizer_path = model_path.join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(AosError::Validation(format!(
                "Tiny-BERT tokenizer not found at {:?}",
                tokenizer_path
            )));
        }

        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| {
            AosError::Validation(format!("Failed to load Tiny-BERT tokenizer: {}", e))
        })?;

        #[cfg(feature = "coreml-backend")]
        {
            let model_path_str = model_path
                .to_str()
                .ok_or_else(|| AosError::Validation("Invalid model path".into()))?;

            let c_path = std::ffi::CString::new(model_path_str)
                .map_err(|e| AosError::Validation(format!("Invalid path encoding: {}", e)))?;

            // Pin to ANE (Neural Engine)
            let compute_units = ffi::ComputeUnitPreference::CpuAndNeuralEngine as i32;

            let handle = unsafe {
                ffi::coreml_load_model(c_path.as_ptr(), model_path_str.len(), compute_units)
            };

            if handle.is_null() {
                return Err(AosError::Kernel(format!(
                    "Failed to load Tiny-BERT model on ANE from {:?}",
                    model_path
                )));
            }

            // Track model memory for ANE metrics
            let model_id = format!("tinybert:{}", model_path.display());

            // Use exact on-disk size for memory tracking
            // Note: Actual ANE memory usage is often compressed/optimized, but
            // on-disk size is a reliable, conservative proxy for resource budgeting.
            let model_size = get_directory_size(model_path).unwrap_or(10 * 1024 * 1024);

            ffi::record_model_load(&model_id, model_size);

            info!(
                path = ?model_path,
                dim = dim,
                "Tiny-BERT ANE embedder loaded successfully"
            );

            Ok(Self {
                model_handle: handle,
                model_id,
                tokenizer,
                dim,
            })
        }

        #[cfg(not(feature = "coreml-backend"))]
        {
            let _ = dim; // Unused
            let _ = tokenizer; // Unused
            Err(AosError::Kernel(
                "CoreML backend feature not enabled".into(),
            ))
        }
    }

    /// Compute embedding for text
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if text.trim().is_empty() {
            return vec![0.0; self.dim];
        }

        #[cfg(feature = "coreml-backend")]
        {
            // Tokenize
            let encoding = match self.tokenizer.encode(text, true) {
                Ok(e) => e,
                Err(e) => {
                    debug!("Tiny-BERT tokenization failed: {}", e);
                    return vec![0.0; self.dim];
                }
            };

            let token_ids = encoding.get_ids();
            if token_ids.is_empty() {
                return vec![0.0; self.dim];
            }

            let mut embedding = vec![0.0f32; self.dim];

            // Expected output name for the pooled embedding
            // For Tiny-BERT, this is usually 'pooled_output' or 'last_hidden_state' (at CLS)
            let output_name = b"pooled_output";

            let result = unsafe {
                ffi::coreml_run_inference_named_output(
                    self.model_handle,
                    token_ids.as_ptr(),
                    token_ids.len(),
                    embedding.as_mut_ptr(),
                    self.dim,
                    output_name.as_ptr() as *const i8,
                    output_name.len(),
                )
            };

            if result < 0 {
                debug!("Tiny-BERT ANE inference failed with code {}", result);
                return vec![0.0; self.dim];
            }

            embedding
        }

        #[cfg(not(feature = "coreml-backend"))]
        {
            // Fallback to zero vector if backend not enabled
            vec![0.0; self.dim]
        }
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        self.dim
    }

    fn detect_dimension(model_path: &Path) -> Result<usize> {
        let config_path = model_path.join("config.json");
        if !config_path.exists() {
            return Err(AosError::Validation(format!(
                "config.json not found at {:?}. Please specify dimension explicitly.",
                config_path
            )));
        }

        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| AosError::Validation(format!("Failed to read config.json: {}", e)))?;

        #[derive(Deserialize)]
        struct BertConfig {
            hidden_size: Option<usize>,
            #[serde(alias = "d_model")]
            d_model: Option<usize>,
        }

        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| AosError::Validation(format!("Failed to parse config.json: {}", e)))?;

        config.hidden_size.or(config.d_model).ok_or_else(|| {
            AosError::Validation("Could not find 'hidden_size' or 'd_model' in config.json".into())
        })
    }
}

#[cfg(feature = "coreml-backend")]
impl Drop for TinyBertEmbedder {
    fn drop(&mut self) {
        if !self.model_handle.is_null() {
            // Untrack model memory using stored model_id
            ffi::record_model_unload(&self.model_id);

            unsafe {
                ffi::coreml_unload_model(self.model_handle);
            }
        }
    }
}
