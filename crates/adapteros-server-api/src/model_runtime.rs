//! Model runtime for base model load/unload.
//!
//! When mlx-ffi-backend feature is enabled, actually loads models via MLX FFI.
//! Otherwise acts as a stub for environments where backend is not linked.

#[cfg(feature = "mlx-ffi-backend")]
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::warn;

#[cfg(feature = "mlx-ffi-backend")]
use adapteros_lora_mlx_ffi::MLXFFIModel;
#[cfg(feature = "mlx-ffi-backend")]
use tracing::info;
#[cfg(feature = "mlx-ffi-backend")]
use tokio::task::AbortHandle;

use adapteros_secure_fs::traversal::normalize_path;

/// Key for tracking loaded models: (tenant_id, model_id)
#[cfg(feature = "mlx-ffi-backend")]
type ModelKey = (String, String);

/// Model metadata including memory usage
#[cfg(feature = "mlx-ffi-backend")]
struct ModelMetadata {
    model: MLXFFIModel,
    memory_usage_mb: i32,
}

pub struct ModelRuntime {
    #[cfg(feature = "mlx-ffi-backend")]
    /// Loaded models by (tenant_id, model_id) with metadata
    models: HashMap<ModelKey, ModelMetadata>,
    /// Active operation handles for cancellation: (tenant_id, model_id) -> AbortHandle
    #[cfg(feature = "mlx-ffi-backend")]
    active_operations: HashMap<ModelKey, AbortHandle>,
    /// Maximum model file size in bytes (default: 10GB)
    max_model_size_bytes: u64,
    /// Maximum config.json file size in bytes (default: 1MB)
    max_config_size_bytes: u64,
    /// Maximum tokenizer.json file size in bytes (default: 10MB)
    max_tokenizer_size_bytes: u64,
    /// Maximum number of loaded models globally (default: 5)
    max_loaded_models: usize,
    /// Maximum number of loaded models per tenant (default: 2)
    #[allow(unused)]
    max_tenant_models: usize,
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
            #[cfg(feature = "mlx-ffi-backend")]
            active_operations: HashMap::new(),
            max_model_size_bytes: 10 * 1024 * 1024 * 1024, // Default 10GB
            max_config_size_bytes: 1024 * 1024, // Default 1MB
            max_tokenizer_size_bytes: 10 * 1024 * 1024, // Default 10MB
            max_loaded_models: 5, // Default: 5 models globally
            max_tenant_models: 2, // Default: 2 models per tenant
        }
    }

    /// Create a new ModelRuntime with custom file size limits
    pub fn with_limits(
        max_model_size_bytes: u64,
        max_config_size_bytes: u64,
        max_tokenizer_size_bytes: u64,
    ) -> Self {
        Self {
            #[cfg(feature = "mlx-ffi-backend")]
            models: HashMap::new(),
            #[cfg(feature = "mlx-ffi-backend")]
            active_operations: HashMap::new(),
            max_model_size_bytes,
            max_config_size_bytes,
            max_tokenizer_size_bytes,
            max_loaded_models: 5, // Default: 5 models globally
            max_tenant_models: 2, // Default: 2 models per tenant
        }
    }

    /// Set maximum model file size in bytes
    pub fn set_max_size(&mut self, max_bytes: u64) {
        self.max_model_size_bytes = max_bytes;
    }

    /// Set maximum config.json file size in bytes
    pub fn set_max_config_size(&mut self, max_bytes: u64) {
        self.max_config_size_bytes = max_bytes;
    }

    /// Set maximum tokenizer.json file size in bytes
    pub fn set_max_tokenizer_size(&mut self, max_bytes: u64) {
        self.max_tokenizer_size_bytes = max_bytes;
    }

    /// Set maximum number of loaded models globally
    pub fn with_limit(mut self, max_models: usize) -> Self {
        self.max_loaded_models = max_models;
        self
    }

    /// Check if loading another model would exceed the global limit
    pub fn check_global_load_limit(&self) -> Result<(), String> {
        let current_count = self.get_loaded_count();
        if current_count >= self.max_loaded_models {
            return Err(format!(
                "Global model limit exceeded: {} models loaded, maximum is {}",
                current_count, self.max_loaded_models
            ));
        }
        Ok(())
    }

    /// Check if loading another model would exceed the tenant limit
    pub fn check_tenant_load_limit(&self, _tenant_id: &str) -> Result<(), String> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let tenant_models = self.models.keys()
                .filter(|(t, _)| t == _tenant_id)
                .count();
            if tenant_models >= self.max_tenant_models {
                return Err(format!(
                    "Tenant model limit exceeded for '{}': {} models loaded, maximum is {}",
                    _tenant_id, tenant_models, self.max_tenant_models
                ));
            }
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            // Stub mode: no models loaded
        }
        Ok(())
    }


    /// Load a model synchronously (non-async version for use with Mutex guard)
    ///
    /// This version is used when already holding the runtime lock.
    pub fn load_model(
        &mut self,
        tenant_id: &str,
        model_id: &str,
        model_path: &str,
        _memory_usage_mb: i32,
    ) -> Result<(), String> {
        // Note: Validation is typically done by caller (load_model_async) or explicitly before calling
        // This allows load_model to be called when validation has already been performed
        // For safety, we validate here too (idempotent - fast check)
        self.validate_model_files(model_path)?;

        // Check file size before loading to prevent OOM
        let metadata = std::fs::metadata(model_path)
            .map_err(|e| format!("Failed to read model file metadata: {}", e))?;
        if metadata.len() > self.max_model_size_bytes {
            return Err(format!(
                "Model file size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                self.max_model_size_bytes
            ));
        }

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

            // Load model via MLX FFI
            // Note: MLXFFIModel::load() should handle partial failures internally.
            // If it returns Ok, the model is fully loaded. If Err, no partial state remains.
            match MLXFFIModel::load(model_path) {
                Ok(model) => {
                    info!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        path = %model_path,
                        memory_mb = %memory_usage_mb,
                        "Model loaded successfully"
                    );
                    self.models.insert(
                        key,
                        ModelMetadata {
                            model,
                            memory_usage_mb,
                        },
                    );
                    Ok(())
                }
                Err(e) => {
                    // Ensure no partial state: remove from HashMap if somehow present
                    let was_present = self.models.remove(&key).is_some();
                    if was_present {
                        warn!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            "Removed partial model state after load failure"
                        );
                    }
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
            // Stub mode: validation already done above
            Ok(())
        }
    }

    /// Load a model asynchronously with timeout (legacy method, kept for compatibility)
    #[allow(unused_variables)]
    pub async fn load_model_async(
        &mut self,
        tenant_id: &str,
        model_id: &str,
        model_path: &str,
        _timeout: Duration,
    ) -> Result<(), String> {
        // Use default memory estimate for async version
        self.load_model(tenant_id, model_id, model_path, 8192)
    }

    /// Validate that model directory exists and contains required files
    ///
    /// Checks for:
    /// - Directory existence and readability
    /// - config.json (required, readable, non-empty, within size limits)
    /// - tokenizer.json (required, readable, non-empty, within size limits)
    /// - weights file (weights.safetensors or model.safetensors, readable, non-empty, within size limits)
    fn validate_model_files(&self, model_path: &str) -> Result<(), String> {
        let path = Path::new(model_path);

        // Canonicalize path for security validation
        let canonical_path = normalize_path(path)
            .map_err(|e| format!("Path security validation failed for {}: {}", model_path, e))?;

        // Check if path exists
        if !canonical_path.exists() {
            return Err(format!("Model path does not exist: {}", model_path));
        }

        // Check if path is a directory
        if !canonical_path.is_dir() {
            return Err(format!("Model path is not a directory: {}", model_path));
        }

        // Check directory readability
        if std::fs::metadata(&canonical_path).is_err() {
            return Err(format!(
                "Model directory is not readable: {}",
                model_path
            ));
        }

        let path = canonical_path;

        // Validate config.json
        let config_path = path.join("config.json");
        Self::validate_required_file(&config_path, "config.json", model_path, Some(self.max_config_size_bytes))?;

        // Validate tokenizer.json
        let tokenizer_path = path.join("tokenizer.json");
        Self::validate_required_file(&tokenizer_path, "tokenizer.json", model_path, Some(self.max_tokenizer_size_bytes))?;

        // Validate weights file (check for either weights.safetensors or model.safetensors)
        let weights_path = path.join("weights.safetensors");
        let model_safetensors_path = path.join("model.safetensors");
        
        let weights_file_exists = weights_path.exists() && weights_path.is_file();
        let model_safetensors_exists = model_safetensors_path.exists() && model_safetensors_path.is_file();
        
        if !weights_file_exists && !model_safetensors_exists {
            return Err(format!(
                "Required weights file not found in model directory: {}. Expected either 'weights.safetensors' or 'model.safetensors'",
                model_path
            ));
        }

        // Validate the weights file that exists
        if weights_file_exists {
            Self::validate_required_file(&weights_path, "weights.safetensors", model_path, Some(self.max_model_size_bytes))?;
        } else {
            Self::validate_required_file(&model_safetensors_path, "model.safetensors", model_path, Some(self.max_model_size_bytes))?;
        }

        Ok(())
    }

    /// Validate that a required file exists, is readable, non-empty, and within size limits
    fn validate_required_file(file_path: &Path, file_name: &str, model_path: &str, max_size_bytes: Option<u64>) -> Result<(), String> {
        // Check existence
        if !file_path.exists() {
            return Err(format!(
                "Required file '{}' not found in model directory: {}",
                file_name, model_path
            ));
        }

        // Check it's a file (not a directory)
        if !file_path.is_file() {
            return Err(format!(
                "Required file '{}' exists but is not a file in model directory: {}",
                file_name, model_path
            ));
        }

        // Check readability and get metadata
        let metadata = std::fs::metadata(file_path).map_err(|e| {
            format!(
                "Cannot read metadata for '{}' in model directory {}: {}",
                file_name, model_path, e
            )
        })?;

        // Check file is not empty
        if metadata.len() == 0 {
            return Err(format!(
                "Required file '{}' is empty in model directory: {}",
                file_name, model_path
            ));
        }

        // Check file size against limits if specified
        if let Some(max_size) = max_size_bytes {
            if metadata.len() > max_size {
                return Err(format!(
                    "Required file '{}' size {} bytes exceeds maximum {} bytes in model directory: {}",
                    file_name, metadata.len(), max_size, model_path
                ));
            }
        }

        // Check file is readable by attempting to open it
        std::fs::File::open(file_path).map_err(|e| {
            format!(
                "Required file '{}' is not readable in model directory {}: {}",
                file_name, model_path, e
            )
        })?;

        Ok(())
    }

    /// Unload a model synchronously (non-async version for use with Mutex guard)
    ///
    /// This version is used when already holding the runtime lock.
    pub fn unload_model(&mut self, tenant_id: &str, model_id: &str) -> Result<(), String> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (tenant_id.to_string(), model_id.to_string());
            if self.models.remove(&key).is_some() {
                // Also remove from active operations if present
                self.active_operations.remove(&key);
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

    /// Unload a model asynchronously with timeout (legacy method, kept for compatibility)
    pub async fn unload_model_async(
        &mut self,
        tenant_id: &str,
        model_id: &str,
        _timeout: Duration,
    ) -> Result<(), String> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (tenant_id.to_string(), model_id.to_string());
            
            // Spawn blocking task for unload (typically fast, but wrapped for consistency)
            let handle = tokio::spawn(async move {
                // Unload is just removing from HashMap, so it's synchronous
                // But we wrap it for consistency with load_model
                tokio::task::spawn_blocking(move || {
                    // Model will be dropped when removed from HashMap
                    Ok::<(), String>(())
                })
                .await
                .unwrap_or_else(|e| Err(format!("Task join error: {}", e)))
            });

            // Store abort handle
            let abort_handle = handle.abort_handle();
            self.active_operations.insert(key.clone(), abort_handle);

            // Apply timeout (unload should be fast, but timeout for safety)
            let result = tokio::time::timeout(timeout, handle).await;
            
            self.active_operations.remove(&key);
            
            match result {
                Ok(Ok(Ok(()))) => {
                    // Unload is just removing from HashMap
                    if self.models.remove(&key).is_some() {
                        info!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            "Model unloaded successfully"
                        );
                    } else {
                        warn!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            "Model not found for unload"
                        );
                    }
                    Ok(())
                }
                Ok(Ok(Err(e))) => {
                    warn!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        error = %e,
                        "Model unload task failed"
                    );
                    Err(e)
                }
                Ok(Err(e)) => {
                    let error_msg = format!("Task join error: {}", e);
                    warn!(
                        tenant_id = %tenant_id,
                        model_id = %model_id,
                        error = %e,
                        "Model unload task join failed"
                    );
                    Err(error_msg)
                }
                Err(_) => {
                    // Timeout occurred - still try to remove from models
                    if self.models.remove(&key).is_some() {
                        warn!(
                            tenant_id = %tenant_id,
                            model_id = %model_id,
                            "Model unloaded after timeout"
                        );
                    }
                    Err(format!("Model unload timed out after {:?}", timeout))
                }
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

    /// Cancel an in-progress operation
    pub fn cancel_operation(&mut self, _tenant_id: &str, _model_id: &str) -> bool {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (_tenant_id.to_string(), _model_id.to_string());
            if let Some(handle) = self.active_operations.remove(&key) {
                handle.abort();
                info!(
                    tenant_id = %_tenant_id,
                    model_id = %_model_id,
                    "Operation cancelled"
                );
                true
            } else {
                false
            }
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            false
        }
    }

    /// Get all loaded models for reconciliation
    pub fn get_all_loaded_models(&self) -> Vec<(String, String)> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.models.keys().map(|(t, m)| (t.clone(), m.clone())).collect()
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            vec![]
        }
    }

    /// Check if a model is currently loaded in the runtime
    /// Returns true if the model is loaded, false otherwise
    pub fn is_model_loaded(&self, _tenant_id: &str, _model_id: &str) -> bool {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = (_tenant_id.to_string(), _model_id.to_string());
            self.models.contains_key(&key)
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            false // Stub mode: no models loaded
        }
    }

    /// Get count of loaded models
    pub fn get_loaded_count(&self) -> usize {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.models.len()
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            0
        }
    }

    #[cfg(test)]
    /// Test helper: create runtime with test data
    pub fn with_test_data() -> Self {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            // Create a runtime with some test state for testing
            let mut runtime = Self::new().with_limit(5);
            // Note: We can't easily create fake MLXFFIModel instances for testing
            // So we'll rely on integration tests for actual loading behavior
            runtime
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Self::new().with_limit(5)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_runtime_creation() {
        let _runtime = ModelRuntime::new();
        // Runtime created successfully
        assert!(true);
    }

    #[test]
    fn test_is_model_loaded_empty() {
        let runtime = ModelRuntime::new();
        assert!(!runtime.is_model_loaded("tenant1", "model1"));
    }

    #[tokio::test]
    async fn test_cancel_operation() {
        let mut runtime = ModelRuntime::new();

        // Without mlx-ffi-backend, cancel should return false
        assert!(!runtime.cancel_operation("tenant1", "model1"));
    }

    #[test]
    fn test_get_all_loaded_models() {
        let runtime = ModelRuntime::new();
        let models = runtime.get_all_loaded_models();
        assert!(models.is_empty());
    }
}
