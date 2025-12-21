//! Prefix KV Cache for efficient prefix prefilling
//!
//! This module implements a prefix KV cache that stores key-value tensors
//! for static prefixes (system boilerplate, tenant policy, mode prefixes)
//! to avoid redundant prefill computation on repeated requests.
//!
//! ## Key Features
//!
//! - **Cryptographic Key**: Cache entries are keyed by `prefix_kv_key_b3`
//!   computed from context digest, prefix tokens, tokenizer, and model identity
//! - **Single-Flight**: Concurrent cache misses for the same key are deduplicated
//! - **LRU Eviction**: Evicts least-recently-used entries when capacity is exceeded
//! - **UMA Optimized**: Stores KV tensors as Vec<f32> for zero-copy MLX access
//!
//! See PRD: PrefixKvCache v1

use adapteros_core::singleflight::{SingleFlightMetrics, SingleFlightSync};
use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Operation label for SingleFlight metrics
const PREFIX_KV_BUILD_OPERATION: &str = "prefix_kv_build";

// =============================================================================
// PrefixKvEntry
// =============================================================================

/// A cached prefix KV entry containing per-layer key/value tensors.
#[derive(Debug)]
pub struct PrefixKvEntry {
    /// Per-layer key tensors (index = layer, inner vec = flattened KV data)
    pub keys: Vec<Vec<f32>>,
    /// Per-layer value tensors (index = layer, inner vec = flattened KV data)
    pub values: Vec<Vec<f32>>,
    /// Tenant that owns this entry
    pub tenant_id: String,
    /// Number of prefix tokens cached
    pub prefix_cached_token_count: u32,
    /// Total bytes of KV data
    pub kv_bytes: u64,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last access timestamp (atomic for concurrent updates)
    last_access_ns: AtomicU64,
    /// Active reference count (for eviction safety)
    pub active_refcount: AtomicU32,
}

impl PrefixKvEntry {
    /// Create a new prefix KV entry
    pub fn new(
        keys: Vec<Vec<f32>>,
        values: Vec<Vec<f32>>,
        tenant_id: String,
        prefix_cached_token_count: u32,
    ) -> Self {
        let kv_bytes = Self::compute_kv_bytes(&keys, &values);
        let now = Instant::now();

        Self {
            keys,
            values,
            tenant_id,
            prefix_cached_token_count,
            kv_bytes,
            created_at: now,
            last_access_ns: AtomicU64::new(0),
            active_refcount: AtomicU32::new(0),
        }
    }

    /// Compute total bytes for key and value tensors
    fn compute_kv_bytes(keys: &[Vec<f32>], values: &[Vec<f32>]) -> u64 {
        let key_bytes: usize = keys.iter().map(|k| k.len() * 4).sum();
        let value_bytes: usize = values.iter().map(|v| v.len() * 4).sum();
        (key_bytes + value_bytes) as u64
    }

    /// Record an access (updates last_access timestamp)
    pub fn record_access(&self) {
        let now_ns = Instant::now().elapsed().as_nanos() as u64;
        self.last_access_ns.store(now_ns, Ordering::Relaxed);
    }

    /// Get the last access time in nanoseconds since creation
    pub fn last_access_ns(&self) -> u64 {
        self.last_access_ns.load(Ordering::Relaxed)
    }

    /// Increment active refcount
    pub fn acquire(&self) -> u32 {
        self.active_refcount.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Decrement active refcount
    pub fn release(&self) -> u32 {
        self.active_refcount.fetch_sub(1, Ordering::SeqCst) - 1
    }

    /// Check if entry has active references
    pub fn is_in_use(&self) -> bool {
        self.active_refcount.load(Ordering::SeqCst) > 0
    }
}

// =============================================================================
// CacheStats
// =============================================================================

/// Statistics for the prefix KV cache
#[derive(Debug, Clone, Default)]
pub struct PrefixKvCacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Number of evictions
    pub evictions: u64,
    /// Number of current entries
    pub entry_count: u64,
    /// Current bytes used
    pub used_bytes: u64,
    /// Maximum bytes allowed
    pub max_bytes: u64,
    /// Number of in-flight builds
    pub in_flight_builds: u64,
}

impl PrefixKvCacheStats {
    /// Compute hit rate as percentage (0.0 to 100.0)
    pub fn hit_rate_percent(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

// =============================================================================
// PrefixKvCache
// =============================================================================

/// Main prefix KV cache implementation.
///
/// Thread-safe cache for prefix KV tensors with single-flight deduplication
/// and LRU eviction.
pub struct PrefixKvCache {
    /// Cache entries keyed by prefix_kv_key_b3
    entries: RwLock<HashMap<B3Hash, Arc<PrefixKvEntry>>>,
    /// Maximum byte budget
    max_bytes: u64,
    /// Current bytes used
    used_bytes: AtomicU64,
    /// SingleFlight for deduplicating concurrent builds
    /// Uses String error type since AosError is not Clone
    singleflight: SingleFlightSync<B3Hash, Arc<PrefixKvEntry>, String>,
    /// Statistics
    stats: RwLock<PrefixKvCacheStats>,
}

impl PrefixKvCache {
    /// Create a new prefix KV cache with the specified byte budget.
    pub fn new(max_bytes: u64) -> Self {
        tracing::info!(
            max_bytes_mb = max_bytes as f64 / (1024.0 * 1024.0),
            "Creating prefix KV cache"
        );

        Self {
            entries: RwLock::new(HashMap::new()),
            max_bytes,
            used_bytes: AtomicU64::new(0),
            singleflight: SingleFlightSync::new(PREFIX_KV_BUILD_OPERATION),
            stats: RwLock::new(PrefixKvCacheStats {
                max_bytes,
                ..Default::default()
            }),
        }
    }

    /// Create a new prefix KV cache with Prometheus metrics.
    pub fn with_metrics(max_bytes: u64, metrics: Arc<dyn SingleFlightMetrics>) -> Self {
        tracing::info!(
            max_bytes_mb = max_bytes as f64 / (1024.0 * 1024.0),
            "Creating prefix KV cache with metrics"
        );

        Self {
            entries: RwLock::new(HashMap::new()),
            max_bytes,
            used_bytes: AtomicU64::new(0),
            singleflight: SingleFlightSync::with_metrics(PREFIX_KV_BUILD_OPERATION, metrics),
            stats: RwLock::new(PrefixKvCacheStats {
                max_bytes,
                ..Default::default()
            }),
        }
    }

    /// Get an entry from the cache.
    ///
    /// Returns `Some(entry)` on cache hit, `None` on miss.
    /// Automatically records access time for LRU.
    pub fn get(&self, key: &B3Hash) -> Option<Arc<PrefixKvEntry>> {
        let entries = self.entries.read();
        if let Some(entry) = entries.get(key) {
            entry.record_access();
            let mut stats = self.stats.write();
            stats.hits += 1;
            tracing::trace!(
                key = %key.to_hex()[..16],
                prefix_tokens = entry.prefix_cached_token_count,
                "Prefix KV cache hit"
            );
            Some(Arc::clone(entry))
        } else {
            let mut stats = self.stats.write();
            stats.misses += 1;
            None
        }
    }

    /// Insert an entry into the cache.
    ///
    /// Evicts LRU entries if necessary to make room.
    /// Returns an error if the entry is larger than the max budget.
    pub fn insert(&self, key: B3Hash, entry: PrefixKvEntry) -> Result<Arc<PrefixKvEntry>> {
        let entry_bytes = entry.kv_bytes;

        // Check if entry fits within budget
        if entry_bytes > self.max_bytes {
            return Err(AosError::Validation(format!(
                "Prefix KV entry ({} bytes) exceeds max budget ({} bytes)",
                entry_bytes, self.max_bytes
            )));
        }

        // Evict until we have room
        self.evict_until_fits(entry_bytes)?;

        let entry = Arc::new(entry);

        // Insert into cache
        {
            let mut entries = self.entries.write();
            entries.insert(key, Arc::clone(&entry));
        }

        // Update stats
        self.used_bytes.fetch_add(entry_bytes, Ordering::SeqCst);
        {
            let mut stats = self.stats.write();
            stats.entry_count += 1;
            stats.used_bytes = self.used_bytes.load(Ordering::SeqCst);
        }

        tracing::debug!(
            key = %key.to_hex()[..16],
            prefix_tokens = entry.prefix_cached_token_count,
            kv_bytes = entry_bytes,
            "Inserted prefix KV cache entry"
        );

        Ok(entry)
    }

    /// Get or build an entry (single-flight deduplication).
    ///
    /// If the key is in the cache, returns the cached entry.
    /// If the key is being built by another thread, waits for that build.
    /// Otherwise, runs the builder function and caches the result.
    ///
    /// # Arguments
    /// * `key` - The prefix KV cache key
    /// * `builder` - Function to build the entry on cache miss
    ///
    /// # Returns
    /// The cached or newly built entry.
    pub fn get_or_build<F>(&self, key: B3Hash, builder: F) -> Result<Arc<PrefixKvEntry>>
    where
        F: FnOnce() -> Result<PrefixKvEntry>,
    {
        // Fast path: check cache first
        if let Some(entry) = self.get(&key) {
            return Ok(entry);
        }

        // Use SingleFlightSync for build deduplication
        let entry = self
            .singleflight
            .get_or_load(key, || self.build_and_cache_entry(&key, builder))
            .map_err(AosError::Lifecycle)?;

        Ok(entry)
    }

    /// Build an entry and cache it (called by SingleFlightSync leader).
    ///
    /// Re-checks cache before building to handle the race where a very fast
    /// build completes before other threads register as waiters.
    fn build_and_cache_entry<F>(
        &self,
        key: &B3Hash,
        builder: F,
    ) -> std::result::Result<Arc<PrefixKvEntry>, String>
    where
        F: FnOnce() -> Result<PrefixKvEntry>,
    {
        // Re-check cache before building. This handles the race where:
        // 1. Multiple threads pass the fast-path cache check
        // 2. One becomes SingleFlight leader and completes very quickly
        // 3. Another becomes a NEW leader (because entry was removed)
        {
            let entries = self.entries.read();
            if let Some(entry) = entries.get(key) {
                tracing::debug!(
                    key = %key.to_hex()[..16],
                    "Prefix KV found in cache during SingleFlight leader re-check"
                );
                entry.record_access();
                let mut stats = self.stats.write();
                stats.hits += 1;
                return Ok(Arc::clone(entry));
            }
        }

        // Run the builder
        tracing::debug!(
            key = %key.to_hex()[..16],
            "Building prefix KV cache entry"
        );

        let entry_result = builder().map_err(|e| e.to_string())?;

        // Try to insert into cache
        self.insert(*key, entry_result).map_err(|e| e.to_string())
    }

    /// Evict LRU entries until the specified bytes can fit.
    fn evict_until_fits(&self, needed_bytes: u64) -> Result<()> {
        let current_used = self.used_bytes.load(Ordering::SeqCst);
        let available = self.max_bytes.saturating_sub(current_used);

        if available >= needed_bytes {
            return Ok(());
        }

        let to_free = needed_bytes - available;
        let mut freed = 0u64;
        let mut evicted_keys = Vec::new();

        // Find LRU entries to evict
        {
            let entries = self.entries.read();
            let mut candidates: Vec<_> = entries
                .iter()
                .filter(|(_, entry)| !entry.is_in_use())
                .map(|(key, entry)| (*key, entry.last_access_ns(), entry.kv_bytes))
                .collect();

            // Sort by last access (oldest first)
            candidates.sort_by_key(|(_, access_ns, _)| *access_ns);

            for (key, _, bytes) in candidates {
                evicted_keys.push(key);
                freed += bytes;
                if freed >= to_free {
                    break;
                }
            }
        }

        // Perform evictions
        if freed < to_free {
            return Err(AosError::MemoryPressure(format!(
                "Cannot evict enough prefix KV entries: need {} bytes, can free {} bytes",
                to_free, freed
            )));
        }

        {
            let mut entries = self.entries.write();
            let mut stats = self.stats.write();

            for key in &evicted_keys {
                if let Some(entry) = entries.remove(key) {
                    self.used_bytes.fetch_sub(entry.kv_bytes, Ordering::SeqCst);
                    stats.evictions += 1;
                    stats.entry_count = stats.entry_count.saturating_sub(1);

                    tracing::debug!(
                        key = %key.to_hex()[..16],
                        kv_bytes = entry.kv_bytes,
                        "Evicted prefix KV cache entry"
                    );
                }
            }

            stats.used_bytes = self.used_bytes.load(Ordering::SeqCst);
        }

        Ok(())
    }

    /// Remove an entry from the cache.
    pub fn remove(&self, key: &B3Hash) -> Option<Arc<PrefixKvEntry>> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.remove(key) {
            self.used_bytes.fetch_sub(entry.kv_bytes, Ordering::SeqCst);

            let mut stats = self.stats.write();
            stats.entry_count = stats.entry_count.saturating_sub(1);
            stats.used_bytes = self.used_bytes.load(Ordering::SeqCst);

            Some(entry)
        } else {
            None
        }
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();
        self.used_bytes.store(0, Ordering::SeqCst);

        let mut stats = self.stats.write();
        stats.entry_count = 0;
        stats.used_bytes = 0;
    }

    /// Get cache statistics.
    ///
    /// Note: `in_flight_builds` is pulled from SingleFlightSync at query time
    /// for accuracy.
    pub fn stats(&self) -> PrefixKvCacheStats {
        let mut stats = self.stats.read().clone();
        // Pull live in-flight count from SingleFlightSync
        stats.in_flight_builds = self.singleflight.stats().pending_loads as u64;
        stats
    }

    /// Get current number of entries.
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Get current bytes used.
    pub fn used_bytes(&self) -> u64 {
        self.used_bytes.load(Ordering::SeqCst)
    }

    /// Get maximum bytes allowed.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }
}

// Thread safety markers
unsafe impl Send for PrefixKvCache {}
unsafe impl Sync for PrefixKvCache {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        tenant: &str,
        tokens: u32,
        layers: usize,
        size_per_layer: usize,
    ) -> PrefixKvEntry {
        let keys: Vec<Vec<f32>> = (0..layers).map(|_| vec![1.0; size_per_layer]).collect();
        let values: Vec<Vec<f32>> = (0..layers).map(|_| vec![2.0; size_per_layer]).collect();
        PrefixKvEntry::new(keys, values, tenant.to_string(), tokens)
    }

    #[test]
    fn test_entry_kv_bytes() {
        let entry = make_entry("tenant1", 100, 32, 1024);
        // 32 layers * 1024 floats * 4 bytes * 2 (K+V) = 262144 bytes
        assert_eq!(entry.kv_bytes, 32 * 1024 * 4 * 2);
    }

    #[test]
    fn test_cache_get_miss() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");

        assert!(cache.get(&key).is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");
        let entry = make_entry("tenant1", 100, 2, 128);

        cache.insert(key, entry).unwrap();

        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.prefix_cached_token_count, 100);
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_eviction() {
        // Small cache: 768 bytes (can only fit one 512-byte entry)
        // Entry size = 2 layers * 32 floats * 4 bytes * 2 (K+V) = 512 bytes
        let cache = PrefixKvCache::new(768);

        let key1 = B3Hash::hash(b"key1");
        let key2 = B3Hash::hash(b"key2");

        // Entry that uses 512 bytes
        let entry1 = make_entry("tenant1", 10, 2, 32);
        cache.insert(key1, entry1).unwrap();

        // Second entry requires eviction (512 + 512 = 1024 > 768)
        let entry2 = make_entry("tenant1", 20, 2, 32);
        cache.insert(key2, entry2).unwrap();

        // First entry should be evicted
        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some());
        assert_eq!(cache.stats().evictions, 1);
    }

    #[test]
    fn test_cache_entry_too_large() {
        let cache = PrefixKvCache::new(1024);
        let key = B3Hash::hash(b"key");

        // Entry larger than max_bytes
        let entry = make_entry("tenant1", 100, 10, 1024);
        let result = cache.insert(key, entry);

        assert!(result.is_err());
    }

    #[test]
    fn test_cache_clear() {
        let cache = PrefixKvCache::new(1024 * 1024);

        let key1 = B3Hash::hash(b"key1");
        let key2 = B3Hash::hash(b"key2");

        cache
            .insert(key1, make_entry("tenant1", 10, 1, 32))
            .unwrap();
        cache
            .insert(key2, make_entry("tenant1", 20, 1, 32))
            .unwrap();

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert_eq!(cache.used_bytes(), 0);
    }

    #[test]
    fn test_get_or_build_hit() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");

        // Pre-populate cache
        cache
            .insert(key, make_entry("tenant1", 100, 2, 128))
            .unwrap();

        // get_or_build should return cached entry without calling builder
        let builder_called = std::sync::atomic::AtomicBool::new(false);
        let result = cache.get_or_build(key, || {
            builder_called.store(true, Ordering::SeqCst);
            Ok(make_entry("tenant1", 999, 2, 128))
        });

        assert!(result.is_ok());
        assert!(!builder_called.load(Ordering::SeqCst));
        assert_eq!(result.unwrap().prefix_cached_token_count, 100);
    }

    #[test]
    fn test_get_or_build_miss() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");

        // get_or_build should call builder and cache result
        let builder_called = std::sync::atomic::AtomicBool::new(false);
        let result = cache.get_or_build(key, || {
            builder_called.store(true, Ordering::SeqCst);
            Ok(make_entry("tenant1", 100, 2, 128))
        });

        assert!(result.is_ok());
        assert!(builder_called.load(Ordering::SeqCst));
        assert_eq!(result.unwrap().prefix_cached_token_count, 100);

        // Entry should now be cached
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_get_or_build_error_no_poison() {
        let cache = PrefixKvCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");

        // Builder fails
        let result = cache.get_or_build(key, || {
            Err(AosError::Validation("build failed".to_string()))
        });

        assert!(result.is_err());

        // Cache should not have a poisoned entry
        assert!(cache.get(&key).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_single_flight_dedup() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(PrefixKvCache::new(1024 * 1024));
        let key = B3Hash::hash(b"shared_key");
        let build_count = Arc::new(AtomicU32::new(0));

        // Spawn multiple threads that all try to get_or_build the same key
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let build_count = Arc::clone(&build_count);

                thread::spawn(move || {
                    cache.get_or_build(key, || {
                        // Simulate expensive build
                        thread::sleep(std::time::Duration::from_millis(10));
                        build_count.fetch_add(1, Ordering::SeqCst);
                        Ok(make_entry("tenant1", 100, 2, 128))
                    })
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result.is_ok());
        }

        // Builder should have been called exactly once
        assert_eq!(
            build_count.load(Ordering::SeqCst),
            1,
            "single-flight should deduplicate concurrent builds"
        );

        // Entry should be in cache
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn test_entry_refcount() {
        let entry = make_entry("tenant1", 100, 2, 128);

        assert!(!entry.is_in_use());

        entry.acquire();
        assert!(entry.is_in_use());

        entry.acquire();
        assert!(entry.is_in_use());

        entry.release();
        assert!(entry.is_in_use());

        entry.release();
        assert!(!entry.is_in_use());
    }

    #[test]
    fn test_no_evict_in_use() {
        // Small cache: 768 bytes (can only fit one 512-byte entry)
        // Entry size = 2 layers * 32 floats * 4 bytes * 2 (K+V) = 512 bytes
        let cache = PrefixKvCache::new(768);

        let key1 = B3Hash::hash(b"key1");
        let key2 = B3Hash::hash(b"key2");

        // Insert first entry and acquire a reference
        let entry1 = make_entry("tenant1", 10, 2, 32);
        cache.insert(key1, entry1).unwrap();
        cache.get(&key1).unwrap().acquire();

        // Try to insert another entry that would require eviction
        // (512 + 512 = 1024 > 768, but key1 is in use)
        let entry2 = make_entry("tenant1", 20, 2, 32);
        let result = cache.insert(key2, entry2);

        // Should fail because key1 is in use and cannot be evicted
        assert!(result.is_err());
    }
}
