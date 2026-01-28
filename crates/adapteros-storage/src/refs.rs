//! Symlink-based ref storage for adapter versioning
//!
//! This module provides git-inspired ref management using filesystem symlinks.
//! Refs are lightweight pointers to content-addressed adapter objects.
//!
//! # Layout
//!
//! ```text
//! {adapter_root}/subjects/{tenant}/{name}/refs/
//!     current -> ../../objects/ab/cdef1234/{hash}.aos
//!     previous -> ../../objects/12/34567890/{hash}.aos
//!     draft -> ../../objects/ff/00112233/{hash}.aos
//!     v1 -> ../../objects/ab/cdef1234/{hash}.aos
//!     v2 -> ../../objects/12/34567890/{hash}.aos
//! ```
//!
//! # Atomic Updates
//!
//! Ref updates use the temp-symlink-rename pattern for atomicity:
//! 1. Create temp symlink: `{ref}.tmp.{uuid}` -> target
//! 2. Rename temp to final: `{ref}.tmp.{uuid}` -> `{ref}`
//!
//! This ensures refs are never in an inconsistent state.

use crate::adapter_refs::{AdapterLayout, AdapterName, AdapterRef};
use crate::error::StorageError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

/// Trait for ref storage operations
#[async_trait]
pub trait RefStore: Send + Sync {
    /// Get a ref by name, returning the target hash
    async fn get_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<Option<String>, StorageError>;

    /// Update (or create) a ref to point to a new target hash
    async fn update_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
        target_hash: &str,
    ) -> Result<(), StorageError>;

    /// Delete a ref
    async fn delete_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<bool, StorageError>;

    /// List all refs for an adapter
    async fn list_refs(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
    ) -> Result<Vec<AdapterRef>, StorageError>;

    /// Resolve a ref to its target object path
    async fn resolve_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<Option<PathBuf>, StorageError>;
}

/// Filesystem-based ref store using symlinks
#[derive(Debug, Clone)]
pub struct FsRefStore {
    layout: AdapterLayout,
}

impl FsRefStore {
    /// Create a new filesystem ref store
    pub fn new(layout: AdapterLayout) -> Self {
        Self { layout }
    }

    /// Get the refs directory for an adapter
    fn refs_dir(&self, adapter: &AdapterName, tenant_id: &str) -> PathBuf {
        self.layout.refs_dir(adapter, tenant_id)
    }

    /// Get the path for a specific ref
    fn ref_path(&self, adapter: &AdapterName, tenant_id: &str, ref_name: &str) -> PathBuf {
        self.layout.ref_path(adapter, tenant_id, ref_name)
    }

    /// Compute the relative path from a ref to an object
    fn relative_object_path(&self, adapter: &AdapterName, tenant_id: &str, hash: &str) -> PathBuf {
        // From refs dir, we need to go up to adapters root, then into objects
        // refs dir: {root}/{kind}/{tenant}/{name}/refs/
        // object:   {root}/objects/{hash[0:2]}/{hash[2:10]}/{hash}.aos
        //
        // Relative path: ../../../../objects/{hash[0:2]}/{hash[2:10]}/{hash}.aos

        let prefix_2 = hash.get(0..2).unwrap_or("00");
        let prefix_8 = hash.get(2..10).unwrap_or("00000000");

        // Count directory depth from refs to root
        let refs_dir = adapter.refs_dir(tenant_id);
        let depth = refs_dir.components().count();

        let mut rel_path = PathBuf::new();
        for _ in 0..depth {
            rel_path.push("..");
        }

        rel_path
            .join("objects")
            .join(prefix_2)
            .join(prefix_8)
            .join(format!("{}.aos", hash))
    }

    /// Extract hash from a symlink target path
    fn hash_from_target(target: &Path) -> Option<String> {
        target
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }

    /// Ensure the refs directory exists
    async fn ensure_refs_dir(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
    ) -> Result<(), StorageError> {
        let refs_dir = self.refs_dir(adapter, tenant_id);
        fs::create_dir_all(&refs_dir).await.map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to create refs dir {}: {}", refs_dir.display(), e),
            ))
        })?;
        Ok(())
    }
}

#[async_trait]
impl RefStore for FsRefStore {
    async fn get_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<Option<String>, StorageError> {
        let ref_path = self.ref_path(adapter, tenant_id, ref_name);

        // Check if ref exists and is a symlink
        match fs::symlink_metadata(&ref_path).await {
            Ok(meta) if meta.file_type().is_symlink() => {
                // Read the symlink target
                let target = fs::read_link(&ref_path).await.map_err(|e| {
                    StorageError::IoError(std::io::Error::new(
                        e.kind(),
                        format!("Failed to read symlink {}: {}", ref_path.display(), e),
                    ))
                })?;

                // Extract hash from target path
                Ok(Self::hash_from_target(&target))
            }
            Ok(_) => {
                // Not a symlink - might be a regular file with hash content
                // Support both symlinks (preferred) and plain files (fallback)
                match fs::read_to_string(&ref_path).await {
                    Ok(content) => Ok(Some(content.trim().to_string())),
                    Err(e) => {
                        warn!(
                            ref_path = %ref_path.display(),
                            error = %e,
                            "Ref exists but is not a symlink and failed to read"
                        );
                        Ok(None)
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to stat ref {}: {}", ref_path.display(), e),
            ))),
        }
    }

    async fn update_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
        target_hash: &str,
    ) -> Result<(), StorageError> {
        // Ensure refs directory exists
        self.ensure_refs_dir(adapter, tenant_id).await?;

        let ref_path = self.ref_path(adapter, tenant_id, ref_name);
        let rel_target = self.relative_object_path(adapter, tenant_id, target_hash);

        // Use atomic rename pattern:
        // 1. Create temp symlink
        // 2. Rename temp to final
        let temp_name = format!("{}.tmp.{}", ref_name, uuid::Uuid::new_v4());
        let temp_path = ref_path.with_file_name(&temp_name);

        // Create temp symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&rel_target, &temp_path).map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create temp symlink {} -> {}: {}",
                        temp_path.display(),
                        rel_target.display(),
                        e
                    ),
                ))
            })?;
        }

        #[cfg(not(unix))]
        {
            // On non-Unix, fall back to writing hash to a file
            fs::write(&temp_path, target_hash).await.map_err(|e| {
                StorageError::IoError(std::io::Error::new(
                    e.kind(),
                    format!("Failed to write temp ref file {}: {}", temp_path.display(), e),
                ))
            })?;
        }

        // Atomic rename temp to final
        fs::rename(&temp_path, &ref_path).await.map_err(|e| {
            // Try to clean up temp file
            let _ = std::fs::remove_file(&temp_path);
            StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to rename temp ref {} -> {}: {}",
                    temp_path.display(),
                    ref_path.display(),
                    e
                ),
            ))
        })?;

        debug!(
            adapter = %adapter,
            tenant = %tenant_id,
            ref_name = %ref_name,
            target = %target_hash,
            "Updated ref"
        );

        Ok(())
    }

    async fn delete_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<bool, StorageError> {
        let ref_path = self.ref_path(adapter, tenant_id, ref_name);

        match fs::remove_file(&ref_path).await {
            Ok(()) => {
                debug!(
                    adapter = %adapter,
                    tenant = %tenant_id,
                    ref_name = %ref_name,
                    "Deleted ref"
                );
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to delete ref {}: {}", ref_path.display(), e),
            ))),
        }
    }

    async fn list_refs(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
    ) -> Result<Vec<AdapterRef>, StorageError> {
        let refs_dir = self.refs_dir(adapter, tenant_id);

        // Check if refs directory exists
        if !refs_dir.exists() {
            return Ok(Vec::new());
        }

        let mut refs = Vec::new();
        let mut entries = fs::read_dir(&refs_dir).await.map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!("Failed to read refs dir {}: {}", refs_dir.display(), e),
            ))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            StorageError::IoError(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read entry in refs dir {}: {}",
                    refs_dir.display(),
                    e
                ),
            ))
        })? {
            let path = entry.path();
            let ref_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Skip temp files
            if ref_name.contains(".tmp.") {
                continue;
            }

            // Get target hash
            if let Some(target_hash) = self.get_ref(adapter, tenant_id, &ref_name).await? {
                // Get modification time
                let updated_at = match fs::metadata(&path).await {
                    Ok(meta) => meta
                        .modified()
                        .ok()
                        .map(|t| {
                            chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
                        })
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                    Err(_) => chrono::Utc::now().to_rfc3339(),
                };

                refs.push(AdapterRef {
                    adapter_name: adapter.clone(),
                    ref_name,
                    target_hash,
                    updated_at,
                });
            }
        }

        // Sort by ref name for consistent ordering
        refs.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));

        Ok(refs)
    }

    async fn resolve_ref(
        &self,
        adapter: &AdapterName,
        tenant_id: &str,
        ref_name: &str,
    ) -> Result<Option<PathBuf>, StorageError> {
        let target_hash = match self.get_ref(adapter, tenant_id, ref_name).await? {
            Some(h) => h,
            None => return Ok(None),
        };

        let object_path = self.layout.object_path(&target_hash);
        if object_path.exists() {
            Ok(Some(object_path))
        } else {
            Ok(None)
        }
    }
}

/// Promote a version by updating refs
///
/// This creates the standard promotion sequence:
/// 1. Move current -> previous
/// 2. Set current -> new_hash
/// 3. Optionally create version tag (v1, v2, etc.)
pub async fn promote_version(
    store: &dyn RefStore,
    adapter: &AdapterName,
    tenant_id: &str,
    new_hash: &str,
    version_tag: Option<&str>,
) -> Result<(), StorageError> {
    use crate::adapter_refs::refs;

    // Get current ref (if exists) to become previous
    if let Some(current_hash) = store.get_ref(adapter, tenant_id, refs::CURRENT).await? {
        // Move current to previous
        store
            .update_ref(adapter, tenant_id, refs::PREVIOUS, &current_hash)
            .await?;
    }

    // Set current to new hash
    store
        .update_ref(adapter, tenant_id, refs::CURRENT, new_hash)
        .await?;

    // Optionally create version tag
    if let Some(tag) = version_tag {
        store
            .update_ref(adapter, tenant_id, tag, new_hash)
            .await?;
    }

    Ok(())
}

/// Rollback to previous version
pub async fn rollback_to_previous(
    store: &dyn RefStore,
    adapter: &AdapterName,
    tenant_id: &str,
) -> Result<bool, StorageError> {
    use crate::adapter_refs::refs;

    // Get previous ref
    let previous_hash = match store.get_ref(adapter, tenant_id, refs::PREVIOUS).await? {
        Some(h) => h,
        None => return Ok(false), // No previous version to rollback to
    };

    // Get current to backup
    if let Some(current_hash) = store.get_ref(adapter, tenant_id, refs::CURRENT).await? {
        // Could optionally save current to a "rollback" ref here
        debug!(
            adapter = %adapter,
            from = %current_hash,
            to = %previous_hash,
            "Rolling back"
        );
    }

    // Set current to previous
    store
        .update_ref(adapter, tenant_id, refs::CURRENT, &previous_hash)
        .await?;

    Ok(true)
}

/// Get the next version tag (v1, v2, v3, etc.)
pub async fn next_version_tag(
    store: &dyn RefStore,
    adapter: &AdapterName,
    tenant_id: &str,
) -> Result<String, StorageError> {
    let refs = store.list_refs(adapter, tenant_id).await?;

    // Find highest version number
    let mut max_version: u32 = 0;
    for r in refs {
        if let Some((major, _, _)) = r.parse_version() {
            max_version = max_version.max(major);
        }
    }

    Ok(format!("v{}", max_version + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_layout(dir: &Path) -> AdapterLayout {
        AdapterLayout::new(dir)
    }

    #[tokio::test]
    async fn test_ref_crud() {
        let dir = tempdir().unwrap();
        let layout = make_layout(dir.path());
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("test-adapter");
        let tenant = "tenant-1";

        // Initially no refs
        assert!(store.get_ref(&adapter, tenant, "current").await.unwrap().is_none());

        // Create a ref
        store.update_ref(&adapter, tenant, "current", "hash123456789abc").await.unwrap();

        // Read it back
        let hash = store.get_ref(&adapter, tenant, "current").await.unwrap();
        assert_eq!(hash, Some("hash123456789abc".to_string()));

        // Update it
        store.update_ref(&adapter, tenant, "current", "newhash987654321").await.unwrap();
        let hash = store.get_ref(&adapter, tenant, "current").await.unwrap();
        assert_eq!(hash, Some("newhash987654321".to_string()));

        // Delete it
        let deleted = store.delete_ref(&adapter, tenant, "current").await.unwrap();
        assert!(deleted);

        // Gone
        assert!(store.get_ref(&adapter, tenant, "current").await.unwrap().is_none());

        // Delete non-existent is ok
        let deleted = store.delete_ref(&adapter, tenant, "current").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_list_refs() {
        let dir = tempdir().unwrap();
        let layout = make_layout(dir.path());
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("multi-ref");
        let tenant = "tenant-1";

        // Create multiple refs
        store.update_ref(&adapter, tenant, "current", "hash1").await.unwrap();
        store.update_ref(&adapter, tenant, "previous", "hash2").await.unwrap();
        store.update_ref(&adapter, tenant, "v1", "hash3").await.unwrap();
        store.update_ref(&adapter, tenant, "v2", "hash4").await.unwrap();

        // List them
        let refs = store.list_refs(&adapter, tenant).await.unwrap();
        assert_eq!(refs.len(), 4);

        let ref_names: Vec<&str> = refs.iter().map(|r| r.ref_name.as_str()).collect();
        assert!(ref_names.contains(&"current"));
        assert!(ref_names.contains(&"previous"));
        assert!(ref_names.contains(&"v1"));
        assert!(ref_names.contains(&"v2"));
    }

    #[tokio::test]
    async fn test_promote_version() {
        let dir = tempdir().unwrap();
        let layout = make_layout(dir.path());
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("promote-test");
        let tenant = "tenant-1";

        // Initial version
        promote_version(&store, &adapter, tenant, "hash_v1", Some("v1")).await.unwrap();

        assert_eq!(store.get_ref(&adapter, tenant, "current").await.unwrap(), Some("hash_v1".to_string()));
        assert_eq!(store.get_ref(&adapter, tenant, "v1").await.unwrap(), Some("hash_v1".to_string()));
        assert!(store.get_ref(&adapter, tenant, "previous").await.unwrap().is_none());

        // Second version
        promote_version(&store, &adapter, tenant, "hash_v2", Some("v2")).await.unwrap();

        assert_eq!(store.get_ref(&adapter, tenant, "current").await.unwrap(), Some("hash_v2".to_string()));
        assert_eq!(store.get_ref(&adapter, tenant, "previous").await.unwrap(), Some("hash_v1".to_string()));
        assert_eq!(store.get_ref(&adapter, tenant, "v2").await.unwrap(), Some("hash_v2".to_string()));
    }

    #[tokio::test]
    async fn test_rollback() {
        let dir = tempdir().unwrap();
        let layout = make_layout(dir.path());
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("rollback-test");
        let tenant = "tenant-1";

        // Can't rollback with no previous
        let rolled = rollback_to_previous(&store, &adapter, tenant).await.unwrap();
        assert!(!rolled);

        // Set up versions
        promote_version(&store, &adapter, tenant, "hash_v1", Some("v1")).await.unwrap();
        promote_version(&store, &adapter, tenant, "hash_v2", Some("v2")).await.unwrap();

        // Rollback
        let rolled = rollback_to_previous(&store, &adapter, tenant).await.unwrap();
        assert!(rolled);

        assert_eq!(store.get_ref(&adapter, tenant, "current").await.unwrap(), Some("hash_v1".to_string()));
    }

    #[tokio::test]
    async fn test_next_version_tag() {
        let dir = tempdir().unwrap();
        let layout = make_layout(dir.path());
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("version-test");
        let tenant = "tenant-1";

        // No versions yet
        let next = next_version_tag(&store, &adapter, tenant).await.unwrap();
        assert_eq!(next, "v1");

        // Add some versions
        store.update_ref(&adapter, tenant, "v1", "hash1").await.unwrap();
        store.update_ref(&adapter, tenant, "v2", "hash2").await.unwrap();

        let next = next_version_tag(&store, &adapter, tenant).await.unwrap();
        assert_eq!(next, "v3");
    }

    #[test]
    fn test_relative_object_path() {
        let layout = AdapterLayout::new("/var/adapters");
        let store = FsRefStore::new(layout);

        let adapter = AdapterName::subject("test");
        let tenant = "tenant-1";
        let hash = "abcdef1234567890abcdef1234567890";

        let rel_path = store.relative_object_path(&adapter, tenant, hash);

        // Should go up from subjects/tenant-1/test/refs/ to root, then into objects
        assert!(rel_path.starts_with(".."));
        assert!(rel_path.to_string_lossy().contains("objects"));
        assert!(rel_path.to_string_lossy().contains("ab"));
        assert!(rel_path.to_string_lossy().contains("cdef1234"));
    }
}
