//! Hot-swap adapter loading and unloading

use adapteros_core::{AosError, Result};
use tracing::warn;
use adapteros_secure_fs::traversal::{
    check_path_traversal, join_paths_safe, normalize_path,
    PathValidationConfig, validate_file_path_comprehensive, safe_file_exists, safe_file_metadata
};
use adapteros_single_file_adapter::{LoadOptions, SingleFileAdapterLoader};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

/// Adapter loader for hot-swap operations
pub struct AdapterLoader {
    /// Base path for adapter files
    base_path: PathBuf,
    /// Currently loaded adapters (adapter_id -> path)
    loaded: HashMap<u16, PathBuf>,
    /// Enable memory-mapped loading
    use_mmap: bool,
    /// Maximum cache size for memory-mapped adapters (MB)
    mmap_cache_size_mb: usize,
    /// Enable hot-swap capabilities
    hot_swap_enabled: bool,
    /// Optional mmap loader for .aos files
    mmap_loader: Option<Arc<tokio::sync::Mutex<adapteros_single_file_adapter::MmapAdapterLoader>>>,
    /// Maximum adapter file size in bytes (default: 500MB)
    max_adapter_size_bytes: u64,
    /// Per-tenant file size limits (tenant_id -> max_bytes)
    per_tenant_limits: HashMap<String, u64>,
}

impl AdapterLoader {
    /// Create a new adapter loader
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            loaded: HashMap::new(),
            use_mmap: false,
            mmap_cache_size_mb: 512,
            hot_swap_enabled: false,
            mmap_loader: None,
            max_adapter_size_bytes: 500 * 1024 * 1024, // Default 500MB
            per_tenant_limits: HashMap::new(),
        }
    }

    /// Set maximum adapter file size in bytes
    pub fn set_max_size(&mut self, max_bytes: u64) {
        self.max_adapter_size_bytes = max_bytes;
    }

    /// Set per-tenant file size limits
    pub fn set_per_tenant_limits(&mut self, limits: HashMap<String, u64>) {
        self.per_tenant_limits = limits;
    }

    /// Add or update a per-tenant file size limit
    pub fn set_tenant_limit(&mut self, tenant_id: &str, max_bytes: u64) {
        self.per_tenant_limits.insert(tenant_id.to_string(), max_bytes);
    }

    /// Enable memory-mapped adapter loading
    pub fn enable_mmap(&mut self, cache_size_mb: usize) {
        self.use_mmap = true;
        self.mmap_cache_size_mb = cache_size_mb;
        tracing::info!(
            "Enabled memory-mapped adapter loading with cache size: {} MB",
            cache_size_mb
        );
    }

    /// Inject a concrete mmap loader to be used by .aos loading path
    pub fn set_mmap_loader(
        &mut self,
        loader: Option<Arc<tokio::sync::Mutex<adapteros_single_file_adapter::MmapAdapterLoader>>>,
    ) {
        self.mmap_loader = loader;
    }

    /// Enable hot-swap capabilities
    pub fn enable_hot_swap(&mut self) {
        self.hot_swap_enabled = true;
        tracing::info!("Enabled hot-swap capabilities for dynamic adapter loading");
    }

    /// Check if mmap is enabled
    pub fn is_mmap_enabled(&self) -> bool {
        self.use_mmap
    }

    /// Check if hot-swap is enabled
    pub fn is_hot_swap_enabled(&self) -> bool {
        self.hot_swap_enabled
    }

    /// Load an adapter from disk (blocking call, use load_adapter_async for async contexts)
    pub fn load_adapter(&mut self, adapter_id: u16, adapter_name: &str, tenant_id: Option<&str>) -> Result<AdapterHandle> {
        let adapter_path = self.resolve_path(adapter_name)?;

        // Configure path validation for adapter loading
        let path_config = PathValidationConfig {
            allowed_bases: vec![self.base_path.clone()],
            max_path_length: 4096,
            max_file_size_bytes: self.max_adapter_size_bytes,
            per_tenant_limits: self.per_tenant_limits.clone(),
            enable_streaming_validation: true,
            max_header_size_bytes: 1024 * 1024,
        };

        // Validate adapter path with enhanced security
        validate_file_path_comprehensive(&adapter_path, &path_config)
            .map_err(|e| AosError::Lifecycle(format!("Invalid adapter path: {}", e)))?;

        let path_exists = safe_file_exists(&adapter_path, &path_config.allowed_bases)
            .map_err(|e| AosError::Lifecycle(format!("Cannot access adapter file: {}", e)))?;

        if !path_exists {
            return Err(AosError::Lifecycle(format!(
                "Adapter file not found: {}",
                adapter_path.display()
            )));
        }

        // Check file size before loading to prevent OOM using safe metadata read
        let metadata = safe_file_metadata(&adapter_path, &path_config.allowed_bases)
            .map_err(|e| AosError::Lifecycle(format!("Failed to read file metadata: {}", e)))?;

        // Check global limit
        if metadata.len() > self.max_adapter_size_bytes {
            // Log security violation
            warn!(
                security_violation = "adapter_file_size_exceeded_global_limit",
                adapter_id = adapter_id,
                tenant_id = ?tenant_id,
                file_size = metadata.len(),
                max_size = self.max_adapter_size_bytes,
                "Adapter file size exceeds global limit"
            );
            return Err(AosError::PolicyViolation(format!(
                "Adapter file size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                self.max_adapter_size_bytes
            )));
        }

        // Check per-tenant limit
        if let Some(tenant_id) = tenant_id {
            if let Some(tenant_limit) = self.per_tenant_limits.get(tenant_id) {
                if metadata.len() > *tenant_limit {
                    // Log security violation
                    warn!(
                        security_violation = "adapter_file_size_exceeded_tenant_limit",
                        adapter_id = adapter_id,
                        tenant_id = %tenant_id,
                        file_size = metadata.len(),
                        tenant_limit = *tenant_limit,
                        "Adapter file size exceeds tenant limit"
                    );
                    return Err(AosError::PolicyViolation(format!(
                        "Adapter file size {} bytes exceeds tenant '{}' limit of {} bytes",
                        metadata.len(),
                        tenant_id,
                        tenant_limit
                    )));
                }
            }
        }

        // Load adapter weights (supports both .aos and .safetensors)
        let weights_data = self.load_adapter_weights(&adapter_path)?;
        let memory_bytes = weights_data.len();

        self.loaded.insert(adapter_id, adapter_path.clone());

        tracing::info!(
            "Loaded adapter {} ({}) from {} ({} bytes)",
            adapter_id,
            adapter_name,
            adapter_path.display(),
            memory_bytes
        );

        Ok(AdapterHandle {
            adapter_id,
            path: adapter_path,
            memory_bytes,
        })
    }

    /// Load an adapter asynchronously using DeterministicExecutor
    pub async fn load_adapter_async(
        &mut self,
        adapter_id: u16,
        adapter_name: &str,
        tenant_id: Option<&str>,
    ) -> Result<AdapterHandle> {
        // Resolve path with security validation first
        let adapter_path = self.resolve_path(adapter_name)?;
        
        // Prefer memory-mapped .aos path if configured and available
        if self.use_mmap && adapter_path.extension() == Some(OsStr::new("aos")) {
            if let Some(mmap_loader) = self.mmap_loader.clone() {
                return self
                    .load_aos_mmap(adapter_id, adapter_name, &mmap_loader, tenant_id)
                    .await;
            }
        }
        
        // Perform the blocking load operation in a blocking task
        let adapter_path_clone = adapter_path.clone();
        let adapter_name_clone = adapter_name.to_string();

        let max_size = self.max_adapter_size_bytes;
        let base_path = self.base_path.clone();
        let tenant_limits = self.per_tenant_limits.clone();
        let tenant_id_clone = tenant_id.map(|s| s.to_string());
        let handle = tokio::task::spawn_blocking(move || {
            // Configure path validation for adapter loading
            let path_config = PathValidationConfig {
                allowed_bases: vec![base_path],
                max_path_length: 4096,
                max_file_size_bytes: max_size,
                per_tenant_limits: tenant_limits,
                enable_streaming_validation: true,
                max_header_size_bytes: 1024 * 1024,
            };

            // Validate adapter path with enhanced security
            validate_file_path_comprehensive(&adapter_path_clone, &path_config)
                .map_err(|e| AosError::Lifecycle(format!("Invalid adapter path: {}", e)))?;

            let path_exists = safe_file_exists(&adapter_path_clone, &path_config.allowed_bases)
                .map_err(|e| AosError::Lifecycle(format!("Cannot access adapter file: {}", e)))?;

            if !path_exists {
                return Err(AosError::Lifecycle(format!(
                    "Adapter file not found: {}",
                    adapter_path_clone.display()
                )));
            }

            // Check file size before loading to prevent OOM using safe metadata read
            let metadata = safe_file_metadata(&adapter_path_clone, &path_config.allowed_bases)
                .map_err(|e| AosError::Lifecycle(format!("Failed to read file metadata: {}", e)))?;

            // Check global limit
            if metadata.len() > max_size {
                // Log security violation
                warn!(
                    security_violation = "adapter_file_size_exceeded_global_limit",
                    adapter_id = adapter_id,
                    tenant_id = ?tenant_id_clone,
                    file_size = metadata.len(),
                    max_size = max_size,
                    "Adapter file size exceeds global limit"
                );
                return Err(AosError::PolicyViolation(format!(
                    "Adapter file size {} bytes exceeds maximum {} bytes",
                    metadata.len(),
                    max_size
                )));
            }

            // Check per-tenant limit
            if let Some(ref tenant_id) = tenant_id_clone {
                if let Some(tenant_limit) = path_config.per_tenant_limits.get(tenant_id) {
                    if metadata.len() > *tenant_limit {
                        // Log security violation
                        warn!(
                            security_violation = "adapter_file_size_exceeded_tenant_limit",
                            adapter_id = adapter_id,
                            tenant_id = %tenant_id,
                            file_size = metadata.len(),
                            tenant_limit = *tenant_limit,
                            "Adapter file size exceeds tenant limit"
                        );
                        return Err(AosError::PolicyViolation(format!(
                            "Adapter file size {} bytes exceeds tenant '{}' limit of {} bytes",
                            metadata.len(),
                            tenant_id,
                            tenant_limit
                        )));
                    }
                }
            }

            // Load adapter weights from SafeTensors format
            let weights_data = std::fs::read(&adapter_path_clone)
                .map_err(|e| AosError::Lifecycle(format!("Failed to read adapter file: {}", e)))?;

            tracing::info!(
                "Loaded adapter {} ({}) from {} ({} bytes)",
                adapter_id,
                adapter_name_clone,
                adapter_path_clone.display(),
                weights_data.len()
            );

            Ok(AdapterHandle {
                adapter_id,
                path: adapter_path_clone,
                memory_bytes: weights_data.len(),
            })
        })
        .await
        .map_err(|e| AosError::Lifecycle(format!("Failed to spawn load task: {}", e)))??;

        // Update internal state
        self.loaded.insert(adapter_id, handle.path.clone());

        Ok(handle)
    }

    /// Unload an adapter from memory
    pub fn unload_adapter(&mut self, adapter_id: u16) -> Result<()> {
        if self.loaded.remove(&adapter_id).is_none() {
            return Err(AosError::Lifecycle(format!(
                "Adapter {} not loaded",
                adapter_id
            )));
        }

        // Free adapter weights from memory
        self.free_adapter_weights(adapter_id)?;
        Ok(())
    }

    /// Check if adapter is loaded
    pub fn is_loaded(&self, adapter_id: u16) -> bool {
        self.loaded.contains_key(&adapter_id)
    }

    /// Get number of loaded adapters
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Load adapter weights from .aos or .safetensors file
    fn load_adapter_weights(&self, adapter_path: &PathBuf) -> Result<Vec<u8>> {
        use std::fs;

        // Check file extension to determine format
        let extension = adapter_path.extension().and_then(|s| s.to_str());

        match extension {
            Some("aos") => {
                // Load from .aos file
                tracing::debug!("Loading adapter from .aos file: {}", adapter_path.display());

                // Use tokio runtime to load async
                let runtime = tokio::runtime::Handle::try_current().ok().or_else(|| {
                    // If no runtime, create one
                    Some(tokio::runtime::Runtime::new().ok()?.handle().clone())
                });

                if let Some(handle) = runtime {
                    handle.block_on(async {
                        let options = LoadOptions {
                            skip_verification: false,
                            skip_signature_check: false,
                            use_mmap: false,
                        };
                        let adapter =
                            SingleFileAdapterLoader::load_with_options(adapter_path, options)
                                .await
                                .map_err(|e| {
                                    AosError::Lifecycle(format!("Failed to load .aos file: {}", e))
                                })?;

                        // Verify signature if present
                        if adapter.is_signed() {
                            match adapter.verify() {
                                Ok(true) => {
                                    tracing::info!(
                                        "✓ Adapter signature verified for {}",
                                        adapter_path.display()
                                    );
                                }
                                Ok(false) => {
                                    tracing::warn!(
                                        "⚠ Invalid signature for {}",
                                        adapter_path.display()
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "✗ Signature verification failed for {}: {}",
                                        adapter_path.display(),
                                        e
                                    );
                                }
                            }
                        }

                        tracing::info!(
                            "Loaded .aos adapter: {} v{} (format v{})",
                            adapter.manifest.adapter_id,
                            adapter.manifest.version,
                            adapter.manifest.format_version
                        );

                        // Convert AdapterWeights to Vec<u8> for compatibility
                        // For v2 format, serialize the weights structure
                        let weights_bytes = serde_json::to_vec(&adapter.weights).map_err(|e| {
                            AosError::Lifecycle(format!("Failed to serialize weights: {}", e))
                        })?;

                        Ok(weights_bytes)
                    })
                } else {
                    Err(AosError::Lifecycle(
                        "No tokio runtime available for async .aos loading".to_string(),
                    ))
                }
            }
            _ => {
                // Load from .safetensors or other format
                tracing::debug!(
                    "Loading adapter from SafeTensors file: {}",
                    adapter_path.display()
                );

                if self.use_mmap {
                    use std::fs::File;
                    let file = File::open(adapter_path).map_err(|e| {
                        AosError::Lifecycle(format!("Failed to open adapter file for mmap: {}", e))
                    })?;
                    let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }
                        .map_err(|e| AosError::Lifecycle(format!("mmap failed: {}", e)))?;
                    Ok(mmap.to_vec())
                } else {
                    let weights_data = fs::read(adapter_path).map_err(|e| {
                        AosError::Lifecycle(format!("Failed to read adapter file: {}", e))
                    })?;
                    Ok(weights_data)
                }
            }
        }
    }

    async fn load_aos_mmap(
        &mut self,
        adapter_id: u16,
        adapter_name: &str,
        loader: &Arc<tokio::sync::Mutex<adapteros_single_file_adapter::MmapAdapterLoader>>,
        tenant_id: Option<&str>,
    ) -> Result<AdapterHandle> {
        let path = self.resolve_path(adapter_name)?;

        // Configure path validation for adapter loading
        let path_config = PathValidationConfig {
            allowed_bases: vec![self.base_path.clone()],
            max_path_length: 4096,
            max_file_size_bytes: self.max_adapter_size_bytes,
            per_tenant_limits: self.per_tenant_limits.clone(),
            enable_streaming_validation: true,
            max_header_size_bytes: 1024 * 1024,
        };

        // Validate adapter path with enhanced security
        validate_file_path_comprehensive(&path, &path_config)
            .map_err(|e| AosError::Lifecycle(format!("Invalid adapter path: {}", e)))?;

        // Check file size before loading to prevent OOM using async safe metadata read
        // Note: tokio::fs::metadata is used here since it's async, but we could enhance this
        // with a safe async version in the future
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|e| AosError::Lifecycle(format!("Failed to read file metadata: {}", e)))?;

        // Check global limit
        if metadata.len() > self.max_adapter_size_bytes {
            // Log security violation
            warn!(
                security_violation = "adapter_file_size_exceeded_global_limit",
                adapter_id = adapter_id,
                tenant_id = ?tenant_id,
                file_size = metadata.len(),
                max_size = self.max_adapter_size_bytes,
                "Adapter file size exceeds global limit"
            );
            return Err(AosError::PolicyViolation(format!(
                "Adapter file size {} bytes exceeds maximum {} bytes",
                metadata.len(),
                self.max_adapter_size_bytes
            )));
        }

        // Check per-tenant limit
        if let Some(tenant_id) = tenant_id {
            if let Some(tenant_limit) = self.per_tenant_limits.get(tenant_id) {
                if metadata.len() > *tenant_limit {
                    // Log security violation
                    warn!(
                        security_violation = "adapter_file_size_exceeded_tenant_limit",
                        adapter_id = adapter_id,
                        tenant_id = %tenant_id,
                        file_size = metadata.len(),
                        tenant_limit = *tenant_limit,
                        "Adapter file size exceeds tenant limit"
                    );
                    return Err(AosError::PolicyViolation(format!(
                        "Adapter file size {} bytes exceeds tenant '{}' limit of {} bytes",
                        metadata.len(),
                        tenant_id,
                        tenant_limit
                    )));
                }
            }
        }
        
        let options = adapteros_single_file_adapter::LoadOptions {
            skip_verification: false,
            skip_signature_check: false,
            use_mmap: true,
        };
        let mmap_adapter = loader.lock().await.load(&path, &options)?;

        self.loaded.insert(adapter_id, path.clone());
        Ok(AdapterHandle {
            adapter_id,
            path,
            memory_bytes: mmap_adapter.file_len(),
        })
    }

    /// Free adapter weights from memory
    fn free_adapter_weights(&self, adapter_id: u16) -> Result<()> {
        // In a real implementation, this would:
        // 1. Zeroize the memory containing weights
        // 2. Release GPU memory if applicable
        // 3. Update memory tracking

        // For now, just log the operation
        tracing::debug!("Freed adapter weights for adapter {}", adapter_id);
        Ok(())
    }

    /// Estimate adapter size based on rank (simplified)
    #[allow(dead_code)]
    fn estimate_adapter_size(_adapter_name: &str) -> usize {
        // Simplified: assume 16MB per adapter
        // In reality, calculate based on rank * target_modules * model_dim
        16 * 1024 * 1024
    }

    /// Resolve adapter file path from flexible identifiers with security validation
    ///
    /// Supports the following layouts:
    /// - .aos files:    `<root>/<id>.aos` (PREFERRED)
    /// - Hex-based:     `<root>/<hex>.safetensors` or `<root>/<hex>/weights.safetensors`
    /// - Packaged dir:  `<root>/<id>/weights.safetensors`
    /// - Legacy flat:   `<root>/<id>.safetensors`
    ///
    /// All paths are validated to prevent path traversal attacks and canonicalized
    /// to ensure they remain within the base_path directory.
    pub fn resolve_path(&self, adapter_name: &str) -> Result<PathBuf> {
        // Validate adapter_name doesn't contain traversal patterns
        check_path_traversal(adapter_name)?;

        let mut name = adapter_name.to_string();
        if let Some(rest) = name.strip_prefix("b3:") {
            name = rest.to_string();
        }

        // Validate sanitized name
        check_path_traversal(&name)?;

        let is_hex = name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit());

        let mut candidates: Vec<PathBuf> = Vec::new();

        // 1. FIRST: Try .aos files (preferred format) with secure path joining
        if let Ok(path) = join_paths_safe(&self.base_path, format!("{}.aos", &name)) {
            candidates.push(path);
        }
        if adapter_name != name {
            if let Ok(path) = join_paths_safe(&self.base_path, format!("{}.aos", adapter_name)) {
                candidates.push(path);
            }
        }

        // 2. Then try SafeTensors formats with secure path joining
        if !is_hex {
            // Prefer packaged dir over flat for non-hex ids
            if let Ok(dir_path) = join_paths_safe(&self.base_path, &name) {
                if let Ok(path) = join_paths_safe(&dir_path, "weights.safetensors") {
                    candidates.push(path);
                }
            }
            if let Ok(path) = join_paths_safe(&self.base_path, format!("{}.safetensors", &name)) {
                candidates.push(path);
            }
        } else {
            // For hex, try flat first (CAS-like) then packaged dir
            if let Ok(path) = join_paths_safe(&self.base_path, format!("{}.safetensors", &name)) {
                candidates.push(path);
            }
            if let Ok(dir_path) = join_paths_safe(&self.base_path, &name) {
                if let Ok(path) = join_paths_safe(&dir_path, "weights.safetensors") {
                    candidates.push(path);
                }
            }
        }

        // Also consider the raw adapter_name value (could include prefixes or legacy ids)
        if adapter_name != name {
            if let Ok(dir_path) = join_paths_safe(&self.base_path, adapter_name) {
                if let Ok(path) = join_paths_safe(&dir_path, "weights.safetensors") {
                    candidates.push(path);
                }
            }
            if let Ok(path) = join_paths_safe(&self.base_path, format!("{}.safetensors", adapter_name)) {
                candidates.push(path);
            }
        }

        // Find first existing candidate
        let resolved = match candidates.into_iter().find(|p| p.exists()) {
            Some(path) => path,
            None => {
                // Default fallback - still needs to be validated
                join_paths_safe(&self.base_path, format!("{}.aos", &name))
                    .map_err(|e| AosError::Security(format!("Failed to resolve adapter path: {}", e)))?
            }
        };

        // Canonicalize and verify the path is within base_path
        let canonical_base = self.base_path
            .canonicalize()
            .map_err(|e| AosError::Security(format!("Failed to canonicalize base path: {}", e)))?;
        
        let canonical_resolved = normalize_path(&resolved)?;
        
        // Verify resolved path is within base_path
        if !canonical_resolved.starts_with(&canonical_base) {
            return Err(AosError::Security(format!(
                "Resolved adapter path {} is outside base directory {}",
                canonical_resolved.display(),
                canonical_base.display()
            )));
        }

        Ok(canonical_resolved)
    }
}

/// Handle to a loaded adapter
#[derive(Debug, Clone)]
pub struct AdapterHandle {
    pub adapter_id: u16,
    pub path: PathBuf,
    pub memory_bytes: usize,
}

impl AdapterHandle {
    /// Get memory footprint in bytes
    pub fn memory_bytes(&self) -> usize {
        self.memory_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_loader_basic() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader");
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        // Create a fake adapter file
        let adapter_path = temp_dir.join("test_adapter.safetensors");
        fs::write(&adapter_path, b"fake adapter data").expect("Test file write should succeed");

        let mut loader = AdapterLoader::new(temp_dir.clone());

        // Load adapter
        let handle = loader
            .load_adapter(0, "test_adapter", None)
            .expect("Test adapter load should succeed");
        assert_eq!(handle.adapter_id, 0);
        assert!(loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 1);

        // Unload adapter
        loader
            .unload_adapter(0)
            .expect("Test adapter unload should succeed");
        assert!(!loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 0);

        // Cleanup
        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_resolve_prefers_packaged_dir_over_flat_for_non_hex() {
        let temp_dir = std::env::temp_dir().join("mplora_loader_pref");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let name = "my_adapter"; // non-hex id
                                 // Create both packaged dir and flat file
        let packaged_dir = temp_dir.join(name);
        fs::create_dir_all(&packaged_dir).unwrap();
        let packaged_path = packaged_dir.join("weights.safetensors");
        fs::write(&packaged_path, b"packaged").unwrap();

        let flat_path = temp_dir.join(format!("{}.safetensors", name));
        fs::write(&flat_path, b"flat").unwrap();

        let mut loader = AdapterLoader::new(temp_dir.clone());
        let handle = loader
            .load_adapter(42, name, None)
            .expect("should load packaged path");
        // Should pick packaged_dir/weights.safetensors
        assert_eq!(handle.path, packaged_path);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_error_when_no_candidate_path_exists() {
        let temp_dir = std::env::temp_dir().join("mplora_loader_missing");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut loader = AdapterLoader::new(temp_dir.clone());
        let name = "nonexistent_adapter";
        let err = loader
            .load_adapter(1, name, None)
            .expect_err("should error when missing");
        let msg = format!("{}", err);
        // Should mention the attempted path
        assert!(msg.contains("Adapter file not found:"));
        assert!(msg.contains(&format!("{}.safetensors", name)));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
