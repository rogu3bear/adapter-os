//! LoRA adapter cache for hotswap optimization
//!
//! Caches deserialized LoRA weights to avoid repeated disk I/O during
//! adapter loads. Uses LRU eviction when memory limits are reached.

use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for MLX LoRA adapter cache
#[derive(Debug, Clone)]
pub struct MLXAdapterCacheConfig {
    /// Maximum number of adapters to keep cached
    pub max_cached_adapters: usize,
    /// Maximum bytes per adapter
    pub max_adapter_size_bytes: usize,
    /// Total cache size limit in bytes
    pub max_total_cache_bytes: usize,
    /// TTL for cached adapters
    pub adapter_ttl: Duration,
}

impl Default for MLXAdapterCacheConfig {
    fn default() -> Self {
        Self {
            max_cached_adapters: 16,
            max_adapter_size_bytes: 500 * 1024 * 1024,
            max_total_cache_bytes: 4 * 1024 * 1024 * 1024,
            adapter_ttl: Duration::from_secs(3600),
        }
    }
}

/// Cached LoRA adapter entry
struct CachedAdapter {
    adapter_id: u16,
    weights: Arc<Vec<u8>>,
    size_bytes: usize,
    last_accessed: Instant,
    access_count: u64,
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct AdapterCacheStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub total_bytes_cached: usize,
    pub adapter_count: usize,
}

/// MLX LoRA adapter cache with LRU eviction
pub struct MLXAdapterCache {
    cache: RwLock<HashMap<u16, CachedAdapter>>,
    lru_order: RwLock<VecDeque<u16>>,
    config: MLXAdapterCacheConfig,
    total_cached_bytes: RwLock<usize>,
    stats: RwLock<AdapterCacheStats>,
}

impl MLXAdapterCache {
    /// Create new adapter cache with config
    pub fn new(config: MLXAdapterCacheConfig) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            lru_order: RwLock::new(VecDeque::new()),
            config,
            total_cached_bytes: RwLock::new(0),
            stats: RwLock::new(AdapterCacheStats::default()),
        }
    }

    /// Try to get cached adapter weights (cache hit)
    pub fn get_cached(&self, adapter_id: u16) -> Option<Arc<Vec<u8>>> {
        let mut cache = self.cache.write();
        let mut lru = self.lru_order.write();

        if let Some(entry) = cache.get_mut(&adapter_id) {
            entry.last_accessed = Instant::now();
            entry.access_count += 1;

            // Move to end of LRU (most recently used)
            if let Some(pos) = lru.iter().position(|&id| id == adapter_id) {
                lru.remove(pos);
            }
            lru.push_back(adapter_id);

            let mut stats = self.stats.write();
            stats.cache_hits += 1;

            tracing::debug!(adapter_id, hits = stats.cache_hits, "Adapter cache hit");
            return Some(Arc::clone(&entry.weights));
        }

        let mut stats = self.stats.write();
        stats.cache_misses += 1;
        tracing::debug!(
            adapter_id,
            misses = stats.cache_misses,
            "Adapter cache miss"
        );
        None
    }

    /// Cache adapter weights after load
    pub fn cache_adapter(&self, adapter_id: u16, weights: Vec<u8>) -> Result<()> {
        let size_bytes = weights.len();

        if size_bytes > self.config.max_adapter_size_bytes {
            return Err(AosError::ResourceExhaustion(format!(
                "Adapter {} size {} exceeds limit {}",
                adapter_id, size_bytes, self.config.max_adapter_size_bytes
            )));
        }

        let weights_arc = Arc::new(weights);

        let mut total = self.total_cached_bytes.write();
        let mut cache = self.cache.write();
        let mut lru = self.lru_order.write();
        let mut stats = self.stats.write();

        // Evict LRU adapters until we have space
        while (*total + size_bytes > self.config.max_total_cache_bytes
            || cache.len() >= self.config.max_cached_adapters)
            && !lru.is_empty()
        {
            if let Some(lru_id) = lru.pop_front() {
                if let Some(evicted) = cache.remove(&lru_id) {
                    *total = total.saturating_sub(evicted.size_bytes);
                    stats.evictions += 1;
                    tracing::info!(
                        evicted_id = lru_id,
                        freed_mb = evicted.size_bytes / (1024 * 1024),
                        "LRU adapter evicted"
                    );
                }
            }
        }

        // Insert new adapter
        cache.insert(
            adapter_id,
            CachedAdapter {
                adapter_id,
                weights: weights_arc,
                size_bytes,
                last_accessed: Instant::now(),
                access_count: 1,
            },
        );
        lru.push_back(adapter_id);
        *total += size_bytes;

        stats.total_bytes_cached = *total;
        stats.adapter_count = cache.len();

        tracing::info!(
            adapter_id,
            size_mb = size_bytes / (1024 * 1024),
            total_mb = *total / (1024 * 1024),
            "Adapter cached"
        );

        Ok(())
    }

    /// Manually evict adapter from cache
    pub fn evict_adapter(&self, adapter_id: u16) -> usize {
        let mut cache = self.cache.write();
        let mut lru = self.lru_order.write();
        let mut total = self.total_cached_bytes.write();
        let mut stats = self.stats.write();

        if let Some(entry) = cache.remove(&adapter_id) {
            if let Some(pos) = lru.iter().position(|&id| id == adapter_id) {
                lru.remove(pos);
            }
            *total = total.saturating_sub(entry.size_bytes);
            stats.evictions += 1;
            stats.total_bytes_cached = *total;
            stats.adapter_count = cache.len();

            tracing::debug!(adapter_id, freed = entry.size_bytes, "Adapter evicted");
            return entry.size_bytes;
        }
        0
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> AdapterCacheStats {
        self.stats.read().clone()
    }

    /// Clear all cached adapters
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        let mut lru = self.lru_order.write();
        let mut total = self.total_cached_bytes.write();
        let mut stats = self.stats.write();

        cache.clear();
        lru.clear();
        *total = 0;
        *stats = AdapterCacheStats::default();

        tracing::info!("Adapter cache cleared");
    }

    /// Get hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let stats = self.stats.read();
        let total = stats.cache_hits + stats.cache_misses;
        if total == 0 {
            0.0
        } else {
            stats.cache_hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let cache = MLXAdapterCache::new(MLXAdapterCacheConfig::default());
        let weights = vec![1u8; 1000];

        assert!(cache.get_cached(42).is_none());
        cache.cache_adapter(42, weights).unwrap();
        assert!(cache.get_cached(42).is_some());

        let stats = cache.get_stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }

    #[test]
    fn test_lru_eviction() {
        let config = MLXAdapterCacheConfig {
            max_cached_adapters: 2,
            max_total_cache_bytes: 10000,
            ..Default::default()
        };
        let cache = MLXAdapterCache::new(config);

        cache.cache_adapter(1, vec![1u8; 100]).unwrap();
        cache.cache_adapter(2, vec![2u8; 100]).unwrap();
        cache.cache_adapter(3, vec![3u8; 100]).unwrap(); // Should evict adapter 1

        assert!(cache.get_cached(1).is_none()); // Evicted
        assert!(cache.get_cached(2).is_some());
        assert!(cache.get_cached(3).is_some());
    }
}
