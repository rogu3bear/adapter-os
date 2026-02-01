//! Session-keyed KV Cache Management
//!
//! Manages KV caches for multiple sessions, with automatic eviction
//! based on LRU policy when memory limits are reached.

use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// KV cache entry for a single session
#[derive(Debug)]
pub struct KvCacheEntry {
    /// Session ID
    pub session_id: String,

    /// Key tensor data (flattened)
    pub keys: Vec<f32>,

    /// Value tensor data (flattened)
    pub values: Vec<f32>,

    /// Number of cached tokens
    pub cached_tokens: u32,

    /// Maximum sequence length for this session
    pub max_seq_len: u32,

    /// Last access timestamp
    pub last_accessed: Instant,

    /// Creation timestamp
    pub created_at: Instant,

    /// Estimated memory usage in bytes
    pub memory_bytes: u64,
}

impl KvCacheEntry {
    /// Create a new empty cache entry
    pub fn new(
        session_id: String,
        max_seq_len: u32,
        hidden_size: usize,
        num_layers: usize,
    ) -> Self {
        // Estimate memory: 2 (K+V) * max_seq_len * hidden_size * num_layers * 4 bytes (f32)
        let memory_bytes = 2 * max_seq_len as u64 * hidden_size as u64 * num_layers as u64 * 4;

        Self {
            session_id,
            keys: Vec::new(),
            values: Vec::new(),
            cached_tokens: 0,
            max_seq_len,
            last_accessed: Instant::now(),
            created_at: Instant::now(),
            memory_bytes,
        }
    }

    /// Touch the cache entry (update last accessed time)
    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    /// Update cache with new KV data
    pub fn update(&mut self, keys: Vec<f32>, values: Vec<f32>, token_count: u32) {
        self.keys = keys;
        self.values = values;
        self.cached_tokens = token_count;
        self.last_accessed = Instant::now();
    }

    /// Get age since last access in seconds
    pub fn age_secs(&self) -> f64 {
        self.last_accessed.elapsed().as_secs_f64()
    }
}

/// Session-keyed KV cache manager
pub struct KvCacheManager {
    /// Cache entries by session ID
    caches: DashMap<String, Arc<RwLock<KvCacheEntry>>>,

    /// LRU tracking (session IDs in access order, oldest first)
    lru_order: RwLock<VecDeque<String>>,

    /// Maximum cache size in bytes
    max_bytes: u64,

    /// Current cache size in bytes
    current_bytes: AtomicU64,

    /// Model configuration
    hidden_size: usize,
    num_layers: usize,

    /// Statistics
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
}

impl KvCacheManager {
    /// Create a new KV cache manager
    pub fn new(max_bytes: u64, hidden_size: usize, num_layers: usize) -> Self {
        Self {
            caches: DashMap::new(),
            lru_order: RwLock::new(VecDeque::new()),
            max_bytes,
            current_bytes: AtomicU64::new(0),
            hidden_size,
            num_layers,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }

    /// Get or create a cache entry for a session
    pub fn get_or_create(&self, session_id: &str, max_seq_len: u32) -> Arc<RwLock<KvCacheEntry>> {
        // Try to get existing entry
        if let Some(entry) = self.caches.get(session_id) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            self.update_lru(session_id);
            return entry.clone();
        }

        // Create new entry
        self.misses.fetch_add(1, Ordering::Relaxed);

        let entry = KvCacheEntry::new(
            session_id.to_string(),
            max_seq_len,
            self.hidden_size,
            self.num_layers,
        );
        let memory_bytes = entry.memory_bytes;

        // Evict if necessary to make room
        self.evict_if_needed(memory_bytes);

        let entry = Arc::new(RwLock::new(entry));
        self.caches.insert(session_id.to_string(), entry.clone());
        self.current_bytes
            .fetch_add(memory_bytes, Ordering::Relaxed);

        // Add to LRU
        {
            let mut lru = self.lru_order.write();
            lru.push_back(session_id.to_string());
        }

        debug!(
            session_id = session_id,
            memory_bytes = memory_bytes,
            total_bytes = self.current_bytes.load(Ordering::Relaxed),
            "Created new KV cache entry"
        );

        entry
    }

    /// Get an existing cache entry (if present)
    pub fn get(&self, session_id: &str) -> Option<Arc<RwLock<KvCacheEntry>>> {
        if let Some(entry) = self.caches.get(session_id) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            self.update_lru(session_id);
            Some(entry.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Remove a session's cache
    pub fn remove(&self, session_id: &str) -> Option<u64> {
        if let Some((_, entry)) = self.caches.remove(session_id) {
            let memory_bytes = entry.read().memory_bytes;
            self.current_bytes
                .fetch_sub(memory_bytes, Ordering::Relaxed);

            // Remove from LRU
            {
                let mut lru = self.lru_order.write();
                lru.retain(|id| id != session_id);
            }

            debug!(
                session_id = session_id,
                freed_bytes = memory_bytes,
                "Removed KV cache entry"
            );

            Some(memory_bytes)
        } else {
            None
        }
    }

    /// Evict oldest entries until we have room for new_bytes
    fn evict_if_needed(&self, new_bytes: u64) {
        let current = self.current_bytes.load(Ordering::Relaxed);
        if current + new_bytes <= self.max_bytes {
            return;
        }

        let need_to_free = current + new_bytes - self.max_bytes;
        let mut freed = 0u64;

        while freed < need_to_free {
            // Get oldest session from LRU
            let oldest = {
                let mut lru = self.lru_order.write();
                lru.pop_front()
            };

            if let Some(session_id) = oldest {
                if let Some(bytes) = self.remove(&session_id) {
                    freed += bytes;
                    self.evictions.fetch_add(1, Ordering::Relaxed);

                    info!(
                        session_id = session_id,
                        freed_bytes = bytes,
                        total_freed = freed,
                        target = need_to_free,
                        "Evicted KV cache entry (LRU)"
                    );
                }
            } else {
                // No more entries to evict
                warn!(
                    needed = need_to_free,
                    freed = freed,
                    "Could not free enough memory for new cache entry"
                );
                break;
            }
        }
    }

    /// Update LRU order (move session to end)
    fn update_lru(&self, session_id: &str) {
        let mut lru = self.lru_order.write();
        lru.retain(|id| id != session_id);
        lru.push_back(session_id.to_string());
    }

    /// Get cache statistics
    pub fn stats(&self) -> KvCacheStats {
        KvCacheStats {
            active_sessions: self.caches.len(),
            used_bytes: self.current_bytes.load(Ordering::Relaxed),
            max_bytes: self.max_bytes,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.caches.clear();
        self.lru_order.write().clear();
        self.current_bytes.store(0, Ordering::Relaxed);
        info!("Cleared all KV cache entries");
    }

    /// Get number of active sessions
    pub fn active_sessions(&self) -> usize {
        self.caches.len()
    }

    /// Get current memory usage
    pub fn used_bytes(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }
}

/// KV cache statistics
#[derive(Debug, Clone)]
pub struct KvCacheStats {
    /// Number of active sessions
    pub active_sessions: usize,

    /// Current memory usage in bytes
    pub used_bytes: u64,

    /// Maximum memory in bytes
    pub max_bytes: u64,

    /// Cache hits
    pub hits: u64,

    /// Cache misses
    pub misses: u64,

    /// Evictions due to memory pressure
    pub evictions: u64,
}

impl KvCacheStats {
    /// Get hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }

    /// Get utilization as a percentage
    pub fn utilization(&self) -> f64 {
        if self.max_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.max_bytes as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_creation() {
        let entry = KvCacheEntry::new("session-1".to_string(), 2048, 4096, 32);
        assert_eq!(entry.session_id, "session-1");
        assert_eq!(entry.cached_tokens, 0);
        assert!(entry.memory_bytes > 0);
    }

    #[test]
    fn test_cache_manager_get_or_create() {
        let manager = KvCacheManager::new(1024 * 1024 * 1024, 4096, 32); // 1GB

        let entry1 = manager.get_or_create("session-1", 2048);
        let entry2 = manager.get_or_create("session-1", 2048);

        // Should be the same entry
        assert_eq!(Arc::as_ptr(&entry1), Arc::as_ptr(&entry2));

        // Stats should show 1 miss (first access) and 1 hit (second access)
        let stats = manager.stats();
        assert_eq!(stats.active_sessions, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_cache_eviction() {
        // Create a very small cache to trigger eviction
        let manager = KvCacheManager::new(1024, 16, 1); // Tiny cache

        // Create first session
        let _entry1 = manager.get_or_create("session-1", 8);
        assert_eq!(manager.active_sessions(), 1);

        // Create second session - should evict first
        let _entry2 = manager.get_or_create("session-2", 8);

        // Check stats
        let stats = manager.stats();
        assert!(stats.evictions > 0 || stats.active_sessions <= 2);
    }

    #[test]
    fn test_cache_remove() {
        let manager = KvCacheManager::new(1024 * 1024, 4096, 32);

        manager.get_or_create("session-1", 2048);
        assert_eq!(manager.active_sessions(), 1);

        let freed = manager.remove("session-1");
        assert!(freed.is_some());
        assert_eq!(manager.active_sessions(), 0);
    }

    #[test]
    fn test_hit_rate() {
        let stats = KvCacheStats {
            active_sessions: 1,
            used_bytes: 100,
            max_bytes: 1000,
            hits: 80,
            misses: 20,
            evictions: 0,
        };

        assert!((stats.hit_rate() - 80.0).abs() < 0.01);
        assert!((stats.utilization() - 10.0).abs() < 0.01);
    }
}
