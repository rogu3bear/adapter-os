//! Lightweight byte-oriented storage abstraction with a filesystem backend.
//!
//! This provides a narrow interface that callers (datasets, training artifacts)
//! can depend on without committing to direct filesystem semantics. The
//! filesystem implementation preserves the current on-disk layout while making
//! it easy to swap for object storage later.

use crate::ensure_free_space;
use adapteros_core::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;

/// Logical storage category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    DatasetFile,
    AdapterArtifact,
    /// Canonical content-addressable dataset storage
    CanonicalDataset,
}

/// Dataset category for canonical storage organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetCategory {
    /// Codebase-derived datasets (from code ingestion)
    Codebase,
    /// System metrics datasets
    Metrics,
    /// Synthetic/generated datasets
    Synthetic,
    /// User-uploaded datasets
    Upload,
    /// Custom category
    Custom(String),
}

impl DatasetCategory {
    /// Get the directory name for this category.
    pub fn as_dir_name(&self) -> &str {
        match self {
            DatasetCategory::Codebase => "codebase",
            DatasetCategory::Metrics => "metrics",
            DatasetCategory::Synthetic => "synthetic",
            DatasetCategory::Upload => "upload",
            DatasetCategory::Custom(name) => name.as_str(),
        }
    }
}

/// Canonical key for a stored object.
#[derive(Debug, Clone)]
pub struct StorageKey {
    pub tenant_id: Option<String>,
    pub object_id: String,
    pub version_id: Option<String>,
    pub file_name: String,
    pub kind: StorageKind,
    /// Content hash for canonical storage (BLAKE3 hex string)
    pub content_hash: Option<String>,
    /// Dataset category for canonical storage
    pub category: Option<DatasetCategory>,
}

impl StorageKey {
    /// Create a storage key for a standard dataset file.
    pub fn dataset_file(
        tenant_id: Option<String>,
        dataset_id: impl Into<String>,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            object_id: dataset_id.into(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        }
    }

    /// Create a storage key for an adapter artifact.
    pub fn adapter_artifact(
        tenant_id: Option<String>,
        adapter_id: impl Into<String>,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            object_id: adapter_id.into(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::AdapterArtifact,
            content_hash: None,
            category: None,
        }
    }

    /// Create a canonical content-addressable storage key for a dataset file.
    ///
    /// Path scheme: `{datasets_root}/canonical/{category}/{hash_prefix}/{content_hash}/{version?}/{file_name}`
    pub fn canonical_dataset(
        content_hash: impl Into<String>,
        category: DatasetCategory,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        let hash = content_hash.into();
        Self {
            tenant_id: None,
            object_id: hash.clone(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::CanonicalDataset,
            content_hash: Some(hash),
            category: Some(category),
        }
    }

    /// Create a canonical dataset key with tenant isolation.
    pub fn canonical_dataset_with_tenant(
        tenant_id: impl Into<String>,
        content_hash: impl Into<String>,
        category: DatasetCategory,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        let hash = content_hash.into();
        Self {
            tenant_id: Some(tenant_id.into()),
            object_id: hash.clone(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::CanonicalDataset,
            content_hash: Some(hash),
            category: Some(category),
        }
    }
}

/// Location + size metadata for a stored object.
#[derive(Debug, Clone)]
pub struct StorageLocation {
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[async_trait]
pub trait ByteStorage: Send + Sync {
    /// Resolve the absolute path for a logical key.
    fn path_for(&self, key: &StorageKey) -> Result<PathBuf>;

    /// Store bytes at the resolved location, overwriting if present.
    async fn store_bytes(&self, key: &StorageKey, data: &[u8]) -> Result<StorageLocation>;

    /// Open and read bytes for a key.
    async fn open_bytes(&self, key: &StorageKey) -> Result<Bytes>;

    /// Delete bytes for a key (no error if missing).
    async fn delete(&self, key: &StorageKey) -> Result<()>;

    /// Stat a key, returning size and path.
    async fn stat(&self, key: &StorageKey) -> Result<StorageLocation>;
}

/// Filesystem-backed implementation that mirrors current layout conventions.
#[derive(Debug, Clone)]
pub struct FsByteStorage {
    datasets_root: PathBuf,
    adapters_root: PathBuf,
}

impl FsByteStorage {
    pub fn new(datasets_root: PathBuf, adapters_root: PathBuf) -> Self {
        Self {
            datasets_root,
            adapters_root,
        }
    }

    fn dataset_path(&self, key: &StorageKey) -> PathBuf {
        // Canonical layout: {datasets_root}/files/{workspace_id}/{dataset_id}/{versions/{version_id}?}/{file}
        // - tenant_id maps to workspace_id for workspace scoping
        // - object_id is the dataset_id
        // - version_id, when present, goes under a "versions" subdirectory
        let mut base = self.datasets_root.join("files");

        // Add workspace scoping from tenant_id if provided
        if let Some(tenant) = &key.tenant_id {
            base = base.join(tenant);
        }

        // Add dataset_id
        base = base.join(&key.object_id);

        // Add version path if present (using canonical "versions" subdirectory)
        if let Some(ver) = &key.version_id {
            base = base.join("versions").join(ver);
        }

        base.join(&key.file_name)
    }

    fn adapter_path(&self, key: &StorageKey) -> PathBuf {
        // Preserve adapter repo layout: {adapters_root}/{tenant}/{adapter}/{file}
        // If no tenant is provided, fall back to top-level.
        let mut base = if let Some(tenant) = &key.tenant_id {
            self.adapters_root.join(tenant)
        } else {
            self.adapters_root.clone()
        };
        base = base.join(&key.object_id);
        if let Some(ver) = &key.version_id {
            base = base.join(ver);
        }
        base.join(&key.file_name)
    }

    /// Build canonical content-addressable path for a dataset.
    ///
    /// Path scheme: `{datasets_root}/canonical/{category}/{hash_prefix}/{content_hash}/{version?}/{file_name}`
    fn canonical_dataset_path(&self, key: &StorageKey) -> Result<PathBuf> {
        let hash = key.content_hash.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation(
                "Canonical dataset key requires content_hash".to_string(),
            )
        })?;

        let category = key.category.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation(
                "Canonical dataset key requires category".to_string(),
            )
        })?;

        // Validate hash format (hex string, minimum length 4 for prefix)
        if hash.len() < 4 {
            return Err(adapteros_core::AosError::Validation(format!(
                "Content hash too short for canonical storage: {}",
                hash
            )));
        }

        // Extract hash prefix for directory sharding (first 2 chars)
        let hash_prefix = &hash[..2];

        // Build path: canonical/{category}/{prefix}/{hash}/{version?}/{file}
        let mut base = self
            .datasets_root
            .join("canonical")
            .join(category.as_dir_name())
            .join(hash_prefix)
            .join(hash);

        // Add version subdirectory if specified
        if let Some(ver) = &key.version_id {
            base = base.join(ver);
        }

        // Add tenant isolation if specified
        if let Some(tenant) = &key.tenant_id {
            base = base.join("tenants").join(tenant);
        }

        Ok(base.join(&key.file_name))
    }

    async fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to create parent dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl ByteStorage for FsByteStorage {
    fn path_for(&self, key: &StorageKey) -> Result<PathBuf> {
        let path = match key.kind {
            StorageKind::DatasetFile => self.dataset_path(key),
            StorageKind::AdapterArtifact => self.adapter_path(key),
            StorageKind::CanonicalDataset => return self.canonical_dataset_path(key),
        };
        let abs = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(path)
        };
        Ok(abs)
    }

    async fn store_bytes(&self, key: &StorageKey, data: &[u8]) -> Result<StorageLocation> {
        let path = self.path_for(key)?;
        let parent = path.parent().unwrap_or(Path::new("."));
        ensure_free_space(parent, "byte store write").map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Failed to ensure free space for {}: {}",
                path.display(),
                e
            ))
        })?;
        Self::ensure_parent(&path).await?;
        fs::write(&path, data).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to write {}: {}", path.display(), e))
        })?;
        let size_bytes = data.len() as u64;
        Ok(StorageLocation { path, size_bytes })
    }

    async fn open_bytes(&self, key: &StorageKey) -> Result<Bytes> {
        let path = self.path_for(key)?;
        let mut file = fs::File::open(&path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to open {}: {}", path.display(), e))
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
        Ok(Bytes::from(buf))
    }

    async fn delete(&self, key: &StorageKey) -> Result<()> {
        let path = self.path_for(key)?;
        if fs::remove_file(&path).await.is_err() {
            // Best-effort; missing is not fatal.
        }
        Ok(())
    }

    async fn stat(&self, key: &StorageKey) -> Result<StorageLocation> {
        let path = self.path_for(key)?;
        let meta = fs::metadata(&path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to stat {}: {}", path.display(), e))
        })?;
        Ok(StorageLocation {
            path,
            size_bytes: meta.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fs_byte_storage_roundtrip() {
        let dir = tempdir().unwrap();
        let ds_root = dir.path().join("datasets");
        let ad_root = dir.path().join("adapters");
        let store = FsByteStorage::new(ds_root.clone(), ad_root.clone());

        let key = StorageKey {
            tenant_id: Some("t1".into()),
            object_id: "obj".into(),
            version_id: Some("v1".into()),
            file_name: "file.bin".into(),
            kind: StorageKind::DatasetFile,
        };

        let location = store.store_bytes(&key, b"hello").await.unwrap();
        assert!(location.path.exists());
        assert_eq!(location.size_bytes, 5);

        let bytes = store.open_bytes(&key).await.unwrap();
        assert_eq!(bytes, Bytes::from_static(b"hello"));

        let stat = store.stat(&key).await.unwrap();
        assert_eq!(stat.size_bytes, 5);

        store.delete(&key).await.unwrap();
        assert!(!location.path.exists());
    }

    #[test]
    fn dataset_path_with_tenant_and_version() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Full key with tenant_id and version_id
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: Some("v2".into()),
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
        };

        let path = store.dataset_path(&key);
        // Canonical layout: files/{workspace_id}/{dataset_id}/versions/{version_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/workspace-123/dataset-456/versions/v2/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_with_tenant_no_version() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Key with tenant_id but no version_id
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
        };

        let path = store.dataset_path(&key);
        // Layout without version: files/{workspace_id}/{dataset_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/workspace-123/dataset-456/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_without_tenant() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Key without tenant_id (legacy/unscoped)
        let key = StorageKey {
            tenant_id: None,
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
        };

        let path = store.dataset_path(&key);
        // Fallback layout without workspace: files/{dataset_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/dataset-456/train.jsonl")
        );
    }
}
