//! Model runtime for base model load/unload.
//!
//! When mlx-ffi-backend feature is enabled, actually loads models via MLX FFI.
//! Otherwise acts as a stub for environments where backend is not linked.

#[cfg(feature = "mlx-ffi-backend")]
use std::collections::HashMap;
use std::path::Path;
use tracing::warn;

#[cfg(feature = "mlx-ffi-backend")]
use adapteros_lora_mlx_ffi::MLXFFIModel;
#[cfg(feature = "mlx-ffi-backend")]
use tracing::info;

/// Key for tracking loaded models: (tenant_id, model_id)
#[cfg(feature = "mlx-ffi-backend")]
type ModelKey = (String, String);

pub struct ModelRuntime {
    #[cfg(feature = "mlx-ffi-backend")]
    /// Loaded models by (tenant_id, model_id)
    models: HashMap<ModelKey, MLXFFIModel>,
}

impl Default for ModelRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRuntime {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "mlx-ffi-backend")]
            models: HashMap::new(),
        }
    }

    pub fn load_model(
        &mut self,
        tenant_id: &str,
        model_id: &str,
        model_path: &str,
    ) -> Result<(), String> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (tenant_id.to_string(), model_id.to_string());
            // Check if model is already loaded
            if self.models.contains_key(&key) {
                info!(
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    "Model already loaded, skipping"
                );
                return Ok(());
            }

            // Verify model path exists
            if !Path::new(model_path).exists() {
                return Err(format!("Model path does not exist: {}", model_path));
            }

            // Load model via MLX FFI
            match MLXFFIModel::load(model_path) {
                Ok(model) => {
                    info!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        path = %model_path,
                        "Model loaded successfully"
                    );
                    self.models.insert(key, model);
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to load MLX model from {}: {}", model_path, e);
                    warn!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        error = %e,
                        "Model load failed"
                    );
                    Err(error_msg)
                }
            }
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            warn!(
                tenant_id = %tenant_id,
                model_id = %model_id,
                "Model runtime stub: mlx-ffi-backend feature not enabled"
            );
            // Stub mode: validate path exists but don't actually load
            if !Path::new(model_path).exists() {
                return Err(format!("Model path does not exist: {}", model_path));
            }
            Ok(())
        }
    }

    pub fn unload_model(&mut self, tenant_id: &str, model_id: &str) -> Result<(), String> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (tenant_id.to_string(), model_id.to_string());
            if self.models.remove(&key).is_some() {
                info!(
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    "Model unloaded successfully"
                );
                Ok(())
            } else {
                warn!(
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    "Model not found for unload"
                );
                Ok(()) // Not an error if already unloaded
            }
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            warn!(
                tenant_id = %tenant_id,
                model_id = %model_id,
                "Model runtime stub: unload no-op"
            );
            Ok(())
        }
    }

    pub fn snapshot_all_models(
        &self,
        _tenant_id: &str,
    ) -> (Vec<crate::types::BaseModelStatusResponse>, i32, i32) {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let tenant_models: Vec<_> = self
                .models
                .keys()
                .filter(|(tid, _)| tid == _tenant_id)
                .map(|(_, mid)| mid.clone())
                .collect();

            let responses: Vec<_> = tenant_models
                .into_iter()
                .map(|model_id| crate::types::BaseModelStatusResponse {
                    model_id,
                    status: "loaded".to_string(),
                    loaded_at: None,
                    memory_usage_mb: None,
                    is_loaded: true,
                })
                .collect();

            let total_memory = 0; // TODO: Calculate actual memory usage
            let active_count = responses.len() as i32;

            (responses, total_memory, active_count)
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            (
                vec![], // no models loaded in stub
                0,      // total memory
                0,      // active count
            )
        }
    }
}
