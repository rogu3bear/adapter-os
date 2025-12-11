//! Model handle cache for per-worker deduplication
//!
//! This module provides [`ModelHandleCache`], a thread-safe LRU cache that
//! deduplicates loaded models within a single worker process. Different
//! backend types (Metal, CoreML, MLX) have different model handle types,
//! so we use a type-erased [`ModelHandle`] enum.
//!
//! # Design Note: Relationship to `adapteros-memory::ModelCache`
//!
//! This cache is **intentionally separate** from `ModelCache` in `adapteros-memory`:
//!
//! | Aspect | `ModelHandleCache` (here) | `ModelCache` (adapteros-memory) |
//! |--------|---------------------------|----------------------------------|
//! | Scope | Per-worker process dedup | Control plane memory management |
//! | Key | `ModelKey` (backend + hash) | Generic `K` |
//! | Value | Type-erased `ModelHandle` | Generic `T` with tenant/quality |
//! | Eviction | Simple LRU + memory limit | Quality-scored with `UnifiedMemoryManager` |
//! | Use case | Avoid redundant model loads | Tenant-aware caching with eviction policies |
//!
//! The worker cache is specialized for:
//! - Backend-aware deduplication (same model, different backends = separate entries)
//! - Fast `get_or_load` pattern without tenant/quality overhead
//! - Type-erased storage for heterogeneous backend handle types
//!
//! If these caches need to be consolidated in the future, consider making
//! `ModelCache` support the `get_or_load` pattern and type-erased values.

use crate::{base_model_state::BaseModelState, model_key::ModelKey};
use adapteros_core::{constants::BYTES_PER_MB, AosError, Result};
use adapteros_telemetry::metrics::critical_components::CriticalComponentMetrics;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Handle;

#[cfg(feature = "multi-backend")]
use adapteros_lora_mlx_ffi::MLXFFIModel;

/// Type-erased model handle for caching across backends
///
/// Different backends have different model handle types:
/// - Metal: Raw bytes passed to `MetalKernels::load()`
/// - MLX: Loaded `MLXFFIModel` ready for inference
/// - CoreML: No handle needed (FFI manages model internally)
#[derive(Clone)]
pub enum ModelHandle {
    /// Metal: raw model bytes (SafeTensors format)
    Metal(Arc<Vec<u8>>),

    /// MLX: loaded model ready for inference
    #[cfg(feature = "multi-backend")]
    Mlx(Arc<MLXFFIModel>),

    /// CoreML: no handle needed (FFI manages model lifecycle)
    CoreML,
}

impl ModelHandle {
    /// Get the Metal model bytes, or error if wrong variant
    pub fn as_metal_bytes(&self) -> Result<Arc<Vec<u8>>> {
        match self {
            ModelHandle::Metal(bytes) => Ok(Arc::clone(bytes)),
            #[cfg(feature = "multi-backend")]
            ModelHandle::Mlx(_) => Err(AosError::Internal(
                "Expected Metal handle, got MLX".to_string(),
            )),
            ModelHandle::CoreML => Err(AosError::Internal(
                "Expected Metal handle, got CoreML".to_string(),
            )),
        }
    }

    /// Get the MLX model, or error if wrong variant
    #[cfg(feature = "multi-backend")]
    pub fn as_mlx_model(&self) -> Result<Arc<MLXFFIModel>> {
        match self {
            ModelHandle::Mlx(model) => Ok(Arc::clone(model)),
            ModelHandle::Metal(_) => Err(AosError::Internal(
                "Expected MLX handle, got Metal".to_string(),
            )),
            ModelHandle::CoreML => Err(AosError::Internal(
                "Expected MLX handle, got CoreML".to_string(),
            )),
        }
    }

    /// Check if this is a CoreML handle
    pub fn is_coreml(&self) -> bool {
        matches!(self, ModelHandle::CoreML)
    }

    /// Get the variant name for logging
    pub fn variant_name(&self) -> &'static str {
        match self {
            ModelHandle::Metal(_) => "Metal",
            #[cfg(feature = "multi-backend")]
            ModelHandle::Mlx(_) => "Mlx",
            ModelHandle::CoreML => "CoreML",
        }
    }
}

impl std::fmt::Debug for ModelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelHandle::Metal(bytes) => {
                write!(f, "ModelHandle::Metal({} bytes)", bytes.len())
            }
            #[cfg(feature = "multi-backend")]
            ModelHandle::Mlx(_) => write!(f, "ModelHandle::Mlx(...)"),
            ModelHandle::CoreML => write!(f, "ModelHandle::CoreML"),
        }
    }
}

/// Listener for cache lifecycle events.
pub trait CacheEventListener: Send + Sync {
    fn on_load(&self, _key: &ModelKey, _memory_bytes: u64) {}
    fn on_reuse(&self, _key: &ModelKey) {}
    fn on_evict(&self, _key: &ModelKey) {}
    fn on_error(&self, _key: &ModelKey, _error: &AosError) {}
}

/// RAII guard that keeps a model marked active while in scope.
pub struct ActiveGuard<'a> {
    cache: &'a ModelHandleCache,
    key: ModelKey,
    released: bool,
}

impl<'a> ActiveGuard<'a> {
    fn new(cache: &'a ModelHandleCache, key: ModelKey) -> Self {
        Self {
            cache,
            key,
            released: false,
        }
    }

    /// Explicitly release the active mark before the guard drops.
    pub fn release(mut self) {
        if !self.released {
            self.cache.mark_inactive(&self.key);
            self.released = true;
        }
    }
}

impl<'a> Drop for ActiveGuard<'a> {
    fn drop(&mut self) {
        if !self.released {
            self.cache.mark_inactive(&self.key);
        }
    }
}

/// Cached model entry with metadata
pub struct CachedModelEntry {
    /// The cached model handle
    pub handle: ModelHandle,
    /// When the model was loaded
    pub loaded_at: Instant,
    /// Number of times this model has been accessed
    pub access_count: u64,
    /// Estimated memory usage in bytes
    pub memory_bytes: u64,
}

/// Thread-safe LRU cache for model handles
///
/// This cache ensures that the same model is only loaded once per worker
/// process, even if multiple code paths request it. The cache key is
/// `(backend_type, manifest_hash)` to ensure different backends and
/// model versions are cached separately.
///
/// # Pinning
///
/// Base models can be "pinned" to prevent eviction during adapter churn.
/// Use [`get_or_load_base_model`] to load and auto-pin, or [`pin`]/[`unpin`]
/// for manual control. Pinned entries are never evicted, even under memory
/// pressure.
pub struct ModelHandleCache {
    /// The cache storage
    cache: RwLock<HashMap<ModelKey, CachedModelEntry>>,
    /// Active usage counts for eviction guards
    active_counts: RwLock<HashMap<ModelKey, u64>>,
    /// Maximum memory usage in bytes
    max_memory_bytes: u64,
    /// Cache statistics
    stats: RwLock<CacheStats>,
    /// Keys that are pinned and should not be evicted
    pinned_keys: RwLock<HashSet<ModelKey>>,
    /// Optional listeners keyed per model for lifecycle events
    listeners: RwLock<HashMap<ModelKey, Arc<dyn CacheEventListener>>>,
    /// Optional telemetry metrics for Prometheus export
    metrics: Option<Arc<CriticalComponentMetrics>>,
}

/// Cache statistics for observability
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_memory_bytes: u64,
    /// Count of eviction attempts blocked because the entry was pinned
    pub eviction_skip_pinned_count: u64,
    /// Count of eviction attempts blocked because the entry was marked active
    pub eviction_skip_active_count: u64,
}

impl CacheStats {
    /// Calculate hit ratio (0.0 to 1.0)
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl ModelHandleCache {
    /// Create a new cache with the given maximum memory limit
    pub fn new(max_memory_bytes: u64) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            active_counts: RwLock::new(HashMap::new()),
            max_memory_bytes,
            stats: RwLock::new(CacheStats::default()),
            pinned_keys: RwLock::new(HashSet::new()),
            listeners: RwLock::new(HashMap::new()),
            metrics: None,
        }
    }

    /// Create a new cache with telemetry metrics enabled
    pub fn new_with_metrics(max_memory_bytes: u64, metrics: Arc<CriticalComponentMetrics>) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            active_counts: RwLock::new(HashMap::new()),
            max_memory_bytes,
            stats: RwLock::new(CacheStats::default()),
            pinned_keys: RwLock::new(HashSet::new()),
            listeners: RwLock::new(HashMap::new()),
            metrics: Some(metrics),
        }
    }

    /// Set telemetry metrics after construction
    pub fn set_metrics(&mut self, metrics: Arc<CriticalComponentMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Register a lifecycle listener for a specific key.
    pub fn register_listener(&self, key: ModelKey, listener: Arc<dyn CacheEventListener>) {
        self.listeners.write().insert(key, listener);
    }

    /// Convenience: register BaseModelState to receive cache events for `key`.
    pub fn register_base_model_state(
        &self,
        key: ModelKey,
        state: Arc<tokio::sync::Mutex<BaseModelState>>,
    ) {
        self.register_listener(key, Arc::new(BaseModelStateEventHandler::new(state)));
    }

    /// Remove a lifecycle listener for a specific key.
    pub fn remove_listener(&self, key: &ModelKey) {
        self.listeners.write().remove(key);
    }

    fn notify_load(&self, key: &ModelKey, memory_bytes: u64) {
        if let Some(listener) = self.listeners.read().get(key) {
            listener.on_load(key, memory_bytes);
        }
    }

    fn notify_reuse(&self, key: &ModelKey) {
        if let Some(listener) = self.listeners.read().get(key) {
            listener.on_reuse(key);
        }
    }

    fn notify_evict(&self, key: &ModelKey) {
        if let Some(listener) = self.listeners.read().get(key) {
            listener.on_evict(key);
        }
    }

    fn notify_error(&self, key: &ModelKey, error: &AosError) {
        if let Some(listener) = self.listeners.read().get(key) {
            listener.on_error(key, error);
        }
    }

    /// Mark a cached model as active to prevent eviction. Returns false if key
    /// does not currently exist in the cache.
    pub fn mark_active(&self, key: &ModelKey) -> bool {
        if !self.cache.read().contains_key(key) {
            return false;
        }
        let mut active = self.active_counts.write();
        *active.entry(key.clone()).or_insert(0) += 1;
        true
    }

    /// Mark a cached model as inactive. Returns false if the key was not active.
    pub fn mark_inactive(&self, key: &ModelKey) -> bool {
        let mut active = self.active_counts.write();
        match active.get_mut(key) {
            Some(count) if *count > 1 => {
                *count -= 1;
                true
            }
            Some(_) => {
                active.remove(key);
                true
            }
            None => false,
        }
    }

    /// Whether a cached model is currently marked active.
    pub fn is_active(&self, key: &ModelKey) -> bool {
        self.active_counts
            .read()
            .get(key)
            .map(|c| *c > 0)
            .unwrap_or(false)
    }

    /// Begin an active usage guard that releases on drop.
    pub fn begin_use(&self, key: &ModelKey) -> Option<ActiveGuard<'_>> {
        if self.mark_active(key) {
            Some(ActiveGuard::new(self, key.clone()))
        } else {
            None
        }
    }

    /// Get or load a model, using the cache for deduplication
    ///
    /// The loader function is only called on cache miss. It should return
    /// the model handle and its estimated memory usage in bytes.
    ///
    /// # Thread Safety
    ///
    /// This function uses a read-lock fast path for cache hits, and
    /// upgrades to a write-lock only on cache miss. Multiple concurrent
    /// cache misses for the same key may result in multiple loads, but
    /// only one will be stored (the first to acquire the write lock).
    pub fn get_or_load<F>(&self, key: &ModelKey, loader: F) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        // Fast path: read lock for cache hit
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(key) {
                let mut stats = self.stats.write();
                stats.hits += 1;
                if let Some(ref m) = self.metrics {
                    m.record_model_cache_hit();
                }
                self.notify_reuse(key);
                tracing::debug!(
                    key = %key.short_hex(),
                    access_count = entry.access_count,
                    "Model cache hit"
                );
                return Ok(entry.handle.clone());
            }
        }

        // Slow path: cache miss, need to load
        tracing::info!(key = %key.short_hex(), "Model cache miss, loading from disk");

        let loader_result = loader();
        if let Err(ref e) = loader_result {
            self.notify_error(key, e);
        }
        let (handle, memory_bytes) = loader_result?;

        // Acquire write lock and insert
        let mut cache = self.cache.write();

        // Double-check: another thread may have loaded while we were loading
        if let Some(existing) = cache.get(key) {
            tracing::debug!(key = %key.short_hex(), "Model loaded by another thread, reusing");
            let mut stats = self.stats.write();
            stats.hits += 1;
            if let Some(ref m) = self.metrics {
                m.record_model_cache_hit();
            }
            self.notify_reuse(key);
            return Ok(existing.handle.clone());
        }

        // Evict if necessary to make room
        self.evict_for_size_locked(&mut cache, memory_bytes)?;

        // Insert the new entry
        cache.insert(
            key.clone(),
            CachedModelEntry {
                handle: handle.clone(),
                loaded_at: Instant::now(),
                access_count: 1,
                memory_bytes,
            },
        );

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.misses += 1;
            stats.total_memory_bytes += memory_bytes;
        }
        if let Some(ref m) = self.metrics {
            m.record_model_cache_miss();
        }

        tracing::info!(
            key = %key.short_hex(),
            memory_mb = memory_bytes / (1024 * 1024),
            "Model loaded and cached"
        );
        self.notify_load(key, memory_bytes);

        Ok(handle)
    }

    /// Get or load a base model, automatically pinning it to prevent eviction
    ///
    /// Base models should remain resident while adapters are hot-swapped.
    /// This method calls [`get_or_load`] and then pins the entry.
    ///
    /// # Warning
    ///
    /// The pinned model will **never** be evicted until explicitly unpinned via
    /// [`unpin()`]. Callers MUST ensure `unpin()` is called when the base model
    /// is no longer needed, or the cache will grow unbounded. This is especially
    /// important for long-running workers where models may be swapped out.
    ///
    /// To monitor for pinning leaks in production:
    /// - Watch the `model_cache_pinned_entries` gauge
    /// - Watch the `model_cache_eviction_blocked_pinned_total` counter rate
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handle = cache.get_or_load_base_model(&base_key, || {
    ///     Ok((ModelHandle::Metal(Arc::new(model_bytes)), size))
    /// })?;
    /// // base_key is now pinned and won't be evicted
    ///
    /// // When done with the base model:
    /// cache.unpin(&base_key);
    /// ```
    pub fn get_or_load_base_model<F>(&self, key: &ModelKey, loader: F) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        let handle = self.get_or_load(key, loader)?;

        // Auto-pin the base model
        {
            let mut pinned = self.pinned_keys.write();
            if pinned.insert(key.clone()) {
                if let Some(ref m) = self.metrics {
                    m.set_pinned_entries_count(pinned.len());
                }
                tracing::info!(
                    key = %key.short_hex(),
                    "Base model pinned to prevent eviction"
                );
            }
        }

        // Base models are considered active while resident.
        let _ = self.mark_active(key);

        Ok(handle)
    }

    /// Pin a cache entry to prevent eviction
    ///
    /// Returns `true` if the key was found in cache and pinned,
    /// `false` if the key is not in the cache.
    pub fn pin(&self, key: &ModelKey) -> bool {
        // Check if key exists in cache first
        let exists = self.cache.read().contains_key(key);
        if !exists {
            return false;
        }

        let mut pinned = self.pinned_keys.write();
        let was_new = pinned.insert(key.clone());
        if was_new {
            if let Some(ref m) = self.metrics {
                m.set_pinned_entries_count(pinned.len());
            }
        }
        tracing::debug!(key = %key.short_hex(), "Model pinned");
        true
    }

    /// Unpin a cache entry, allowing it to be evicted
    ///
    /// Returns `true` if the key was pinned and is now unpinned,
    /// `false` if the key was not pinned.
    pub fn unpin(&self, key: &ModelKey) -> bool {
        let mut pinned = self.pinned_keys.write();
        let removed = pinned.remove(key);
        if removed {
            if let Some(ref m) = self.metrics {
                m.set_pinned_entries_count(pinned.len());
            }
            tracing::debug!(key = %key.short_hex(), "Model unpinned");
        }
        removed
    }

    /// Check if a cache entry is pinned
    pub fn is_pinned(&self, key: &ModelKey) -> bool {
        self.pinned_keys.read().contains(key)
    }

    /// Get the number of pinned entries
    pub fn pinned_count(&self) -> usize {
        self.pinned_keys.read().len()
    }

    /// Evict models to make room for a new entry (called with write lock held)
    ///
    /// Pinned entries are never evicted. If all evictable entries are exhausted
    /// but the target is not reached, the function returns early (allowing the
    /// cache to exceed its limit temporarily).
    fn evict_for_size_locked(
        &self,
        cache: &mut HashMap<ModelKey, CachedModelEntry>,
        needed_bytes: u64,
    ) -> Result<()> {
        let current: u64 = cache.values().map(|e| e.memory_bytes).sum();
        if current + needed_bytes <= self.max_memory_bytes {
            return Ok(());
        }

        // Get pinned keys for filtering
        let pinned = self.pinned_keys.read();
        let active = self.active_counts.read();

        // LRU eviction: sort by loaded_at (oldest first), then by access_count
        // Filter out pinned entries and active entries
        let mut entries: Vec<_> = cache
            .iter()
            .filter(|(k, _)| !pinned.contains(*k) && active.get(*k).copied().unwrap_or(0) == 0)
            .map(|(k, e)| (k.clone(), e.loaded_at, e.access_count, e.memory_bytes))
            .collect();

        // Sort by: oldest first, then least accessed
        entries.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

        // Count how many pinned entries we're skipping
        let pinned_in_cache = cache.keys().filter(|k| pinned.contains(*k)).count();
        let active_in_cache = cache
            .keys()
            .filter(|k| active.get(*k).copied().unwrap_or(0) > 0)
            .count();
        drop(pinned); // Release lock before modifying stats
        drop(active);

        let mut freed = 0u64;
        let target = current + needed_bytes - self.max_memory_bytes;

        for (key, _, _, mem) in entries {
            if freed >= target {
                break;
            }
            cache.remove(&key);
            self.active_counts.write().remove(&key);
            self.notify_evict(&key);
            self.listeners.write().remove(&key);
            freed += mem;

            let mut stats = self.stats.write();
            stats.evictions += 1;
            stats.total_memory_bytes = stats.total_memory_bytes.saturating_sub(mem);

            tracing::info!(
                key = %key.short_hex(),
                freed_mb = mem / (1024 * 1024),
                "Evicted model from cache"
            );
        }

        // Track pinned entries encountered during eviction (even if we freed enough)
        // to surface that pinned bases constrained eviction options.
        if target > 0 && pinned_in_cache > 0 {
            let mut stats = self.stats.write();
            stats.eviction_skip_pinned_count += pinned_in_cache as u64;

            // Emit telemetry for each blocked eviction attempt
            if let Some(ref m) = self.metrics {
                for _ in 0..pinned_in_cache {
                    m.record_eviction_blocked_pinned();
                }
            }

            tracing::warn!(
                pinned_count = pinned_in_cache,
                freed_mb = freed / (1024 * 1024),
                target_mb = target / (1024 * 1024),
                "Could not free enough memory due to pinned entries"
            );
        }

        if target > 0 && active_in_cache > 0 {
            let mut stats = self.stats.write();
            stats.eviction_skip_active_count += active_in_cache as u64;

            tracing::warn!(
                active_count = active_in_cache,
                freed_mb = freed / (1024 * 1024),
                target_mb = target / (1024 * 1024),
                "Could not free enough memory due to active entries"
            );
        }

        if freed < target {
            return Err(AosError::Config(format!(
                "Model cache budget exceeded: needed {} MB, freed {} MB (pinned={}, active={}), max {} MB",
                needed_bytes / BYTES_PER_MB,
                freed / BYTES_PER_MB,
                pinned_in_cache,
                active_in_cache,
                self.max_memory_bytes / BYTES_PER_MB
            )));
        }

        Ok(())
    }

    /// Get current memory usage in bytes
    pub fn memory_usage(&self) -> u64 {
        self.cache.read().values().map(|e| e.memory_bytes).sum()
    }

    /// Get number of cached models
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.stats.read().clone()
    }

    /// Clear the cache (for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
        let mut pinned = self.pinned_keys.write();
        pinned.clear();
        self.active_counts.write().clear();
        self.listeners.write().clear();
        if let Some(ref m) = self.metrics {
            m.set_pinned_entries_count(0);
        }
        let mut stats = self.stats.write();
        *stats = CacheStats::default();
    }
}

/// Event handler that forwards cache lifecycle updates into BaseModelState.
#[derive(Clone)]
pub struct BaseModelStateEventHandler {
    state: Arc<tokio::sync::Mutex<BaseModelState>>,
}

impl BaseModelStateEventHandler {
    pub fn new(state: Arc<tokio::sync::Mutex<BaseModelState>>) -> Self {
        Self { state }
    }

    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(fut);
        } else {
            tracing::debug!("No async runtime available to publish BaseModelState cache events");
        }
    }
}

impl CacheEventListener for BaseModelStateEventHandler {
    fn on_load(&self, _key: &ModelKey, memory_bytes: u64) {
        let state = self.state.clone();
        let memory_mb = ((memory_bytes + BYTES_PER_MB - 1) / BYTES_PER_MB) as u32;
        self.spawn(async move {
            let mut guard = state.lock().await;
            if let Err(e) = guard.mark_loaded(memory_mb).await {
                tracing::warn!(error = %e, "Failed to record base model load");
            }
        });
    }

    fn on_reuse(&self, _key: &ModelKey) {
        let state = self.state.clone();
        self.spawn(async move {
            let mut guard = state.lock().await;
            let memory_mb = guard.memory_usage_mb().unwrap_or(0);
            if let Err(e) = guard.mark_loaded(memory_mb).await {
                tracing::warn!(error = %e, "Failed to record base model reuse");
            }
        });
    }

    fn on_evict(&self, _key: &ModelKey) {
        let state = self.state.clone();
        self.spawn(async move {
            let mut guard = state.lock().await;
            if let Err(e) = guard.mark_unloaded().await {
                tracing::warn!(error = %e, "Failed to record base model eviction");
            }
        });
    }

    fn on_error(&self, _key: &ModelKey, error: &AosError) {
        let state = self.state.clone();
        let message = error.to_string();
        self.spawn(async move {
            let mut guard = state.lock().await;
            if let Err(e) = guard.mark_error(message).await {
                tracing::warn!(error = %e, "Failed to record base model error");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_lora_kernel_api::attestation::BackendType;

    fn make_key(backend: BackendType, data: &[u8]) -> ModelKey {
        ModelKey::new(backend, B3Hash::hash(data))
    }

    #[test]
    fn test_cache_hit() {
        let cache = ModelHandleCache::new(1024 * 1024 * 1024); // 1GB
        let key = make_key(BackendType::Metal, b"model1");

        let mut load_count = 0;

        // First load: cache miss
        let result1 = cache.get_or_load(&key, || {
            load_count += 1;
            Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
        });
        assert!(result1.is_ok());
        assert_eq!(load_count, 1);

        // Second load: cache hit
        let result2 = cache.get_or_load(&key, || {
            load_count += 1;
            Ok((ModelHandle::Metal(Arc::new(vec![4, 5, 6])), 3))
        });
        assert!(result2.is_ok());
        assert_eq!(load_count, 1); // Should NOT have loaded again

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_different_backends_separate() {
        let cache = ModelHandleCache::new(1024 * 1024 * 1024);
        let key1 = make_key(BackendType::Metal, b"model");
        let key2 = make_key(BackendType::Mock, b"model"); // Same hash, different backend

        let mut load_count = 0;

        // Load Metal
        cache
            .get_or_load(&key1, || {
                load_count += 1;
                Ok((ModelHandle::Metal(Arc::new(vec![1])), 1))
            })
            .unwrap();

        // Load Mock (same manifest hash, different backend)
        cache
            .get_or_load(&key2, || {
                load_count += 1;
                Ok((ModelHandle::CoreML, 0))
            })
            .unwrap();

        assert_eq!(load_count, 2); // Both should have loaded
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_eviction_on_memory_pressure() {
        let cache = ModelHandleCache::new(100); // Very small: 100 bytes

        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        // Load first model: 60 bytes
        cache
            .get_or_load(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap();

        assert_eq!(cache.len(), 1);

        // Load second model: 60 bytes -> should evict first
        cache
            .get_or_load(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap();

        // First should have been evicted
        assert_eq!(cache.len(), 1);
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn test_eviction_blocks_when_pinned() {
        let cache = ModelHandleCache::new(100);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        cache
            .get_or_load(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 80])), 80))
            })
            .unwrap();
        assert!(cache.pin(&key1));

        let result = cache.get_or_load(&key2, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
        });
        assert!(result.is_err(), "Pinned entry should prevent eviction");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_eviction_blocks_when_active() {
        let cache = ModelHandleCache::new(100);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        cache
            .get_or_load(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 80])), 80))
            })
            .unwrap();
        assert!(cache.mark_active(&key1));

        let result = cache.get_or_load(&key2, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
        });
        assert!(result.is_err(), "Active entry should prevent eviction");
        assert_eq!(cache.len(), 1);

        assert!(cache.mark_inactive(&key1));
    }

    #[test]
    fn test_metal_bytes_accessor() {
        let bytes = Arc::new(vec![1, 2, 3, 4, 5]);
        let handle = ModelHandle::Metal(Arc::clone(&bytes));

        let retrieved = handle.as_metal_bytes().unwrap();
        assert_eq!(*retrieved, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_coreml_has_no_bytes() {
        let handle = ModelHandle::CoreML;
        assert!(handle.is_coreml());
        assert!(handle.as_metal_bytes().is_err());
    }

    #[test]
    fn test_cache_stats() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"model");

        // Initial stats
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_ratio(), 0.0);

        // First load (miss)
        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // Second load (hit)
        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_ratio(), 0.5);
    }

    #[test]
    fn test_manifest_hash_change_causes_cache_miss() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model_a");
        let key2 = make_key(BackendType::Metal, b"model_b"); // different manifest hash

        cache
            .get_or_load(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();

        let stats = cache.stats();
        assert_eq!(
            stats.misses, 2,
            "Different manifest hashes must not share cache entries"
        );
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_pin_unpin() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"model");

        // Pin non-existent key should fail
        assert!(!cache.pin(&key));
        assert!(!cache.is_pinned(&key));
        assert_eq!(cache.pinned_count(), 0);

        // Load model
        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // Now pin should succeed
        assert!(cache.pin(&key));
        assert!(cache.is_pinned(&key));
        assert_eq!(cache.pinned_count(), 1);

        // Unpin should succeed
        assert!(cache.unpin(&key));
        assert!(!cache.is_pinned(&key));
        assert_eq!(cache.pinned_count(), 0);

        // Second unpin should return false
        assert!(!cache.unpin(&key));
    }

    #[test]
    fn test_get_or_load_base_model_auto_pins() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"base_model");

        // Load base model - should auto-pin
        cache
            .get_or_load_base_model(&key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
            })
            .unwrap();

        assert!(cache.is_pinned(&key));
        assert_eq!(cache.pinned_count(), 1);
    }

    #[test]
    fn test_pinned_entry_not_evicted() {
        let cache = ModelHandleCache::new(100); // Very small: 100 bytes

        let base_key = make_key(BackendType::Metal, b"base_model");
        let adapter_key = make_key(BackendType::Metal, b"adapter");

        // Load and pin base model: 50 bytes
        cache
            .get_or_load_base_model(&base_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
            })
            .unwrap();

        assert!(cache.is_pinned(&base_key));

        // Load adapter: 60 bytes (would normally evict base due to size)
        cache
            .get_or_load(&adapter_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap_err();

        // Pinned entry remains; adapter load is rejected to honor budget
        assert_eq!(cache.len(), 1);
        assert!(cache.is_pinned(&base_key));

        // Stats should show eviction was blocked
        let stats = cache.stats();
        assert_eq!(stats.evictions, 0, "No evictions should have occurred");
        assert!(
            stats.eviction_skip_pinned_count > 0,
            "Evictions should have been blocked by pinning"
        );
    }

    #[test]
    fn test_unpinned_entry_evicted_first() {
        let cache = ModelHandleCache::new(100); // 100 bytes

        let pinned_key = make_key(BackendType::Metal, b"pinned");
        let unpinned_key = make_key(BackendType::Metal, b"unpinned");
        let new_key = make_key(BackendType::Metal, b"new");

        // Load pinned model: 40 bytes
        cache
            .get_or_load_base_model(&pinned_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 40])), 40))
            })
            .unwrap();

        // Load unpinned model: 40 bytes
        cache
            .get_or_load(&unpinned_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 40])), 40))
            })
            .unwrap();

        assert_eq!(cache.len(), 2);

        // Load new model: 40 bytes (should evict unpinned, not pinned)
        cache
            .get_or_load(&new_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 40])), 40))
            })
            .unwrap();

        // Pinned should still be there, unpinned should be evicted
        assert_eq!(cache.len(), 2);
        assert!(cache.is_pinned(&pinned_key));

        let stats = cache.stats();
        assert_eq!(stats.evictions, 1, "One eviction should have occurred");
    }

    #[test]
    fn test_active_entry_blocks_eviction() {
        let cache = ModelHandleCache::new(100);
        let active_key = make_key(BackendType::Metal, b"active");
        let other_key = make_key(BackendType::Metal, b"other");

        cache
            .get_or_load(&active_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap();
        assert!(cache.mark_active(&active_key));

        cache
            .get_or_load(&other_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap_err();

        assert_eq!(cache.len(), 1, "active entry must stay resident");
        let stats = cache.stats();
        assert_eq!(stats.evictions, 0);
        assert!(
            stats.eviction_skip_active_count > 0,
            "active eviction skips should be tracked"
        );
    }

    #[test]
    fn test_base_model_state_listener_receives_events() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Default)]
        struct CountingListener {
            loads: AtomicUsize,
            evicts: AtomicUsize,
        }

        impl CacheEventListener for CountingListener {
            fn on_load(&self, _key: &ModelKey, _memory_bytes: u64) {
                self.loads.fetch_add(1, Ordering::SeqCst);
            }

            fn on_evict(&self, _key: &ModelKey) {
                self.evicts.fetch_add(1, Ordering::SeqCst);
            }
        }

        let cache = ModelHandleCache::new(80);
        let listener = Arc::new(CountingListener::default());

        let base_key = make_key(BackendType::Metal, b"base");
        cache.register_listener(base_key.clone(), listener.clone());

        cache
            .get_or_load(&base_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 40])), 40))
            })
            .unwrap();

        // Load a second model large enough to evict the first
        let other_key = make_key(BackendType::Metal, b"other");
        cache
            .get_or_load(&other_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap();

        assert_eq!(listener.loads.load(Ordering::SeqCst), 1);
        assert_eq!(listener.evicts.load(Ordering::SeqCst), 1);
    }
}
