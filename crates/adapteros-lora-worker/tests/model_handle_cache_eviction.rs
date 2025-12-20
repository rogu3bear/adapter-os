//! PRD-RECT-003: Backend Cache Eviction Observability Tests
//!
//! These tests validate that the model handle cache has stable eviction
//! policies and proper observability for cache operations.

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_worker::model_handle_cache::{
    CacheEventListener, ModelHandle, ModelHandleCache,
};
use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Test listener that counts events
struct CountingListener {
    loads: AtomicU64,
    reuses: AtomicU64,
    evictions: AtomicU64,
    errors: AtomicU64,
}

impl CountingListener {
    fn new() -> Self {
        Self {
            loads: AtomicU64::new(0),
            reuses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }
}

impl CacheEventListener for CountingListener {
    fn on_load(&self, _key: &ModelKey, _memory_bytes: u64) {
        self.loads.fetch_add(1, Ordering::Relaxed);
    }

    fn on_reuse(&self, _key: &ModelKey) {
        self.reuses.fetch_add(1, Ordering::Relaxed);
    }

    fn on_evict(&self, _key: &ModelKey) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    fn on_error(&self, _key: &ModelKey, _error: &adapteros_core::AosError) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
}

fn create_test_key(hash: &str) -> ModelKey {
    ModelKey::new(
        BackendType::Metal,
        B3Hash::hash(hash.as_bytes()),
        ModelCacheIdentity::new("test-kernel", "fp16", "per_request"),
    )
}

fn create_test_handle(size: usize) -> ModelHandle {
    ModelHandle::Metal(Arc::new(vec![0u8; size]))
}

/// Helper to load and cache a model
fn load_into_cache(cache: &ModelHandleCache, key: &ModelKey, size: u64) {
    cache
        .get_or_load(key, || Ok((create_test_handle(size as usize), size)))
        .expect("load should succeed");
}

// ============================================================================
// PRD-RECT-003: Cache Eviction Policy Tests
// ============================================================================

#[test]
fn cache_tracks_hits_and_misses() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024); // 100 MB
    let key = create_test_key("test-model-hash");

    // Initial access is a miss (load)
    let stats_before = cache.stats();
    assert_eq!(stats_before.hits, 0);
    assert_eq!(stats_before.misses, 0);

    // Load the model
    load_into_cache(&cache, &key, 1024);

    // First load results in a miss
    let stats_after_load = cache.stats();
    assert_eq!(stats_after_load.misses, 1, "First load should be a miss");

    // Access again should be a hit
    cache
        .get_or_load(&key, || panic!("loader should not be called on cache hit"))
        .expect("get should succeed");

    let stats_after = cache.stats();
    assert_eq!(stats_after.hits, 1, "Should record a hit on second access");
}

#[test]
fn cache_tracks_memory_usage() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024); // 100 MB

    let key1 = create_test_key("model-1");
    let key2 = create_test_key("model-2");

    load_into_cache(&cache, &key1, 1024);
    load_into_cache(&cache, &key2, 2048);

    let stats = cache.stats();
    assert_eq!(
        stats.total_memory_bytes, 3072,
        "Memory should be sum of all entries"
    );
}

#[test]
fn pinned_entries_are_not_evicted() {
    // Small cache to force eviction
    let cache = ModelHandleCache::new(2048);

    let key1 = create_test_key("pinned-model");
    let key2 = create_test_key("unpinned-model");
    let key3 = create_test_key("new-model");

    // Load and pin first model
    load_into_cache(&cache, &key1, 1024);
    cache.pin(&key1);

    // Load second model (unpinned)
    load_into_cache(&cache, &key2, 1024);

    // Load third model - should evict key2, not key1
    load_into_cache(&cache, &key3, 1024);

    // Pinned model should still be present (hit, not miss)
    let hit_before = cache.stats().hits;
    cache
        .get_or_load(&key1, || panic!("pinned model should be in cache"))
        .expect("get pinned should succeed");
    let hit_after = cache.stats().hits;

    assert!(hit_after > hit_before, "Pinned model should be a cache hit");

    let stats = cache.stats();
    assert!(
        stats.eviction_skip_pinned_count > 0 || stats.evictions > 0,
        "Should have tracked eviction or pinned skip"
    );
}

#[test]
fn cache_stats_track_eviction_counts() {
    // Small cache to force eviction
    let cache = ModelHandleCache::new(1024);

    let key1 = create_test_key("old-model");
    let key2 = create_test_key("new-model");

    // Load first model (takes full cache)
    load_into_cache(&cache, &key1, 1024);

    // Load second model - should trigger eviction of first
    load_into_cache(&cache, &key2, 1024);

    let stats = cache.stats();
    assert!(
        stats.evictions >= 1,
        "Should have evicted at least one entry"
    );
}

#[test]
fn unpinned_returns_true_for_unpinned_model() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);
    let key = create_test_key("model-to-unpin");

    load_into_cache(&cache, &key, 1024);
    cache.pin(&key);
    assert!(cache.is_pinned(&key), "Model should be pinned");

    let unpinned = cache.unpin(&key);
    assert!(unpinned, "Unpin should return true");
    assert!(!cache.is_pinned(&key), "Model should not be pinned anymore");
}

// Note: cache.clear() is #[cfg(test)] within crate only - not accessible from external tests
// The clear functionality is tested in the crate's internal test module

#[test]
fn cache_reports_correct_entry_count() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);

    assert_eq!(cache.len(), 0, "Empty cache should have 0 entries");

    let key1 = create_test_key("model-1");
    load_into_cache(&cache, &key1, 1024);
    assert_eq!(cache.len(), 1, "Cache should have 1 entry");

    let key2 = create_test_key("model-2");
    load_into_cache(&cache, &key2, 1024);
    assert_eq!(cache.len(), 2, "Cache should have 2 entries");
}

// ============================================================================
// Eviction Order Tests (PRD-RECT-003: Stable Eviction Policy)
// ============================================================================

#[test]
fn eviction_removes_oldest_entry_first() {
    // Cache that can hold exactly 2 entries
    let cache = ModelHandleCache::new(2048);

    let key1 = create_test_key("oldest-model");
    let key2 = create_test_key("newer-model");
    let key3 = create_test_key("newest-model");

    // Insert in order: key1 (oldest), then key2
    load_into_cache(&cache, &key1, 1024);
    std::thread::sleep(std::time::Duration::from_millis(10));
    load_into_cache(&cache, &key2, 1024);

    // Inserting key3 should evict key1 (oldest)
    load_into_cache(&cache, &key3, 1024);

    // key1 should be evicted (accessing it triggers a new load = miss)
    let misses_before = cache.stats().misses;
    let _ = cache.get_or_load(&key1, || Ok((create_test_handle(1024), 1024)));
    let misses_after = cache.stats().misses;

    assert!(
        misses_after > misses_before,
        "Oldest entry (key1) should be evicted and reloaded"
    );
}

#[test]
fn pinned_count_is_accurate() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);

    let key1 = create_test_key("model-1");
    let key2 = create_test_key("model-2");

    load_into_cache(&cache, &key1, 1024);
    load_into_cache(&cache, &key2, 1024);

    assert_eq!(cache.pinned_count(), 0, "No pinned entries initially");

    cache.pin(&key1);
    assert_eq!(cache.pinned_count(), 1, "One pinned entry");

    cache.pin(&key2);
    assert_eq!(cache.pinned_count(), 2, "Two pinned entries");

    cache.unpin(&key1);
    assert_eq!(cache.pinned_count(), 1, "One pinned entry after unpin");
}

#[test]
fn hit_ratio_calculation_is_correct() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);
    let key = create_test_key("test-model");

    // Load the model (1 miss)
    load_into_cache(&cache, &key, 1024);

    // Access 3 more times (3 hits)
    for _ in 0..3 {
        cache
            .get_or_load(&key, || panic!("should hit cache"))
            .expect("get should succeed");
    }

    let stats = cache.stats();
    let ratio = stats.hit_ratio();

    // Should have 3 hits, 1 miss = 75% hit ratio
    let expected = 3.0 / 4.0;
    assert!(
        (ratio - expected).abs() < 0.01,
        "Hit ratio should be ~0.75 (75%), got {}",
        ratio
    );
}

#[test]
fn empty_cache_has_zero_hit_ratio() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);
    let stats = cache.stats();
    assert_eq!(
        stats.hit_ratio(),
        0.0,
        "Empty cache should have 0% hit ratio"
    );
}

#[test]
fn memory_usage_reports_correct_total() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);

    assert_eq!(cache.memory_usage(), 0, "Empty cache has 0 memory usage");

    let key1 = create_test_key("model-1");
    load_into_cache(&cache, &key1, 1000);

    assert_eq!(
        cache.memory_usage(),
        1000,
        "Memory usage should reflect loaded model"
    );

    let key2 = create_test_key("model-2");
    load_into_cache(&cache, &key2, 2000);

    assert_eq!(
        cache.memory_usage(),
        3000,
        "Memory usage should be cumulative"
    );
}

#[test]
fn is_empty_returns_correct_status() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);

    assert!(cache.is_empty(), "New cache should be empty");

    let key = create_test_key("model");
    load_into_cache(&cache, &key, 1024);

    assert!(!cache.is_empty(), "Cache with entries should not be empty");

    // Note: cache.clear() is #[cfg(test)] within crate only
    // Testing cleared state covered in crate's internal tests
}

// ============================================================================
// Listener Tests (PRD-RECT-003: Event Observability)
// ============================================================================

#[test]
fn listener_receives_load_events() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);
    let listener = Arc::new(CountingListener::new());
    let key = create_test_key("listener-model");

    cache.register_listener(key.clone(), listener.clone());

    load_into_cache(&cache, &key, 1024);

    assert_eq!(
        listener.loads.load(Ordering::Relaxed),
        1,
        "Listener should receive load event"
    );
}

#[test]
fn listener_receives_reuse_events() {
    let cache = ModelHandleCache::new(100 * 1024 * 1024);
    let listener = Arc::new(CountingListener::new());
    let key = create_test_key("reuse-model");

    cache.register_listener(key.clone(), listener.clone());

    // First load
    load_into_cache(&cache, &key, 1024);

    // Second access - should be a hit and trigger reuse
    cache
        .get_or_load(&key, || panic!("should hit cache"))
        .expect("get should succeed");

    assert_eq!(
        listener.reuses.load(Ordering::Relaxed),
        1,
        "Listener should receive reuse event on cache hit"
    );
}

#[test]
fn listener_receives_eviction_events() {
    let cache = ModelHandleCache::new(100); // Very small to force eviction
    let listener = Arc::new(CountingListener::new());
    let key1 = create_test_key("evict-model-1");
    let key2 = create_test_key("evict-model-2");

    cache.register_listener(key1.clone(), listener.clone());

    // Load first model
    load_into_cache(&cache, &key1, 60);

    // Load second model - should evict first
    load_into_cache(&cache, &key2, 60);

    assert_eq!(
        listener.evictions.load(Ordering::Relaxed),
        1,
        "Listener should receive eviction event"
    );
}
