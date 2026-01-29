//! Adapter version repository
//!
//! Provides CRUD operations for adapter version metadata, supporting:
//! - Content-addressed version storage
//! - Lineage tracking (parent → child relationships)
//! - Version history and promotion tracking

use crate::adapter_refs::{AdapterName, AdapterVersion};
use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::IndexManager;
use std::sync::Arc;
use tracing::{debug, error};

/// Index definitions for adapter versions
pub mod version_indexes {
    /// Index versions by content hash (primary lookup)
    pub const BY_HASH: &str = "adapter_versions_by_hash";

    /// Index versions by adapter name (list all versions of an adapter)
    pub const BY_NAME: &str = "adapter_versions_by_name";

    /// Index versions by parent hash (lineage traversal)
    pub const BY_PARENT: &str = "adapter_versions_by_parent";

    /// Index versions by tenant
    pub const BY_TENANT: &str = "adapter_versions_by_tenant";

    /// Compound index: tenant + adapter name
    pub const BY_TENANT_NAME: &str = "adapter_versions_by_tenant_name";
}

/// Repository for adapter version metadata
pub struct AdapterVersionRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
}

impl AdapterVersionRepository {
    /// Create a new adapter version repository
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    /// Store a new version
    pub async fn create(&self, version: &AdapterVersion) -> Result<(), StorageError> {
        let key = version_key(&version.hash);

        // Check if version already exists (content-addressed, so this is a no-op if same)
        if self.backend.exists(&key).await? {
            debug!(
                hash = %version.hash,
                "Version already exists, skipping create"
            );
            return Ok(());
        }

        // Serialize and store
        let value = bincode::serialize(version)?;
        self.backend.set(&key, value).await?;

        // Update indexes
        self.update_indexes(version).await?;

        debug!(
            hash = %version.hash,
            name = %version.name,
            version = %version.version,
            "Created adapter version"
        );

        Ok(())
    }

    /// Get a version by content hash
    pub async fn get(&self, hash: &str) -> Result<Option<AdapterVersion>, StorageError> {
        let key = version_key(hash);

        match self.backend.get(&key).await? {
            Some(bytes) => {
                let version: AdapterVersion = bincode::deserialize(&bytes)?;
                Ok(Some(version))
            }
            None => Ok(None),
        }
    }

    /// Delete a version (use with caution - may orphan refs)
    pub async fn delete(&self, hash: &str) -> Result<bool, StorageError> {
        let key = version_key(hash);

        // Get version for index cleanup
        let version = match self.get(hash).await? {
            Some(v) => v,
            None => return Ok(false),
        };

        // Delete from storage
        let deleted = self.backend.delete(&key).await?;

        if deleted {
            // Remove from indexes
            self.remove_from_indexes(&version).await?;
        }

        Ok(deleted)
    }

    /// List all versions for an adapter
    pub async fn list_by_name(
        &self,
        tenant_id: &str,
        name: &AdapterName,
    ) -> Result<Vec<AdapterVersion>, StorageError> {
        // Use | as separator to avoid conflict with index key format
        let compound_key = format!("{}|{}", tenant_id, name);
        let hashes = self
            .index_manager
            .query_index(version_indexes::BY_TENANT_NAME, &compound_key)
            .await?;

        self.load_versions(&hashes).await
    }

    /// List all versions for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<AdapterVersion>, StorageError> {
        let hashes = self
            .index_manager
            .query_index(version_indexes::BY_TENANT, tenant_id)
            .await?;

        self.load_versions(&hashes).await
    }

    /// Get all child versions (versions that have this hash as parent)
    pub async fn get_children(
        &self,
        parent_hash: &str,
    ) -> Result<Vec<AdapterVersion>, StorageError> {
        let hashes = self
            .index_manager
            .query_index(version_indexes::BY_PARENT, parent_hash)
            .await?;

        self.load_versions(&hashes).await
    }

    /// Get the lineage (ancestor chain) of a version
    pub async fn get_lineage(
        &self,
        hash: &str,
        max_depth: usize,
    ) -> Result<Vec<AdapterVersion>, StorageError> {
        let mut lineage = Vec::new();
        let mut current_hash = hash.to_string();
        let mut visited = std::collections::HashSet::new();

        for _ in 0..max_depth {
            if visited.contains(&current_hash) {
                // Circular reference detected
                error!(hash = %current_hash, "Circular reference in version lineage");
                break;
            }
            visited.insert(current_hash.clone());

            let version = match self.get(&current_hash).await? {
                Some(v) => v,
                None => break,
            };

            let parent = version.parent_hash.clone();
            lineage.push(version);

            match parent {
                Some(p) => current_hash = p,
                None => break,
            }
        }

        Ok(lineage)
    }

    /// Find the latest version by semantic version for an adapter
    pub async fn get_latest(
        &self,
        tenant_id: &str,
        name: &AdapterName,
    ) -> Result<Option<AdapterVersion>, StorageError> {
        let versions = self.list_by_name(tenant_id, name).await?;

        // Parse and sort by semver
        let mut with_semver: Vec<_> = versions
            .into_iter()
            .filter_map(|v| parse_semver(&v.version).map(|semver| (semver, v)))
            .collect();

        // Sort descending by semver
        with_semver.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(with_semver.into_iter().next().map(|(_, v)| v))
    }

    // =========================================================================
    // Internal helpers
    // =========================================================================

    async fn load_versions(&self, hashes: &[String]) -> Result<Vec<AdapterVersion>, StorageError> {
        let keys: Vec<String> = hashes.iter().map(|h| version_key(h)).collect();
        let values = self.backend.batch_get(&keys).await?;

        let mut versions = Vec::new();
        for (hash, value_opt) in hashes.iter().zip(values.iter()) {
            if let Some(bytes) = value_opt {
                match bincode::deserialize::<AdapterVersion>(bytes) {
                    Ok(v) => versions.push(v),
                    Err(e) => {
                        error!(hash = %hash, error = %e, "Failed to deserialize version");
                    }
                }
            }
        }

        Ok(versions)
    }

    async fn update_indexes(&self, version: &AdapterVersion) -> Result<(), StorageError> {
        let hash = &version.hash;

        // BY_HASH (primary)
        self.index_manager
            .add_to_index(version_indexes::BY_HASH, hash, hash)
            .await?;

        // BY_NAME
        let name_key = version.name.to_string();
        self.index_manager
            .add_to_index(version_indexes::BY_NAME, &name_key, hash)
            .await?;

        // BY_TENANT (extract tenant from metadata or use default)
        let tenant_id = version
            .metadata
            .get("tenant_id")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        self.index_manager
            .add_to_index(version_indexes::BY_TENANT, &tenant_id, hash)
            .await?;

        // BY_TENANT_NAME (compound) - use | as separator to avoid conflict with index key format
        let compound_key = format!("{}|{}", tenant_id, name_key);
        self.index_manager
            .add_to_index(version_indexes::BY_TENANT_NAME, &compound_key, hash)
            .await?;

        // BY_PARENT (if has parent)
        if let Some(parent_hash) = &version.parent_hash {
            self.index_manager
                .add_to_index(version_indexes::BY_PARENT, parent_hash, hash)
                .await?;
        }

        Ok(())
    }

    async fn remove_from_indexes(&self, version: &AdapterVersion) -> Result<(), StorageError> {
        let hash = &version.hash;

        // BY_HASH
        self.index_manager
            .remove_from_index(version_indexes::BY_HASH, hash, hash)
            .await?;

        // BY_NAME
        let name_key = version.name.to_string();
        self.index_manager
            .remove_from_index(version_indexes::BY_NAME, &name_key, hash)
            .await?;

        // BY_TENANT
        let tenant_id = version
            .metadata
            .get("tenant_id")
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        self.index_manager
            .remove_from_index(version_indexes::BY_TENANT, &tenant_id, hash)
            .await?;

        // BY_TENANT_NAME - use | as separator to avoid conflict with index key format
        let compound_key = format!("{}|{}", tenant_id, name_key);
        self.index_manager
            .remove_from_index(version_indexes::BY_TENANT_NAME, &compound_key, hash)
            .await?;

        // BY_PARENT
        if let Some(parent_hash) = &version.parent_hash {
            self.index_manager
                .remove_from_index(version_indexes::BY_PARENT, parent_hash, hash)
                .await?;
        }

        Ok(())
    }
}

/// Generate the storage key for a version
fn version_key(hash: &str) -> String {
    format!("adapter_version:{}", hash)
}

/// Parse a semver string into tuple for comparison
fn parse_semver(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = version.split('.').collect();
    let major = parts.first()?.parse().ok()?;
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::indexing::IndexManager;
    use crate::redb::RedbBackend;

    fn repo_in_memory() -> AdapterVersionRepository {
        let backend = Arc::new(RedbBackend::open_in_memory().unwrap());
        let index_manager = Arc::new(IndexManager::new(backend.clone()));
        AdapterVersionRepository::new(backend, index_manager)
    }

    fn sample_version(hash: &str, name: &str, version: &str) -> AdapterVersion {
        let adapter_name = AdapterName::subject(name);
        let mut v = AdapterVersion::new(hash, adapter_name, version);
        v.metadata
            .insert("tenant_id".to_string(), "test-tenant".to_string());
        v
    }

    #[tokio::test]
    async fn create_and_get_version() {
        let repo = repo_in_memory();
        let version = sample_version("hash123", "test-adapter", "1.0.0");

        repo.create(&version).await.unwrap();

        let fetched = repo.get("hash123").await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.hash, "hash123");
        assert_eq!(fetched.version, "1.0.0");
    }

    #[tokio::test]
    async fn list_by_name() {
        let repo = repo_in_memory();

        // Create multiple versions
        let v1 = sample_version("hash1", "my-adapter", "1.0.0");
        let v2 = sample_version("hash2", "my-adapter", "1.1.0");
        let v3 = sample_version("hash3", "other-adapter", "1.0.0");

        repo.create(&v1).await.unwrap();
        repo.create(&v2).await.unwrap();
        repo.create(&v3).await.unwrap();

        // List versions of my-adapter
        let adapter_name = AdapterName::subject("my-adapter");
        let versions = repo
            .list_by_name("test-tenant", &adapter_name)
            .await
            .unwrap();
        assert_eq!(versions.len(), 2);
    }

    #[tokio::test]
    async fn lineage_tracking() {
        let repo = repo_in_memory();

        // Create a lineage: v1 -> v2 -> v3
        let v1 = sample_version("hash_v1", "lineage-test", "1.0.0");
        let mut v2 = sample_version("hash_v2", "lineage-test", "2.0.0");
        v2.parent_hash = Some("hash_v1".to_string());
        let mut v3 = sample_version("hash_v3", "lineage-test", "3.0.0");
        v3.parent_hash = Some("hash_v2".to_string());

        repo.create(&v1).await.unwrap();
        repo.create(&v2).await.unwrap();
        repo.create(&v3).await.unwrap();

        // Get lineage of v3
        let lineage = repo.get_lineage("hash_v3", 10).await.unwrap();
        assert_eq!(lineage.len(), 3);
        assert_eq!(lineage[0].hash, "hash_v3");
        assert_eq!(lineage[1].hash, "hash_v2");
        assert_eq!(lineage[2].hash, "hash_v1");

        // Get children of v1
        let children = repo.get_children("hash_v1").await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].hash, "hash_v2");
    }

    #[tokio::test]
    async fn get_latest_version() {
        let repo = repo_in_memory();

        let v1 = sample_version("hash1", "latest-test", "1.0.0");
        let v2 = sample_version("hash2", "latest-test", "1.2.0");
        let v3 = sample_version("hash3", "latest-test", "2.0.0");

        repo.create(&v1).await.unwrap();
        repo.create(&v2).await.unwrap();
        repo.create(&v3).await.unwrap();

        let adapter_name = AdapterName::subject("latest-test");
        let latest = repo.get_latest("test-tenant", &adapter_name).await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().version, "2.0.0");
    }

    #[tokio::test]
    async fn delete_version() {
        let repo = repo_in_memory();
        let version = sample_version("delete-me", "delete-test", "1.0.0");

        repo.create(&version).await.unwrap();
        assert!(repo.get("delete-me").await.unwrap().is_some());

        let deleted = repo.delete("delete-me").await.unwrap();
        assert!(deleted);

        assert!(repo.get("delete-me").await.unwrap().is_none());
    }

    #[test]
    fn parse_semver_works() {
        assert_eq!(parse_semver("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("2.0"), Some((2, 0, 0)));
        assert_eq!(parse_semver("3"), Some((3, 0, 0)));
        assert_eq!(parse_semver("invalid"), None);
    }
}
