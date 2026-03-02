//! JTI (JWT ID) cache persistence for worker token replay defense.
//!
//! This module provides persistent storage for the JTI cache, allowing workers
//! to maintain replay defense across restarts. Without persistence, tokens
//! validated before shutdown could be replayed after restart (within TTL).
//!
//! ## Storage Format
//!
//! JTI entries are stored as JSON array:
//!
//! ```json
//! [
//!   { "jti": "req-123", "exp_unix": 1234567890 },
//!   { "jti": "req-456", "exp_unix": 1234567900 }
//! ]
//! ```
//!
//! ## Lifecycle
//!
//! 1. **Load on startup**: Call `JtiCacheStore::load()` to restore the cache
//! 2. **Validate tokens**: Use `check_and_add()` to validate and cache JTIs
//! 3. **Persist on shutdown**: Call `persist()` before worker exits
//!
//! ## Atomic Writes
//!
//! Persistence uses temp-then-rename pattern for crash safety.

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default JTI cache size (entries).
///
/// This should be sized to hold at least `requests_per_second * token_ttl_seconds * 2`
/// to prevent legitimate evictions from causing replay vulnerabilities.
pub const DEFAULT_JTI_CACHE_SIZE: usize = 10000;

/// Environment variable for configuring JTI cache size.
pub const JTI_CACHE_SIZE_ENV: &str = "AOS_JTI_CACHE_SIZE";

/// A single JTI entry for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JtiEntry {
    /// The JWT ID (request ID)
    pub jti: String,
    /// Unix timestamp when this JTI expires
    pub exp_unix: i64,
}

/// Persistent JTI cache store.
///
/// Wraps an LRU cache with persistence capabilities, allowing the cache
/// to survive worker restarts and maintain replay defense.
pub struct JtiCacheStore {
    /// The in-memory LRU cache
    cache: LruCache<String, i64>,
    /// Path to the persistence file
    persist_path: PathBuf,
    /// Cache capacity
    capacity: usize,
}

impl JtiCacheStore {
    /// Create a new JTI cache store with the given capacity and persistence path.
    ///
    /// Does NOT load from disk - call `load()` or `load_or_new()` instead.
    pub fn new(capacity: usize, persist_path: PathBuf) -> Self {
        let size = NonZeroUsize::new(capacity)
            .unwrap_or(NonZeroUsize::new(DEFAULT_JTI_CACHE_SIZE).unwrap());
        Self {
            cache: LruCache::new(size),
            persist_path,
            capacity,
        }
    }

    /// Load an existing cache from disk, or create a new empty one.
    ///
    /// Expired entries are automatically pruned during load.
    pub fn load_or_new(persist_path: PathBuf) -> Self {
        let capacity = Self::get_cache_size_from_env();
        Self::load_or_new_with_capacity(persist_path, capacity)
    }

    /// Load an existing cache from disk with a specific capacity, or create a new empty one.
    pub fn load_or_new_with_capacity(persist_path: PathBuf, capacity: usize) -> Self {
        match Self::load_from_file(&persist_path, capacity) {
            Ok(store) => {
                tracing::info!(
                    path = %persist_path.display(),
                    entries = store.cache.len(),
                    capacity = capacity,
                    "Loaded JTI cache from disk"
                );
                store
            }
            Err(e) => {
                tracing::debug!(
                    path = %persist_path.display(),
                    error = %e,
                    "No existing JTI cache found, starting fresh"
                );
                Self::new(capacity, persist_path)
            }
        }
    }

    /// Get the cache size from environment variable or return default.
    pub fn get_cache_size_from_env() -> usize {
        std::env::var(JTI_CACHE_SIZE_ENV)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_JTI_CACHE_SIZE)
    }

    /// Load the cache from a file.
    fn load_from_file(path: &Path, capacity: usize) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let entries: Vec<JtiEntry> = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let now = chrono::Utc::now().timestamp();
        let size = NonZeroUsize::new(capacity)
            .unwrap_or(NonZeroUsize::new(DEFAULT_JTI_CACHE_SIZE).unwrap());
        let mut cache = LruCache::new(size);

        // Only load entries that haven't expired
        let mut loaded = 0;
        let mut expired = 0;
        for entry in entries {
            if entry.exp_unix > now {
                cache.put(entry.jti, entry.exp_unix);
                loaded += 1;
            } else {
                expired += 1;
            }
        }

        if expired > 0 {
            tracing::debug!(
                loaded = loaded,
                expired = expired,
                "Pruned expired JTI entries during load"
            );
        }

        Ok(Self {
            cache,
            persist_path: path.to_path_buf(),
            capacity,
        })
    }

    /// Check if a JTI has been seen and add it to the cache if not.
    ///
    /// Returns `true` if this is a replay (JTI already seen and not expired).
    /// Returns `false` if this is a new JTI (added to cache).
    pub fn check_and_add(&mut self, jti: &str, exp: i64) -> bool {
        let now = chrono::Utc::now().timestamp();

        if let Some(&cached_exp) = self.cache.get(jti) {
            // Check if the cached entry is still valid (not expired)
            if cached_exp > now {
                return true; // Replay detected
            }
            // Expired entry, will be overwritten below
        }

        // Add/update the entry
        self.cache.put(jti.to_string(), exp);
        false // Not a replay
    }

    /// Get a reference to the underlying LRU cache.
    ///
    /// Useful for interoperability with existing code that uses raw LruCache.
    pub fn cache(&self) -> &LruCache<String, i64> {
        &self.cache
    }

    /// Get a mutable reference to the underlying LRU cache.
    pub fn cache_mut(&mut self) -> &mut LruCache<String, i64> {
        &mut self.cache
    }

    /// Get the number of entries in the cache.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get the cache capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Persist the cache to disk.
    ///
    /// Uses atomic write (temp-then-rename) to prevent corruption.
    pub fn persist(&self) -> Result<(), std::io::Error> {
        use std::io::Write;

        let now = chrono::Utc::now().timestamp();

        // Collect non-expired entries
        let entries: Vec<JtiEntry> = self
            .cache
            .iter()
            .filter(|(_, &exp)| exp > now)
            .map(|(jti, &exp)| JtiEntry {
                jti: jti.clone(),
                exp_unix: exp,
            })
            .collect();

        // Skip write if no entries to persist
        if entries.is_empty() {
            tracing::debug!("No JTI entries to persist (all expired)");
            // Remove the file if it exists
            if self.persist_path.exists() {
                std::fs::remove_file(&self.persist_path)?;
            }
            return Ok(());
        }

        // Ensure directory exists
        if let Some(parent) = self.persist_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Generate unique temp file name
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = self.persist_path.with_extension(format!("tmp.{}", nanos));

        // Write to temp file with 0600 permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp_path)?;

            let json = serde_json::to_string_pretty(&entries)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        #[cfg(not(unix))]
        {
            let json = serde_json::to_string_pretty(&entries)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(&temp_path, json)?;
        }

        // Atomic rename
        std::fs::rename(&temp_path, &self.persist_path)?;

        tracing::info!(
            path = %self.persist_path.display(),
            entries = entries.len(),
            "Persisted JTI cache to disk"
        );

        Ok(())
    }

    /// Clear the cache and remove the persistence file.
    pub fn clear(&mut self) -> Result<(), std::io::Error> {
        self.cache.clear();
        if self.persist_path.exists() {
            std::fs::remove_file(&self.persist_path)?;
        }
        Ok(())
    }
}

impl Drop for JtiCacheStore {
    fn drop(&mut self) {
        // Attempt to persist on drop, but don't panic on failure
        if let Err(e) = self.persist() {
            tracing::warn!(
                error = %e,
                path = %self.persist_path.display(),
                "Failed to persist JTI cache on drop"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_cache_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jti_cache.json");
        (dir, path)
    }

    #[test]
    fn test_new_cache() {
        let (_dir, path) = test_cache_path();
        let cache = JtiCacheStore::new(100, path);
        assert!(cache.is_empty());
        assert_eq!(cache.capacity(), 100);
    }

    #[test]
    fn test_check_and_add() {
        let (_dir, path) = test_cache_path();
        let mut cache = JtiCacheStore::new(100, path);

        let exp = chrono::Utc::now().timestamp() + 60; // expires in 60 seconds

        // First check should not be a replay
        assert!(!cache.check_and_add("req-1", exp));
        assert_eq!(cache.len(), 1);

        // Second check with same JTI should be a replay
        assert!(cache.check_and_add("req-1", exp));
        assert_eq!(cache.len(), 1);

        // Different JTI should not be a replay
        assert!(!cache.check_and_add("req-2", exp));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_expired_jti_not_replay() {
        let (_dir, path) = test_cache_path();
        let mut cache = JtiCacheStore::new(100, path);

        // Add an expired JTI
        let exp = chrono::Utc::now().timestamp() - 10; // expired 10 seconds ago
        cache.cache_mut().put("req-expired".to_string(), exp);

        // Should not be detected as replay since it's expired
        let new_exp = chrono::Utc::now().timestamp() + 60;
        assert!(!cache.check_and_add("req-expired", new_exp));
    }

    #[test]
    fn test_persist_and_load() {
        let (dir, path) = test_cache_path();

        let exp = chrono::Utc::now().timestamp() + 3600; // expires in 1 hour

        // Create and populate cache
        {
            let mut cache = JtiCacheStore::new(100, path.clone());
            cache.check_and_add("req-1", exp);
            cache.check_and_add("req-2", exp);
            cache.check_and_add("req-3", exp);
            cache.persist().unwrap();
        }

        // Load and verify
        {
            let cache = JtiCacheStore::load_or_new_with_capacity(path.clone(), 100);
            assert_eq!(cache.len(), 3);
            // Check that the entries are still there
            assert!(cache.cache().peek("req-1").is_some());
            assert!(cache.cache().peek("req-2").is_some());
            assert!(cache.cache().peek("req-3").is_some());
        }

        drop(dir);
    }

    #[test]
    fn test_expired_entries_pruned_on_load() {
        let (dir, path) = test_cache_path();

        // Create cache with mixed expiry times
        {
            let mut cache = JtiCacheStore::new(100, path.clone());
            let now = chrono::Utc::now().timestamp();
            cache.cache_mut().put("expired-1".to_string(), now - 10);
            cache.cache_mut().put("expired-2".to_string(), now - 5);
            cache.cache_mut().put("valid-1".to_string(), now + 3600);
            cache.cache_mut().put("valid-2".to_string(), now + 7200);
            cache.persist().unwrap();
        }

        // Load and verify only valid entries are loaded
        {
            let cache = JtiCacheStore::load_or_new_with_capacity(path.clone(), 100);
            assert_eq!(cache.len(), 2);
            assert!(cache.cache().peek("valid-1").is_some());
            assert!(cache.cache().peek("valid-2").is_some());
            assert!(cache.cache().peek("expired-1").is_none());
            assert!(cache.cache().peek("expired-2").is_none());
        }

        drop(dir);
    }

    #[test]
    fn test_persist_empty_removes_file() {
        let (_dir, path) = test_cache_path();

        // Create a cache file
        let mut cache = JtiCacheStore::new(100, path.clone());
        let exp = chrono::Utc::now().timestamp() + 60;
        cache.check_and_add("req-1", exp);
        cache.persist().unwrap();
        assert!(path.exists());

        // Clear and persist
        cache.clear().unwrap();
        cache.persist().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let (_dir, path) = test_cache_path();
        let cache = JtiCacheStore::load_or_new_with_capacity(path, 100);
        assert!(cache.is_empty());
    }
}
