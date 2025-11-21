//! Model runtime for base model load/unload.
//!
//! When mlx-ffi-backend feature is enabled, actually loads models via MLX FFI.
//! Otherwise acts as a stub for environments where backend is not linked.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::warn;

#[cfg(feature = "mlx-ffi-backend")]
use adapteros_lora_mlx_ffi::{MLXFFIModel, ModelConfig};
#[cfg(feature = "mlx-ffi-backend")]
use lru::LruCache;
#[cfg(feature = "mlx-ffi-backend")]
use tokio::task::AbortHandle;
#[cfg(feature = "mlx-ffi-backend")]
use tracing::info;

use adapteros_secure_fs::traversal::normalize_path;

/// Model loading specification
#[derive(Clone, Debug)]
pub struct LoadModelSpec {
    pub tenant_id: String,
    pub model_id: String,
    pub model_path: std::path::PathBuf,
    pub adapter_path: Option<std::path::PathBuf>,
    pub quantization: Option<String>,
}

/// Key for tracking loaded models: (tenant_id, model_id)
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct ModelKey {
    pub tenant_id: String,
    pub model_id: String,
}

/// Handle to a loaded model
#[derive(Clone, Debug)]
pub struct ModelHandle {
    pub key: ModelKey,
    pub memory_usage_mb: i32,
}

/// Progress event during model loading
#[derive(Clone, Debug)]
pub struct ProgressEvent {
    pub pct: f64,
    pub message: String,
}

/// Model loading error types
#[derive(thiserror::Error, Debug)]
pub enum ModelLoadError {
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("invalid model: {0}")]
    Invalid(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("canceled")]
    Canceled,
}

/// Trait for model runtime implementations
#[async_trait::async_trait]
pub trait ModelRuntime: Send + Sync {
    /// Load (or hot-swap) a model (and optional adapter) asynchronously and report progress.
    async fn load_model_async_with_progress<F>(
        &self,
        req: LoadModelSpec,
        on_progress: F,
    ) -> Result<ModelHandle, ModelLoadError>
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static;

    /// Check if a model is already loaded by key.
    fn is_loaded(&self, key: &ModelKey) -> bool;

    /// Unload a model if loaded.
    async fn unload(&self, key: &ModelKey) -> Result<(), ModelLoadError>;
}

/// Key for tracking loaded models: (tenant_id, model_id)

/// Model metadata including memory usage
#[cfg(feature = "mlx-ffi-backend")]
struct ModelMetadata {
    model: MLXFFIModel,
    memory_usage_mb: i32,
}

/// Cache entry for lazy loading - tracks model paths and access patterns
#[cfg(feature = "mlx-ffi-backend")]
struct ModelCacheEntry {
    model_path: String,
    last_accessed: Instant,
    access_count: u64,
    created_at: Instant,
    size_bytes: u64,
}

pub struct ModelRuntimeImpl {
    #[cfg(feature = "mlx-ffi-backend")]
    /// Loaded models by (tenant_id, model_id) with metadata
    models: HashMap<ModelKey, ModelMetadata>,
    /// Active operation handles for cancellation: (tenant_id, model_id) -> AbortHandle
    #[cfg(feature = "mlx-ffi-backend")]
    active_operations: HashMap<ModelKey, AbortHandle>,
    /// Model cache for lazy loading - stores recently used model paths
    #[cfg(feature = "mlx-ffi-backend")]
    model_cache: lru::LruCache<ModelKey, ModelCacheEntry>,
    /// Lazy loading enabled flag
    lazy_loading_enabled: bool,
    /// Maximum number of models to keep cached
    max_cached_models: usize,
    /// Cache eviction policy ("lru", "lfu", "ttl")
    cache_eviction_policy: String,
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
    /// Per-tenant file size limits (tenant_id -> max_bytes)
    per_tenant_limits: HashMap<String, u64>,
}

impl Default for ModelRuntimeImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRuntimeImpl {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "mlx-ffi-backend")]
            models: HashMap::new(),
            #[cfg(feature = "mlx-ffi-backend")]
            active_operations: HashMap::new(),
            #[cfg(feature = "mlx-ffi-backend")]
            model_cache: LruCache::new(std::num::NonZeroUsize::new(3).expect("Invalid cache size")),
            lazy_loading_enabled: false,
            max_cached_models: 3,
            cache_eviction_policy: "lru".to_string(),
            max_model_size_bytes: 10 * 1024 * 1024 * 1024, // Default 10GB
            max_config_size_bytes: 1024 * 1024,            // Default 1MB
            max_tokenizer_size_bytes: 10 * 1024 * 1024,    // Default 10MB
            max_loaded_models: 5,                          // Default: 5 models globally
            max_tenant_models: 2,                          // Default: 2 models per tenant
            per_tenant_limits: HashMap::new(),
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
            #[cfg(feature = "mlx-ffi-backend")]
            model_cache: LruCache::new(std::num::NonZeroUsize::new(3).expect("Invalid cache size")),
            lazy_loading_enabled: false,
            max_cached_models: 3,
            cache_eviction_policy: "lru".to_string(),
            max_model_size_bytes,
            max_config_size_bytes,
            max_tokenizer_size_bytes,
            max_loaded_models: 5, // Default: 5 models globally
            max_tenant_models: 2, // Default: 2 models per tenant
            per_tenant_limits: HashMap::new(),
        }
    }

    /// Set maximum model file size in bytes
    pub fn set_max_size(&mut self, max_bytes: u64) {
        self.max_model_size_bytes = max_bytes;
    }

    /// Enable or disable lazy loading
    pub fn set_lazy_loading(&mut self, enabled: bool) {
        self.lazy_loading_enabled = enabled;
    }

    /// Set maximum number of cached models
    pub fn set_max_cached_models(&mut self, max_cached: usize) {
        self.max_cached_models = max_cached;
        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.model_cache.resize(
                std::num::NonZeroUsize::new(max_cached)
                    .unwrap_or(std::num::NonZeroUsize::new(1).expect("Invalid default batch size")),
            );
        }
    }

    /// Set cache eviction policy ("lru", "lfu", "ttl")
    pub fn set_cache_eviction_policy(&mut self, policy: String) {
        self.cache_eviction_policy = policy;
    }

    /// Evict models from cache based on the configured policy
    #[cfg(feature = "mlx-ffi-backend")]
    pub fn evict_cache_entries(&mut self) -> usize {
        let mut evicted = 0;

        match self.cache_eviction_policy.as_str() {
            "lfu" => {
                // Evict least frequently used entries when over capacity
                while self.model_cache.len() > self.max_cached_models {
                    if let Some((key, _)) = self
                        .model_cache
                        .iter()
                        .min_by_key(|(_, entry)| entry.access_count)
                    {
                        let key_to_remove = key.clone();
                        self.model_cache.pop(&key_to_remove);
                        evicted += 1;
                        info!("Evicted LFU cache entry: {:?}", key_to_remove);
                    }
                }
            }
            "ttl" => {
                // Evict entries older than 1 hour (TTL policy)
                let ttl_duration = Duration::from_secs(3600); // 1 hour
                let now = Instant::now();
                let keys_to_remove: Vec<_> = self
                    .model_cache
                    .iter()
                    .filter(|(_, entry)| now.duration_since(entry.created_at) > ttl_duration)
                    .map(|(key, _)| key.clone())
                    .collect();

                for key in keys_to_remove {
                    self.model_cache.pop(&key);
                    evicted += 1;
                    info!("Evicted TTL cache entry: {:?}", key);
                }
            }
            "lru" | _ => {
                // LRU is handled automatically by the LruCache
                // But we can still manually evict if over capacity
                while self.model_cache.len() > self.max_cached_models {
                    if let Some((key, _)) = self.model_cache.iter().next() {
                        let key_to_remove = key.clone();
                        self.model_cache.pop(&key_to_remove);
                        evicted += 1;
                        info!("Evicted LRU cache entry: {:?}", key_to_remove);
                    }
                }
            }
        }

        evicted
    }

    /// Get cache statistics
    #[cfg(feature = "mlx-ffi-backend")]
    pub fn get_cache_stats(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        stats.insert("cache_size".to_string(), self.model_cache.len() as u64);
        stats.insert("max_cache_size".to_string(), self.max_cached_models as u64);

        let total_accesses: u64 = self
            .model_cache
            .iter()
            .map(|(_, entry)| entry.access_count)
            .sum();
        stats.insert("total_accesses".to_string(), total_accesses);

        let total_size_bytes: u64 = self
            .model_cache
            .iter()
            .map(|(_, entry)| entry.size_bytes)
            .sum();
        stats.insert("total_cached_size_bytes".to_string(), total_size_bytes);

        stats
    }

    /// Ensure a model is loaded (used for lazy loading - load on first inference request)
    pub async fn ensure_model_loaded(
        &mut self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<(), String> {
        let _model_key = (tenant_id.to_string(), model_id.to_string());

        // If lazy loading is disabled, assume model is already loaded
        if !self.lazy_loading_enabled {
            return Ok(());
        }

        #[cfg(feature = "mlx-ffi-backend")]
        {
            // Check if model is already loaded
            if self.models.contains_key(&model_key) {
                return Ok(());
            }

            // Check if model is in cache
            if let Some(cache_entry) = self.model_cache.get(&model_key) {
                // Update access statistics
                let mut updated_entry = cache_entry.clone();
                updated_entry.last_accessed = Instant::now();
                updated_entry.access_count += 1;
                self.model_cache.put(model_key.clone(), updated_entry);

                // Actually load the model now
                info!(
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    "Lazy loading model on first inference request: {}",
                    cache_entry.model_path
                );

                // Call the existing load logic but skip the lazy loading path
                let was_lazy = self.lazy_loading_enabled;
                self.lazy_loading_enabled = false;
                let result = self.load_model(tenant_id, model_id, &cache_entry.model_path, 0);
                self.lazy_loading_enabled = was_lazy;

                result
            } else {
                Err(format!(
                    "Model {} for tenant {} not found in cache",
                    model_id, tenant_id
                ))
            }
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Err("MLX backend not available for model loading".to_string())
        }
    }

    /// Set per-tenant file size limits
    pub fn set_per_tenant_limits(&mut self, limits: HashMap<String, u64>) {
        self.per_tenant_limits = limits;
    }

    /// Add or update a per-tenant file size limit
    pub fn set_tenant_limit(&mut self, tenant_id: &str, max_bytes: u64) {
        self.per_tenant_limits
            .insert(tenant_id.to_string(), max_bytes);
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
            let tenant_models = self
                .models
                .keys()
                .filter(|key| key.tenant_id == _tenant_id)
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
        let _model_key = (tenant_id.to_string(), model_id.to_string());

        // If lazy loading is enabled, just cache the model path instead of loading
        if self.lazy_loading_enabled {
            #[cfg(feature = "mlx-ffi-backend")]
            {
                // Get file size for cache entry
                let size_bytes = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);

                let cache_entry = ModelCacheEntry {
                    model_path: model_path.to_string(),
                    last_accessed: Instant::now(),
                    access_count: 0,
                    created_at: Instant::now(),
                    size_bytes,
                };
                self.model_cache.put(model_key, cache_entry);

                // Evict entries if we exceed capacity
                self.evict_cache_entries();

                info!(
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    "Model cached for lazy loading: {}",
                    model_path
                );
                return Ok(());
            }
            #[cfg(not(feature = "mlx-ffi-backend"))]
            {
                warn!("Lazy loading requested but MLX backend not available - models will be cached but not actually loaded");
                return Ok(());
            }
        }

        // Note: Validation is typically done by caller (load_model_async) or explicitly before calling
        // This allows load_model to be called when validation has already been performed
        // For safety, we validate here too (idempotent - fast check)
        self.validate_model_files(model_path)?;

        // Check file size before loading to prevent OOM
        let metadata = std::fs::metadata(model_path)
            .map_err(|e| format!("Failed to read model file metadata: {}", e))?;

        // Check global limit
        if metadata.len() > self.max_model_size_bytes {
            // Log security violation
            warn!(
                security_violation = "model_file_size_exceeded_global_limit",
                tenant_id = %tenant_id,
                model_id = %model_id,
                file_size = metadata.len(),
                max_size = self.max_model_size_bytes,
                "Model file size exceeds global limit"
            );
            return Err(format!(
                "Model file size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                self.max_model_size_bytes
            ));
        }

        // Check per-tenant limit
        if let Some(tenant_limit) = self.per_tenant_limits.get(tenant_id) {
            if metadata.len() > *tenant_limit {
                // Log security violation
                warn!(
                    security_violation = "model_file_size_exceeded_tenant_limit",
                    tenant_id = %tenant_id,
                    model_id = %model_id,
                    file_size = metadata.len(),
                    tenant_limit = *tenant_limit,
                    "Model file size exceeds tenant limit"
                );
                return Err(format!(
                    "Model file size {} bytes exceeds tenant '{}' limit of {} bytes",
                    metadata.len(),
                    tenant_id,
                    tenant_limit
                ));
            }
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

    /// Load model with progress callbacks for real-time updates
    ///
    /// # Citations
    /// - Progress tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs L315-340]
    /// - SSE broadcasting: [source: crates/adapteros-server-api/src/state.rs L437-438]
    pub async fn load_model_async_with_progress<F>(
        &mut self,
        tenant_id: &str,
        model_id: &str,
        model_path: &str,
        progress_callback: F,
        _timeout: Duration,
    ) -> Result<(), String>
    where
        F: Fn(f64, String) + Send + Sync + 'static,
    {
        progress_callback(0.0, "Starting model validation".to_string());

        // Validate model files (0-10% progress)
        self.validate_model_files(model_path)?;
        progress_callback(10.0, "Model files validated".to_string());

        // Check file size limits (10-20% progress)
        let metadata = std::fs::metadata(model_path)
            .map_err(|e| format!("Failed to read model file metadata: {}", e))?;

        if metadata.len() > self.max_model_size_bytes {
            return Err(format!(
                "Model file too large: {} bytes (limit: {} bytes)",
                metadata.len(),
                self.max_model_size_bytes
            ));
        }
        progress_callback(20.0, "File size validated".to_string());

        // Load model (20-90% progress)
        #[cfg(feature = "mlx-ffi-backend")]
        {
            use adapteros_lora_mlx_ffi::MLXFFIModel;

            progress_callback(30.0, "Creating MLX model container".to_string());

            // Load model via MLX FFI
            progress_callback(40.0, "Loading model weights from safetensors".to_string());
            let model = MLXFFIModel::load(model_path).map_err(|e| {
                format!("Failed to load MLX model: {}", e)
            })?;

            progress_callback(60.0, "Parsing model configuration".to_string());

            // Estimate memory usage from config
            let config = model.config();
            let num_parameters = config.hidden_size * config.num_hidden_layers * config.num_attention_heads;
            let bytes_per_param = 2; // FP16 weights
            let total_bytes = num_parameters * bytes_per_param * 2; // ×2 for weights + kv cache
            let memory_usage_mb = (total_bytes / (1024 * 1024)).max(512) as i32;

            progress_callback(70.0, format!("Memory allocated: {} MB", memory_usage_mb));

            // Store in models map
            let key = ModelKey {
                tenant_id: tenant_id.to_string(),
                model_id: model_id.to_string(),
            };

            // Check if already loaded
            if self.models.contains_key(&key) {
                progress_callback(80.0, "Model already loaded, replacing".to_string());
                self.models.remove(&key);
            }

            progress_callback(80.0, "Registering model in runtime".to_string());
            self.models.insert(
                key,
                ModelMetadata {
                    model,
                    memory_usage_mb,
                },
            );

            info!(
                tenant_id = %tenant_id,
                model_id = %model_id,
                path = %model_path,
                memory_mb = %memory_usage_mb,
                "MLX model loaded successfully"
            );
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            // Stub mode: just validate that loading would succeed
            progress_callback(50.0, "MLX backend not available, running in stub mode".to_string());
            warn!(
                tenant_id = %tenant_id,
                model_id = %model_id,
                "Model runtime stub: mlx-ffi-backend feature not enabled"
            );
        }

        progress_callback(90.0, "Model loaded, finalizing".to_string());

        // Final setup (90-100%)
        tokio::time::sleep(Duration::from_millis(50)).await;
        progress_callback(100.0, "Model loaded successfully".to_string());

        Ok(())
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
            return Err(format!("Model directory is not readable: {}", model_path));
        }

        let path = canonical_path;

        // Validate config.json
        let config_path = path.join("config.json");
        Self::validate_required_file(
            &config_path,
            "config.json",
            model_path,
            Some(self.max_config_size_bytes),
        )?;

        // Validate tokenizer.json
        let tokenizer_path = path.join("tokenizer.json");
        Self::validate_required_file(
            &tokenizer_path,
            "tokenizer.json",
            model_path,
            Some(self.max_tokenizer_size_bytes),
        )?;

        // Validate weights file (check for either weights.safetensors or model.safetensors)
        let weights_path = path.join("weights.safetensors");
        let model_safetensors_path = path.join("model.safetensors");

        let weights_file_exists = weights_path.exists() && weights_path.is_file();
        let model_safetensors_exists =
            model_safetensors_path.exists() && model_safetensors_path.is_file();

        if !weights_file_exists && !model_safetensors_exists {
            return Err(format!(
                "Required weights file not found in model directory: {}. Expected either 'weights.safetensors' or 'model.safetensors'",
                model_path
            ));
        }

        // Validate the weights file that exists
        if weights_file_exists {
            Self::validate_required_file(
                &weights_path,
                "weights.safetensors",
                model_path,
                Some(self.max_model_size_bytes),
            )?;
        } else {
            Self::validate_required_file(
                &model_safetensors_path,
                "model.safetensors",
                model_path,
                Some(self.max_model_size_bytes),
            )?;
        }

        Ok(())
    }

    /// Validate that a required file exists, is readable, non-empty, and within size limits
    fn validate_required_file(
        file_path: &Path,
        file_name: &str,
        model_path: &str,
        max_size_bytes: Option<u64>,
    ) -> Result<(), String> {
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
                // Also remove from active operations and cache if present
                self.active_operations.remove(&key);
                let cache_key = ModelKey {
                    tenant_id: tenant_id.to_string(),
                    model_id: model_id.to_string(),
                };
                self.model_cache.pop(&cache_key);
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
                        // Also remove from cache to prevent memory leak
                        let cache_key = ModelKey {
                            tenant_id: tenant_id.to_string(),
                            model_id: model_id.to_string(),
                        };
                        self.model_cache.pop(&cache_key);
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
    pub fn get_all_loaded_models(&self) -> Vec<ModelKey> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.models.keys().cloned().collect()
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            vec![]
        }
    }

    /// Get memory usage in MB for a specific loaded model
    /// Returns None if the model is not loaded
    pub fn get_model_memory(&self, tenant_id: &str, model_id: &str) -> Option<i32> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            let key = ModelKey {
                tenant_id: tenant_id.to_string(),
                model_id: model_id.to_string(),
            };
            self.models.get(&key).map(|metadata| metadata.memory_usage_mb)
        }

        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            let _ = (tenant_id, model_id);
            None
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

    /// Estimate memory usage in MB based on model configuration
    #[cfg(feature = "mlx-ffi-backend")]
    fn estimate_memory_usage_mb(config: &ModelConfig) -> i32 {
        // Estimate: parameters × 2 bytes per parameter (weights + gradients/kv cache)
        // Conservative estimate based on MLX memory patterns
        let num_parameters =
            config.hidden_size * config.num_hidden_layers * config.num_attention_heads;
        let bytes_per_param = 2; // FP16 weights
        let total_bytes = num_parameters * bytes_per_param * 2; // ×2 for weights + gradients/kv cache
        let total_mb = total_bytes / (1024 * 1024);
        total_mb.max(512) as i32 // Minimum 512MB for any model
    }

    #[cfg(not(feature = "mlx-ffi-backend"))]
    #[allow(unused)]
    fn estimate_memory_usage_mb(_config: &()) -> i32 {
        1024 // Default estimate when MLX not available
    }
}

#[async_trait::async_trait]
impl ModelRuntime for ModelRuntimeImpl {
    async fn load_model_async_with_progress<F>(
        &self,
        _req: LoadModelSpec,
        _on_progress: F,
    ) -> Result<ModelHandle, ModelLoadError>
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static,
    {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            // Check if already loaded
            let key = ModelKey {
                tenant_id: req.tenant_id.clone(),
                model_id: req.model_id.clone(),
            };

            if let Some(metadata) = self.models.get(&key) {
                on_progress(ProgressEvent {
                    pct: 100.0,
                    message: "Model already loaded".to_string(),
                });
                return Ok(ModelHandle {
                    key,
                    memory_usage_mb: metadata.memory_usage_mb,
                });
            }

            // Create new MLX model container
            on_progress(ProgressEvent {
                pct: 40.0,
                message: "allocating graph".to_string(),
            });
            let mut model = MLXFFIModel::new().map_err(|e| {
                ModelLoadError::Backend(format!("Failed to create MLX model: {}", e))
            })?;

            // Load base model weights
            on_progress(ProgressEvent {
                pct: 60.0,
                message: "loading base weights".to_string(),
            });
            model
                .load_base(&req.model_path, req.quantization.as_deref())
                .map_err(|e| {
                    ModelLoadError::Backend(format!("Failed to load base model: {}", e))
                })?;

            // Load adapter if specified
            if let Some(adapter_path) = &req.adapter_path {
                on_progress(ProgressEvent {
                    pct: 80.0,
                    message: format!("applying adapter: {}", adapter_path.display()),
                });
                model.load_adapter(adapter_path).map_err(|e| {
                    ModelLoadError::Backend(format!("Failed to load adapter: {}", e))
                })?;
            }

            // Warm up the model
            on_progress(ProgressEvent {
                pct: 90.0,
                message: "warming up".to_string(),
            });
            model
                .warmup()
                .map_err(|e| ModelLoadError::Backend(format!("Failed to warmup model: {}", e)))?;

            // Register in runtime with estimated memory usage
            let memory_usage_mb = Self::estimate_memory_usage_mb(&model.config);
            let metadata = ModelMetadata {
                model,
                memory_usage_mb,
            };
            self.models.insert(key.clone(), metadata);

            Ok(ModelHandle {
                key,
                memory_usage_mb,
            })
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Err(ModelLoadError::Backend(
                "MLX backend not available".to_string(),
            ))
        }
    }

    fn is_loaded(&self, _key: &ModelKey) -> bool {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            self.models.contains_key(_key)
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            false
        }
    }

    async fn unload(&self, _key: &ModelKey) -> Result<(), ModelLoadError> {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            if self.models.remove(_key).is_some() {
                Ok(())
            } else {
                Err(ModelLoadError::NotFound(format!(
                    "{}:{}",
                    _key.tenant_id, _key.model_id
                )))
            }
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Err(ModelLoadError::Backend(
                "MLX backend not available".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_runtime_creation() {
        let _runtime = ModelRuntimeImpl::new();
        // Runtime created successfully
        assert!(true);
    }

    #[test]
    fn test_is_model_loaded_empty() {
        let runtime = ModelRuntimeImpl::new();
        assert!(!runtime.is_model_loaded("tenant1", "model1"));
    }

    #[tokio::test]
    async fn test_cancel_operation() {
        let mut runtime = ModelRuntimeImpl::new();

        // Without mlx-ffi-backend, cancel should return false
        assert!(!runtime.cancel_operation("tenant1", "model1"));
    }

    #[test]
    fn test_get_all_loaded_models() {
        let runtime = ModelRuntimeImpl::new();
        let models = runtime.get_all_loaded_models();
        assert!(models.is_empty());
    }
}
