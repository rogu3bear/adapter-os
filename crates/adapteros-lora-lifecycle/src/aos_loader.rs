//! Direct .aos file loading with memory-mapping for hot-swap
//!
//! Loads .aos files directly from the AOS store without unpacking,
//! using memory-mapped I/O for efficient weight access.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::AosStore;
use adapteros_single_file_adapter::{AdapterManifest, LoadOptions, SingleFileAdapterLoader};
use memmap2::{Mmap, MmapOptions};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

/// Handle to memory-mapped .aos file
pub struct AosMmapHandle {
    /// Content-addressable hash
    pub aos_hash: B3Hash,
    /// Adapter manifest
    pub manifest: AdapterManifest,
    /// Memory-mapped file
    mmap: Option<Mmap>,
    /// File path (for re-mapping if needed)
    file_path: PathBuf,
    /// Estimated memory usage
    pub memory_bytes: usize,
}

impl AosMmapHandle {
    /// Open .aos file with memory-mapping
    pub fn open(aos_hash: B3Hash, aos_store: &AosStore) -> Result<Self> {
        let file_path = aos_store.get(&aos_hash)?;
        let file = File::open(&file_path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        // Memory-map the file
        let mmap = unsafe {
            MmapOptions::new()
                .map(&file)
                .map_err(|e| AosError::Io(format!("Failed to memory-map .aos file: {}", e)))?
        };

        let memory_bytes = mmap.len();

        // Load manifest only (fast, doesn't decompress full file)
        let manifest = tokio::runtime::Handle::current()
            .block_on(async { SingleFileAdapterLoader::load_manifest_only(&file_path).await })?;

        info!(
            "Memory-mapped .aos {}: {} v{} ({} bytes)",
            aos_hash.to_hex(),
            manifest.adapter_id,
            manifest.version,
            memory_bytes
        );

        Ok(Self {
            aos_hash,
            manifest,
            mmap: Some(mmap),
            file_path,
            memory_bytes,
        })
    }

    /// Get manifest
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    /// Check if memory-mapped
    pub fn is_mapped(&self) -> bool {
        self.mmap.is_some()
    }

    /// Unmap from memory (for eviction)
    pub fn unmap(&mut self) {
        if self.mmap.take().is_some() {
            debug!("Unmapped .aos {}", self.aos_hash.to_hex());
        }
    }

    /// Re-map if unmapped
    pub fn remap(&mut self) -> Result<()> {
        if self.mmap.is_some() {
            return Ok(());
        }

        let file = File::open(&self.file_path)
            .map_err(|e| AosError::Io(format!("Failed to reopen .aos file: {}", e)))?;

        let mmap = unsafe {
            MmapOptions::new()
                .map(&file)
                .map_err(|e| AosError::Io(format!("Failed to remap .aos file: {}", e)))?
        };

        self.memory_bytes = mmap.len();
        self.mmap = Some(mmap);

        debug!("Remapped .aos {}", self.aos_hash.to_hex());
        Ok(())
    }

    /// Load full adapter (decompresses, slower)
    pub async fn load_full(&self) -> Result<adapteros_single_file_adapter::SingleFileAdapter> {
        SingleFileAdapterLoader::load(&self.file_path).await
    }

    /// Get raw bytes (for direct weight access)
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.mmap.as_ref().map(|m| m.as_ref())
    }
}

/// Manager for direct .aos loading with hot-swap support
pub struct AosDirectLoader {
    aos_store: Arc<AosStore>,
    /// Currently loaded handles: adapter_id -> handle
    handles: parking_lot::RwLock<std::collections::HashMap<String, AosMmapHandle>>,
}

impl AosDirectLoader {
    /// Create new AOS direct loader
    pub fn new(aos_store: Arc<AosStore>) -> Self {
        Self {
            aos_store,
            handles: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Load adapter by hash
    pub async fn load(&self, aos_hash: &B3Hash) -> Result<AosMmapHandle> {
        let handle = AosMmapHandle::open(*aos_hash, &self.aos_store)?;
        let adapter_id = handle.manifest.adapter_id.clone();

        // Store handle
        {
            let mut handles = self.handles.write();
            handles.insert(adapter_id, handle.clone());
        }

        Ok(handle)
    }

    /// Load adapter by ID (resolves to latest version)
    pub async fn load_by_id(&self, adapter_id: &str) -> Result<AosMmapHandle> {
        let aos_hash = self.aos_store.resolve(adapter_id)?;
        self.load(&aos_hash).await
    }

    /// Hot-swap: atomically replace adapter with new version
    pub async fn hot_swap(&self, adapter_id: &str, new_hash: &B3Hash) -> Result<HotSwapResult> {
        let start = std::time::Instant::now();

        // Load new version
        let new_handle = AosMmapHandle::open(*new_hash, &self.aos_store)?;

        // Atomic swap
        let old_hash = {
            let mut handles = self.handles.write();
            let old_handle = handles.insert(adapter_id.to_string(), new_handle);
            old_handle.map(|h| h.aos_hash)
        };

        let elapsed = start.elapsed();

        info!(
            "Hot-swapped adapter {}: {:?} -> {} in {:?}",
            adapter_id,
            old_hash.as_ref().map(|h| h.to_hex()),
            new_hash.to_hex(),
            elapsed
        );

        Ok(HotSwapResult {
            adapter_id: adapter_id.to_string(),
            old_hash,
            new_hash: *new_hash,
            swap_duration: elapsed,
        })
    }

    /// Get currently loaded adapter handle
    pub fn get(&self, adapter_id: &str) -> Option<AosMmapHandle> {
        let handles = self.handles.read();
        handles.get(adapter_id).cloned()
    }

    /// Unload adapter (unmap from memory)
    pub fn unload(&self, adapter_id: &str) -> Result<()> {
        let mut handles = self.handles.write();
        if let Some(mut handle) = handles.remove(adapter_id) {
            handle.unmap();
            info!("Unloaded adapter {}", adapter_id);
        }
        Ok(())
    }

    /// Get all loaded adapters
    pub fn list_loaded(&self) -> Vec<String> {
        let handles = self.handles.read();
        handles.keys().cloned().collect()
    }

    /// Get total memory usage
    pub fn total_memory(&self) -> usize {
        let handles = self.handles.read();
        handles.values().map(|h| h.memory_bytes).sum()
    }

    /// Get memory usage breakdown
    pub fn memory_breakdown(&self) -> Vec<(String, usize)> {
        let handles = self.handles.read();
        handles
            .iter()
            .map(|(id, h)| (id.clone(), h.memory_bytes))
            .collect()
    }
}

/// Result of hot-swap operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSwapResult {
    pub adapter_id: String,
    pub old_hash: Option<B3Hash>,
    pub new_hash: B3Hash,
    pub swap_duration: std::time::Duration,
}

// Implement Clone for AosMmapHandle (doesn't clone mmap, just metadata)
impl Clone for AosMmapHandle {
    fn clone(&self) -> Self {
        // Note: We don't clone the mmap, caller must remap if needed
        Self {
            aos_hash: self.aos_hash,
            manifest: self.manifest.clone(),
            mmap: None,
            file_path: self.file_path.clone(),
            memory_bytes: self.memory_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{
        LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    async fn create_test_aos(dir: &std::path::Path, adapter_id: &str) -> PathBuf {
        let aos_path = dir.join(format!("{}.aos", adapter_id));

        let adapter = SingleFileAdapter::create(
            adapter_id.to_string(),
            vec![1, 2, 3, 4, 5],
            vec![],
            TrainingConfig::default(),
            LineageInfo {
                adapter_id: adapter_id.to_string(),
                version: "1.0.0".to_string(),
                parent_version: None,
                parent_hash: None,
                mutations: vec![],
                quality_delta: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();

        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        aos_path
    }

    #[tokio::test]
    async fn test_mmap_loading() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let aos_store = Arc::new(AosStore::new(store_dir).await.unwrap());

        // Create and store adapter
        let aos_path = create_test_aos(temp_dir.path(), "test_adapter").await;
        let aos_hash = aos_store.store(&aos_path).await.unwrap();

        // Load with mmap
        let handle = AosMmapHandle::open(aos_hash, &aos_store).unwrap();

        assert_eq!(handle.manifest.adapter_id, "test_adapter");
        assert!(handle.is_mapped());
        assert!(handle.memory_bytes > 0);
    }

    #[tokio::test]
    async fn test_hot_swap() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let aos_store = Arc::new(AosStore::new(store_dir).await.unwrap());

        // Create v1
        let v1_path = create_test_aos(temp_dir.path(), "adapter").await;
        let v1_hash = aos_store.store(&v1_path).await.unwrap();

        // Create v2
        let v2_path = create_test_aos(temp_dir.path(), "adapter").await;
        let v2_hash = aos_store.store(&v2_path).await.unwrap();

        // Load v1
        let loader = AosDirectLoader::new(aos_store.clone());
        loader.load(&v1_hash).await.unwrap();

        // Hot-swap to v2
        let result = loader.hot_swap("adapter", &v2_hash).await.unwrap();

        assert_eq!(result.old_hash, Some(v1_hash));
        assert_eq!(result.new_hash, v2_hash);
        assert!(result.swap_duration.as_millis() < 100);
    }
}
