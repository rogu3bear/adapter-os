//! Content-addressed cache with symlinks for model files
//!
//! This module implements a content-addressed storage system for model files
//! using BLAKE3 hashing and symlinks for deduplication.
//!
//! ## Architecture
//!
//! ```text
//! var/model-cache/
//!   blobs/           # Content-addressed storage (B3 hash)
//!     b3-abc123.safetensors
//!   models/          # Model directories with symlinks
//!     qwen2.5-7b/
//!       model.safetensors → ../../blobs/b3-abc123.safetensors
//!       config.json
//!   downloads/       # In-progress downloads
//!   locks/           # File locks for concurrent access
//! ```
//!
//! The cache directory is configurable via the `AOS_MODEL_CACHE_DIR` environment
//! variable. The default is `var/model-cache` relative to the working directory.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_model_hub::cache::ModelCache;
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let cache = ModelCache::new(PathBuf::from("var/model-cache"))?;
//!
//! // Store a blob by content
//! let data = b"model weights data";
//! let blob_path = cache.store_blob(data)?;
//!
//! // Create a model directory with symlinks
//! let hash = blake3::hash(data);
//! let b3_hash = adapteros_core::B3Hash::from_bytes(*hash.as_bytes());
//! cache.create_model_symlink("qwen2.5-7b", "model.safetensors", &b3_hash)?;
//!
//! // Check if model is complete
//! let complete = cache.is_model_complete("qwen2.5-7b", &["model.safetensors", "config.json"]);
//! # Ok(())
//! # }
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[cfg(not(unix))]
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::symlink;

/// Model cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size in bytes (0 = unlimited)
    pub max_size_bytes: u64,
    /// Enable automatic garbage collection
    pub auto_gc: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
            auto_gc: true,
        }
    }
}

/// Content-addressed cache for model files
///
/// Stores model files in a content-addressed blob store and creates
/// symlinks in model directories for deduplication.
///
/// [source: crates/adapteros-model-hub/src/cache.rs]
pub struct ModelCache {
    cache_dir: PathBuf,
    blobs_dir: PathBuf,
    models_dir: PathBuf,
    downloads_dir: PathBuf,
    locks_dir: PathBuf,
    config: CacheConfig,
    _locks: Mutex<HashMap<String, FileLockHandle>>,
}

impl ModelCache {
    /// Create a new model cache
    ///
    /// Creates the directory structure:
    /// - `cache_dir/blobs/` - Content-addressed blobs
    /// - `cache_dir/models/` - Model directories with symlinks
    /// - `cache_dir/downloads/` - In-progress downloads
    /// - `cache_dir/locks/` - File locks
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        let blobs_dir = cache_dir.join("blobs");
        let models_dir = cache_dir.join("models");
        let downloads_dir = cache_dir.join("downloads");
        let locks_dir = cache_dir.join("locks");

        // Create all directories
        for dir in &[&blobs_dir, &models_dir, &downloads_dir, &locks_dir] {
            fs::create_dir_all(dir).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create cache directory {}: {}",
                    dir.display(),
                    e
                ))
            })?;
        }

        info!(
            cache_dir = %cache_dir.display(),
            "Initialized model cache"
        );

        Ok(Self {
            cache_dir,
            blobs_dir,
            models_dir,
            downloads_dir,
            locks_dir,
            config: CacheConfig::default(),
            _locks: Mutex::new(HashMap::new()),
        })
    }

    /// Create a new cache with custom configuration
    pub fn with_config(cache_dir: PathBuf, config: CacheConfig) -> Result<Self> {
        let mut cache = Self::new(cache_dir)?;
        cache.config = config;
        Ok(cache)
    }

    /// Store a blob by content and return its path
    ///
    /// The blob is stored with a content-addressed filename:
    /// `b3-{hash_hex}.{extension}`
    ///
    /// If a blob with the same hash already exists, it is not duplicated.
    pub fn store_blob(&self, data: &[u8]) -> Result<PathBuf> {
        let hash = blake3::hash(data);
        let b3_hash = B3Hash::from_bytes(*hash.as_bytes());

        self.store_blob_with_hash(data, &b3_hash, "bin")
    }

    /// Store a blob with a specific hash and extension
    pub fn store_blob_with_hash(
        &self,
        data: &[u8],
        hash: &B3Hash,
        extension: &str,
    ) -> Result<PathBuf> {
        let filename = format!("b3-{}.{}", hash.to_hex(), extension);
        let blob_path = self.blobs_dir.join(&filename);

        // If blob already exists, verify it
        if blob_path.exists() {
            debug!(
                path = %blob_path.display(),
                hash = %hash.to_hex(),
                "Blob already exists"
            );

            // Verify existing blob
            let existing_data = fs::read(&blob_path)
                .map_err(|e| AosError::Io(format!("Failed to read existing blob: {}", e)))?;

            let existing_hash = blake3::hash(&existing_data);
            let existing_b3_hash = B3Hash::from_bytes(*existing_hash.as_bytes());

            if existing_b3_hash != *hash {
                return Err(AosError::Verification(format!(
                    "Blob hash mismatch: expected {}, found {}",
                    hash.to_hex(),
                    existing_b3_hash.to_hex()
                )));
            }

            return Ok(blob_path);
        }

        // Write to temporary file first
        let temp_path = self.downloads_dir.join(format!("{}.tmp", filename));
        let mut file = File::create(&temp_path)
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        file.write_all(data)
            .map_err(|e| AosError::Io(format!("Failed to write blob data: {}", e)))?;

        file.sync_all()
            .map_err(|e| AosError::Io(format!("Failed to sync blob data: {}", e)))?;

        drop(file);

        // Atomically move to final location
        fs::rename(&temp_path, &blob_path)
            .map_err(|e| AosError::Io(format!("Failed to move blob to cache: {}", e)))?;

        info!(
            path = %blob_path.display(),
            hash = %hash.to_hex(),
            size = data.len(),
            "Stored blob"
        );

        Ok(blob_path)
    }

    /// Get the path to a blob by hash
    ///
    /// Returns `Some(path)` if the blob exists, `None` otherwise.
    pub fn get_blob(&self, hash: &B3Hash) -> Option<PathBuf> {
        // Try common extensions
        for ext in &["bin", "safetensors", "json", "txt", "dat"] {
            let filename = format!("b3-{}.{}", hash.to_hex(), ext);
            let path = self.blobs_dir.join(&filename);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Create a symlink from a model directory to a blob
    ///
    /// Creates `models/{model_id}/{filename}` pointing to the blob
    /// identified by `blob_hash`.
    #[cfg(unix)]
    pub fn create_model_symlink(
        &self,
        model_id: &str,
        filename: &str,
        blob_hash: &B3Hash,
    ) -> Result<PathBuf> {
        let model_dir = self.models_dir.join(model_id);
        fs::create_dir_all(&model_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create model directory {}: {}",
                model_dir.display(),
                e
            ))
        })?;

        let symlink_path = model_dir.join(filename);

        // Find the blob
        let blob_path = self.get_blob(blob_hash).ok_or_else(|| {
            AosError::Io(format!("Blob not found for hash: {}", blob_hash.to_hex()))
        })?;

        // Calculate relative path from symlink to blob
        // From: models/{model_id}/{filename}
        // To:   blobs/b3-{hash}.{ext}
        // Relative: ../../blobs/b3-{hash}.{ext}
        let blob_filename = blob_path
            .file_name()
            .ok_or_else(|| AosError::Io("Invalid blob path".to_string()))?;

        let relative_target = PathBuf::from("..")
            .join("..")
            .join("blobs")
            .join(blob_filename);

        // Remove existing symlink if present
        if symlink_path.exists() || symlink_path.symlink_metadata().is_ok() {
            fs::remove_file(&symlink_path)
                .map_err(|e| AosError::Io(format!("Failed to remove existing symlink: {}", e)))?;
        }

        // Create symlink
        symlink(&relative_target, &symlink_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to create symlink {} -> {}: {}",
                symlink_path.display(),
                relative_target.display(),
                e
            ))
        })?;

        debug!(
            symlink = %symlink_path.display(),
            target = %relative_target.display(),
            blob_hash = %blob_hash.to_hex(),
            "Created model symlink"
        );

        Ok(symlink_path)
    }

    /// Create a symlink (non-Unix platforms - copies file instead)
    #[cfg(not(unix))]
    pub fn create_model_symlink(
        &self,
        model_id: &str,
        filename: &str,
        blob_hash: &B3Hash,
    ) -> Result<PathBuf> {
        let model_dir = self.models_dir.join(model_id);
        fs::create_dir_all(&model_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create model directory {}: {}",
                model_dir.display(),
                e
            ))
        })?;

        let dest_path = model_dir.join(filename);

        // Find the blob
        let blob_path = self.get_blob(blob_hash).ok_or_else(|| {
            AosError::Io(format!("Blob not found for hash: {}", blob_hash.to_hex()))
        })?;

        // Copy file instead of symlink on non-Unix
        fs::copy(&blob_path, &dest_path)
            .map_err(|e| AosError::Io(format!("Failed to copy blob to model directory: {}", e)))?;

        warn!(
            dest = %dest_path.display(),
            blob = %blob_path.display(),
            "Created file copy (symlinks not supported on this platform)"
        );

        Ok(dest_path)
    }

    /// Get the path to a model directory
    pub fn get_model_path(&self, model_id: &str) -> PathBuf {
        self.models_dir.join(model_id)
    }

    /// Check if a model is complete (all expected files exist)
    pub fn is_model_complete(&self, model_id: &str, expected_files: &[&str]) -> bool {
        let model_dir = self.get_model_path(model_id);

        if !model_dir.exists() {
            return false;
        }

        for filename in expected_files {
            let file_path = model_dir.join(filename);
            if !file_path.exists() {
                debug!(
                    model_id = model_id,
                    missing_file = filename,
                    "Model incomplete"
                );
                return false;
            }
        }

        true
    }

    /// Acquire a lock for a model to prevent concurrent downloads
    pub fn acquire_lock(&self, model_id: &str) -> Result<FileLock> {
        let lock_path = self.locks_dir.join(format!("{}.lock", model_id));
        FileLock::acquire(&lock_path)
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let mut total_blobs = 0;
        let mut total_blob_size = 0u64;
        let mut total_models = 0;

        // Count blobs
        if let Ok(entries) = fs::read_dir(&self.blobs_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        total_blobs += 1;
                        total_blob_size += metadata.len();
                    }
                }
            }
        }

        // Count models
        if let Ok(entries) = fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_dir() {
                        total_models += 1;
                    }
                }
            }
        }

        Ok(CacheStats {
            total_blobs,
            total_blob_size,
            total_models,
            cache_dir: self.cache_dir.clone(),
        })
    }

    /// Remove a model directory (symlinks only, blobs remain)
    pub fn remove_model(&self, model_id: &str) -> Result<()> {
        let model_dir = self.get_model_path(model_id);

        if !model_dir.exists() {
            return Ok(());
        }

        fs::remove_dir_all(&model_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to remove model directory {}: {}",
                model_dir.display(),
                e
            ))
        })?;

        info!(model_id = model_id, "Removed model directory");
        Ok(())
    }

    /// Clean up unused blobs (garbage collection)
    ///
    /// Removes blobs that are not referenced by any model.
    pub fn garbage_collect(&self) -> Result<GcStats> {
        use std::collections::HashSet;

        let mut referenced_blobs = HashSet::new();

        // Collect all blob references from models
        if let Ok(entries) = fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                if let Ok(model_dir) = entry.path().read_dir() {
                    for file_entry in model_dir.flatten() {
                        if let Ok(metadata) = file_entry.metadata() {
                            // Check if it's a symlink
                            if metadata.is_symlink() {
                                if let Ok(target) = fs::read_link(file_entry.path()) {
                                    if let Some(filename) = target.file_name() {
                                        referenced_blobs.insert(filename.to_os_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut removed_count = 0;
        let mut reclaimed_bytes = 0u64;

        // Remove unreferenced blobs
        if let Ok(entries) = fs::read_dir(&self.blobs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    if !referenced_blobs.contains(&filename.to_os_string()) {
                        if let Ok(metadata) = fs::metadata(&path) {
                            let size = metadata.len();
                            if fs::remove_file(&path).is_ok() {
                                removed_count += 1;
                                reclaimed_bytes += size;
                                debug!(
                                    path = %path.display(),
                                    size = size,
                                    "Removed unreferenced blob"
                                );
                            }
                        }
                    }
                }
            }
        }

        info!(
            removed_blobs = removed_count,
            reclaimed_bytes = reclaimed_bytes,
            "Garbage collection complete"
        );

        Ok(GcStats {
            removed_blobs: removed_count,
            reclaimed_bytes,
        })
    }
}

/// File lock for preventing concurrent downloads
///
/// Uses platform-specific file locking to ensure only one process
/// can download/modify a model at a time.
#[cfg(unix)]
pub struct FileLock {
    path: PathBuf,
    _lock: nix::fcntl::Flock<File>,
}

#[cfg(not(unix))]
pub struct FileLock {
    path: PathBuf,
    _file: File,
}

impl FileLock {
    /// Acquire a lock on the given path
    ///
    /// Creates the lock file if it doesn't exist.
    /// Blocks until the lock is acquired.
    #[cfg(unix)]
    pub fn acquire(path: &Path) -> Result<Self> {
        use nix::fcntl::{Flock, FlockArg};

        let file = File::create(path)
            .map_err(|e| AosError::Io(format!("Failed to create lock file: {}", e)))?;

        // Acquire exclusive lock (blocks until available)
        let lock = Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, e)| AosError::Io(format!("Failed to acquire file lock: {:?}", e)))?;

        debug!(path = %path.display(), "Acquired file lock");

        Ok(Self {
            path: path.to_path_buf(),
            _lock: lock,
        })
    }

    /// Try to acquire a lock without blocking
    #[cfg(unix)]
    pub fn try_acquire(path: &Path) -> Result<Self> {
        use nix::fcntl::{Flock, FlockArg};

        let file = File::create(path)
            .map_err(|e| AosError::Io(format!("Failed to create lock file: {}", e)))?;

        // Try to acquire exclusive lock (non-blocking)
        let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|(_, e)| AosError::Io(format!("Failed to acquire file lock: {:?}", e)))?;

        debug!(path = %path.display(), "Acquired file lock (non-blocking)");

        Ok(Self {
            path: path.to_path_buf(),
            _lock: lock,
        })
    }

    /// Non-Unix lock implementation (creates empty file)
    #[cfg(not(unix))]
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = File::create(path)
            .map_err(|e| AosError::Io(format!("Failed to create lock file: {}", e)))?;

        warn!(
            path = %path.display(),
            "File locking not fully supported on this platform"
        );

        Ok(Self {
            path: path.to_path_buf(),
            _file: file,
        })
    }

    /// Non-Unix try_acquire (same as acquire)
    #[cfg(not(unix))]
    pub fn try_acquire(path: &Path) -> Result<Self> {
        Self::acquire(path)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Lock is automatically released when file is closed
        // Clean up lock file
        let _ = fs::remove_file(&self.path);
        debug!(path = %self.path.display(), "Released file lock");
    }
}

/// Internal handle for tracking locks in the cache
struct FileLockHandle {
    _lock: FileLock,
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_blobs: usize,
    pub total_blob_size: u64,
    pub total_models: usize,
    pub cache_dir: PathBuf,
}

/// Garbage collection statistics
#[derive(Debug, Clone)]
pub struct GcStats {
    pub removed_blobs: usize,
    pub reclaimed_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    fn create_test_cache() -> (ModelCache, TempDir) {
        let temp_dir = new_test_tempdir();
        let cache = ModelCache::new(temp_dir.path().to_path_buf()).unwrap();
        (cache, temp_dir)
    }

    #[test]
    fn test_cache_creation() {
        let (cache, _temp_dir) = create_test_cache();
        assert!(cache.blobs_dir.exists());
        assert!(cache.models_dir.exists());
        assert!(cache.downloads_dir.exists());
        assert!(cache.locks_dir.exists());
    }

    #[test]
    fn test_store_blob() {
        let (cache, _temp_dir) = create_test_cache();

        let data = b"test model weights";
        let blob_path = cache.store_blob(data).unwrap();

        assert!(blob_path.exists());
        let stored_data = fs::read(&blob_path).unwrap();
        assert_eq!(stored_data, data);
    }

    #[test]
    fn test_store_blob_deduplication() {
        let (cache, _temp_dir) = create_test_cache();

        let data = b"test model weights";
        let blob_path1 = cache.store_blob(data).unwrap();
        let blob_path2 = cache.store_blob(data).unwrap();

        // Same hash should result in same path
        assert_eq!(blob_path1, blob_path2);

        // Should only exist once
        let stats = cache.stats().unwrap();
        assert_eq!(stats.total_blobs, 1);
    }

    #[test]
    fn test_get_blob() {
        let (cache, _temp_dir) = create_test_cache();

        let data = b"test data";
        let hash = blake3::hash(data);
        let b3_hash = B3Hash::from_bytes(*hash.as_bytes());

        // Should not exist yet
        assert!(cache.get_blob(&b3_hash).is_none());

        // Store blob
        cache.store_blob_with_hash(data, &b3_hash, "bin").unwrap();

        // Should now exist
        assert!(cache.get_blob(&b3_hash).is_some());
    }

    #[test]
    #[cfg(unix)]
    fn test_create_model_symlink() {
        let (cache, _temp_dir) = create_test_cache();

        let data = b"model weights";
        let hash = blake3::hash(data);
        let b3_hash = B3Hash::from_bytes(*hash.as_bytes());

        // Store blob
        cache
            .store_blob_with_hash(data, &b3_hash, "safetensors")
            .unwrap();

        // Create symlink
        let symlink_path = cache
            .create_model_symlink("test-model", "model.safetensors", &b3_hash)
            .unwrap();

        assert!(symlink_path.exists());

        // Verify it's a symlink
        let metadata = fs::symlink_metadata(&symlink_path).unwrap();
        assert!(metadata.is_symlink());

        // Verify it points to correct blob
        let content = fs::read(&symlink_path).unwrap();
        assert_eq!(content, data);
    }

    #[test]
    fn test_is_model_complete() {
        let (cache, _temp_dir) = create_test_cache();

        let model_id = "test-model";

        // Model doesn't exist yet
        assert!(!cache.is_model_complete(model_id, &["model.safetensors", "config.json"]));

        // Create model directory with files
        let model_dir = cache.get_model_path(model_id);
        fs::create_dir_all(&model_dir).unwrap();

        // Add first file
        fs::write(model_dir.join("model.safetensors"), b"weights").unwrap();
        assert!(!cache.is_model_complete(model_id, &["model.safetensors", "config.json"]));

        // Add second file
        fs::write(model_dir.join("config.json"), b"{}").unwrap();
        assert!(cache.is_model_complete(model_id, &["model.safetensors", "config.json"]));
    }

    #[test]
    fn test_file_lock() {
        let temp_dir = new_test_tempdir();
        let lock_path = temp_dir.path().join("test.lock");

        let lock = FileLock::acquire(&lock_path).unwrap();
        assert!(lock_path.exists());

        drop(lock);
        // Lock file should be cleaned up
        assert!(!lock_path.exists());
    }

    #[test]
    fn test_cache_stats() {
        let (cache, _temp_dir) = create_test_cache();

        // Empty cache
        let stats = cache.stats().unwrap();
        assert_eq!(stats.total_blobs, 0);
        assert_eq!(stats.total_models, 0);

        // Add some blobs
        cache.store_blob(b"data1").unwrap();
        cache.store_blob(b"data2").unwrap();

        let stats = cache.stats().unwrap();
        assert_eq!(stats.total_blobs, 2);
        assert!(stats.total_blob_size > 0);
    }

    #[test]
    fn test_remove_model() {
        let (cache, _temp_dir) = create_test_cache();

        let model_id = "test-model";
        let model_dir = cache.get_model_path(model_id);
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("config.json"), b"{}").unwrap();

        assert!(model_dir.exists());

        cache.remove_model(model_id).unwrap();
        assert!(!model_dir.exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_garbage_collect() {
        let (cache, _temp_dir) = create_test_cache();

        // Create some blobs
        let data1 = b"data1";
        let hash1 = blake3::hash(data1);
        let b3_hash1 = B3Hash::from_bytes(*hash1.as_bytes());
        cache.store_blob_with_hash(data1, &b3_hash1, "bin").unwrap();

        let data2 = b"data2";
        cache.store_blob(data2).unwrap();

        // Reference only first blob
        cache
            .create_model_symlink("model1", "data.bin", &b3_hash1)
            .unwrap();

        // GC should remove unreferenced blob
        let gc_stats = cache.garbage_collect().unwrap();
        assert_eq!(gc_stats.removed_blobs, 1);
        assert!(gc_stats.reclaimed_bytes > 0);

        // Verify only referenced blob remains
        let stats = cache.stats().unwrap();
        assert_eq!(stats.total_blobs, 1);
    }
}
