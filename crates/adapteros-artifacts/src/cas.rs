//! Content-addressed storage

use adapteros_core::{AosError, B3Hash, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Content-addressed store organized by hash
pub struct CasStore {
    root: PathBuf,
}

impl CasStore {
    /// Create a new CAS store
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .map_err(|e| AosError::Artifact(format!("Failed to create CAS root: {}", e)))?;

        Ok(Self { root })
    }

    /// Store bytes, returning the content hash
    pub fn store(&self, class: &str, bytes: &[u8]) -> Result<B3Hash> {
        let hash = B3Hash::hash(bytes);
        let path = self.path_for(class, &hash);

        // Create parent directory
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AosError::Artifact(format!("Failed to create directory: {}", e)))?;
        }

        // Write atomically via temp file
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| AosError::Artifact(format!("Failed to create temp file: {}", e)))?;

        file.write_all(bytes)
            .map_err(|e| AosError::Artifact(format!("Failed to write: {}", e)))?;

        fs::rename(&temp_path, &path)
            .map_err(|e| AosError::Artifact(format!("Failed to rename: {}", e)))?;

        Ok(hash)
    }

    /// Load bytes by hash
    pub fn load(&self, class: &str, hash: &B3Hash) -> Result<Vec<u8>> {
        let path = self.path_for(class, hash);

        if !path.exists() {
            return Err(AosError::Artifact(format!("Artifact not found: {}", hash)));
        }

        let bytes =
            fs::read(&path).map_err(|e| AosError::Artifact(format!("Failed to read: {}", e)))?;

        // Verify hash
        let actual_hash = B3Hash::hash(&bytes);
        if actual_hash != *hash {
            return Err(AosError::Artifact(format!(
                "Hash mismatch: expected {}, got {}",
                hash, actual_hash
            )));
        }

        Ok(bytes)
    }

    /// Check if artifact exists
    pub fn exists(&self, class: &str, hash: &B3Hash) -> bool {
        self.path_for(class, hash).exists()
    }

    /// Get path for a hash
    fn path_for(&self, class: &str, hash: &B3Hash) -> PathBuf {
        let hex = hash.to_hex();
        self.root
            .join(class)
            .join(&hex[..2])
            .join(&hex[2..4])
            .join(hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cas_store_load() {
        let temp = TempDir::new().expect("Test temp directory creation should succeed");
        let store = CasStore::new(temp.path()).expect("Test CAS store creation should succeed");

        let data = b"test data";
        let hash = store
            .store("test", data)
            .expect("Test CAS store operation should succeed");

        let loaded = store
            .load("test", &hash)
            .expect("Test CAS load operation should succeed");
        assert_eq!(data, loaded.as_slice());
    }

    #[test]
    fn test_cas_hash_verification() {
        let temp = TempDir::new().expect("Test temp directory creation should succeed");
        let store = CasStore::new(temp.path()).expect("Test CAS store creation should succeed");

        let data = b"test data";
        let hash1 = B3Hash::hash(data);
        let hash2 = store
            .store("test", data)
            .expect("Test CAS store operation should succeed");

        assert_eq!(hash1, hash2);
    }
}
