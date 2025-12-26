//! LRU cache for memory-mapped adapters

use crate::implementation::LoadedAdapter;
use crate::metrics::CacheMetrics;
use adapteros_core::{AosError, Result};
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, trace};

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_size_bytes: u64,
    pub max_count: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 1024 * 1024 * 1024,
            max_count: 100,
        }
    }
}

pub struct AdapterCache {
    cache: RwLock<LruCache<PathBuf, Arc<LoadedAdapter>>>,
    config: CacheConfig,
    metrics: Arc<CacheMetrics>,
}

impl AdapterCache {
    pub fn new(config: CacheConfig) -> Self {
        let capacity = if config.max_count > 0 {
            NonZeroUsize::new(config.max_count).unwrap()
        } else {
            NonZeroUsize::new(100).unwrap()
        };

        Self {
            cache: RwLock::new(LruCache::new(capacity)),
            config,
            metrics: Arc::new(CacheMetrics::new()),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(CacheConfig::default())
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Option<Arc<LoadedAdapter>> {
        let path = path.as_ref();
        let mut cache = self.cache.write();

        if let Some(adapter) = cache.get(&path.to_path_buf()) {
            trace!(path = %path.display(), "Cache hit");
            self.metrics.record_hit();
            Some(Arc::clone(adapter))
        } else {
            trace!(path = %path.display(), "Cache miss");
            self.metrics.record_miss();
            None
        }
    }

    pub fn insert<P: AsRef<Path>>(&self, path: P, adapter: Arc<LoadedAdapter>) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        // Calculate approximate size from Metal buffers
        let size = Self::calculate_adapter_size(&adapter);

        if self.config.max_size_bytes > 0 {
            self.evict_for_size(size)?;
        }

        let mut cache = self.cache.write();

        if let Some((evicted_path, evicted_adapter)) = cache.push(path.clone(), adapter) {
            debug!(path = %evicted_path.display(), "Evicted adapter");
            let evicted_size = Self::calculate_adapter_size(&evicted_adapter);
            self.metrics.record_eviction(evicted_size);
        }

        self.metrics.update_size(size as i64);
        debug!(path = %path.display(), size_bytes = size, "Inserted adapter");

        Ok(())
    }

    pub fn remove<P: AsRef<Path>>(&self, path: P) -> Option<Arc<LoadedAdapter>> {
        let mut cache = self.cache.write();

        if let Some(adapter) = cache.pop(&path.as_ref().to_path_buf()) {
            let size = Self::calculate_adapter_size(&adapter);
            self.metrics.update_size(-(size as i64));
            Some(adapter)
        } else {
            None
        }
    }

    /// Calculate approximate size of a LoadedAdapter from its Metal buffers
    fn calculate_adapter_size(adapter: &LoadedAdapter) -> u64 {
        adapter.buffers.values().map(|buffer| buffer.length()).sum()
    }

    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    pub fn size_bytes(&self) -> u64 {
        self.metrics.size_bytes()
    }

    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    pub fn metrics(&self) -> Arc<CacheMetrics> {
        Arc::clone(&self.metrics)
    }


    fn evict_for_size(&self, needed_size: u64) -> Result<()> {
        let current_size = self.size_bytes();
        let max_size = self.config.max_size_bytes;

        if current_size + needed_size <= max_size {
            return Ok(());
        }

        let mut cache = self.cache.write();
        let mut freed_size = 0u64;

        while current_size + needed_size - freed_size > max_size {
            if let Some((_path, adapter)) = cache.pop_lru() {
                let size = adapter.size_bytes();
                freed_size += size;
                self.metrics.record_eviction(size);
            } else {
                return Err(AosError::Io(format!(
                    "Cannot make room for {} bytes (max: {})",
                    needed_size, max_size
                )));
            }
        }

        self.metrics.update_size(-(freed_size as i64));
        Ok(())
    }

    // ==================== MoE Support Methods ====================

    /// Get all cached MoE adapters
    pub fn get_moe_adapters(&self) -> Vec<(PathBuf, Arc<LoadedAdapter>)> {
        let cache = self.cache.read();
        cache
            .iter()
            .filter(|(_, adapter)| adapter.is_moe_adapter())
            .map(|(path, adapter)| (path.clone(), Arc::clone(adapter)))
            .collect()
    }

    /// Get total memory used by MoE adapters
    pub fn moe_size_bytes(&self) -> u64 {
        let cache = self.cache.read();
        cache
            .iter()
            .filter(|(_, adapter)| adapter.is_moe_adapter())
            .map(|(_, adapter)| adapter.size_bytes())
            .sum()
    }

    /// Get count of MoE adapters in cache
    pub fn moe_count(&self) -> usize {
        let cache = self.cache.read();
        cache
            .iter()
            .filter(|(_, adapter)| adapter.is_moe_adapter())
            .count()
    }

    /// Evict all MoE adapters from cache
    ///
    /// Returns the total bytes freed.
    pub fn evict_all_moe_adapters(&self) -> u64 {
        let mut cache = self.cache.write();
        let mut freed_size = 0u64;

        // Collect paths of MoE adapters to evict
        let moe_paths: Vec<PathBuf> = cache
            .iter()
            .filter(|(_, adapter)| adapter.is_moe_adapter())
            .map(|(path, _)| path.clone())
            .collect();

        for path in moe_paths {
            if let Some(adapter) = cache.pop(&path) {
                let size = adapter.size_bytes();
                freed_size += size;
                self.metrics.record_eviction(size);
                debug!(path = %path.display(), size_bytes = size, "Evicted MoE adapter");
            }
        }

        self.metrics.update_size(-(freed_size as i64));
        freed_size
    }

    /// Evict MoE adapters to make room for specified size
    ///
    /// This method specifically targets MoE adapters for eviction,
    /// useful when loading a new MoE adapter and wanting to reclaim
    /// memory from other MoE adapters first.
    pub fn evict_moe_for_size(&self, needed_size: u64) -> Result<u64> {
        let current_moe_size = self.moe_size_bytes();

        if needed_size <= current_moe_size {
            return Ok(0); // Already have enough headroom
        }

        let mut cache = self.cache.write();
        let mut freed_size = 0u64;

        // Get MoE adapters sorted by LRU order (oldest first)
        let mut moe_entries: Vec<(PathBuf, u64)> = cache
            .iter()
            .filter(|(_, adapter)| adapter.is_moe_adapter())
            .map(|(path, adapter)| (path.clone(), adapter.size_bytes()))
            .collect();

        // Evict until we have enough room
        for (path, size) in moe_entries.drain(..) {
            if freed_size >= needed_size {
                break;
            }

            if cache.pop(&path).is_some() {
                freed_size += size;
                self.metrics.record_eviction(size);
                debug!(
                    path = %path.display(),
                    size_bytes = size,
                    "Evicted MoE adapter to make room"
                );
            }
        }

        self.metrics.update_size(-(freed_size as i64));

        if freed_size < needed_size {
            return Err(AosError::Io(format!(
                "Cannot free {} bytes from MoE adapters (only freed {})",
                needed_size, freed_size
            )));
        }

        Ok(freed_size)
    }

    /// Check if inserting an MoE adapter would exceed memory limits
    pub fn would_exceed_limit_for_moe(&self, adapter_size: u64) -> bool {
        self.size_bytes() + adapter_size > self.config.max_size_bytes
    }

    /// Get memory pressure ratio for MoE adapters (0.0 - 1.0+)
    pub fn moe_memory_pressure(&self) -> f64 {
        let moe_size = self.moe_size_bytes();
        let max_size = self.config.max_size_bytes;
        if max_size == 0 {
            return 0.0;
        }
        moe_size as f64 / max_size as f64
    }
}
