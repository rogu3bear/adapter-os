//! Content-addressable storage for .aos adapter files
//!
//! Provides Git-like storage where .aos files are stored by their manifest hash,
//! enabling deduplication, immutability, and fast lookups.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_single_file_adapter::{AdapterManifest, SingleFileAdapterLoader};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::fs;
use tracing::{debug, info, warn};

/// Metadata for stored .aos file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosMetadata {
    /// Manifest hash (content address)
    pub manifest_hash: B3Hash,
    /// Adapter ID from manifest
    pub adapter_id: String,
    /// Adapter version
    pub version: String,
    /// File size in bytes
    pub file_size: u64,
    /// Storage path (relative to base)
    pub storage_path: PathBuf,
    /// When stored
    pub stored_at: String,
    /// Format version
    pub format_version: u8,
    /// Whether signature is present and valid
    pub signature_valid: bool,
    /// Category (code, docs, etc.)
    pub category: String,
    /// Parent adapter hash (for delta adapters)
    pub parent_hash: Option<B3Hash>,
}

/// Content-addressable .aos storage
pub struct AosStore {
    /// Base storage path (e.g., /var/aos/store)
    base_path: PathBuf,
    /// In-memory index: manifest_hash -> metadata
    index: Arc<RwLock<HashMap<B3Hash, AosMetadata>>>,
    /// Reverse index: adapter_id -> latest hash
    id_index: Arc<RwLock<HashMap<String, B3Hash>>>,
}

impl AosStore {
    /// Create new AOS store
    pub async fn new(base_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_path).await
            .map_err(|e| AosError::Io(format!("Failed to create store directory: {}", e)))?;
        
        let store = Self {
            base_path,
            index: Arc::new(RwLock::new(HashMap::new())),
            id_index: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Rebuild index from disk
        store.rebuild_index().await?;
        
        Ok(store)
    }

    /// Store .aos file by its manifest hash
    pub async fn store(&self, aos_path: &Path) -> Result<B3Hash> {
        info!("Storing .aos file: {}", aos_path.display());
        
        // Load manifest to compute hash
        let manifest = SingleFileAdapterLoader::load_manifest_only(aos_path).await?;
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|e| AosError::Io(format!("Failed to serialize manifest: {}", e)))?;
        let manifest_hash = B3Hash::hash(&manifest_bytes);
        
        // Check if already stored
        if self.exists(&manifest_hash) {
            debug!("AOS {} already in store", manifest_hash.to_hex());
            return Ok(manifest_hash);
        }
        
        // Store at: store/<first_2_hex>/<full_hash>.aos
        let store_path = self.cas_path(&manifest_hash);
        fs::create_dir_all(store_path.parent().unwrap()).await
            .map_err(|e| AosError::Io(format!("Failed to create store subdirectory: {}", e)))?;
        
        fs::copy(aos_path, &store_path).await
            .map_err(|e| AosError::Io(format!("Failed to copy to store: {}", e)))?;
        
        // Get file size
        let file_size = fs::metadata(&store_path).await?.len();
        
        // Load full adapter to check signature
        let adapter = SingleFileAdapterLoader::load(&store_path).await?;
        let signature_valid = adapter.is_signed() && adapter.verify().unwrap_or(false);
        
        // Create metadata
        let metadata = AosMetadata {
            manifest_hash,
            adapter_id: manifest.adapter_id.clone(),
            version: manifest.version.clone(),
            file_size,
            storage_path: self.relative_path(&manifest_hash),
            stored_at: chrono::Utc::now().to_rfc3339(),
            format_version: manifest.format_version,
            signature_valid,
            category: manifest.category.clone(),
            parent_hash: adapter.lineage.parent_hash
                .as_ref()
                .and_then(|h| B3Hash::from_hex(h).ok()),
        };
        
        // Update indexes
        {
            let mut index = self.index.write().unwrap();
            index.insert(manifest_hash, metadata.clone());
        }
        {
            let mut id_index = self.id_index.write().unwrap();
            id_index.insert(manifest.adapter_id.clone(), manifest_hash);
        }
        
        info!(
            "Stored AOS {}: {} v{} ({} bytes)",
            manifest_hash.to_hex(),
            metadata.adapter_id,
            metadata.version,
            file_size
        );
        
        Ok(manifest_hash)
    }

    /// Retrieve .aos path by manifest hash
    pub fn get(&self, hash: &B3Hash) -> Result<PathBuf> {
        let path = self.cas_path(hash);
        if path.exists() {
            Ok(path)
        } else {
            Err(AosError::NotFound(format!(
                "AOS {} not found in store",
                hash.to_hex()
            )))
        }
    }

    /// Get metadata for stored .aos
    pub fn get_metadata(&self, hash: &B3Hash) -> Option<AosMetadata> {
        self.index.read().unwrap().get(hash).cloned()
    }

    /// Resolve adapter_id to latest .aos hash
    pub fn resolve(&self, adapter_id: &str) -> Result<B3Hash> {
        self.id_index
            .read()
            .unwrap()
            .get(adapter_id)
            .copied()
            .ok_or_else(|| {
                AosError::NotFound(format!("No AOS found for adapter_id: {}", adapter_id))
            })
    }

    /// Check if .aos exists in store
    pub fn exists(&self, hash: &B3Hash) -> bool {
        self.index.read().unwrap().contains_key(hash)
    }

    /// List all stored .aos files
    pub fn list_all(&self) -> Vec<AosMetadata> {
        self.index.read().unwrap().values().cloned().collect()
    }

    /// List .aos files by category
    pub fn list_by_category(&self, category: &str) -> Vec<AosMetadata> {
        self.index
            .read()
            .unwrap()
            .values()
            .filter(|m| m.category == category)
            .cloned()
            .collect()
    }

    /// Get all adapter IDs
    pub fn list_adapter_ids(&self) -> Vec<String> {
        self.id_index.read().unwrap().keys().cloned().collect()
    }

    /// Delete .aos from store
    pub async fn delete(&self, hash: &B3Hash) -> Result<()> {
        let path = self.cas_path(hash);
        if path.exists() {
            fs::remove_file(&path).await
                .map_err(|e| AosError::Io(format!("Failed to delete AOS: {}", e)))?;
        }
        
        // Remove from index
        let metadata = {
            let mut index = self.index.write().unwrap();
            index.remove(hash)
        };
        
        if let Some(metadata) = metadata {
            let mut id_index = self.id_index.write().unwrap();
            if let Some(current_hash) = id_index.get(&metadata.adapter_id) {
                if current_hash == hash {
                    id_index.remove(&metadata.adapter_id);
                }
            }
        }
        
        info!("Deleted AOS {}", hash.to_hex());
        Ok(())
    }

    /// Rebuild index from disk
    pub async fn rebuild_index(&self) -> Result<()> {
        info!("Rebuilding AOS store index from disk...");
        
        let mut index = HashMap::new();
        let mut id_index = HashMap::new();
        let mut count = 0;
        
        // Scan store directory
        let mut entries = fs::read_dir(&self.base_path).await
            .map_err(|e| AosError::Io(format!("Failed to read store directory: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))? 
        {
            let subdir = entry.path();
            if !subdir.is_dir() {
                continue;
            }
            
            // Scan subdirectory for .aos files
            let mut subdir_entries = fs::read_dir(&subdir).await
                .map_err(|e| AosError::Io(format!("Failed to read subdirectory: {}", e)))?;
            
            while let Some(file_entry) = subdir_entries.next_entry().await
                .map_err(|e| AosError::Io(format!("Failed to read file entry: {}", e)))? 
            {
                let file_path = file_entry.path();
                if file_path.extension().and_then(|s| s.to_str()) != Some("aos") {
                    continue;
                }
                
                // Load manifest
                match SingleFileAdapterLoader::load_manifest_only(&file_path).await {
                    Ok(manifest) => {
                        let manifest_bytes = serde_json::to_vec(&manifest)?;
                        let manifest_hash = B3Hash::hash(&manifest_bytes);
                        
                        // Verify filename matches hash
                        let expected_name = format!("{}.aos", manifest_hash.to_hex());
                        if file_path.file_name().unwrap().to_str().unwrap() != expected_name {
                            warn!(
                                "Filename mismatch: {:?} != {}",
                                file_path.file_name(),
                                expected_name
                            );
                            continue;
                        }
                        
                        let file_size = fs::metadata(&file_path).await?.len();
                        
                        // Load full adapter to check signature
                        let adapter = SingleFileAdapterLoader::load(&file_path).await?;
                        let signature_valid = adapter.is_signed() && adapter.verify().unwrap_or(false);
                        
                        let metadata = AosMetadata {
                            manifest_hash,
                            adapter_id: manifest.adapter_id.clone(),
                            version: manifest.version.clone(),
                            file_size,
                            storage_path: self.relative_path(&manifest_hash),
                            stored_at: chrono::Utc::now().to_rfc3339(),
                            format_version: manifest.format_version,
                            signature_valid,
                            category: manifest.category.clone(),
                            parent_hash: adapter.lineage.parent_hash
                                .as_ref()
                                .and_then(|h| B3Hash::from_hex(h).ok()),
                        };
                        
                        index.insert(manifest_hash, metadata.clone());
                        id_index.insert(metadata.adapter_id.clone(), manifest_hash);
                        count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to load AOS {:?}: {}", file_path, e);
                    }
                }
            }
        }
        
        *self.index.write().unwrap() = index;
        *self.id_index.write().unwrap() = id_index;
        
        info!("Rebuilt index with {} AOS files", count);
        Ok(())
    }

    /// Get content-addressed path for hash
    fn cas_path(&self, hash: &B3Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.base_path
            .join(&hex[..2])
            .join(format!("{}.aos", hex))
    }

    /// Get relative path for hash
    fn relative_path(&self, hash: &B3Hash) -> PathBuf {
        let hex = hash.to_hex();
        PathBuf::from(&hex[..2]).join(format!("{}.aos", hex))
    }

    /// Get storage statistics
    pub fn stats(&self) -> AosStoreStats {
        let index = self.index.read().unwrap();
        let total_size: u64 = index.values().map(|m| m.file_size).sum();
        let signed_count = index.values().filter(|m| m.signature_valid).count();
        
        AosStoreStats {
            total_adapters: index.len(),
            total_size_bytes: total_size,
            signed_adapters: signed_count,
            unique_adapter_ids: self.id_index.read().unwrap().len(),
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosStoreStats {
    pub total_adapters: usize,
    pub total_size_bytes: u64,
    pub signed_adapters: usize,
    pub unique_adapter_ids: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{
        LineageInfo, SingleFileAdapter, SingleFileAdapterPackager,
        TrainingConfig, TrainingExample,
    };
    use std::collections::HashMap as StdHashMap;
    use tempfile::TempDir;

    async fn create_test_aos(dir: &Path, adapter_id: &str) -> PathBuf {
        let aos_path = dir.join(format!("{}.aos", adapter_id));
        
        let adapter = SingleFileAdapter::create(
            adapter_id.to_string(),
            vec![1, 2, 3, 4, 5],
            vec![TrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: StdHashMap::new(),
                weight: 1.0,
            }],
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
    async fn test_aos_store_basic() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let store = AosStore::new(store_dir).await.unwrap();
        
        // Create test .aos
        let aos_path = create_test_aos(temp_dir.path(), "test_adapter").await;
        
        // Store it
        let hash = store.store(&aos_path).await.unwrap();
        
        // Retrieve it
        let retrieved_path = store.get(&hash).unwrap();
        assert!(retrieved_path.exists());
        
        // Check metadata
        let metadata = store.get_metadata(&hash).unwrap();
        assert_eq!(metadata.adapter_id, "test_adapter");
        assert_eq!(metadata.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_aos_store_resolve() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let store = AosStore::new(store_dir).await.unwrap();
        
        let aos_path = create_test_aos(temp_dir.path(), "resolve_test").await;
        let hash = store.store(&aos_path).await.unwrap();
        
        // Resolve by adapter_id
        let resolved_hash = store.resolve("resolve_test").unwrap();
        assert_eq!(resolved_hash, hash);
    }

    #[tokio::test]
    async fn test_aos_store_list() {
        let temp_dir = TempDir::new().unwrap();
        let store_dir = temp_dir.path().join("store");
        let store = AosStore::new(store_dir).await.unwrap();
        
        // Store multiple adapters
        for i in 0..3 {
            let aos_path = create_test_aos(temp_dir.path(), &format!("adapter_{}", i)).await;
            store.store(&aos_path).await.unwrap();
        }
        
        // List all
        let all = store.list_all();
        assert_eq!(all.len(), 3);
        
        // Check stats
        let stats = store.stats();
        assert_eq!(stats.total_adapters, 3);
        assert_eq!(stats.unique_adapter_ids, 3);
    }
}

