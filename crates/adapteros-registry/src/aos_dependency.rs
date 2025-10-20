//! Dependency resolution for delta adapters
//!
//! Resolves adapter dependency chains and validates all required parents are available.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_single_file_adapter::{SingleFileAdapter, SingleFileAdapterLoader};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::aos_store::AosStore;

/// Dependency chain resolver for .aos files
pub struct AosDependencyResolver {
    aos_store: Arc<AosStore>,
    /// Cache of resolved chains: hash -> full chain
    chain_cache: parking_lot::RwLock<HashMap<B3Hash, Vec<B3Hash>>>,
}

impl AosDependencyResolver {
    /// Create new resolver
    pub fn new(aos_store: Arc<AosStore>) -> Self {
        Self {
            aos_store,
            chain_cache: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// Resolve adapter and all parents (returns chain from base -> current)
    pub async fn resolve_chain(&self, aos_hash: &B3Hash) -> Result<Vec<B3Hash>> {
        // Check cache first
        {
            let cache = self.chain_cache.read();
            if let Some(chain) = cache.get(aos_hash) {
                debug!("Cache hit for dependency chain: {}", aos_hash.to_hex());
                return Ok(chain.clone());
            }
        }

        // Build chain by following parent links
        let mut chain = Vec::new();
        let mut current = *aos_hash;
        let mut visited = HashSet::new();

        loop {
            // Detect cycles
            if !visited.insert(current) {
                return Err(AosError::Dependency(format!(
                    "Circular dependency detected at {}",
                    current.to_hex()
                )));
            }

            // Add current to chain
            chain.push(current);

            // Get adapter to check for parent
            let aos_path = self.aos_store.get(&current)?;
            let adapter = SingleFileAdapterLoader::load(&aos_path).await?;

            // Check for parent
            if let Some(parent_hash_str) = &adapter.lineage.parent_hash {
                let parent_hash = B3Hash::from_hex(parent_hash_str).map_err(|e| {
                    AosError::Dependency(format!("Invalid parent hash: {}", e))
                })?;
                current = parent_hash;
            } else {
                // Reached base adapter
                break;
            }
        }

        // Reverse to get base -> current order
        chain.reverse();

        // Cache result
        {
            let mut cache = self.chain_cache.write();
            cache.insert(*aos_hash, chain.clone());
        }

        info!(
            "Resolved dependency chain for {}: {} adapters",
            aos_hash.to_hex(),
            chain.len()
        );

        Ok(chain)
    }

    /// Check if all dependencies are available in store
    pub async fn check_available(&self, aos_hash: &B3Hash) -> Result<DependencyCheckResult> {
        let chain = self.resolve_chain(aos_hash).await?;
        let mut missing = Vec::new();
        let mut available = Vec::new();

        for hash in &chain {
            if self.aos_store.exists(hash) {
                available.push(*hash);
            } else {
                missing.push(*hash);
            }
        }

        let all_available = missing.is_empty();

        if all_available {
            Ok(DependencyCheckResult {
                all_available: true,
                chain: chain.clone(),
                missing: vec![],
                available: chain,
            })
        } else {
            Err(AosError::Dependency(format!(
                "Missing {} parent adapters: {:?}",
                missing.len(),
                missing
                    .iter()
                    .map(|h| h.to_hex())
                    .collect::<Vec<_>>()
            )))
        }
    }

    /// Get dependency tree (for visualization)
    pub async fn get_dependency_tree(&self, aos_hash: &B3Hash) -> Result<DependencyTree> {
        let aos_path = self.aos_store.get(aos_hash)?;
        let adapter = SingleFileAdapterLoader::load(&aos_path).await?;

        let mut children = Vec::new();

        // Find all adapters that depend on this one
        for metadata in self.aos_store.list_all() {
            if let Some(parent_hash) = metadata.parent_hash {
                if parent_hash == *aos_hash {
                    // Recursively get children
                    let child_tree = Box::new(self.get_dependency_tree(&metadata.manifest_hash).await?);
                    children.push(child_tree);
                }
            }
        }

        Ok(DependencyTree {
            hash: *aos_hash,
            adapter_id: adapter.manifest.adapter_id.clone(),
            version: adapter.manifest.version.clone(),
            parent: adapter
                .lineage
                .parent_hash
                .as_ref()
                .and_then(|h| B3Hash::from_hex(h).ok()),
            children,
        })
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.chain_cache.write().clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.chain_cache.read();
        CacheStats {
            entries: cache.len(),
            total_chains: cache.values().map(|v| v.len()).sum(),
        }
    }
}

/// Result of dependency check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyCheckResult {
    pub all_available: bool,
    pub chain: Vec<B3Hash>,
    pub missing: Vec<B3Hash>,
    pub available: Vec<B3Hash>,
}

/// Dependency tree structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyTree {
    pub hash: B3Hash,
    pub adapter_id: String,
    pub version: String,
    pub parent: Option<B3Hash>,
    pub children: Vec<Box<DependencyTree>>,
}

impl DependencyTree {
    /// Get all hashes in tree
    pub fn all_hashes(&self) -> Vec<B3Hash> {
        let mut hashes = vec![self.hash];
        for child in &self.children {
            hashes.extend(child.all_hashes());
        }
        hashes
    }

    /// Get depth of tree
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }

    /// Count total adapters in tree
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_chains: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_single_file_adapter::{
        LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig,
    };
    use std::collections::HashMap as StdHashMap;
    use tempfile::TempDir;

    async fn create_test_adapter(
        store_dir: &std::path::Path,
        adapter_id: &str,
        parent_hash: Option<String>,
    ) -> B3Hash {
        let aos_path = store_dir.join(format!("{}.aos", adapter_id));

        let adapter = SingleFileAdapter::create(
            adapter_id.to_string(),
            vec![1, 2, 3],
            vec![],
            TrainingConfig::default(),
            LineageInfo {
                adapter_id: adapter_id.to_string(),
                version: "1.0.0".to_string(),
                parent_version: None,
                parent_hash,
                mutations: vec![],
                quality_delta: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();

        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        // Store in AOS store
        let store = AosStore::new(store_dir.join("store")).await.unwrap();
        store.store(&aos_path).await.unwrap()
    }

    #[tokio::test]
    async fn test_dependency_chain() {
        let temp_dir = TempDir::new().unwrap();

        // Create base adapter
        let base_hash = create_test_adapter(temp_dir.path(), "base", None).await;

        // Create child adapter
        let child_hash =
            create_test_adapter(temp_dir.path(), "child", Some(base_hash.to_hex())).await;

        // Create resolver
        let store = Arc::new(
            AosStore::new(temp_dir.path().join("store"))
                .await
                .unwrap(),
        );
        let resolver = AosDependencyResolver::new(store);

        // Resolve chain
        let chain = resolver.resolve_chain(&child_hash).await.unwrap();

        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], base_hash);
        assert_eq!(chain[1], child_hash);
    }

    #[tokio::test]
    async fn test_dependency_check() {
        let temp_dir = TempDir::new().unwrap();

        let base_hash = create_test_adapter(temp_dir.path(), "base", None).await;
        let child_hash =
            create_test_adapter(temp_dir.path(), "child", Some(base_hash.to_hex())).await;

        let store = Arc::new(
            AosStore::new(temp_dir.path().join("store"))
                .await
                .unwrap(),
        );
        let resolver = AosDependencyResolver::new(store);

        // All should be available
        let result = resolver.check_available(&child_hash).await.unwrap();
        assert!(result.all_available);
        assert_eq!(result.chain.len(), 2);
    }
}

