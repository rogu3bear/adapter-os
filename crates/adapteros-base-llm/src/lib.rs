//! Base LLM integration for adapterOS
//!
//! Implements Layer 1 of the five-tier adapter hierarchy.
//! Provides foundation model (Qwen2.5-7B-Instruct) integration with
//! deterministic execution guarantees.

#![allow(unused_imports)]
#![allow(dead_code)]

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

pub use error::{BaseLLMError, Result as BaseLLMResult};
pub use metadata::{BaseLLMMetadata, ModelArchitecture};
pub use qwen::QwenBaseLLM;

/// Base LLM trait for foundation models
///
/// All base LLMs must implement this trait to ensure deterministic behavior
/// and integration with the adapterOS runtime.
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
