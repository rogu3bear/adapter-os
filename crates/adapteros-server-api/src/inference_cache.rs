//! Semantic inference cache for deterministic response reuse.
//!
//! This module provides an in-memory cache for inference responses, keyed by
//! (context_id, canonical_input_digest). When a request matches a cached entry,
//! the cached response is returned immediately with the original receipt_digest,
//! avoiding redundant computation.
//!
//! # Design
//!
//! - **Cache Key**: Composite of context_id (model + adapters + config) and
//!   canonical_input_digest (canonicalized request body hash from middleware).
//! - **Cache Value**: Output text, receipt_digest, and usage statistics.
//! - **Eviction**: LRU with configurable max size and TTL-based expiration.
//! - **Thread Safety**: Uses DashMap for lock-free concurrent access.
//!
//! # Usage
//!
//! The cache is integrated into the inference pipeline:
//! 1. Canonicalization middleware computes `canonical_input_digest`
//! 2. Handler extracts context_id from request parameters
//! 3. Cache lookup before inference execution
//! 4. On cache hit: return cached response immediately
//! 5. On cache miss: execute inference, store result, return fresh response
//!
//! [source: crates/adapteros-server-api/src/inference_cache.rs]

use adapteros_core::B3Hash;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Default time-to-live for cached inference results (1 hour).
pub const DEFAULT_CACHE_TTL_SECS: u64 = 3600;

/// Default maximum number of cached entries.
pub const DEFAULT_CACHE_MAX_SIZE: usize = 10_000;

/// Configuration for the inference cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InferenceCacheConfig {
    /// Whether caching is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Time-to-live in seconds for cached entries.
    #[serde(default = "default_ttl_secs")]
    pub ttl_secs: u64,
    /// Maximum number of entries in the cache.
    #[serde(default = "default_max_size")]
    pub max_size: usize,
    /// Whether to cache per-tenant (true) or globally (false).
    #[serde(default)]
    pub per_tenant: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_ttl_secs() -> u64 {
    DEFAULT_CACHE_TTL_SECS
}

fn default_max_size() -> usize {
    DEFAULT_CACHE_MAX_SIZE
}

impl Default for InferenceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_secs: DEFAULT_CACHE_TTL_SECS,
            max_size: DEFAULT_CACHE_MAX_SIZE,
            per_tenant: false,
        }
    }
}

/// Composite cache key: (context_id, canonical_input_digest).
///
/// The context_id captures the model, adapters, and configuration.
/// The canonical_input_digest captures the canonicalized request body.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InferenceCacheKey {
    /// Context ID (hash of model + adapters + config).
    pub context_id: B3Hash,
    /// Canonical input digest (hash of canonicalized request body).
    pub canonical_digest: B3Hash,
    /// Optional tenant ID for per-tenant caching.
    pub tenant_id: Option<String>,
}

impl InferenceCacheKey {
    /// Create a new cache key.
    pub fn new(context_id: B3Hash, canonical_digest: B3Hash, tenant_id: Option<String>) -> Self {
        Self {
            context_id,
            canonical_digest,
            tenant_id,
        }
    }

    /// Create a global (non-tenant-scoped) cache key.
    pub fn global(context_id: B3Hash, canonical_digest: B3Hash) -> Self {
        Self::new(context_id, canonical_digest, None)
    }

    /// Convert to a string key for DashMap.
    fn to_string_key(&self) -> String {
        match &self.tenant_id {
            Some(tid) => format!(
                "{}:{}:{}",
                tid,
                self.context_id.to_hex(),
                self.canonical_digest.to_hex()
            ),
            None => format!(
                "{}:{}",
                self.context_id.to_hex(),
                self.canonical_digest.to_hex()
            ),
        }
    }
}

/// Cached inference result.
#[derive(Debug, Clone)]
pub struct CachedInferenceResult {
    /// Output text from inference.
    pub output_text: String,
    /// Output tokens (if available).
    pub output_tokens: Vec<u32>,
    /// Number of tokens generated.
    pub tokens_generated: usize,
    /// Receipt digest (hex string) for determinism verification.
    pub receipt_digest_hex: Option<String>,
    /// Prompt tokens used.
    pub prompt_tokens: usize,
    /// Completion tokens generated.
    pub completion_tokens: usize,
    /// Model used for inference.
    pub model: Option<String>,
    /// Finish reason.
    pub finish_reason: String,
    /// Timestamp when this result was cached (Unix timestamp).
    pub cached_at: i64,
    /// Original request ID for tracing.
    pub original_request_id: String,
}

impl CachedInferenceResult {
    /// Check if this cached result has expired.
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = chrono::Utc::now().timestamp();
        let age = now - self.cached_at;
        // For zero TTL, entries are considered expired as soon as they age past 0 seconds
        // (i.e., next second after creation)
        age > ttl_secs as i64
    }
}

/// Internal cache entry with metadata for LRU tracking.
#[derive(Debug)]
struct CacheEntry {
    result: CachedInferenceResult,
    /// Last access timestamp for LRU eviction.
    last_accessed: AtomicU64,
}

impl CacheEntry {
    fn new(result: CachedInferenceResult) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        Self {
            result,
            last_accessed: AtomicU64::new(now),
        }
    }

    fn touch(&self) {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        self.last_accessed.store(now, Ordering::Relaxed);
    }

    fn last_accessed_millis(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
    }
}

/// Cache statistics for monitoring.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceCacheStats {
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// Total entries evicted due to TTL expiration.
    pub ttl_evictions: u64,
    /// Total entries evicted due to size limit.
    pub lru_evictions: u64,
    /// Current number of entries.
    pub current_size: usize,
}

/// Semantic inference cache with LRU eviction and TTL expiration.
pub struct InferenceCache {
    /// The cache storage.
    cache: DashMap<String, Arc<CacheEntry>>,
    /// Configuration.
    config: InferenceCacheConfig,
    /// Statistics counters.
    stats: InferenceCacheStatsInner,
}

/// Internal statistics tracking.
struct InferenceCacheStatsInner {
    hits: AtomicU64,
    misses: AtomicU64,
    ttl_evictions: AtomicU64,
    lru_evictions: AtomicU64,
}

impl Default for InferenceCacheStatsInner {
    fn default() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            ttl_evictions: AtomicU64::new(0),
            lru_evictions: AtomicU64::new(0),
        }
    }
}

impl InferenceCache {
    /// Create a new inference cache with the given configuration.
    pub fn new(config: InferenceCacheConfig) -> Self {
        info!(
            enabled = config.enabled,
            ttl_secs = config.ttl_secs,
            max_size = config.max_size,
            per_tenant = config.per_tenant,
            "Inference cache initialized"
        );
        Self {
            cache: DashMap::with_capacity(config.max_size.min(1000)),
            config,
            stats: InferenceCacheStatsInner::default(),
        }
    }

    /// Create a cache with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(InferenceCacheConfig::default())
    }

    /// Check if caching is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the TTL duration.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.config.ttl_secs)
    }

    /// Look up a cached result.
    ///
    /// Returns `Some(result)` if found and not expired, `None` otherwise.
    pub fn get(&self, key: &InferenceCacheKey) -> Option<CachedInferenceResult> {
        if !self.config.enabled {
            return None;
        }

        let string_key = key.to_string_key();

        match self.cache.get(&string_key) {
            Some(entry) => {
                // Check TTL expiration
                if entry.result.is_expired(self.config.ttl_secs) {
                    debug!(
                        context_id = %key.context_id.to_hex(),
                        canonical_digest = %key.canonical_digest.to_hex(),
                        "Cache entry expired (TTL)"
                    );
                    // Remove expired entry
                    drop(entry); // Release read lock
                    self.cache.remove(&string_key);
                    self.stats.ttl_evictions.fetch_add(1, Ordering::Relaxed);
                    self.stats.misses.fetch_add(1, Ordering::Relaxed);
                    return None;
                }

                // Update access time for LRU
                entry.touch();
                self.stats.hits.fetch_add(1, Ordering::Relaxed);

                debug!(
                    context_id = %key.context_id.to_hex(),
                    canonical_digest = %key.canonical_digest.to_hex(),
                    original_request_id = %entry.result.original_request_id,
                    "Cache hit"
                );

                Some(entry.result.clone())
            }
            None => {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store a result in the cache.
    ///
    /// If the cache is at capacity, evicts the least recently used entry.
    pub fn put(&self, key: InferenceCacheKey, result: CachedInferenceResult) {
        if !self.config.enabled {
            return;
        }

        // Evict if at capacity
        if self.cache.len() >= self.config.max_size {
            self.evict_lru();
        }

        let string_key = key.to_string_key();
        let entry = Arc::new(CacheEntry::new(result));

        debug!(
            context_id = %key.context_id.to_hex(),
            canonical_digest = %key.canonical_digest.to_hex(),
            "Storing inference result in cache"
        );

        self.cache.insert(string_key, entry);
    }

    /// Evict the least recently used entry.
    fn evict_lru(&self) {
        let mut oldest_key: Option<String> = None;
        let mut oldest_time = u64::MAX;

        // Find the LRU entry
        for entry in self.cache.iter() {
            let access_time = entry.value().last_accessed_millis();
            if access_time < oldest_time {
                oldest_time = access_time;
                oldest_key = Some(entry.key().clone());
            }
        }

        // Remove the LRU entry
        if let Some(key) = oldest_key {
            if self.cache.remove(&key).is_some() {
                self.stats.lru_evictions.fetch_add(1, Ordering::Relaxed);
                debug!(key = %key, "Evicted LRU cache entry");
            }
        }
    }

    /// Remove a specific entry from the cache.
    pub fn invalidate(&self, key: &InferenceCacheKey) {
        let string_key = key.to_string_key();
        if self.cache.remove(&string_key).is_some() {
            debug!(
                context_id = %key.context_id.to_hex(),
                canonical_digest = %key.canonical_digest.to_hex(),
                "Cache entry invalidated"
            );
        }
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        let count = self.cache.len();
        self.cache.clear();
        info!(entries_cleared = count, "Inference cache cleared");
    }

    /// Clean up expired entries.
    ///
    /// Should be called periodically to reclaim memory.
    pub fn cleanup_expired(&self) -> usize {
        let ttl_secs = self.config.ttl_secs;
        let initial_count = self.cache.len();

        self.cache.retain(|_, entry| {
            let expired = entry.result.is_expired(ttl_secs);
            if expired {
                self.stats.ttl_evictions.fetch_add(1, Ordering::Relaxed);
            }
            !expired
        });

        let removed = initial_count - self.cache.len();
        if removed > 0 {
            info!(
                removed = removed,
                remaining = self.cache.len(),
                "Cleaned up expired inference cache entries"
            );
        }

        removed
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> InferenceCacheStats {
        InferenceCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            ttl_evictions: self.stats.ttl_evictions.load(Ordering::Relaxed),
            lru_evictions: self.stats.lru_evictions.load(Ordering::Relaxed),
            current_size: self.cache.len(),
        }
    }

    /// Get current cache size.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get the cache hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

impl Default for InferenceCache {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Builder for creating CachedInferenceResult from inference response.
pub struct CachedInferenceResultBuilder {
    output_text: String,
    output_tokens: Vec<u32>,
    tokens_generated: usize,
    receipt_digest_hex: Option<String>,
    prompt_tokens: usize,
    completion_tokens: usize,
    model: Option<String>,
    finish_reason: String,
    original_request_id: String,
}

impl CachedInferenceResultBuilder {
    /// Create a new builder.
    pub fn new(output_text: String, original_request_id: String) -> Self {
        Self {
            output_text,
            output_tokens: Vec::new(),
            tokens_generated: 0,
            receipt_digest_hex: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            model: None,
            finish_reason: "stop".to_string(),
            original_request_id,
        }
    }

    /// Set output tokens.
    pub fn with_output_tokens(mut self, tokens: Vec<u32>) -> Self {
        self.tokens_generated = tokens.len();
        self.output_tokens = tokens;
        self
    }

    /// Set tokens generated count (if tokens not available).
    pub fn with_tokens_generated(mut self, count: usize) -> Self {
        self.tokens_generated = count;
        self
    }

    /// Set receipt digest.
    pub fn with_receipt_digest(mut self, digest: Option<String>) -> Self {
        self.receipt_digest_hex = digest;
        self
    }

    /// Set usage statistics.
    pub fn with_usage(mut self, prompt_tokens: usize, completion_tokens: usize) -> Self {
        self.prompt_tokens = prompt_tokens;
        self.completion_tokens = completion_tokens;
        self
    }

    /// Set model.
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Set finish reason.
    pub fn with_finish_reason(mut self, reason: String) -> Self {
        self.finish_reason = reason;
        self
    }

    /// Build the cached result.
    pub fn build(self) -> CachedInferenceResult {
        CachedInferenceResult {
            output_text: self.output_text,
            output_tokens: self.output_tokens,
            tokens_generated: self.tokens_generated,
            receipt_digest_hex: self.receipt_digest_hex,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            model: self.model,
            finish_reason: self.finish_reason,
            cached_at: chrono::Utc::now().timestamp(),
            original_request_id: self.original_request_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> InferenceCacheKey {
        InferenceCacheKey::global(B3Hash::hash(b"test-context"), B3Hash::hash(b"test-input"))
    }

    fn test_result() -> CachedInferenceResult {
        CachedInferenceResultBuilder::new("Hello, world!".to_string(), "req-123".to_string())
            .with_tokens_generated(3)
            .with_usage(10, 3)
            .with_receipt_digest(Some("abc123".to_string()))
            .with_model(Some("test-model".to_string()))
            .build()
    }

    #[test]
    fn test_cache_put_get() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result.clone());

        let cached = cache.get(&key).expect("should have cached result");
        assert_eq!(cached.output_text, "Hello, world!");
        assert_eq!(cached.tokens_generated, 3);
        assert_eq!(cached.prompt_tokens, 10);
        assert_eq!(cached.completion_tokens, 3);
        assert_eq!(cached.receipt_digest_hex, Some("abc123".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();

        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_disabled() {
        let config = InferenceCacheConfig {
            enabled: false,
            ..Default::default()
        };
        let cache = InferenceCache::new(config);
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_ttl_expiration() {
        // Use a 2-second TTL and wait 2.5 seconds to ensure expiration
        let config = InferenceCacheConfig {
            enabled: true,
            ttl_secs: 2, // 2 second TTL
            ..Default::default()
        };
        let cache = InferenceCache::new(config);
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result);
        // Verify entry exists immediately
        assert!(
            cache.get(&key).is_some(),
            "Entry should exist immediately after put"
        );
        // Entry should still exist after 1 second
        std::thread::sleep(std::time::Duration::from_secs(1));
        assert!(
            cache.get(&key).is_some(),
            "Entry should still exist at 1 second"
        );
        // Wait for TTL to expire (additional 1.5 seconds = 2.5 total > 2 second TTL)
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            cache.get(&key).is_none(),
            "Entry should be expired after TTL"
        );
    }

    #[test]
    fn test_cache_lru_eviction() {
        let config = InferenceCacheConfig {
            enabled: true,
            max_size: 2,
            ..Default::default()
        };
        let cache = InferenceCache::new(config);

        // Add 3 entries, first should be evicted
        let key1 = InferenceCacheKey::global(B3Hash::hash(b"ctx1"), B3Hash::hash(b"input1"));
        let key2 = InferenceCacheKey::global(B3Hash::hash(b"ctx2"), B3Hash::hash(b"input2"));
        let key3 = InferenceCacheKey::global(B3Hash::hash(b"ctx3"), B3Hash::hash(b"input3"));

        cache.put(key1.clone(), test_result());
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.put(key2.clone(), test_result());
        std::thread::sleep(std::time::Duration::from_millis(5));
        cache.put(key3.clone(), test_result());

        // First entry should have been evicted (LRU)
        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some());
        assert!(cache.get(&key3).is_some());
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result);
        assert!(cache.get(&key).is_some());

        cache.invalidate(&key);
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_clear() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_stats() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();
        let result = test_result();

        // Miss
        cache.get(&key);
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);

        // Hit
        cache.put(key.clone(), result);
        cache.get(&key);
        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 1);
    }

    #[test]
    fn test_cache_hit_rate() {
        let cache = InferenceCache::with_defaults();
        let key = test_key();
        let result = test_result();

        // No operations yet
        assert_eq!(cache.hit_rate(), 0.0);

        // 1 miss
        cache.get(&key);
        assert_eq!(cache.hit_rate(), 0.0);

        // 1 hit
        cache.put(key.clone(), result);
        cache.get(&key);
        assert_eq!(cache.hit_rate(), 0.5); // 1 hit / 2 total
    }

    #[test]
    fn test_per_tenant_key() {
        let key1 = InferenceCacheKey::new(
            B3Hash::hash(b"ctx"),
            B3Hash::hash(b"input"),
            Some("tenant-a".to_string()),
        );
        let key2 = InferenceCacheKey::new(
            B3Hash::hash(b"ctx"),
            B3Hash::hash(b"input"),
            Some("tenant-b".to_string()),
        );
        let key3 = InferenceCacheKey::global(B3Hash::hash(b"ctx"), B3Hash::hash(b"input"));

        // Same context and input but different tenants = different keys
        assert_ne!(key1.to_string_key(), key2.to_string_key());
        assert_ne!(key1.to_string_key(), key3.to_string_key());
    }

    #[test]
    fn test_cleanup_expired() {
        // Use 2-second TTL and wait for expiration
        let config = InferenceCacheConfig {
            enabled: true,
            ttl_secs: 2,
            ..Default::default()
        };
        let cache = InferenceCache::new(config);
        let key = test_key();
        let result = test_result();

        cache.put(key.clone(), result);
        assert_eq!(cache.len(), 1);

        // Wait for TTL to expire (2.5 seconds > 2 second TTL)
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let removed = cache.cleanup_expired();
        assert_eq!(removed, 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_builder() {
        let result = CachedInferenceResultBuilder::new("Output".to_string(), "req-456".to_string())
            .with_output_tokens(vec![1, 2, 3])
            .with_usage(100, 50)
            .with_receipt_digest(Some("digest123".to_string()))
            .with_model(Some("model-v1".to_string()))
            .with_finish_reason("length".to_string())
            .build();

        assert_eq!(result.output_text, "Output");
        assert_eq!(result.output_tokens, vec![1, 2, 3]);
        assert_eq!(result.tokens_generated, 3);
        assert_eq!(result.prompt_tokens, 100);
        assert_eq!(result.completion_tokens, 50);
        assert_eq!(result.receipt_digest_hex, Some("digest123".to_string()));
        assert_eq!(result.model, Some("model-v1".to_string()));
        assert_eq!(result.finish_reason, "length");
        assert_eq!(result.original_request_id, "req-456");
    }
}
