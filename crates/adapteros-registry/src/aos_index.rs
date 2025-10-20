//! Fast in-memory AOS manifest index for sub-millisecond lookups
//!
//! Provides LRU-cached manifests and category/tag indexes for fast queries.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_single_file_adapter::{AdapterManifest, SingleFileAdapterLoader};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

use super::aos_store::{AosMetadata, AosStore};

/// Fast in-memory AOS manifest index
pub struct AosIndex {
    /// adapter_id -> latest aos_hash
    by_id: Arc<RwLock<HashMap<String, B3Hash>>>,
    /// category -> [aos_hash]
    by_category: Arc<RwLock<HashMap<String, Vec<B3Hash>>>>,
    /// aos_hash -> cached manifest (LRU, max 1000 entries)
    manifests: Arc<RwLock<LruCache<B3Hash, AdapterManifest>>>,
    /// version index: (adapter_id, version) -> aos_hash
    by_version: Arc<RwLock<HashMap<(String, String), B3Hash>>>,
}

impl AosIndex {
    /// Create new empty index
    pub fn new() -> Self {
        Self {
            by_id: Arc::new(RwLock::new(HashMap::new())),
            by_category: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(1000).unwrap(),
            ))),
            by_version: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add manifest to index
    pub fn add(&self, aos_hash: B3Hash, manifest: AdapterManifest) {
        // Update by_id
        {
            let mut by_id = self.by_id.write().unwrap();
            by_id.insert(manifest.adapter_id.clone(), aos_hash);
        }

        // Update by_category
        {
            let mut by_category = self.by_category.write().unwrap();
            by_category
                .entry(manifest.category.clone())
                .or_insert_with(Vec::new)
                .push(aos_hash);
        }

        // Update by_version
        {
            let mut by_version = self.by_version.write().unwrap();
            by_version.insert(
                (manifest.adapter_id.clone(), manifest.version.clone()),
                aos_hash,
            );
        }

        // Cache manifest
        {
            let mut manifests = self.manifests.write().unwrap();
            manifests.put(aos_hash, manifest);
        }
    }

    /// Remove manifest from index
    pub fn remove(&self, aos_hash: &B3Hash) {
        // Remove from cache first to get manifest
        let manifest = {
            let mut manifests = self.manifests.write().unwrap();
            manifests.pop(aos_hash)
        };

        if let Some(manifest) = manifest {
            // Remove from by_id
            {
                let mut by_id = self.by_id.write().unwrap();
                if let Some(current_hash) = by_id.get(&manifest.adapter_id) {
                    if current_hash == aos_hash {
                        by_id.remove(&manifest.adapter_id);
                    }
                }
            }

            // Remove from by_category
            {
                let mut by_category = self.by_category.write().unwrap();
                if let Some(hashes) = by_category.get_mut(&manifest.category) {
                    hashes.retain(|h| h != aos_hash);
                }
            }

            // Remove from by_version
            {
                let mut by_version = self.by_version.write().unwrap();
                by_version.remove(&(manifest.adapter_id.clone(), manifest.version.clone()));
            }
        }
    }

    /// Get manifest by hash (from cache)
    pub fn get_manifest(&self, aos_hash: &B3Hash) -> Option<AdapterManifest> {
        self.manifests.write().unwrap().get(aos_hash).cloned()
    }

    /// Resolve adapter_id to latest hash
    pub fn resolve(&self, adapter_id: &str) -> Option<B3Hash> {
        self.by_id.read().unwrap().get(adapter_id).copied()
    }

    /// Resolve specific version
    pub fn resolve_version(&self, adapter_id: &str, version: &str) -> Option<B3Hash> {
        self.by_version
            .read()
            .unwrap()
            .get(&(adapter_id.to_string(), version.to_string()))
            .copied()
    }

    /// Query adapters by category
    pub fn query_by_category(&self, category: &str) -> Vec<B3Hash> {
        self.by_category
            .read()
            .unwrap()
            .get(category)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all categories
    pub fn list_categories(&self) -> Vec<String> {
        self.by_category.read().unwrap().keys().cloned().collect()
    }

    /// Get all adapter IDs
    pub fn list_adapter_ids(&self) -> Vec<String> {
        self.by_id.read().unwrap().keys().cloned().collect()
    }

    /// Get category statistics
    pub fn category_stats(&self) -> Vec<CategoryStats> {
        let by_category = self.by_category.read().unwrap();
        by_category
            .iter()
            .map(|(category, hashes)| CategoryStats {
                category: category.clone(),
                adapter_count: hashes.len(),
            })
            .collect()
    }

    /// Rebuild index from AOS store
    pub async fn rebuild(&self, aos_store: &AosStore) -> Result<()> {
        info!("Rebuilding AOS index from store...");

        let mut by_id = HashMap::new();
        let mut by_category: HashMap<String, Vec<B3Hash>> = HashMap::new();
        let mut by_version = HashMap::new();
        let mut manifests = LruCache::new(NonZeroUsize::new(1000).unwrap());
        let mut count = 0;

        // Scan all stored AOS files
        for metadata in aos_store.list_all() {
            let aos_path = aos_store.get(&metadata.manifest_hash)?;

            // Load manifest
            match SingleFileAdapterLoader::load_manifest_only(&aos_path).await {
                Ok(manifest) => {
                    by_id.insert(manifest.adapter_id.clone(), metadata.manifest_hash);
                    by_category
                        .entry(manifest.category.clone())
                        .or_insert_with(Vec::new)
                        .push(metadata.manifest_hash);
                    by_version.insert(
                        (manifest.adapter_id.clone(), manifest.version.clone()),
                        metadata.manifest_hash,
                    );
                    manifests.put(metadata.manifest_hash, manifest);
                    count += 1;
                }
                Err(e) => {
                    debug!(
                        "Failed to load manifest for {}: {}",
                        metadata.manifest_hash.to_hex(),
                        e
                    );
                }
            }
        }

        *self.by_id.write().unwrap() = by_id;
        *self.by_category.write().unwrap() = by_category;
        *self.by_version.write().unwrap() = by_version;
        *self.manifests.write().unwrap() = manifests;

        info!("Rebuilt index with {} manifests", count);
        Ok(())
    }

    /// Get index statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_adapters: self.by_id.read().unwrap().len(),
            cached_manifests: self.manifests.write().unwrap().len(),
            categories: self.by_category.read().unwrap().len(),
            versions: self.by_version.read().unwrap().len(),
        }
    }
}

impl Default for AosIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Category statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub category: String,
    pub adapter_count: usize,
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_adapters: usize,
    pub cached_manifests: usize,
    pub categories: usize,
    pub versions: usize,
}

/// Query builder for complex index queries
pub struct AosQuery<'a> {
    index: &'a AosIndex,
    category_filter: Option<String>,
    id_filter: Option<String>,
    version_filter: Option<String>,
}

impl<'a> AosQuery<'a> {
    /// Create new query
    pub fn new(index: &'a AosIndex) -> Self {
        Self {
            index,
            category_filter: None,
            id_filter: None,
            version_filter: None,
        }
    }

    /// Filter by category
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category_filter = Some(category.into());
        self
    }

    /// Filter by adapter ID
    pub fn adapter_id(mut self, id: impl Into<String>) -> Self {
        self.id_filter = Some(id.into());
        self
    }

    /// Filter by version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version_filter = Some(version.into());
        self
    }

    /// Execute query and return matching hashes
    pub fn execute(&self) -> Vec<B3Hash> {
        // Start with all or category-filtered
        let mut results = if let Some(category) = &self.category_filter {
            self.index.query_by_category(category)
        } else {
            self.index
                .by_id
                .read()
                .unwrap()
                .values()
                .copied()
                .collect()
        };

        // Apply ID filter
        if let Some(id) = &self.id_filter {
            if let Some(hash) = self.index.resolve(id) {
                results.retain(|h| h == &hash);
            } else {
                results.clear();
            }
        }

        // Apply version filter
        if let Some(version) = &self.version_filter {
            if let Some(id) = &self.id_filter {
                if let Some(hash) = self.index.resolve_version(id, version) {
                    results.retain(|h| h == &hash);
                } else {
                    results.clear();
                }
            }
        }

        results
    }

    /// Execute query and return manifests
    pub fn execute_with_manifests(&self) -> Vec<(B3Hash, Option<AdapterManifest>)> {
        self.execute()
            .into_iter()
            .map(|hash| {
                let manifest = self.index.get_manifest(&hash);
                (hash, manifest)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig, TrainingExample};
    use std::collections::HashMap as StdHashMap;
    use tempfile::TempDir;

    async fn create_test_adapter(id: &str, category: &str, version: &str) -> (PathBuf, AdapterManifest) {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join(format!("{}.aos", id));

        let adapter = SingleFileAdapter::create(
            id.to_string(),
            vec![1, 2, 3],
            vec![],
            TrainingConfig::default(),
            LineageInfo {
                adapter_id: id.to_string(),
                version: version.to_string(),
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

        let manifest = SingleFileAdapterLoader::load_manifest_only(&aos_path)
            .await
            .unwrap();

        (aos_path, manifest)
    }

    #[tokio::test]
    async fn test_index_basic() {
        let index = AosIndex::new();
        let (_, manifest) = create_test_adapter("test", "code", "1.0.0").await;

        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let hash = B3Hash::hash(&manifest_bytes);

        index.add(hash, manifest.clone());

        // Resolve by ID
        assert_eq!(index.resolve("test"), Some(hash));

        // Get manifest
        let cached = index.get_manifest(&hash).unwrap();
        assert_eq!(cached.adapter_id, "test");
    }

    #[tokio::test]
    async fn test_index_category_query() {
        let index = AosIndex::new();

        // Add multiple adapters
        for i in 0..3 {
            let (_, manifest) = create_test_adapter(&format!("adapter_{}", i), "code", "1.0.0").await;
            let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
            let hash = B3Hash::hash(&manifest_bytes);
            index.add(hash, manifest);
        }

        let code_adapters = index.query_by_category("code");
        assert_eq!(code_adapters.len(), 3);
    }

    #[tokio::test]
    async fn test_query_builder() {
        let index = AosIndex::new();
        let (_, manifest) = create_test_adapter("test_query", "code", "1.0.0").await;

        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let hash = B3Hash::hash(&manifest_bytes);
        index.add(hash, manifest);

        // Query by category
        let results = AosQuery::new(&index)
            .category("code")
            .adapter_id("test_query")
            .execute();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], hash);
    }
}

