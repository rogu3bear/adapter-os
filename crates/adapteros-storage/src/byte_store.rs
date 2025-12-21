//! Lightweight byte-oriented storage abstraction with a filesystem backend.
//!
//! This provides a narrow interface that callers (datasets, training artifacts)
//! can depend on without committing to direct filesystem semantics. The
//! filesystem implementation preserves the current on-disk layout while making
//! it easy to swap for object storage later.

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
}

/// Canonical key for a stored object.
#[derive(Debug, Clone)]
pub struct StorageKey {
    pub tenant_id: Option<String>,
    pub object_id: String,
    pub version_id: Option<String>,
    pub file_name: String,
    pub kind: StorageKind,
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
        // Keep existing layout: {datasets_root}/files/{dataset}/{version?}/{file}
        let mut base = self.datasets_root.join("files").join(&key.object_id);
        if let Some(ver) = &key.version_id {
            base = base.join(ver);
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
}

