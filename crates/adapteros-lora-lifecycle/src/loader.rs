//! Hot-swap adapter loading and unloading

use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Adapter loader for hot-swap operations
pub struct AdapterLoader {
    /// Base path for adapter files
    base_path: PathBuf,
    /// Currently loaded adapters (adapter_id -> path)
    loaded: HashMap<u16, PathBuf>,
    /// Expected hashes from manifest
    expected_hashes: HashMap<String, B3Hash>,
}

impl AdapterLoader {
    /// Create a new adapter loader
    pub fn new(base_path: PathBuf, expected_hashes: HashMap<String, B3Hash>) -> Self {
        Self {
            base_path,
            loaded: HashMap::new(),
            expected_hashes,
        }
    }

    fn expected_hash(&self, adapter_name: &str) -> Result<B3Hash> {
        self.expected_hashes
            .get(adapter_name)
            .copied()
            .ok_or_else(|| {
                AosError::Lifecycle(format!(
                    "Missing expected hash for adapter {}",
                    adapter_name
                ))
            })
    }

    /// Load an adapter from disk (blocking call, use load_adapter_async for async contexts)
    pub fn load_adapter(&mut self, adapter_id: u16, adapter_name: &str) -> Result<AdapterHandle> {
        let adapter_path = self.base_path.join(format!("{}.safetensors", adapter_name));

        if !adapter_path.exists() {
            return Err(AosError::Lifecycle(format!(
                "Adapter file not found: {}",
                adapter_path.display()
            )));
        }

        // Load adapter weights from SafeTensors format
        let weights = self.load_adapter_weights(&adapter_path)?;
        let expected_hash = self.expected_hash(adapter_name)?;
        let actual_hash = B3Hash::hash(&weights);

        if actual_hash != expected_hash {
            tracing::error!(
                "Adapter hash mismatch for {} (expected {}, got {})",
                adapter_name,
                expected_hash,
                actual_hash
            );
            return Err(AosError::AdapterHashMismatch {
                adapter_id: adapter_name.to_string(),
                expected: expected_hash,
                actual: actual_hash,
            });
        }
        self.loaded.insert(adapter_id, adapter_path.clone());

        tracing::info!(
            "Loaded adapter {} ({}) from {}",
            adapter_id,
            adapter_name,
            adapter_path.display()
        );

        Ok(AdapterHandle {
            adapter_id,
            path: adapter_path,
            memory_bytes: Self::estimate_adapter_size(adapter_name),
        })
    }

    /// Load an adapter asynchronously using DeterministicExecutor
    pub async fn load_adapter_async(
        &mut self,
        adapter_id: u16,
        adapter_name: &str,
    ) -> Result<AdapterHandle> {
        let base_path = self.base_path.clone();
        let expected_hash = self.expected_hash(adapter_name)?;
        let adapter_name = adapter_name.to_string();

        let handle = tokio::task::spawn_blocking(move || {
            let adapter_path = base_path.join(format!("{}.safetensors", adapter_name));

            if !adapter_path.exists() {
                return Err(AosError::Lifecycle(format!(
                    "Adapter file not found: {}",
                    adapter_path.display()
                )));
            }

            use std::fs;
            let weights_data = fs::read(&adapter_path)
                .map_err(|e| AosError::Lifecycle(format!("Failed to read adapter file: {}", e)))?;

            let actual_hash = B3Hash::hash(&weights_data);
            if actual_hash != expected_hash {
                tracing::error!(
                    "Adapter hash mismatch for {} (expected {}, got {})",
                    adapter_name,
                    expected_hash,
                    actual_hash
                );
                return Err(AosError::AdapterHashMismatch {
                    adapter_id: adapter_name.clone(),
                    expected: expected_hash,
                    actual: actual_hash,
                });
            }

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

    /// Load adapter weights from SafeTensors file
    fn load_adapter_weights(&self, adapter_path: &PathBuf) -> Result<Vec<u8>> {
        use std::fs;

        // Read the SafeTensors file
        let weights_data = fs::read(adapter_path)
            .map_err(|e| AosError::Lifecycle(format!("Failed to read adapter file: {}", e)))?;

        // In a real implementation, this would parse SafeTensors format
        // For now, just return the raw data
        Ok(weights_data)
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
    fn estimate_adapter_size(_adapter_name: &str) -> usize {
        // Simplified: assume 16MB per adapter
        // In reality, calculate based on rank * target_modules * model_dim
        16 * 1024 * 1024
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

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert(
            "test_adapter".to_string(),
            B3Hash::hash(b"fake adapter data"),
        );
        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

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
    fn test_loader_hash_mismatch() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader_mismatch");
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_path = temp_dir.join("test_adapter.safetensors");
        fs::write(&adapter_path, b"fake adapter data").expect("Test file write should succeed");

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("test_adapter".to_string(), B3Hash::hash(b"different data"));

        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        match loader.load_adapter(0, "test_adapter") {
            Err(AosError::AdapterHashMismatch {
                expected,
                actual,
                adapter_id,
            }) => {
                assert_eq!(adapter_id, "test_adapter");
                assert_eq!(expected, B3Hash::hash(b"different data"));
                assert_eq!(actual, B3Hash::hash(b"fake adapter data"));
            }
            Err(e) => panic!("Unexpected error: {}", e),
            Ok(_) => panic!("Expected hash mismatch error"),
        }

        assert!(!loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 0);

        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }
}
