use crate::StorageError;
use adapteros_core::B3Hash;
use async_trait::async_trait;
use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Logical grouping of stored bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageKind {
    DatasetFile,
    DatasetManifest,
    AdapterArtifact,
    AdapterManifest,
    CoremlArtifact,
}

/// Canonical key for locating bytes in storage backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageKey {
    /// Tenant or logical owner (required for per-tenant quotas).
    pub owner_id: String,
    /// Dataset or adapter identifier.
    pub object_id: String,
    /// Optional semantic version or dataset version id.
    pub version_id: Option<String>,
    /// Optional file name (e.g., canonical.jsonl or version.aos).
    pub file_name: Option<String>,
    /// Kind of bytes being stored.
    pub kind: StorageKind,
}

impl StorageKey {
    pub fn with_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }
}

/// Metadata returned after storing bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    /// Fully-qualified storage path (filesystem backend) or logical URI.
    pub location: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// BLAKE3 hash of the stored payload.
    pub hash_b3: String,
}

#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Store raw bytes and return the resolved location and hash.
    async fn store_bytes(
        &self,
        key: &StorageKey,
        bytes: &[u8],
    ) -> Result<StoredObject, StorageError>;

    /// Open and read bytes for a key if present.
    async fn open_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, StorageError>;

    /// Delete bytes for a key. Succeeds if already absent.
    async fn delete_bytes(&self, key: &StorageKey) -> Result<(), StorageError>;

    /// Compute a deterministic storage path for this key (backend-specific).
    fn resolve_path(&self, key: &StorageKey) -> PathBuf;
}

/// Filesystem-backed object store compatible with current AdapterOS layout.
#[derive(Debug, Clone)]
pub struct FsObjectStore {
    dataset_root: PathBuf,
    adapter_root: PathBuf,
}

impl FsObjectStore {
    pub fn new(dataset_root: impl AsRef<Path>, adapter_root: impl AsRef<Path>) -> Self {
        Self {
            dataset_root: absolutize(dataset_root.as_ref()),
            adapter_root: absolutize(adapter_root.as_ref()),
        }
    }

    fn base_dir_for(&self, key: &StorageKey) -> PathBuf {
        match key.kind {
            StorageKind::DatasetFile | StorageKind::DatasetManifest => {
                let mut base = self.dataset_root.join("files").join(&key.object_id);
                if let Some(version) = key.version_id.as_deref() {
                    base = base.join(version);
                }
                base
            }
            StorageKind::AdapterArtifact
            | StorageKind::AdapterManifest
            | StorageKind::CoremlArtifact => {
                let version = key.version_id.as_deref().unwrap_or("latest");
                self.adapter_root
                    .join(&key.owner_id)
                    .join(&key.object_id)
                    .join(version)
            }
        }
    }
}

#[async_trait]
impl ObjectStore for FsObjectStore {
    async fn store_bytes(
        &self,
        key: &StorageKey,
        bytes: &[u8],
    ) -> Result<StoredObject, StorageError> {
        let path = self.resolve_path(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                StorageError::IoError(io::Error::new(
                    e.kind(),
                    format!("Failed to create parent dir {}: {}", parent.display(), e),
                ))
            })?;
        }

        let mut file = fs::File::create(&path).await.map_err(|e| {
            StorageError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to create file {}: {}", path.display(), e),
            ))
        })?;
        file.write_all(bytes).await.map_err(|e| {
            StorageError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to write {}: {}", path.display(), e),
            ))
        })?;

        let hash_b3 = B3Hash::hash(bytes).to_hex();
        let size_bytes = bytes.len() as u64;
        Ok(StoredObject {
            location: path.to_string_lossy().to_string(),
            size_bytes,
            hash_b3,
        })
    }

    async fn open_bytes(&self, key: &StorageKey) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.resolve_path(key);
        match fs::read(&path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to read {}: {}", path.display(), e),
            ))),
        }
    }

    async fn delete_bytes(&self, key: &StorageKey) -> Result<(), StorageError> {
        let path = self.resolve_path(key);
        match fs::remove_file(&path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::IoError(io::Error::new(
                e.kind(),
                format!("Failed to delete {}: {}", path.display(), e),
            ))),
        }
    }

    fn resolve_path(&self, key: &StorageKey) -> PathBuf {
        let mut base = self.base_dir_for(key);
        let file_name = key.file_name.clone().unwrap_or_else(|| match key.kind {
            StorageKind::DatasetFile => "data".to_string(),
            StorageKind::DatasetManifest => "manifest.json".to_string(),
            StorageKind::AdapterArtifact => "adapter.aos".to_string(),
            StorageKind::AdapterManifest => "manifest.json".to_string(),
            StorageKind::CoremlArtifact => "coreml.mlpackage".to_string(),
        });
        base.push(file_name);
        base
    }
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("/"))
        .join(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fs_object_store_roundtrip() {
        let dir = tempdir().unwrap();
        let dataset_root = dir.path().join("datasets");
        let adapter_root = dir.path().join("adapters");
        let store = FsObjectStore::new(&dataset_root, &adapter_root);

        let key = StorageKey {
            owner_id: "tenant1".into(),
            object_id: "dataset1".into(),
            version_id: None,
            file_name: Some("file.jsonl".into()),
            kind: StorageKind::DatasetFile,
        };

        let data = b"hello world";
        let stored = store.store_bytes(&key, data).await.unwrap();
        assert_eq!(stored.size_bytes, data.len() as u64);
        let reopened = store.open_bytes(&key).await.unwrap().unwrap();
        assert_eq!(reopened, data);

        store.delete_bytes(&key).await.unwrap();
        let reopened = store.open_bytes(&key).await.unwrap();
        assert!(reopened.is_none());
    }
}

