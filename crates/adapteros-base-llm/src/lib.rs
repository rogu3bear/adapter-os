//! Base LLM integration for AdapterOS
//!
//! Implements Layer 1 of the five-tier adapter hierarchy.
//! Provides foundation model (Qwen2.5-7B-Instruct) integration with
//! deterministic execution guarantees.

use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::DeterministicExecutor;
use adapteros_trace::Event;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

pub mod error;
pub mod metadata;
pub mod qwen;
#[cfg(feature = "mlx-ffi")]
pub mod mlx_ffi;

pub use error::{BaseLLMError, Result as BaseLLMResult};
pub use metadata::{BaseLLMMetadata, ModelArchitecture};
pub use qwen::QwenBaseLLM;
#[cfg(feature = "mlx-ffi")]
pub use mlx_ffi::QwenMlxFfi;

/// Base LLM trait for foundation models
///
/// All base LLMs must implement this trait to ensure deterministic behavior
/// and integration with the AdapterOS runtime.
pub trait BaseLLM: Send + Sync {
    /// Load model with deterministic initialization
    fn load(&mut self, executor: &mut DeterministicExecutor) -> Result<()>;

    /// Forward pass through base model
    fn forward(&mut self, input_ids: &[u32]) -> Result<Vec<f32>>;

    /// Get model metadata
    fn metadata(&self) -> &BaseLLMMetadata;

    /// Get model state for checkpointing
    fn get_state(&self) -> Result<ModelState>;

    /// Restore model state from checkpoint
    fn restore_state(&mut self, state: &ModelState) -> Result<()>;

    /// Reset model to initial state
    fn reset(&mut self) -> Result<()>;

    /// Generate trace event for this operation
    fn create_trace_event(&self, operation: &str, input_hash: &str) -> Event;
}

/// Model state for checkpointing and restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelState {
    pub model_id: String,
    pub checkpoint_hash: String,
    pub timestamp: u128,
    pub state_data: Vec<u8>,
}

/// Base LLM manager for handling multiple models
pub struct BaseLLMManager {
    models: Arc<RwLock<std::collections::HashMap<String, Box<dyn BaseLLM>>>>,
    active_model: Arc<RwLock<Option<String>>>,
}

impl BaseLLMManager {
    /// Create new base LLM manager
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(std::collections::HashMap::new())),
            active_model: Arc::new(RwLock::new(None)),
        }
    }

    /// Register a base LLM
    pub fn register_model(&self, model_id: String, model: Box<dyn BaseLLM>) -> Result<()> {
        let mut models = self.models.write();
        models.insert(model_id.clone(), model);

        info!("Registered base LLM: {}", model_id);
        Ok(())
    }

    /// Set active model
    pub fn set_active_model(&self, model_id: &str) -> Result<()> {
        let models = self.models.read();
        if !models.contains_key(model_id) {
            return Err(AosError::BaseLLM(format!("Model not found: {}", model_id)));
        }

        let mut active = self.active_model.write();
        *active = Some(model_id.to_string());

        info!("Set active base LLM: {}", model_id);
        Ok(())
    }

    /// Get active model
    pub fn get_active_model(&self) -> Result<Option<Box<dyn BaseLLM>>> {
        let active = self.active_model.read();
        if let Some(model_id) = active.as_ref() {
            let models = self.models.read();
            if models.contains_key(model_id) {
                // Note: This is a limitation of the trait object approach
                // In practice, you'd need to clone or use Arc<dyn BaseLLM>
                warn!("Cannot return trait object directly - use get_model_by_id instead");
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Get model by ID
    /// Note: This returns a reference that's only valid while the lock is held
    /// In practice, you'd need to restructure this to avoid lifetime issues
    pub fn get_model_by_id(&self, model_id: &str) -> bool {
        let models = self.models.read();
        models.contains_key(model_id)
    }

    /// List all registered models
    pub fn list_models(&self) -> Vec<String> {
        let models = self.models.read();
        models.keys().cloned().collect()
    }

    /// Get active model ID
    pub fn get_active_model_id(&self) -> Option<String> {
        let active = self.active_model.read();
        active.clone()
    }
}

impl Default for BaseLLMManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Factory for creating base LLM instances
pub struct BaseLLMFactory;

impl BaseLLMFactory {
    /// Create Qwen base LLM
    pub fn create_qwen(metadata: BaseLLMMetadata) -> Result<QwenBaseLLM> {
        QwenBaseLLM::new(metadata)
    }

    /// Create base LLM from configuration
    pub fn from_config(config: BaseLLMConfig) -> Result<Box<dyn BaseLLM>> {
        match config.model_type {
            ModelType::Qwen => {
                #[cfg(feature = "mlx-ffi")]
                {
                    // Prefer MLX FFI if feature enabled and model_path provided via env or config
                    if std::env::var("AOS_MLX_FFI_MODEL").is_ok() || config.model_path.is_some() {
                        let mut m = QwenMlxFfi::new(config.metadata.clone());
                        // If config had an explicit path, set env to ensure MLXFFIModel::load uses it
                        if let Some(path) = &config.model_path {
                            std::env::set_var("AOS_MLX_FFI_MODEL", path);
                        }
                        return Ok(Box::new(m));
                    }
                }
                let qwen = Self::create_qwen(config.metadata)?;
                Ok(Box::new(qwen))
            }
        }
    }
}

/// Base LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseLLMConfig {
    pub model_type: ModelType,
    pub metadata: BaseLLMMetadata,
    pub model_path: Option<String>,
}

/// Supported model types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelType {
    Qwen,
}

#[cfg(test)]
mod tests {
    use super::*;
    // use adapteros_deterministic_exec::ExecutorConfig; // Not used in current tests

    #[test]
    fn test_base_llm_manager_creation() {
        let manager = BaseLLMManager::new();
        assert!(manager.list_models().is_empty());
        assert!(manager.get_active_model_id().is_none());
    }

    #[test]
    fn test_base_llm_manager_registration() {
        let manager = BaseLLMManager::new();

        // Create a mock base LLM for testing
        let metadata = BaseLLMMetadata {
            model_id: "test-model".to_string(),
            model_hash: "test-hash".to_string(),
            arch: ModelArchitecture::Qwen2,
            vocab_size: 1000,
            hidden_dim: 512,
            n_layers: 4,
            n_heads: 8,
        };

        let qwen = QwenBaseLLM::new(metadata).unwrap();
        manager
            .register_model("test-model".to_string(), Box::new(qwen))
            .unwrap();

        assert_eq!(manager.list_models(), vec!["test-model"]);
    }

    #[test]
    fn test_base_llm_factory() {
        let metadata = BaseLLMMetadata {
            model_id: "test-qwen".to_string(),
            model_hash: "test-hash".to_string(),
            arch: ModelArchitecture::Qwen2,
            vocab_size: 1000,
            hidden_dim: 512,
            n_layers: 4,
            n_heads: 8,
        };

        let config = BaseLLMConfig {
            model_type: ModelType::Qwen,
            metadata,
            model_path: None,
        };

        let model = BaseLLMFactory::from_config(config).unwrap();
        assert_eq!(model.metadata().model_id, "test-qwen");
    }
}

#[cfg(feature = "mlx")]
mod mlx_backend {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::{IntoPyDict, PyList, PyModule, PyTuple};

    // Add Python version check
    #[cfg(not(pyo3_python_version = "3.13"))]
    compile_error!("MLX backend requires Python 3.13 or earlier; current version is incompatible with PyO3");

    /// Load a Qwen model via Python's mlx_lm.load API. Returns (model, tokenizer, generate_fn).
    pub fn load_qwen_via_mlx(model_ref: &str, seed64: u64) -> PyResult<(PyObject, PyObject, PyObject)> {
        Python::with_gil(|py| {
            // Set deterministic seed on MLX
            if let Ok(mx) = PyModule::import_bound(py, "mlx.core") {
                let _ = mx.getattr("random").and_then(|r| r.call_method1("seed", (seed64,)));
            }

            // Import mlx_lm and load model + tokenizer
            let mlx_lm = PyModule::import_bound(py, "mlx_lm")?;

            // load(model_ref) -> (model, tokenizer)
            let load_obj = mlx_lm.getattr("load")?;
            let loaded = load_obj.call1((model_ref,))?;
            let (model, tokenizer): (PyObject, PyObject) = if let Ok(tup) = loaded.downcast::<PyTuple>() {
                // Extract as PyObject tuple
                let m = tup.get_item(0)?.unbind().into_py(py);
                let tok = tup.get_item(1)?.unbind().into_py(py);
                (m, tok)
            } else {
                loaded.extract()?
            };

            // Keep a handle to the generate function
            let generate = mlx_lm.getattr("generate")?.unbind().into_py(py);

            Ok((model, tokenizer, generate))
        })
    }

    /// Text generation using mlx_lm.generate(model, tokenizer, prompt, max_tokens) -> str
    pub fn generate_text(
        model: &PyObject,
        tokenizer: &PyObject,
        generate_fn: &PyObject,
        prompt: &str,
        max_tokens: usize,
    ) -> PyResult<String> {
        Python::with_gil(|py| {
            let text_any = generate_fn.call1(py, (model, tokenizer, prompt, max_tokens))?;
            text_any.extract(py)
        })
    }

    /// Decode token IDs into a prompt string using tokenizer.decode(ids)
    pub fn decode_ids(tokenizer: &PyObject, ids: &[u32]) -> PyResult<String> {
        Python::with_gil(|py| {
            let py_ids = PyList::new_bound(py, ids);
            let s_any = tokenizer.call_method1(py, "decode", (py_ids,))?;
            s_any.extract(py)
        })
    }

    /// Encode text to token IDs using tokenizer.encode(text)
    pub fn encode_ids(tokenizer: &PyObject, text: &str) -> PyResult<Vec<u32>> {
        Python::with_gil(|py| {
            let enc_any = tokenizer.call_method1(py, "encode", (text,))?;
            // Try direct list extraction
            if let Ok(ids) = enc_any.extract::<Vec<u32>>(py) {
                return Ok(ids);
            }
            // Try attribute `ids`
            if let Ok(ids_obj) = enc_any.getattr(py, "ids") {
                return ids_obj.extract::<Vec<u32>>(py);
            }
            Err(pyo3::exceptions::PyException::new_err(
                "Unable to extract token ids from tokenizer.encode",
            ))
        })
    }
}
