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
}
