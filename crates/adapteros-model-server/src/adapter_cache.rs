//! Hot Adapter Cache
//!
//! Manages hot adapters that are cached in the model server for
//! fusion before returning logits to workers.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::MAX_HOT_ADAPTERS;

/// Cached adapter weights
#[derive(Debug)]
pub struct CachedAdapter {
    /// Adapter ID
    pub adapter_id: u32,

    /// Adapter name
    pub adapter_name: String,

    /// LoRA A weights (down projection)
    pub lora_a: Vec<f32>,

    /// LoRA B weights (up projection)
    pub lora_b: Vec<f32>,

    /// Scaling factor
    pub scale: f32,

    /// Memory usage in bytes
    pub memory_bytes: u64,

    /// Load timestamp
    pub loaded_at: Instant,

    /// Last used timestamp
    pub last_used: parking_lot::RwLock<Instant>,
}

impl CachedAdapter {
    /// Create a new cached adapter from weights
    pub fn new(
        adapter_id: u32,
        adapter_name: String,
        lora_a: Vec<f32>,
        lora_b: Vec<f32>,
        scale: f32,
    ) -> Self {
        let memory_bytes = (lora_a.len() + lora_b.len()) as u64 * 4; // f32 = 4 bytes

        Self {
            adapter_id,
            adapter_name,
            lora_a,
            lora_b,
            scale,
            memory_bytes,
            loaded_at: Instant::now(),
            last_used: parking_lot::RwLock::new(Instant::now()),
        }
    }

    /// Touch the adapter (update last used time)
    pub fn touch(&self) {
        *self.last_used.write() = Instant::now();
    }

    /// Get seconds since last use
    pub fn seconds_since_use(&self) -> f64 {
        self.last_used.read().elapsed().as_secs_f64()
    }

    /// Get seconds since load
    pub fn seconds_since_load(&self) -> f64 {
        self.loaded_at.elapsed().as_secs_f64()
    }
}

/// Cache for hot adapters
pub struct AdapterCache {
    /// Cached adapters by ID
    adapters: DashMap<u32, Arc<CachedAdapter>>,

    /// Maximum number of hot adapters
    max_adapters: usize,

    /// Current memory usage in bytes
    current_bytes: AtomicU64,

    /// Maximum memory in bytes (optional limit)
    max_bytes: Option<u64>,

    /// Statistics
    loads: AtomicU64,
    unloads: AtomicU64,
    fusions: AtomicU64,
}

impl AdapterCache {
    /// Create a new adapter cache
    pub fn new(max_adapters: usize, max_bytes: Option<u64>) -> Self {
        Self {
            adapters: DashMap::new(),
            max_adapters,
            current_bytes: AtomicU64::new(0),
            max_bytes,
            loads: AtomicU64::new(0),
            unloads: AtomicU64::new(0),
            fusions: AtomicU64::new(0),
        }
    }

    /// Create with default limits
    pub fn with_defaults() -> Self {
        Self::new(MAX_HOT_ADAPTERS, None)
    }

    /// Load an adapter into the cache
    ///
    /// Returns: (success, evicted_adapter_id)
    pub fn load(
        &self,
        adapter_id: u32,
        adapter_name: String,
        lora_a: Vec<f32>,
        lora_b: Vec<f32>,
        scale: f32,
    ) -> Result<Option<u32>, String> {
        // Check if already loaded
        if self.adapters.contains_key(&adapter_id) {
            debug!(adapter_id = adapter_id, "Adapter already cached");
            return Ok(None);
        }

        let adapter = CachedAdapter::new(adapter_id, adapter_name, lora_a, lora_b, scale);
        let memory_bytes = adapter.memory_bytes;

        // Check memory limit
        if let Some(max) = self.max_bytes {
            if self.current_bytes.load(Ordering::Relaxed) + memory_bytes > max {
                // Try to evict least recently used adapter
                if let Some(evicted_id) = self.evict_lru() {
                    info!(
                        evicted = evicted_id,
                        new_adapter = adapter_id,
                        "Evicted LRU adapter to make room"
                    );
                } else {
                    return Err(format!(
                        "Cannot load adapter {}: memory limit exceeded ({} bytes)",
                        adapter_id, max
                    ));
                }
            }
        }

        // Check adapter count limit
        if self.adapters.len() >= self.max_adapters {
            if let Some(evicted_id) = self.evict_lru() {
                info!(
                    evicted = evicted_id,
                    new_adapter = adapter_id,
                    max_adapters = self.max_adapters,
                    "Evicted LRU adapter (max adapters reached)"
                );
            } else {
                return Err(format!(
                    "Cannot load adapter {}: max adapters ({}) reached",
                    adapter_id, self.max_adapters
                ));
            }
        }

        // Insert the adapter
        self.adapters.insert(adapter_id, Arc::new(adapter));
        self.current_bytes
            .fetch_add(memory_bytes, Ordering::Relaxed);
        self.loads.fetch_add(1, Ordering::Relaxed);

        info!(
            adapter_id = adapter_id,
            memory_bytes = memory_bytes,
            total_adapters = self.adapters.len(),
            "Loaded adapter into cache"
        );

        Ok(None)
    }

    /// Unload an adapter from the cache
    pub fn unload(&self, adapter_id: u32) -> Option<u64> {
        if let Some((_, adapter)) = self.adapters.remove(&adapter_id) {
            let bytes = adapter.memory_bytes;
            self.current_bytes.fetch_sub(bytes, Ordering::Relaxed);
            self.unloads.fetch_add(1, Ordering::Relaxed);

            info!(
                adapter_id = adapter_id,
                freed_bytes = bytes,
                remaining = self.adapters.len(),
                "Unloaded adapter from cache"
            );

            Some(bytes)
        } else {
            None
        }
    }

    /// Evict least recently used adapter
    fn evict_lru(&self) -> Option<u32> {
        let lru = self
            .adapters
            .iter()
            .min_by(|a, b| a.value().last_used.read().cmp(&b.value().last_used.read()))
            .map(|e| *e.key());

        if let Some(id) = lru {
            self.unload(id);
            Some(id)
        } else {
            None
        }
    }

    /// Get an adapter (and touch it)
    pub fn get(&self, adapter_id: u32) -> Option<Arc<CachedAdapter>> {
        self.adapters.get(&adapter_id).map(|entry| {
            entry.value().touch();
            entry.value().clone()
        })
    }

    /// Get multiple adapters for fusion
    pub fn get_many(&self, adapter_ids: &[u32]) -> Vec<Arc<CachedAdapter>> {
        adapter_ids.iter().filter_map(|&id| self.get(id)).collect()
    }

    /// Check if an adapter is cached
    pub fn contains(&self, adapter_id: u32) -> bool {
        self.adapters.contains_key(&adapter_id)
    }

    /// Get all cached adapter IDs
    pub fn cached_ids(&self) -> Vec<u32> {
        self.adapters.iter().map(|e| *e.key()).collect()
    }

    /// Record a fusion operation
    pub fn record_fusion(&self) {
        self.fusions.fetch_add(1, Ordering::Relaxed);
    }

    /// Get cache statistics
    pub fn stats(&self) -> AdapterCacheStats {
        AdapterCacheStats {
            cached_adapters: self.adapters.len(),
            max_adapters: self.max_adapters,
            memory_bytes: self.current_bytes.load(Ordering::Relaxed),
            max_bytes: self.max_bytes,
            loads: self.loads.load(Ordering::Relaxed),
            unloads: self.unloads.load(Ordering::Relaxed),
            fusions: self.fusions.load(Ordering::Relaxed),
        }
    }

    /// Clear all cached adapters
    pub fn clear(&self) {
        self.adapters.clear();
        self.current_bytes.store(0, Ordering::Relaxed);
        info!("Cleared all cached adapters");
    }
}

/// Adapter cache statistics
#[derive(Debug, Clone)]
pub struct AdapterCacheStats {
    pub cached_adapters: usize,
    pub max_adapters: usize,
    pub memory_bytes: u64,
    pub max_bytes: Option<u64>,
    pub loads: u64,
    pub unloads: u64,
    pub fusions: u64,
}

impl AdapterCacheStats {
    /// Get utilization as a percentage (by count)
    pub fn utilization(&self) -> f64 {
        if self.max_adapters == 0 {
            0.0
        } else {
            (self.cached_adapters as f64 / self.max_adapters as f64) * 100.0
        }
    }

    /// Get memory utilization as a percentage
    pub fn memory_utilization(&self) -> Option<f64> {
        self.max_bytes.map(|max| {
            if max == 0 {
                0.0
            } else {
                (self.memory_bytes as f64 / max as f64) * 100.0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_adapter() {
        let cache = AdapterCache::new(4, None);

        let result = cache.load(
            1,
            "adapter-1".to_string(),
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            1.0,
        );

        assert!(result.is_ok());
        assert!(cache.contains(1));
        assert_eq!(cache.adapters.len(), 1);
    }

    #[test]
    fn test_unload_adapter() {
        let cache = AdapterCache::new(4, None);

        cache
            .load(
                1,
                "adapter-1".to_string(),
                vec![1.0; 100],
                vec![1.0; 100],
                1.0,
            )
            .unwrap();

        let freed = cache.unload(1);
        assert!(freed.is_some());
        assert_eq!(freed.unwrap(), 800); // 200 f32s * 4 bytes
        assert!(!cache.contains(1));
    }

    #[test]
    fn test_lru_eviction() {
        let cache = AdapterCache::new(2, None); // Max 2 adapters

        // Load 2 adapters
        cache
            .load(1, "a1".to_string(), vec![1.0], vec![1.0], 1.0)
            .unwrap();
        cache
            .load(2, "a2".to_string(), vec![1.0], vec![1.0], 1.0)
            .unwrap();

        // Touch adapter 2 to make it more recently used
        cache.get(2);

        // Load a third adapter - should evict adapter 1 (LRU)
        cache
            .load(3, "a3".to_string(), vec![1.0], vec![1.0], 1.0)
            .unwrap();

        assert!(!cache.contains(1)); // Evicted
        assert!(cache.contains(2)); // Kept (more recently used)
        assert!(cache.contains(3)); // Newly loaded
    }

    #[test]
    fn test_get_many() {
        let cache = AdapterCache::new(4, None);

        cache
            .load(1, "a1".to_string(), vec![1.0], vec![1.0], 1.0)
            .unwrap();
        cache
            .load(2, "a2".to_string(), vec![2.0], vec![2.0], 1.0)
            .unwrap();
        cache
            .load(3, "a3".to_string(), vec![3.0], vec![3.0], 1.0)
            .unwrap();

        let adapters = cache.get_many(&[1, 3, 5]); // 5 doesn't exist
        assert_eq!(adapters.len(), 2);
    }

    #[test]
    fn test_stats() {
        let cache = AdapterCache::new(4, Some(1000));

        cache
            .load(1, "a1".to_string(), vec![1.0; 50], vec![1.0; 50], 1.0)
            .unwrap();

        let stats = cache.stats();
        assert_eq!(stats.cached_adapters, 1);
        assert_eq!(stats.max_adapters, 4);
        assert_eq!(stats.memory_bytes, 400); // 100 f32s * 4 bytes
        assert_eq!(stats.loads, 1);
    }
}
