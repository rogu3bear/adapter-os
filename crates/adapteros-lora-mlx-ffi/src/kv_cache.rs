//! KV cache management for efficient token generation in MLX backend
//!
//! This module implements key-value (KV) cache operations that optimize generation speed
//! by caching past computations. Instead of recomputing attention over all previous tokens,
//! we store the key and value tensors and reuse them for subsequent tokens.
//!
//! Architecture:
//! - `MLXKVCache` - Main cache structure managing per-layer key/value tensors
//! - `CacheLayer` - Per-layer cache with position tracking
//! - Thread-safe access via Arc<RwLock<>>
//! - Automatic eviction when max capacity exceeded

use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Per-layer KV cache storing key and value tensors
#[derive(Debug, Clone)]
pub struct CacheLayer {
    /// Cached key tensors: (position, tensor_data)
    pub keys: Vec<Vec<f32>>,
    /// Cached value tensors: (position, tensor_data)
    pub values: Vec<Vec<f32>>,
    /// Number of cached positions
    pub cached_positions: usize,
    /// Maximum cache size per layer (positions)
    pub max_positions: usize,
}

impl CacheLayer {
    /// Create a new per-layer cache
    pub fn new(max_positions: usize) -> Self {
        Self {
            keys: Vec::with_capacity(max_positions),
            values: Vec::with_capacity(max_positions),
            cached_positions: 0,
            max_positions,
        }
    }

    /// Add key/value tensors for a position
    pub fn add_position(&mut self, key: Vec<f32>, value: Vec<f32>) {
        if self.cached_positions >= self.max_positions {
            // Evict oldest entry (FIFO)
            if !self.keys.is_empty() {
                self.keys.remove(0);
                self.values.remove(0);
            }
        } else {
            self.cached_positions += 1;
        }

        self.keys.push(key);
        self.values.push(value);
    }

    /// Get concatenated key tensor for all cached positions
    pub fn get_keys(&self) -> Vec<f32> {
        let mut result = Vec::new();
        for key in &self.keys {
            result.extend(key);
        }
        result
    }

    /// Get concatenated value tensor for all cached positions
    pub fn get_values(&self) -> Vec<f32> {
        let mut result = Vec::new();
        for value in &self.values {
            result.extend(value);
        }
        result
    }

    /// Get key tensor at specific position
    pub fn get_key_at(&self, position: usize) -> Option<&[f32]> {
        self.keys.get(position).map(|v| v.as_slice())
    }

    /// Get value tensor at specific position
    pub fn get_value_at(&self, position: usize) -> Option<&[f32]> {
        self.values.get(position).map(|v| v.as_slice())
    }

    /// Clear all cached tensors for this layer
    pub fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
        self.cached_positions = 0;
    }

    /// Get memory usage in bytes
    pub fn memory_bytes(&self) -> usize {
        let keys_bytes: usize = self.keys.iter().map(|k| k.len() * 4).sum();
        let values_bytes: usize = self.values.iter().map(|v| v.len() * 4).sum();
        keys_bytes + values_bytes
    }
}

/// Configuration for KV cache
#[derive(Debug, Clone)]
pub struct KVCacheConfig {
    /// Number of transformer layers
    pub num_layers: usize,
    /// Maximum sequence length to cache (positions)
    pub max_seq_length: usize,
    /// Key/value tensor dimension per position
    pub hidden_dim: usize,
    /// Number of attention heads
    pub num_heads: usize,
    /// Key/value dimension per head
    pub head_dim: usize,
}

impl Default for KVCacheConfig {
    fn default() -> Self {
        Self {
            num_layers: 32,
            max_seq_length: 4096,
            hidden_dim: 4096,
            num_heads: 32,
            head_dim: 128,
        }
    }
}

impl KVCacheConfig {
    /// Estimate total memory required for this cache config (bytes)
    pub fn memory_estimate(&self) -> usize {
        let per_position_bytes = 2 * self.hidden_dim * 4; // 2 for K and V, 4 for f32
        per_position_bytes * self.max_seq_length * self.num_layers
    }
}

/// Main KV cache structure for multi-layer transformer
#[derive(Debug)]
pub struct MLXKVCache {
    /// Per-layer caches indexed by layer number
    caches: Arc<RwLock<HashMap<usize, CacheLayer>>>,
    /// Configuration
    config: KVCacheConfig,
    /// Total cached positions across all layers
    total_cached_positions: Arc<RwLock<usize>>,
    /// Statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache statistics for monitoring and debugging
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total cache hits (uses of cached values)
    pub cache_hits: u64,
    /// Total cache misses (recomputation needed)
    pub cache_misses: u64,
    /// Total evictions due to capacity limits
    pub evictions: u64,
    /// Peak memory usage in bytes
    pub peak_memory_bytes: usize,
    /// Number of clear operations
    pub clears: u64,
}

impl MLXKVCache {
    /// Create a new KV cache for multi-layer transformer
    ///
    /// # Arguments
    /// * `config` - Cache configuration with layer counts and dimensions
    pub fn new(config: KVCacheConfig) -> Self {
        tracing::info!(
            num_layers = config.num_layers,
            max_seq_length = config.max_seq_length,
            memory_mb = config.memory_estimate() as f32 / (1024.0 * 1024.0),
            "Creating MLX KV cache"
        );

        Self {
            caches: Arc::new(RwLock::new(HashMap::new())),
            config,
            total_cached_positions: Arc::new(RwLock::new(0)),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get configured transformer layer count
    pub fn num_layers(&self) -> usize {
        self.config.num_layers
    }

    /// Update cache with new key/value tensors for a layer
    ///
    /// # Arguments
    /// * `layer_idx` - Transformer layer index
    /// * `key` - Key tensor data for this position
    /// * `value` - Value tensor data for this position
    ///
    /// # Returns
    /// Result indicating success or cache overflow
    pub fn mlx_kv_cache_update(
        &self,
        layer_idx: usize,
        key: Vec<f32>,
        value: Vec<f32>,
    ) -> Result<()> {
        if key.is_empty() || value.is_empty() {
            return Err(AosError::Validation(
                "Key and value tensors cannot be empty".to_string(),
            ));
        }

        let mut caches = self.caches.write();

        // Update the cache and collect stats in a scope to release the mutable borrow
        let (new_positions, positions_increased) = {
            let cache = caches
                .entry(layer_idx)
                .or_insert_with(|| CacheLayer::new(self.config.max_seq_length));

            let old_positions = cache.cached_positions;
            cache.add_position(key, value);
            (
                cache.cached_positions,
                cache.cached_positions > old_positions,
            )
        };

        // Track statistics
        if positions_increased {
            let mut total = self.total_cached_positions.write();
            *total += 1;
        }

        // Update peak memory - compute directly from locked guard to avoid deadlock
        // (get_memory_usage would try to acquire a read lock while we hold a write lock)
        let current_memory: usize = caches.values().map(|c| c.memory_bytes()).sum();
        let mut stats = self.stats.write();
        if current_memory > stats.peak_memory_bytes {
            stats.peak_memory_bytes = current_memory;
        }

        tracing::trace!(
            layer_idx = layer_idx,
            cached_positions = new_positions,
            "Updated KV cache for layer"
        );

        Ok(())
    }

    /// Retrieve cached keys for a layer
    ///
    /// # Arguments
    /// * `layer_idx` - Transformer layer index
    ///
    /// # Returns
    /// Concatenated key tensor data or error if layer not cached
    pub fn mlx_kv_cache_get_keys(&self, layer_idx: usize) -> Result<Vec<f32>> {
        let mut stats = self.stats.write();

        let caches = self.caches.read();
        if let Some(cache) = caches.get(&layer_idx) {
            if cache.cached_positions > 0 {
                stats.cache_hits += 1;
                let keys = cache.get_keys();
                tracing::trace!(
                    layer_idx = layer_idx,
                    elements = keys.len(),
                    "Retrieved keys from KV cache"
                );
                return Ok(keys);
            }
        }

        stats.cache_misses += 1;
        Err(AosError::Lifecycle(format!(
            "No cached keys for layer {}",
            layer_idx
        )))
    }

    /// Retrieve cached values for a layer
    ///
    /// # Arguments
    /// * `layer_idx` - Transformer layer index
    ///
    /// # Returns
    /// Concatenated value tensor data or error if layer not cached
    pub fn mlx_kv_cache_get_values(&self, layer_idx: usize) -> Result<Vec<f32>> {
        let mut stats = self.stats.write();

        let caches = self.caches.read();
        if let Some(cache) = caches.get(&layer_idx) {
            if cache.cached_positions > 0 {
                stats.cache_hits += 1;
                let values = cache.get_values();
                tracing::trace!(
                    layer_idx = layer_idx,
                    elements = values.len(),
                    "Retrieved values from KV cache"
                );
                return Ok(values);
            }
        }

        stats.cache_misses += 1;
        Err(AosError::Lifecycle(format!(
            "No cached values for layer {}",
            layer_idx
        )))
    }

    /// Clean up and free all cached data
    ///
    /// This releases all allocated memory for key/value tensors.
    pub fn mlx_kv_cache_free(&self) {
        let mut caches = self.caches.write();
        caches.clear();
        *self.total_cached_positions.write() = 0;

        let mut stats = self.stats.write();
        stats.clears += 1;

        tracing::debug!("MLX KV cache cleared and freed");
    }

    /// Get cache size (number of cached positions)
    pub fn get_size(&self) -> usize {
        *self.total_cached_positions.read()
    }

    /// Get memory usage in bytes
    pub fn get_memory_usage(&self) -> usize {
        let caches = self.caches.read();
        caches.values().map(|c| c.memory_bytes()).sum()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.get_size() == 0
    }

    /// Clear cache for specific layer
    pub fn clear_layer(&self, layer_idx: usize) {
        let mut caches = self.caches.write();
        if let Some(cache) = caches.get_mut(&layer_idx) {
            let old_positions = cache.cached_positions;
            cache.clear();
            *self.total_cached_positions.write() -= old_positions;
        }
    }

    /// Clear all caches
    pub fn clear_all(&self) {
        self.mlx_kv_cache_free();
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        self.stats.read().clone()
    }

    /// Reset statistics (for benchmarking)
    pub fn reset_stats(&self) {
        *self.stats.write() = CacheStats::default();
    }

    /// Get cache hit rate as percentage (0.0 to 1.0)
    pub fn get_hit_rate(&self) -> f32 {
        let stats = self.stats.read();
        let total = stats.cache_hits + stats.cache_misses;
        if total == 0 {
            0.0
        } else {
            stats.cache_hits as f32 / total as f32
        }
    }

    /// Get number of layers with cached data
    pub fn get_num_cached_layers(&self) -> usize {
        self.caches.read().len()
    }

    /// Get cached positions for a specific layer
    pub fn get_layer_cached_positions(&self, layer_idx: usize) -> usize {
        self.caches
            .read()
            .get(&layer_idx)
            .map(|c| c.cached_positions)
            .unwrap_or(0)
    }

    /// Get detailed cache status
    pub fn get_status(&self) -> CacheStatus {
        let caches = self.caches.read();
        let mut layer_stats = HashMap::new();
        let mut total_memory = 0;

        for (layer_idx, cache) in caches.iter() {
            let memory = cache.memory_bytes();
            layer_stats.insert(
                *layer_idx,
                LayerCacheStatus {
                    cached_positions: cache.cached_positions,
                    memory_bytes: memory,
                },
            );
            total_memory += memory;
        }

        CacheStatus {
            num_cached_layers: caches.len(),
            total_cached_positions: self.get_size(),
            total_memory_bytes: total_memory,
            layer_stats,
            stats: self.stats.read().clone(),
        }
    }

    /// Format cache information for logging
    pub fn format_status(&self) -> String {
        let status = self.get_status();
        format!(
            "MLX KV Cache: {} layers, {} positions, {:.2} MB, hit_rate={:.1}%",
            status.num_cached_layers,
            status.total_cached_positions,
            status.total_memory_bytes as f32 / (1024.0 * 1024.0),
            self.get_hit_rate() * 100.0
        )
    }
}

/// Detailed cache status information
#[derive(Debug, Clone)]
pub struct CacheStatus {
    /// Number of layers with cached data
    pub num_cached_layers: usize,
    /// Total cached positions across all layers
    pub total_cached_positions: usize,
    /// Total memory usage in bytes
    pub total_memory_bytes: usize,
    /// Per-layer statistics
    pub layer_stats: HashMap<usize, LayerCacheStatus>,
    /// Overall cache statistics
    pub stats: CacheStats,
}

/// Per-layer cache status
#[derive(Debug, Clone)]
pub struct LayerCacheStatus {
    /// Number of cached positions in this layer
    pub cached_positions: usize,
    /// Memory usage in bytes for this layer
    pub memory_bytes: usize,
}

impl Clone for MLXKVCache {
    fn clone(&self) -> Self {
        Self {
            caches: self.caches.clone(),
            config: self.config.clone(),
            total_cached_positions: self.total_cached_positions.clone(),
            stats: self.stats.clone(),
        }
    }
}

// Thread-safe marker traits
unsafe impl Send for MLXKVCache {}
unsafe impl Sync for MLXKVCache {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_layer_creation() {
        let layer = CacheLayer::new(100);
        assert_eq!(layer.cached_positions, 0);
        assert!(layer.keys.is_empty());
        assert!(layer.values.is_empty());
    }

    #[test]
    fn test_cache_layer_add_position() {
        let mut layer = CacheLayer::new(100);
        let key = vec![1.0, 2.0, 3.0];
        let value = vec![4.0, 5.0, 6.0];

        layer.add_position(key.clone(), value.clone());

        assert_eq!(layer.cached_positions, 1);
        assert_eq!(layer.keys[0], key);
        assert_eq!(layer.values[0], value);
    }

    #[test]
    fn test_cache_layer_get_keys_and_values() {
        let mut layer = CacheLayer::new(100);
        layer.add_position(vec![1.0, 2.0], vec![3.0, 4.0]);
        layer.add_position(vec![5.0, 6.0], vec![7.0, 8.0]);

        let keys = layer.get_keys();
        let values = layer.get_values();

        assert_eq!(keys, vec![1.0, 2.0, 5.0, 6.0]);
        assert_eq!(values, vec![3.0, 4.0, 7.0, 8.0]);
    }

    #[test]
    fn test_cache_layer_eviction() {
        let mut layer = CacheLayer::new(2);
        layer.add_position(vec![1.0], vec![1.0]);
        layer.add_position(vec![2.0], vec![2.0]);
        assert_eq!(layer.cached_positions, 2);

        // This should evict the first position
        layer.add_position(vec![3.0], vec![3.0]);
        assert_eq!(layer.cached_positions, 2);
        assert_eq!(layer.keys[0], vec![2.0]);
        assert_eq!(layer.keys[1], vec![3.0]);
    }

    #[test]
    fn test_kv_cache_creation() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);
        assert!(cache.is_empty());
        assert_eq!(cache.get_size(), 0);
    }

    #[test]
    fn test_kv_cache_update_and_retrieve() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        let key = vec![1.0, 2.0, 3.0];
        let value = vec![4.0, 5.0, 6.0];

        cache
            .mlx_kv_cache_update(0, key.clone(), value.clone())
            .unwrap();

        let retrieved_keys = cache.mlx_kv_cache_get_keys(0).unwrap();
        let retrieved_values = cache.mlx_kv_cache_get_values(0).unwrap();

        assert_eq!(retrieved_keys, key);
        assert_eq!(retrieved_values, value);
    }

    #[test]
    fn test_kv_cache_multiple_layers() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache.mlx_kv_cache_update(0, vec![1.0], vec![2.0]).unwrap();
        cache.mlx_kv_cache_update(1, vec![3.0], vec![4.0]).unwrap();
        cache.mlx_kv_cache_update(2, vec![5.0], vec![6.0]).unwrap();

        assert_eq!(cache.get_num_cached_layers(), 3);
        assert_eq!(cache.get_size(), 3);
    }

    #[test]
    fn test_kv_cache_clear() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache.mlx_kv_cache_update(0, vec![1.0], vec![2.0]).unwrap();
        assert_eq!(cache.get_size(), 1);

        cache.mlx_kv_cache_free();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_kv_cache_statistics() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache.mlx_kv_cache_update(0, vec![1.0], vec![2.0]).unwrap();

        // First retrieval - cache hit
        let _ = cache.mlx_kv_cache_get_keys(0);
        let stats = cache.get_stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 0);

        // Try to get non-existent layer - cache miss
        let _ = cache.mlx_kv_cache_get_keys(99);
        let stats = cache.get_stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }

    #[test]
    fn test_kv_cache_hit_rate() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache.mlx_kv_cache_update(0, vec![1.0], vec![2.0]).unwrap();

        // 2 hits
        let _ = cache.mlx_kv_cache_get_keys(0);
        let _ = cache.mlx_kv_cache_get_values(0);

        // 1 miss
        let _ = cache.mlx_kv_cache_get_keys(99);

        let hit_rate = cache.get_hit_rate();
        assert!((hit_rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_memory_tracking() {
        let config = KVCacheConfig {
            num_layers: 32,
            max_seq_length: 4096,
            hidden_dim: 4096,
            num_heads: 32,
            head_dim: 128,
        };

        let cache = MLXKVCache::new(config.clone());
        assert!(cache.get_memory_usage() == 0);

        // Add some data
        cache
            .mlx_kv_cache_update(0, vec![1.0; 100], vec![1.0; 100])
            .unwrap();

        let memory = cache.get_memory_usage();
        assert!(memory > 0);
        assert_eq!(memory, 800); // 100 * 4 + 100 * 4
    }

    #[test]
    fn test_cache_config_memory_estimate() {
        let config = KVCacheConfig {
            num_layers: 32,
            max_seq_length: 4096,
            hidden_dim: 4096,
            num_heads: 32,
            head_dim: 128,
        };

        let estimate = config.memory_estimate();
        // 2 * 4096 * 4 * 4096 * 32 = 4GB
        assert_eq!(estimate, 2 * 4096 * 4 * 4096 * 32);
    }

    #[test]
    fn test_cache_status_retrieval() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache
            .mlx_kv_cache_update(0, vec![1.0; 50], vec![1.0; 50])
            .unwrap();
        cache
            .mlx_kv_cache_update(1, vec![1.0; 75], vec![1.0; 75])
            .unwrap();

        let status = cache.get_status();
        assert_eq!(status.num_cached_layers, 2);
        assert_eq!(status.total_cached_positions, 2);
        assert_eq!(status.total_memory_bytes, 50 * 4 + 50 * 4 + 75 * 4 + 75 * 4);
    }

    #[test]
    fn test_clear_specific_layer() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache.mlx_kv_cache_update(0, vec![1.0], vec![2.0]).unwrap();
        cache.mlx_kv_cache_update(1, vec![3.0], vec![4.0]).unwrap();

        assert_eq!(cache.get_size(), 2);

        cache.clear_layer(0);
        assert_eq!(cache.get_size(), 1);
        assert_eq!(cache.get_layer_cached_positions(0), 0);
        assert_eq!(cache.get_layer_cached_positions(1), 1);
    }

    #[test]
    fn test_format_status() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        cache
            .mlx_kv_cache_update(0, vec![1.0; 100], vec![1.0; 100])
            .unwrap();

        let status_str = cache.format_status();
        assert!(status_str.contains("MLX KV Cache"));
        assert!(status_str.contains("1 layers"));
        assert!(status_str.contains("1 positions"));
    }

    #[test]
    fn test_empty_tensor_validation() {
        let config = KVCacheConfig::default();
        let cache = MLXKVCache::new(config);

        let result = cache.mlx_kv_cache_update(0, vec![], vec![1.0]);
        assert!(result.is_err());

        let result = cache.mlx_kv_cache_update(0, vec![1.0], vec![]);
        assert!(result.is_err());
    }
}
