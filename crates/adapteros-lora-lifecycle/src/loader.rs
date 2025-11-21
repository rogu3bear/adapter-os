//! Hot-swap adapter loading and unloading

use adapteros_core::{AosError, B3Hash, Result};
use memmap2::Mmap;
use safetensors::SafeTensors;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use zeroize::Zeroize;

/// Loaded adapter weights with zeroize-on-drop
struct LoadedWeights {
    /// Raw weight data
    data: Vec<u8>,
    /// Memory-mapped file (kept alive for zero-copy access)
    _mmap: Option<Arc<Mmap>>,
}

impl Drop for LoadedWeights {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

/// Adapter metadata parsed from SafeTensors
#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    /// Total number of parameters
    pub num_parameters: usize,
    /// LoRA rank (if detectable)
    pub rank: Option<usize>,
    /// Target modules (detected from tensor names)
    pub target_modules: Vec<String>,
}

/// Adapter loader for hot-swap operations
pub struct AdapterLoader {
    /// Base path for adapter files
    base_path: PathBuf,
    /// Currently loaded adapters (adapter_id -> (path, weights))
    loaded: HashMap<u16, (PathBuf, LoadedWeights)>,
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

        // Load and parse SafeTensors file
        let (weights_data, metadata) = self.load_and_parse_safetensors(&adapter_path)?;
        let expected_hash = self.expected_hash(adapter_name)?;
        let actual_hash = B3Hash::hash(&weights_data.data);

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

        let memory_bytes = Self::calculate_memory_bytes(&metadata, weights_data.data.len());
        self.loaded.insert(adapter_id, (adapter_path.clone(), weights_data));

        tracing::info!(
            adapter_id = adapter_id,
            adapter_name = adapter_name,
            path = %adapter_path.display(),
            memory_bytes = memory_bytes,
            num_parameters = metadata.num_parameters,
            rank = ?metadata.rank,
            "Loaded adapter"
        );

        Ok(AdapterHandle {
            adapter_id,
            path: adapter_path,
            memory_bytes,
            metadata,
        })
    }

    /// Load an adapter asynchronously using spawn_blocking
    pub async fn load_adapter_async(
        &mut self,
        adapter_id: u16,
        adapter_name: &str,
    ) -> Result<AdapterHandle> {
        let base_path = self.base_path.clone();
        let expected_hash = self.expected_hash(adapter_name)?;
        let adapter_name_owned = adapter_name.to_string();

        let (handle, weights_data) = tokio::task::spawn_blocking(move || {
            let adapter_path = base_path.join(format!("{}.safetensors", adapter_name_owned));

            if !adapter_path.exists() {
                return Err(AosError::Lifecycle(format!(
                    "Adapter file not found: {}",
                    adapter_path.display()
                )));
            }

            // Open and memory-map the file
            let file = File::open(&adapter_path)
                .map_err(|e| AosError::Lifecycle(format!("Failed to open adapter file: {}", e)))?;

            let mmap = unsafe { Mmap::map(&file) }
                .map_err(|e| AosError::Lifecycle(format!("Failed to mmap adapter file: {}", e)))?;

            let mmap = Arc::new(mmap);

            // Parse SafeTensors to extract metadata
            let tensors = SafeTensors::deserialize(&mmap)
                .map_err(|e| AosError::Lifecycle(format!("Failed to parse SafeTensors: {}", e)))?;

            let metadata = AdapterLoader::extract_metadata(&tensors);

            // Read data for hashing (mmap gives us zero-copy access)
            let weights_data = mmap.to_vec();
            let actual_hash = B3Hash::hash(&weights_data);

            if actual_hash != expected_hash {
                tracing::error!(
                    "Adapter hash mismatch for {} (expected {}, got {})",
                    adapter_name_owned,
                    expected_hash,
                    actual_hash
                );
                return Err(AosError::AdapterHashMismatch {
                    adapter_id: adapter_name_owned.clone(),
                    expected: expected_hash,
                    actual: actual_hash,
                });
            }

            let memory_bytes = AdapterLoader::calculate_memory_bytes(&metadata, weights_data.len());

            let loaded_weights = LoadedWeights {
                data: weights_data,
                _mmap: Some(mmap),
            };

            tracing::info!(
                adapter_id = adapter_id,
                adapter_name = adapter_name_owned,
                path = %adapter_path.display(),
                memory_bytes = memory_bytes,
                num_parameters = metadata.num_parameters,
                rank = ?metadata.rank,
                "Loaded adapter async"
            );

            Ok((
                AdapterHandle {
                    adapter_id,
                    path: adapter_path,
                    memory_bytes,
                    metadata,
                },
                loaded_weights,
            ))
        })
        .await
        .map_err(|e| AosError::Lifecycle(format!("Failed to spawn load task: {}", e)))??;

        // Update internal state
        self.loaded.insert(adapter_id, (handle.path.clone(), weights_data));

        Ok(handle)
    }

    /// Unload an adapter from memory
    ///
    /// This removes the adapter from the loaded map and zeroizes the weights
    /// via the LoadedWeights drop implementation.
    pub fn unload_adapter(&mut self, adapter_id: u16) -> Result<()> {
        if let Some((path, _weights)) = self.loaded.remove(&adapter_id) {
            // Weights are automatically zeroized when dropped
            tracing::info!(
                adapter_id = adapter_id,
                path = %path.display(),
                "Unloaded adapter (weights zeroized)"
            );
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter {} not loaded",
                adapter_id
            )))
        }
    }

    /// Check if adapter is loaded
    pub fn is_loaded(&self, adapter_id: u16) -> bool {
        self.loaded.contains_key(&adapter_id)
    }

    /// Get number of loaded adapters
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Load and parse SafeTensors file, returning weights and metadata
    fn load_and_parse_safetensors(&self, adapter_path: &PathBuf) -> Result<(LoadedWeights, AdapterMetadata)> {
        // Open and memory-map the file for efficient reading
        let file = File::open(adapter_path)
            .map_err(|e| AosError::Lifecycle(format!("Failed to open adapter file: {}", e)))?;

        let mmap = unsafe { Mmap::map(&file) }
            .map_err(|e| AosError::Lifecycle(format!("Failed to mmap adapter file: {}", e)))?;

        let mmap = Arc::new(mmap);

        // Parse SafeTensors to extract metadata
        let tensors = SafeTensors::deserialize(&mmap)
            .map_err(|e| AosError::Lifecycle(format!("Failed to parse SafeTensors: {}", e)))?;

        let metadata = Self::extract_metadata(&tensors);

        // Keep data in memory for hashing and potential GPU upload
        let weights_data = mmap.to_vec();

        Ok((
            LoadedWeights {
                data: weights_data,
                _mmap: Some(mmap),
            },
            metadata,
        ))
    }

    /// Extract metadata from parsed SafeTensors
    fn extract_metadata(tensors: &SafeTensors) -> AdapterMetadata {
        let mut num_parameters = 0usize;
        let mut target_modules = Vec::new();
        let mut detected_rank: Option<usize> = None;

        for (name, tensor_view) in tensors.tensors() {
            let shape = tensor_view.shape();
            let tensor_params: usize = shape.iter().product();
            num_parameters += tensor_params;

            // Detect target modules from tensor names (e.g., "lora_A.q_proj")
            if name.contains("lora_A") || name.contains("lora_B") {
                // Extract module name
                let module_name = name
                    .replace("lora_A.", "")
                    .replace("lora_B.", "")
                    .replace(".weight", "");
                if !target_modules.contains(&module_name) {
                    target_modules.push(module_name);
                }

                // Detect LoRA rank from lora_A shape [rank, hidden_dim]
                // or lora_B shape [hidden_dim, rank]
                if name.contains("lora_A") && shape.len() >= 2 {
                    detected_rank = Some(shape[0]);
                } else if name.contains("lora_B") && shape.len() >= 2 {
                    detected_rank = Some(shape[1]);
                }
            }
        }

        AdapterMetadata {
            num_parameters,
            rank: detected_rank,
            target_modules,
        }
    }

    /// Calculate memory usage based on metadata and raw data size
    fn calculate_memory_bytes(metadata: &AdapterMetadata, raw_size: usize) -> usize {
        // Base memory is the raw file size
        let base_memory = raw_size;

        // Add overhead for:
        // - Parsed tensor structures (~10%)
        // - GPU buffer alignment padding
        // - Metadata and indices
        let overhead_factor = 1.15;

        let estimated = (base_memory as f64 * overhead_factor) as usize;

        tracing::debug!(
            raw_size = raw_size,
            estimated = estimated,
            num_parameters = metadata.num_parameters,
            "Calculated adapter memory usage"
        );

        estimated
    }

    /// Get raw weight data for an adapter (for GPU upload)
    pub fn get_weights(&self, adapter_id: u16) -> Option<&[u8]> {
        self.loaded.get(&adapter_id).map(|(_, weights)| weights.data.as_slice())
    }
}

/// Handle to a loaded adapter
#[derive(Debug, Clone)]
pub struct AdapterHandle {
    pub adapter_id: u16,
    pub path: PathBuf,
    pub memory_bytes: usize,
    pub metadata: AdapterMetadata,
}

impl AdapterHandle {
    /// Get memory footprint in bytes
    pub fn memory_bytes(&self) -> usize {
        self.memory_bytes
    }

    /// Get LoRA rank if detected
    pub fn rank(&self) -> Option<usize> {
        self.metadata.rank
    }

    /// Get number of parameters
    pub fn num_parameters(&self) -> usize {
        self.metadata.num_parameters
    }

    /// Get target modules
    pub fn target_modules(&self) -> &[String] {
        &self.metadata.target_modules
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safetensors::tensor::serialize;
    use std::fs;

    /// Create a valid SafeTensors file with test data
    fn create_test_safetensors(path: &std::path::Path) -> Vec<u8> {
        use std::collections::HashMap as StdHashMap;

        // Create simple LoRA-style tensors
        let lora_a_data: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4]; // rank=2, dim=2
        let lora_b_data: Vec<f32> = vec![0.5, 0.6, 0.7, 0.8]; // dim=2, rank=2

        let lora_a_bytes: Vec<u8> = lora_a_data
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let lora_b_bytes: Vec<u8> = lora_b_data
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let mut tensors = StdHashMap::new();
        tensors.insert(
            "lora_A.q_proj.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_a_bytes,
            )
            .expect("Test TensorView creation should succeed"),
        );
        tensors.insert(
            "lora_B.q_proj.weight".to_string(),
            safetensors::tensor::TensorView::new(
                safetensors::Dtype::F32,
                vec![2, 2],
                &lora_b_bytes,
            )
            .expect("Test TensorView creation should succeed"),
        );

        let serialized = serialize(tensors, &None).expect("Test SafeTensors serialization should succeed");
        fs::write(path, &serialized).expect("Test file write should succeed");
        serialized
    }

    #[test]
    fn test_loader_basic() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        // Create a valid SafeTensors adapter file
        let adapter_path = temp_dir.join("test_adapter.safetensors");
        let serialized = create_test_safetensors(&adapter_path);

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert(
            "test_adapter".to_string(),
            B3Hash::hash(&serialized),
        );
        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        // Load adapter
        let handle = loader
            .load_adapter(0, "test_adapter")
            .expect("Test adapter load should succeed");
        assert_eq!(handle.adapter_id, 0);
        assert!(loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 1);

        // Verify metadata was extracted
        assert_eq!(handle.metadata.num_parameters, 8); // 4 + 4 parameters
        assert_eq!(handle.metadata.rank, Some(2));
        assert!(handle.metadata.target_modules.contains(&"q_proj".to_string()));

        // Verify we can get weights
        assert!(loader.get_weights(0).is_some());

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
        let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let adapter_path = temp_dir.join("test_adapter.safetensors");
        let serialized = create_test_safetensors(&adapter_path);

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
                assert_eq!(actual, B3Hash::hash(&serialized));
            }
            Err(e) => panic!("Unexpected error: {}", e),
            Ok(_) => panic!("Expected hash mismatch error"),
        }

        assert!(!loader.is_loaded(0));
        assert_eq!(loader.loaded_count(), 0);

        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_loader_file_not_found() {
        let temp_dir = std::env::temp_dir().join("mplora_test_loader_not_found");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let mut expected_hashes = HashMap::new();
        expected_hashes.insert("missing_adapter".to_string(), B3Hash::hash(b"data"));

        let mut loader = AdapterLoader::new(temp_dir.clone(), expected_hashes);

        match loader.load_adapter(0, "missing_adapter") {
            Err(AosError::Lifecycle(msg)) => {
                assert!(msg.contains("not found"));
            }
            Err(e) => panic!("Unexpected error type: {}", e),
            Ok(_) => panic!("Expected file not found error"),
        }

        fs::remove_dir_all(temp_dir).expect("Test cleanup should succeed");
    }

    #[test]
    fn test_unload_not_loaded() {
        let temp_dir = std::env::temp_dir().join("mplora_test_unload_not_loaded");
        fs::create_dir_all(&temp_dir).expect("Test temp directory creation should succeed");

        let mut loader = AdapterLoader::new(temp_dir.clone(), HashMap::new());

        match loader.unload_adapter(99) {
            Err(AosError::Lifecycle(msg)) => {
                assert!(msg.contains("not loaded"));
            }
            Err(e) => panic!("Unexpected error type: {}", e),
            Ok(_) => panic!("Expected not loaded error"),
        }

        let _ = fs::remove_dir_all(temp_dir);
    }
}
