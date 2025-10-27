use std::collections::HashMap;

use adapteros_core::{AosError, Result};

#[cfg(feature = "mlx-ffi-backend")]
use adapteros_base_llm::{BaseLLM, BaseLLMConfig, BaseLLMFactory, BaseLLMMetadata, ModelType};
#[cfg(feature = "mlx-ffi-backend")]
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};

#[cfg_attr(not(feature = "mlx-ffi-backend"), allow(dead_code))]
pub struct ModelRuntime {
    // key: (tenant_id, model_id)
    #[cfg(feature = "mlx-ffi-backend")]
    models: HashMap<(String, String), Box<dyn BaseLLM>>, 
}

impl ModelRuntime {
    pub fn new() -> Self {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            return Self { models: HashMap::new() };
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Self {}
        }
    }

    /// Load a model for a tenant using MLX FFI backend.
    /// When compiled without mlx-ffi-backend, returns a feature-disabled error.
    pub fn load_model(&mut self, tenant_id: &str, model_id: &str, model_path: &str) -> Result<()> {
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            return Err(AosError::FeatureDisabled {
                feature: "mlx-ffi-backend".to_string(),
                reason: "server built without MLX FFI runtime".to_string(),
                alternative: Some("use Metal backend".to_string()),
            });
        }

        #[cfg(feature = "mlx-ffi-backend")]
        {
            // Build base LLM config
            let metadata = BaseLLMMetadata::default();
            let cfg = BaseLLMConfig {
                model_type: ModelType::Qwen,
                metadata,
                model_path: Some(model_path.to_string()),
            };

            // Create model via factory and load with deterministic executor
            let mut model = BaseLLMFactory::from_config(cfg)?;
            let mut exec = DeterministicExecutor::new(ExecutorConfig::default());
            model.load(&mut exec)?;

            self.models
                .insert((tenant_id.to_string(), model_id.to_string()), model);
            Ok(())
        }
    }

    /// Unload a model for a tenant.
    pub fn unload_model(&mut self, tenant_id: &str, model_id: &str) -> Result<()> {
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            return Err(AosError::FeatureDisabled {
                feature: "mlx-ffi-backend".to_string(),
                reason: "server built without MLX FFI runtime".to_string(),
                alternative: Some("use Metal backend".to_string()),
            });
        }

        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.models.remove(&(tenant_id.to_string(), model_id.to_string()));
            Ok(())
        }
    }

    /// Compute a multi-model status snapshot for a tenant.
    /// Returns: (models: Vec<BaseModelStatusResponse>, total_memory_mb, active_model_count)
    pub fn snapshot_all_models(
        &self,
        tenant_id: &str,
    ) -> (Vec<crate::types::BaseModelStatusResponse>, i32, i32) {
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            return (Vec::new(), 0, 0);
        }

        #[cfg(feature = "mlx-ffi-backend")]
        {
            use chrono::Utc;
            let mut models = Vec::new();
            let mut total_mem: i32 = 0;
            let now = Utc::now().to_rfc3339();

            for ((tenant, model_id), model) in &self.models {
                if tenant != tenant_id {
                    continue;
                }
                let meta = model.metadata();
                let status = crate::types::BaseModelStatusResponse {
                    model_id: model_id.clone(),
                    model_name: meta.model_id.clone(),
                    status: "loaded".to_string(),
                    loaded_at: Some(now.clone()),
                    unloaded_at: None,
                    error_message: None,
                    memory_usage_mb: None,
                    is_loaded: true,
                    updated_at: now.clone(),
                };
                models.push(status);
            }
            let active = models.iter().filter(|m| m.is_loaded).count() as i32;
            (models, total_mem, active)
        }
    }
}

