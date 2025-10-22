//! Hot-swap adapter loading and unloading

use adapteros_core::{AosError, Result};
use adapteros_single_file_adapter::SingleFileAdapterLoader;
use std::collections::HashMap;
use std::path::PathBuf;

/// Adapter loader for hot-swap operations
pub struct AdapterLoader {
    /// Base path for adapter files
    base_path: PathBuf,
    /// Currently loaded adapters (adapter_id -> path)
    loaded: HashMap<u16, PathBuf>,
}

impl AdapterLoader {
    /// Create a new adapter loader
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            loaded: HashMap::new(),
        }
    }

    /// Load an adapter from disk (blocking call, use load_adapter_async for async contexts)
    pub fn load_adapter(&mut self, adapter_id: u16, adapter_name: &str) -> Result<AdapterHandle> {
        let adapter_path = self.resolve_path(adapter_name);

        if !adapter_path.exists() {
            return Err(AosError::Lifecycle(format!(
                "Adapter file not found: {}",
                adapter_path.display()
            )));
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
    ) -> Result<AdapterHandle> {
        // Perform the blocking load operation in a blocking task
        let base_path = self.base_path.clone();
        let adapter_name = adapter_name.to_string();

        let handle = tokio::task::spawn_blocking(move || {
            // Resolve candidate paths
            let adapter_path = {
                let mut name = adapter_name.clone();
                if let Some(rest) = name.strip_prefix("b3:") {
                    name = rest.to_string();
                }
                // Prefer sanitized name; avoid using the raw adapter_name with prefix as a path segment
                let candidates = [
                    base_path.join(format!("{}.safetensors", name)),
                    base_path.join(&name).join("weights.safetensors"),
                ];
                candidates
                    .into_iter()
                    .find(|p| p.exists())
                    .unwrap_or(base_path.join(format!("{}.safetensors", name)))
            };

            if !adapter_path.exists() {
                return Err(AosError::Lifecycle(format!(
                    "Adapter file not found: {}",
                    adapter_path.display()
                )));
            }

            // Load adapter weights from SafeTensors format
            use std::fs;
            let weights_data = fs::read(&adapter_path)
                .map_err(|e| AosError::Lifecycle(format!("Failed to read adapter file: {}", e)))?;

            tracing::info!(
                "Loaded adapter {} ({}) from {} ({} bytes)",
                adapter_id,
                adapter_name,
                adapter_path.display(),
                weights_data.len()
            );

            Ok(AdapterHandle {
                adapter_id,
                path: adapter_path.clone(),
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
                        let adapter =
                            SingleFileAdapterLoader::load(adapter_path)
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

                let weights_data = fs::read(adapter_path).map_err(|e| {
                    AosError::Lifecycle(format!("Failed to read adapter file: {}", e))
                })?;

                Ok(weights_data)
            }
        }
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

    /// Resolve adapter file path from flexible identifiers
    ///
    /// Supports the following layouts:
    /// - .aos files:    `<root>/<id>.aos` (PREFERRED)
    /// - Hex-based:     `<root>/<hex>.safetensors` or `<root>/<hex>/weights.safetensors`
    /// - Packaged dir:  `<root>/<id>/weights.safetensors`
    /// - Legacy flat:   `<root>/<id>.safetensors`
    fn resolve_path(&self, adapter_name: &str) -> std::path::PathBuf {
        let mut name = adapter_name.to_string();
        if let Some(rest) = name.strip_prefix("b3:") {
            name = rest.to_string();
        }

        let is_hex = name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit());

        let mut candidates: Vec<std::path::PathBuf> = Vec::new();

        // 1. FIRST: Try .aos files (preferred format)
        candidates.push(self.base_path.join(format!("{}.aos", &name)));
        if adapter_name != name {
            candidates.push(self.base_path.join(format!("{}.aos", adapter_name)));
        }

        // 2. Then try SafeTensors formats
        // Prefer packaged dir over flat for non-hex ids
        if !is_hex {
            candidates.push(self.base_path.join(&name).join("weights.safetensors"));
            candidates.push(self.base_path.join(format!("{}.safetensors", &name)));
        } else {
            // For hex, try flat first (CAS-like) then packaged dir
            candidates.push(self.base_path.join(format!("{}.safetensors", &name)));
            candidates.push(self.base_path.join(&name).join("weights.safetensors"));
        }

        // Also consider the raw adapter_name value (could include prefixes or legacy ids)
        if adapter_name != name {
            candidates.push(
                self.base_path
                    .join(adapter_name)
                    .join("weights.safetensors"),
            );
            candidates.push(self.base_path.join(format!("{}.safetensors", adapter_name)));
        }

        candidates
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| self.base_path.join(format!("{}.aos", &name)))
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
            .load_adapter(0, "test_adapter")
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
            .load_adapter(42, name)
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
            .load_adapter(1, name)
            .expect_err("should error when missing");
        let msg = format!("{}", err);
        // Should mention the attempted path
        assert!(msg.contains("Adapter file not found:"));
        assert!(msg.contains(&format!("{}.safetensors", name)));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
