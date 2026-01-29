//! In-memory idempotency store using DashMap for concurrent access.

use super::types::{CachedResponse, IdempotencyKey, IdempotencyStatus, IDEMPOTENCY_TTL};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Entry stored in the idempotency cache
#[derive(Debug, Clone)]
struct CacheEntry {
    status: IdempotencyStatus,
    response: Option<CachedResponse>,
}

/// Thread-safe idempotency store for deduplicating requests.
///
/// Uses DashMap for lock-free concurrent access and stores:
/// - Completed requests with their cached responses
/// - In-progress requests with locks for waiting callers
pub struct IdempotencyStore {
    /// Cache mapping idempotency keys to their status and response
    cache: DashMap<String, CacheEntry>,
    /// Locks for in-progress requests to prevent thundering herd
    locks: DashMap<String, Arc<RwLock<()>>>,
}

impl Default for IdempotencyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl IdempotencyStore {
    /// Create a new empty idempotency store
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            locks: DashMap::new(),
        }
    }

    /// Check the status of an idempotency key
    pub fn check(&self, key: &IdempotencyKey) -> IdempotencyStatus {
        match self.cache.get(key.as_str()) {
            Some(entry) => entry.status,
            None => IdempotencyStatus::New,
        }
    }

    /// Get cached response if available and not expired
    pub fn get_response(&self, key: &IdempotencyKey) -> Option<CachedResponse> {
        self.cache.get(key.as_str()).and_then(|entry| {
            entry.response.as_ref().and_then(|resp| {
                if resp.is_expired() {
                    None
                } else {
                    Some(resp.clone())
                }
            })
        })
    }

    /// Mark a request as in-progress
    ///
    /// Returns false if the key is already in-progress or completed (race condition)
    ///
    /// FIXED (ADR-0023 Bug #2): Create lock BEFORE cache entry to prevent cleanup_expired()
    /// from removing locks between cache entry creation and lock insertion.
    pub fn mark_in_progress(&self, key: &IdempotencyKey) -> bool {
        // Create lock FIRST to ensure it exists before cache entry is visible
        // This prevents cleanup_expired() from removing the lock between cache entry creation
        // and lock insertion
        let lock = Arc::new(RwLock::new(()));

        // Insert lock first (idempotent - if lock exists, this is a no-op)
        self.locks.insert(key.0.clone(), lock);

        // Use entry API for atomic check-and-insert
        let mut inserted = false;
        self.cache.entry(key.0.clone()).or_insert_with(|| {
            inserted = true;
            CacheEntry {
                status: IdempotencyStatus::InProgress,
                response: None,
            }
        });

        if inserted {
            debug!(key = %key.as_str(), "Marked request as in-progress");
        } else {
            // Cache entry already existed - remove the lock we just created
            // (another request is handling this key)
            self.locks.remove(key.as_str());
        }

        inserted
    }

    /// Store a completed response for an idempotency key
    pub fn store_response(&self, key: &IdempotencyKey, response: CachedResponse) {
        self.cache.insert(
            key.0.clone(),
            CacheEntry {
                status: IdempotencyStatus::Completed,
                response: Some(response),
            },
        );

        // Release the lock so waiters can proceed
        if let Some((_, lock)) = self.locks.remove(key.as_str()) {
            // Drop the lock reference, allowing waiters to wake up
            drop(lock);
        }

        debug!(key = %key.as_str(), "Stored idempotent response");
    }

    /// Remove a key from the store (e.g., on 5xx error to allow retry)
    pub fn remove(&self, key: &IdempotencyKey) {
        self.cache.remove(key.as_str());
        self.locks.remove(key.as_str());
        debug!(key = %key.as_str(), "Removed idempotency key (allowing retry)");
    }

    /// Get the lock for waiting on an in-progress request
    pub fn get_lock(&self, key: &IdempotencyKey) -> Option<Arc<RwLock<()>>> {
        self.locks.get(key.as_str()).map(|l| l.clone())
    }

    /// Clean up expired entries
    ///
    /// Should be called periodically to prevent unbounded memory growth
    pub fn cleanup_expired(&self) -> usize {
        let now = chrono::Utc::now().timestamp();
        let ttl_secs = IDEMPOTENCY_TTL.as_secs() as i64;

        let initial_count = self.cache.len();

        self.cache.retain(|_, entry| {
            match &entry.response {
                Some(resp) => {
                    // Keep if not expired
                    now - resp.created_at < ttl_secs
                }
                None => {
                    // Keep in-progress entries (they'll timeout naturally)
                    entry.status == IdempotencyStatus::InProgress
                }
            }
        });

        // Also clean up orphaned locks
        let cache_keys: std::collections::HashSet<_> =
            self.cache.iter().map(|e| e.key().clone()).collect();
        self.locks.retain(|key, _| cache_keys.contains(key));

        let removed = initial_count - self.cache.len();
        if removed > 0 {
            info!(removed = removed, "Cleaned up expired idempotency entries");
        }

        removed
    }

    /// Get current cache size for metrics
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> IdempotencyKey {
        IdempotencyKey::new("test-key-123")
    }

    fn test_response() -> CachedResponse {
        CachedResponse {
            status_code: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: b"{}".to_vec(),
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    #[test]
    fn test_new_key_returns_new_status() {
        let store = IdempotencyStore::new();
        assert_eq!(store.check(&test_key()), IdempotencyStatus::New);
    }

    #[tokio::test]
    async fn test_concurrent_mark_in_progress_and_cleanup_no_orphaned_locks() {
        // FIXED (ADR-0023 Bug #2): Test that concurrent mark_in_progress() and cleanup_expired()
        // don't create orphaned locks. The fix ensures lock is created BEFORE cache entry.
        use tokio::time::{sleep, Duration};

        let store = Arc::new(IdempotencyStore::new());

        // Spawn concurrent mark_in_progress operations
        let mut handles = vec![];
        for i in 0..10 {
            let store_clone = store.clone();
            let key = IdempotencyKey::new(&format!("test-key-{}", i));
            let handle = tokio::spawn(async move {
                // Small delay to increase chance of race condition
                sleep(Duration::from_millis(i * 2)).await;
                store_clone.mark_in_progress(&key)
            });
            handles.push(handle);
        }

        // Spawn cleanup_expired concurrently
        let store_clone = store.clone();
        let cleanup_handle = tokio::spawn(async move {
            for _ in 0..5 {
                sleep(Duration::from_millis(5)).await;
                store_clone.cleanup_expired();
            }
        });

        // Wait for all mark_in_progress to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Wait for cleanup to complete
        cleanup_handle.await.unwrap();

        // Verify: no orphaned locks exist (all locks should have corresponding cache entries)
        let cache_keys: std::collections::HashSet<_> =
            store.cache.iter().map(|e| e.key().clone()).collect();

        for lock_key in store.locks.iter() {
            assert!(
                cache_keys.contains(lock_key.key()),
                "Found orphaned lock: {} (no corresponding cache entry)",
                lock_key.key()
            );
        }

        // Verify: all cache entries that are InProgress have locks
        for entry in store.cache.iter() {
            if entry.value().status == IdempotencyStatus::InProgress {
                assert!(
                    store.locks.contains_key(entry.key()),
                    "Found InProgress cache entry without lock: {}",
                    entry.key()
                );
            }
        }
    }

    #[test]
    fn test_mark_in_progress() {
        let store = IdempotencyStore::new();
        let key = test_key();

        assert!(store.mark_in_progress(&key));
        assert_eq!(store.check(&key), IdempotencyStatus::InProgress);

        // Second call should fail (key already exists)
        assert!(!store.mark_in_progress(&key));
    }

    #[test]
    fn test_store_and_retrieve_response() {
        let store = IdempotencyStore::new();
        let key = test_key();
        let response = test_response();

        store.mark_in_progress(&key);
        store.store_response(&key, response.clone());

        assert_eq!(store.check(&key), IdempotencyStatus::Completed);

        let cached = store.get_response(&key).expect("should have response");
        assert_eq!(cached.status_code, 200);
        assert_eq!(cached.body, b"{}");
    }

    #[test]
    fn test_remove_allows_retry() {
        let store = IdempotencyStore::new();
        let key = test_key();

        store.mark_in_progress(&key);
        store.remove(&key);

        assert_eq!(store.check(&key), IdempotencyStatus::New);
        assert!(store.mark_in_progress(&key)); // Should succeed again
    }

    #[test]
    fn test_expired_response_not_returned() {
        let store = IdempotencyStore::new();
        let key = test_key();

        // Create an expired response
        let expired_response = CachedResponse {
            status_code: 200,
            headers: vec![],
            body: vec![],
            created_at: chrono::Utc::now().timestamp() - (25 * 60 * 60), // 25 hours ago
        };

        store.mark_in_progress(&key);
        store.store_response(&key, expired_response);

        // Should not return expired response
        assert!(store.get_response(&key).is_none());
    }
}
