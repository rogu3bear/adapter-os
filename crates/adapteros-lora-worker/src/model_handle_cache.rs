//! # Model Handle Cache for Per-Worker Deduplication
//!
//! This module provides [`ModelHandleCache`], a thread-safe LRU cache that
//! deduplicates loaded models within a single worker process. Different
//! backend types (Metal, CoreML, MLX) have different model handle types,
//! so we use a type-erased [`ModelHandle`] enum.
//!
//! ## Eviction Mechanism (4 Blocking Factors)
//!
//! The cache uses LRU eviction with 4 factors that can **block** eviction:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                      Eviction Decision Flow                                 │
//! │                                                                             │
//! │   New model load request (needs N bytes)                                    │
//! │                     │                                                       │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ current + needed <= max_memory? │──yes──▶ Allow load (no eviction)      │
//! │   └─────────────────────────────────┘                                       │
//! │                     │ no                                                    │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ Build eviction candidate list   │                                       │
//! │   │ (sorted: oldest → least used)   │                                       │
//! │   └─────────────────────────────────┘                                       │
//! │                     │                                                       │
//! │    For each candidate:                                                      │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ 1. Is entry PINNED?             │──yes──▶ Skip (never evict base models)│
//! │   └─────────────────────────────────┘                                       │
//! │                     │ no                                                    │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ 2. Is entry ACTIVE?             │──yes──▶ Skip (in-flight inference)    │
//! │   │    (ActiveGuard held)           │                                       │
//! │   └─────────────────────────────────┘                                       │
//! │                     │ no                                                    │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ 3. Re-validate status           │                                       │
//! │   │    (may have changed)           │──changed──▶ Skip (race condition)     │
//! │   └─────────────────────────────────┘                                       │
//! │                     │ unchanged                                             │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ 4. EVICT entry                  │                                       │
//! │   │    Update stats, notify         │                                       │
//! │   └─────────────────────────────────┘                                       │
//! │                     │                                                       │
//! │                     ▼                                                       │
//! │   ┌─────────────────────────────────┐                                       │
//! │   │ freed >= target?                │──yes──▶ Done (load can proceed)       │
//! │   └─────────────────────────────────┘                                       │
//! │                     │ no                                                    │
//! │                     ▼                                                       │
//! │         Continue to next candidate                                          │
//! │         (or allow over-limit if exhausted)                                  │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Critical Considerations
//!
//! 1. **Pinned entries**: Base models are pinned via `pin()` to prevent eviction
//!    during adapter churn. This is intentional but can cause memory pressure
//!    if many base models are pinned simultaneously.
//!
//! 2. **Active guards**: `ActiveGuard` RAII prevents eviction during inference.
//!    If a request holds a guard and another request triggers eviction, the
//!    active entry is skipped. Ensure guards are released promptly.
//!
//! 3. **Race conditions**: Between snapshot and eviction, an entry may become
//!    pinned or active. The re-validation step (line 769-776) handles this.
//!
//! 4. **Over-limit allowed**: If all entries are pinned/active, the cache
//!    temporarily exceeds `max_memory_bytes`. Monitor `eviction_skip_*` stats.
//!
//! ## Observability
//!
//! Track these metrics for cache health:
//! - `eviction_skip_pinned_count`: High = too many pinned bases
//! - `eviction_skip_active_count`: High = long-running inferences or guard leaks
//! - `hit_ratio()`: Low = cache thrashing, increase max_memory_bytes
//!
//! ## Design Note: Relationship to `adapteros-memory::ModelCache`
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
use adapteros_core::{
    constants::BYTES_PER_MB, identity::IdentityEnvelope, singleflight::SingleFlightSync, AosError,
    Result,
};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_telemetry::{
    build_model_eviction_budget_error_event, build_model_load_failed_event,
    make_model_eviction_budget_error_payload, make_model_load_failed_payload,
    metrics::critical_components::CriticalComponentMetrics, TelemetryWriter,
};
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
/// `(backend_type, manifest_hash, kernel_version, quantization, fusion_mode)`
/// to ensure different backends, builds, and execution modes cache separately.
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
    /// Optional telemetry writer for failure events
    telemetry: RwLock<Option<TelemetryWriter>>,
    /// SingleFlight for deduplicating concurrent model loads
    /// Uses String error type since AosError is not Clone
    singleflight: SingleFlightSync<ModelKey, ModelHandle, String>,
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
    /// Maximum number of pinned entries allowed (safety cap to prevent unbounded growth)
    max_pinned_entries: usize,
    /// Base model pinning state + residency counters
    base_model_pin: RwLock<BaseModelPinState>,
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

/// Base model pinning state and residency counters.
#[derive(Debug, Default, Clone)]
pub struct BaseModelPinState {
    /// Whether pinning is enabled for this worker.
    pub enabled: bool,
    /// Optional pin budget override in bytes.
    pub budget_bytes: Option<u64>,
    /// Base model identifier for telemetry.
    pub model_id: Option<String>,
    /// Cache key tracked as the base model.
    pub base_model_key: Option<ModelKey>,
    /// Base model load count (cache inserts).
    pub load_count: u64,
    /// Base model eviction count.
    pub evict_count: u64,
}

/// Operation label for SingleFlight metrics
const MODEL_LOAD_OPERATION: &str = "model_load";

/// Default maximum number of pinned entries allowed to prevent unbounded cache growth.
/// This is a safety cap - operators can override via `with_max_pinned_entries()`.
pub const DEFAULT_MAX_PINNED_ENTRIES: usize = 16;

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
            telemetry: RwLock::new(None),
            singleflight: SingleFlightSync::new(MODEL_LOAD_OPERATION),
            max_memory_bytes,
            stats: RwLock::new(CacheStats::default()),
            pinned_keys: RwLock::new(HashSet::new()),
            listeners: RwLock::new(HashMap::new()),
            metrics: None,
            max_pinned_entries: DEFAULT_MAX_PINNED_ENTRIES,
            base_model_pin: RwLock::new(BaseModelPinState::default()),
        }
    }

    /// Create a new cache with telemetry metrics enabled
    pub fn new_with_metrics(max_memory_bytes: u64, metrics: Arc<CriticalComponentMetrics>) -> Self {
        // Set the pin limit gauge so it's visible in Prometheus
        metrics.set_pin_limit(DEFAULT_MAX_PINNED_ENTRIES);
        Self {
            cache: RwLock::new(HashMap::new()),
            active_counts: RwLock::new(HashMap::new()),
            telemetry: RwLock::new(None),
            // CriticalComponentMetrics implements SingleFlightMetrics for Prometheus reporting
            singleflight: SingleFlightSync::with_metrics(MODEL_LOAD_OPERATION, metrics.clone()),
            max_memory_bytes,
            stats: RwLock::new(CacheStats::default()),
            pinned_keys: RwLock::new(HashSet::new()),
            listeners: RwLock::new(HashMap::new()),
            metrics: Some(metrics),
            max_pinned_entries: DEFAULT_MAX_PINNED_ENTRIES,
            base_model_pin: RwLock::new(BaseModelPinState::default()),
        }
    }

    /// Set the maximum number of pinned entries allowed
    ///
    /// This is a safety cap to prevent unbounded cache growth from pinned models.
    /// When the limit is reached, new pin attempts will be rejected and logged.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of entries that can be pinned. Use `usize::MAX`
    ///   to effectively disable the limit (not recommended for production).
    pub fn with_max_pinned_entries(mut self, limit: usize) -> Self {
        self.max_pinned_entries = limit;
        if let Some(ref m) = self.metrics {
            m.set_pin_limit(limit);
        }
        self
    }

    /// Get the configured maximum number of pinned entries
    pub fn max_pinned_entries(&self) -> usize {
        self.max_pinned_entries
    }

    /// Set telemetry metrics after construction
    pub fn set_metrics(&mut self, metrics: Arc<CriticalComponentMetrics>) {
        metrics.set_pin_limit(self.max_pinned_entries);
        self.metrics = Some(metrics);
    }

    /// Set telemetry writer after construction for failure reporting.
    pub fn set_telemetry(&self, telemetry: TelemetryWriter) {
        *self.telemetry.write() = Some(telemetry);
    }

    /// Get the configured maximum memory budget in bytes
    pub fn max_memory_bytes(&self) -> u64 {
        self.max_memory_bytes
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

    /// Configure base model pinning behavior and telemetry identity.
    pub fn configure_base_model_pinning(
        &self,
        enabled: bool,
        budget_bytes: Option<u64>,
        model_id: Option<String>,
    ) {
        let mut state = self.base_model_pin.write();
        state.enabled = enabled;
        state.budget_bytes = budget_bytes;
        state.model_id = model_id;
        state.base_model_key = None;
        state.load_count = 0;
        state.evict_count = 0;
    }

    /// Snapshot the current base model pinning state.
    pub fn base_model_pin_state(&self) -> BaseModelPinState {
        self.base_model_pin.read().clone()
    }

    /// Check if base model pinning is enabled.
    pub fn base_model_pin_enabled(&self) -> bool {
        self.base_model_pin.read().enabled
    }

    /// Track which cache key corresponds to the base model.
    pub fn set_base_model_key(&self, key: &ModelKey) {
        let mut state = self.base_model_pin.write();
        if state.base_model_key.as_ref() == Some(key) {
            return;
        }
        if let Some(existing) = state.base_model_key.as_ref() {
            tracing::warn!(
                existing = %existing.short_hex(),
                next = %key.short_hex(),
                "Base model key changed; keeping first key for residency tracking"
            );
            return;
        }
        state.base_model_key = Some(key.clone());
        state.load_count = 0;
        state.evict_count = 0;
    }

    /// Remove a lifecycle listener for a specific key.
    pub fn remove_listener(&self, key: &ModelKey) {
        self.listeners.write().remove(key);
    }

    fn notify_load(&self, key: &ModelKey, memory_bytes: u64) {
        if let Some(listener) = self.listeners.read().get(key) {
            listener.on_load(key, memory_bytes);
        }
        self.record_base_model_load(key);
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
        self.record_base_model_evict(key);
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
    /// This function uses a read-lock fast path for cache hits and a
    /// SingleFlightSync guard for cache misses. Only one concurrent miss
    /// runs the loader; other callers wait on the in-flight entry and
    /// observe the same success or error without re-running the loader.
    pub fn get_or_load<F>(&self, key: &ModelKey, loader: F) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        // Fast path: read lock for cache hit
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(key) {
                return Ok(self.cache_hit(key, entry, "Model cache hit"));
            }
        }

        // Use SingleFlightSync for load deduplication.
        // The closure handles the full load + cache insert operation.
        let key_clone = key.clone();
        let handle = self
            .singleflight
            .get_or_load(key.clone(), || {
                self.load_and_cache_model(&key_clone, loader)
            })
            .map_err(AosError::Worker)?;

        Ok(handle)
    }

    /// Internal helper: performs the actual model load and cache insertion.
    /// Called by SingleFlightSync - only the leader executes this.
    fn load_and_cache_model<F>(
        &self,
        key: &ModelKey,
        loader: F,
    ) -> std::result::Result<ModelHandle, String>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        // Re-check cache before loading. This handles the race where:
        // 1. Multiple threads pass the fast-path cache check
        // 2. One becomes SingleFlight leader and completes very quickly
        // 3. Another becomes a NEW leader (because entry was removed)
        // Without this check, the second leader would run the loader again.
        {
            let cache = self.cache.read();
            if let Some(entry) = cache.get(key) {
                tracing::debug!(
                    key = %key.short_hex(),
                    "Model found in cache during SingleFlight leader re-check"
                );
                // Record as hit since we're returning cached value
                let mut stats = self.stats.write();
                stats.hits += 1;
                if let Some(ref m) = self.metrics {
                    m.record_model_cache_hit();
                }
                return Ok(entry.handle.clone());
            }
        }

        tracing::info!(key = %key.short_hex(), "Model cache miss, loading from disk");

        // Run the loader
        let loader_result = loader();
        if let Err(ref e) = loader_result {
            self.notify_error(key, e);
            self.emit_model_load_failure(key, e);
        }

        let (handle, memory_bytes) = match loader_result {
            Ok(v) => v,
            Err(e) => {
                return Err(e.to_string());
            }
        };

        // Acquire write lock and insert
        let mut cache = self.cache.write();

        // Double-check: another thread may have loaded while we were loading
        // (This can happen if there's a race with a non-SingleFlight path)
        if let Some(existing) = cache.get(key) {
            tracing::debug!(
                key = %key.short_hex(),
                "Model loaded by another thread, reusing existing entry"
            );
            return Ok(existing.handle.clone());
        }

        // Evict if necessary to make room
        if let Err(e) = self.evict_for_size_locked(&mut cache, memory_bytes, Some(key)) {
            return Err(e.to_string());
        }

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
    /// # Pin Limit
    ///
    /// If the configured `max_pinned_entries` limit would be exceeded, the model
    /// is still loaded but NOT pinned. A warning is logged and the rejection is
    /// counted in the `model_cache_pin_limit_rejections_total` metric. The model
    /// will be subject to normal LRU eviction.
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
    /// - Watch the `model_cache_pin_limit_rejections_total` counter rate
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handle = cache.get_or_load_base_model(&base_key, || {
    ///     Ok((ModelHandle::Metal(Arc::new(model_bytes)), size))
    /// })?;
    /// // base_key is now pinned and won't be evicted (if under limit)
    ///
    /// // When done with the base model:
    /// cache.unpin(&base_key);
    /// ```
    pub fn get_or_load_base_model<F>(&self, key: &ModelKey, loader: F) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        self.set_base_model_key(key);
        let handle = self.get_or_load(key, loader)?;

        // Auto-pin the base model (respecting the limit)
        {
            let mut pinned = self.pinned_keys.write();

            // Check if already pinned (idempotent)
            if !pinned.contains(key) {
                // Check pin limit before adding new entry
                if pinned.len() >= self.max_pinned_entries {
                    tracing::warn!(
                        key = %key.short_hex(),
                        current_pinned = pinned.len(),
                        max_pinned = self.max_pinned_entries,
                        "Base model loaded but NOT pinned: pin limit reached. Model is subject to LRU eviction."
                    );
                    if let Some(ref m) = self.metrics {
                        m.record_pin_limit_rejection();
                    }
                    // Still return the handle - model is loaded, just not pinned
                    return Ok(handle);
                }

                if pinned.insert(key.clone()) {
                    if let Some(ref m) = self.metrics {
                        m.set_pinned_entries_count(pinned.len());
                        // Update pinned memory gauge
                        drop(pinned); // Release lock before calling pinned_memory_bytes
                        let pinned_mem = self.pinned_memory_bytes();
                        m.set_pinned_memory_bytes(pinned_mem);
                    }
                    tracing::info!(
                        key = %key.short_hex(),
                        "Base model pinned to prevent eviction"
                    );
                }
            }
        }

        // Base models are considered active while resident.
        let _ = self.mark_active(key);

        Ok(handle)
    }

    /// Pin a cache entry to prevent eviction
    ///
    /// Returns `true` if the key was found in cache and pinned,
    /// `false` if the key is not in the cache or if the pin limit is exceeded.
    ///
    /// # Pin Limit
    ///
    /// If the configured `max_pinned_entries` limit would be exceeded by this pin,
    /// the operation is rejected, a warning is logged, and the rejection is counted
    /// in the `model_cache_pin_limit_rejections_total` metric.
    pub fn pin(&self, key: &ModelKey) -> bool {
        // Check if key exists in cache first
        let exists = self.cache.read().contains_key(key);
        if !exists {
            return false;
        }

        let mut pinned = self.pinned_keys.write();

        // If already pinned, return true (idempotent)
        if pinned.contains(key) {
            return true;
        }

        // Check pin limit before adding new entry
        if pinned.len() >= self.max_pinned_entries {
            tracing::warn!(
                key = %key.short_hex(),
                current_pinned = pinned.len(),
                max_pinned = self.max_pinned_entries,
                "Pin rejected: maximum pinned entries limit reached"
            );
            if let Some(ref m) = self.metrics {
                m.record_pin_limit_rejection();
            }
            return false;
        }

        let was_new = pinned.insert(key.clone());
        if was_new {
            if let Some(ref m) = self.metrics {
                m.set_pinned_entries_count(pinned.len());
                // Also update pinned memory gauge
                drop(pinned); // Release lock before calling pinned_memory_bytes
                let pinned_mem = self.pinned_memory_bytes();
                m.set_pinned_memory_bytes(pinned_mem);
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
                // Update pinned memory gauge
                drop(pinned); // Release lock before calling pinned_memory_bytes
                let pinned_mem = self.pinned_memory_bytes();
                m.set_pinned_memory_bytes(pinned_mem);
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

    /// Get all currently pinned keys (for diagnostics and leak detection)
    ///
    /// This method is useful for monitoring and debugging to identify which
    /// models are pinned and potentially leaking if not properly unpinned.
    pub fn pinned_keys(&self) -> Vec<ModelKey> {
        self.pinned_keys.read().iter().cloned().collect()
    }

    /// Get memory usage of pinned entries in bytes
    ///
    /// This metric helps identify when pinned entries are consuming too much memory.
    pub fn pinned_memory_bytes(&self) -> u64 {
        let cache = self.cache.read();
        let pinned = self.pinned_keys.read();
        cache
            .iter()
            .filter(|(k, _)| pinned.contains(*k))
            .map(|(_, e)| e.memory_bytes)
            .sum()
    }

    /// Report stale pinned entries that may be leaking
    ///
    /// Returns entries that have been pinned for longer than the given duration.
    /// This helps operators identify potential memory leaks where models were
    /// pinned but never unpinned.
    pub fn stale_pinned_entries(
        &self,
        threshold: std::time::Duration,
    ) -> Vec<(ModelKey, std::time::Duration)> {
        let now = Instant::now();
        let cache = self.cache.read();
        let pinned = self.pinned_keys.read();

        cache
            .iter()
            .filter(|(k, _)| pinned.contains(*k))
            .filter_map(|(k, e)| {
                let age = now.duration_since(e.loaded_at);
                if age > threshold {
                    Some((k.clone(), age))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Audit pinned entries and log a report
    ///
    /// This method is intended to be called periodically (e.g., every 5 minutes)
    /// to help operators detect potential pin leaks. It logs:
    /// - Current pinned count vs limit
    /// - Total memory used by pinned entries
    /// - Any stale pinned entries (older than the given threshold)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Run audit every 5 minutes
    /// cache.audit_pinned_entries(Duration::from_secs(3600)); // 1 hour threshold
    /// ```
    pub fn audit_pinned_entries(&self, stale_threshold: std::time::Duration) {
        let pinned_count = self.pinned_count();
        let pinned_memory = self.pinned_memory_bytes();
        let max_pinned = self.max_pinned_entries;
        let stale_entries = self.stale_pinned_entries(stale_threshold);

        // Always log current state at debug level
        tracing::debug!(
            pinned_count = pinned_count,
            max_pinned = max_pinned,
            pinned_memory_mb = pinned_memory / (1024 * 1024),
            "Pinned entries audit"
        );

        // Warn if approaching or at the limit
        if pinned_count >= max_pinned {
            tracing::warn!(
                pinned_count = pinned_count,
                max_pinned = max_pinned,
                pinned_memory_mb = pinned_memory / (1024 * 1024),
                "Pin limit reached! New base models will NOT be pinned and may be evicted."
            );
        } else if pinned_count as f64 / max_pinned as f64 >= 0.8 {
            tracing::warn!(
                pinned_count = pinned_count,
                max_pinned = max_pinned,
                pinned_memory_mb = pinned_memory / (1024 * 1024),
                "Approaching pin limit (80%+ utilization)"
            );
        }

        // Log stale entries that may indicate leaks
        if !stale_entries.is_empty() {
            tracing::warn!(
                stale_count = stale_entries.len(),
                threshold_secs = stale_threshold.as_secs(),
                "Detected stale pinned entries - potential memory leak"
            );
            for (key, age) in &stale_entries {
                tracing::warn!(
                    key = %key.short_hex(),
                    age_secs = age.as_secs(),
                    "Stale pinned entry"
                );
            }
        }
    }

    /// Unpin all entries (emergency memory recovery)
    ///
    /// # Warning
    ///
    /// This should only be used in emergency situations when memory pressure
    /// is critical. Unpinning base models may cause inference failures if
    /// they are evicted while adapters depend on them.
    pub fn unpin_all(&self) -> usize {
        let mut pinned = self.pinned_keys.write();
        let count = pinned.len();
        if count > 0 {
            tracing::warn!(
                count = count,
                "Emergency unpin_all called - all pinned models may now be evicted"
            );
            pinned.clear();
            if let Some(ref m) = self.metrics {
                m.set_pinned_entries_count(0);
                m.set_pinned_memory_bytes(0);
            }
        }
        count
    }

    /// Unpin and immediately evict all unpinned entries
    ///
    /// This is a more aggressive cleanup that first unpins all entries,
    /// then evicts any that can be evicted. Useful for graceful shutdown
    /// or memory pressure situations.
    pub fn cleanup_all(&self) {
        // First unpin everything
        let unpinned = self.unpin_all();
        if unpinned > 0 {
            tracing::info!(unpinned = unpinned, "Unpinned all entries for cleanup");
        }

        // Mark all entries as inactive
        {
            let mut active = self.active_counts.write();
            active.clear();
        }

        // Evict all entries by setting a zero target
        let mut cache = self.cache.write();
        let keys: Vec<_> = cache.keys().cloned().collect();
        for key in keys {
            if let Some(entry) = cache.remove(&key) {
                self.notify_evict(&key);
                self.listeners.write().remove(&key);
                let mut stats = self.stats.write();
                stats.evictions += 1;
                stats.total_memory_bytes =
                    stats.total_memory_bytes.saturating_sub(entry.memory_bytes);
                tracing::info!(key = %key.short_hex(), "Evicted model during cleanup");
            }
        }
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
        model_key: Option<&ModelKey>,
    ) -> Result<()> {
        let current: u64 = cache.values().map(|e| e.memory_bytes).sum();
        if current + needed_bytes <= self.max_memory_bytes {
            return Ok(());
        }

        // Log memory threshold crossing
        let usage_pct = (current as f64 / self.max_memory_bytes as f64) * 100.0;
        let after_load_pct =
            ((current + needed_bytes) as f64 / self.max_memory_bytes as f64) * 100.0;
        tracing::info!(
            target: "inference.cache",
            current_mb = current / BYTES_PER_MB,
            needed_mb = needed_bytes / BYTES_PER_MB,
            max_mb = self.max_memory_bytes / BYTES_PER_MB,
            usage_pct = format!("{:.1}", usage_pct),
            after_load_pct = format!("{:.1}", after_load_pct),
            model_key = model_key.map(|k| k.short_hex()).unwrap_or_default(),
            "Memory threshold crossing: eviction required"
        );

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

        // PRD-RECT-003: Sort by oldest first, then least accessed, then by ModelKey for determinism.
        // The final ModelKey comparison ensures deterministic eviction order when
        // loaded_at and access_count are equal (common in tests and rapid sequential loads).
        entries.sort_by(|a, b| {
            a.1.cmp(&b.1) // loaded_at: oldest first
                .then_with(|| a.2.cmp(&b.2)) // access_count: least accessed first
                .then_with(|| a.0.cmp(&b.0)) // ModelKey: deterministic tie-breaker
        });

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

            // Re-validate: check if status changed since we collected entries
            {
                let is_active = self.active_counts.read().get(&key).copied().unwrap_or(0) > 0;
                let is_pinned = self.pinned_keys.read().contains(&key);
                if is_active || is_pinned {
                    continue; // Skip - became active or pinned after our snapshot
                }
            }

            // Safe to evict
            cache.remove(&key);
            self.active_counts.write().remove(&key);
            self.notify_evict(&key);
            self.listeners.write().remove(&key);
            freed += mem;

            let mut stats = self.stats.write();
            stats.evictions += 1;
            stats.total_memory_bytes = stats.total_memory_bytes.saturating_sub(mem);

            tracing::warn!(
                target: "inference.cache",
                key = %key.short_hex(),
                freed_mb = mem / BYTES_PER_MB,
                total_freed_mb = freed / BYTES_PER_MB,
                target_mb = target / BYTES_PER_MB,
                eviction_count = stats.evictions,
                remaining_memory_mb = stats.total_memory_bytes / BYTES_PER_MB,
                "Cache eviction: model removed due to memory pressure"
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
                target: "inference.cache",
                pinned_count = pinned_in_cache,
                freed_mb = freed / BYTES_PER_MB,
                target_mb = target / BYTES_PER_MB,
                "Cache eviction blocked: pinned entries preventing memory recovery"
            );
        }

        if target > 0 && active_in_cache > 0 {
            let mut stats = self.stats.write();
            stats.eviction_skip_active_count += active_in_cache as u64;

            tracing::warn!(
                target: "inference.cache",
                active_count = active_in_cache,
                freed_mb = freed / BYTES_PER_MB,
                target_mb = target / BYTES_PER_MB,
                "Cache eviction blocked: active entries preventing memory recovery"
            );
        }

        if freed < target {
            if let Some(key) = model_key {
                self.emit_eviction_budget_error(
                    key,
                    needed_bytes,
                    freed,
                    pinned_in_cache,
                    active_in_cache,
                );
            }
            return Err(AosError::CacheBudgetExceeded {
                needed_mb: needed_bytes / BYTES_PER_MB,
                freed_mb: freed / BYTES_PER_MB,
                pinned_count: pinned_in_cache,
                active_count: active_in_cache,
                max_mb: self.max_memory_bytes / BYTES_PER_MB,
                model_key: model_key.map(|k| k.manifest_hash.to_hex()[..12].to_string()),
            });
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
        *self.base_model_pin.write() = BaseModelPinState::default();
        // Note: SingleFlightSync manages its own state and cleans up automatically
        if let Some(ref m) = self.metrics {
            m.set_pinned_entries_count(0);
        }
        let mut stats = self.stats.write();
        *stats = CacheStats::default();
    }

    fn telemetry_identity(&self) -> IdentityEnvelope {
        IdentityEnvelope::new(
            "system".to_string(),
            "worker".to_string(),
            "model_cache".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        )
    }

    fn telemetry_writer(&self) -> Option<TelemetryWriter> {
        self.telemetry.read().clone()
    }

    fn backend_label(key: &ModelKey) -> &'static str {
        match key.backend_type {
            BackendType::Metal => "metal",
            BackendType::MLX => "mlx",
            BackendType::CoreML => "coreml",
            BackendType::Mock => "mock",
        }
    }

    fn emit_model_load_failure(&self, key: &ModelKey, error: &AosError) {
        let Some(writer) = self.telemetry_writer() else {
            return;
        };

        let payload = make_model_load_failed_payload(
            key.short_hex(),
            Self::backend_label(key),
            error.to_string(),
        );
        match build_model_load_failed_event(self.telemetry_identity(), payload) {
            Ok(event) => {
                if let Err(e) = writer.log_event(event) {
                    tracing::warn!(error = %e, "Failed to emit model load failure telemetry");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to build model load failure telemetry");
            }
        }
    }

    fn emit_eviction_budget_error(
        &self,
        key: &ModelKey,
        needed_bytes: u64,
        freed_bytes: u64,
        pinned_entries: usize,
        active_entries: usize,
    ) {
        let Some(writer) = self.telemetry_writer() else {
            return;
        };

        let payload = make_model_eviction_budget_error_payload(
            key.short_hex(),
            Self::backend_label(key),
            needed_bytes,
            freed_bytes,
            pinned_entries,
            active_entries,
            self.max_memory_bytes,
        );

        match build_model_eviction_budget_error_event(self.telemetry_identity(), payload) {
            Ok(event) => {
                if let Err(e) = writer.log_event(event) {
                    tracing::warn!(error = %e, "Failed to emit eviction budget telemetry");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to build eviction budget telemetry");
            }
        }
    }

    fn cache_hit(
        &self,
        key: &ModelKey,
        entry: &CachedModelEntry,
        message: &'static str,
    ) -> ModelHandle {
        let mut stats = self.stats.write();
        stats.hits += 1;
        if let Some(ref m) = self.metrics {
            m.record_model_cache_hit();
        }
        self.notify_reuse(key);
        tracing::debug!(
            key = %key.short_hex(),
            access_count = entry.access_count,
            "{message}"
        );
        entry.handle.clone()
    }

    fn record_base_model_load(&self, key: &ModelKey) {
        let mut state = self.base_model_pin.write();
        if state.base_model_key.as_ref() == Some(key) {
            state.load_count += 1;
        }
    }

    fn record_base_model_evict(&self, key: &ModelKey) {
        let mut state = self.base_model_pin.write();
        if state.base_model_key.as_ref() == Some(key) {
            state.evict_count += 1;
        }
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
        let memory_mb = memory_bytes.div_ceil(BYTES_PER_MB) as u32;
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    fn make_identity(
        kernel: &str,
        quant: &str,
        fusion: &str,
    ) -> crate::model_key::ModelCacheIdentity {
        crate::model_key::ModelCacheIdentity::new(kernel, quant, fusion)
    }

    fn make_key(backend: BackendType, data: &[u8]) -> ModelKey {
        ModelKey::new(
            backend,
            B3Hash::hash(data),
            crate::model_key::ModelCacheIdentity::for_backend(backend),
        )
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
    fn test_identity_fields_cause_cache_miss() {
        let cache = ModelHandleCache::new(1024);
        let base_hash = B3Hash::hash(b"model_identity_v2");

        let key_kernel = ModelKey::new(
            BackendType::Metal,
            base_hash,
            make_identity("k1", "fp16", "per_request"),
        );
        let key_kernel_changed = ModelKey::new(
            BackendType::Metal,
            base_hash,
            make_identity("k2", "fp16", "per_request"),
        );
        let key_quant_changed = ModelKey::new(
            BackendType::Metal,
            base_hash,
            make_identity("k2", "int4", "per_request"),
        );
        let key_fusion_changed = ModelKey::new(
            BackendType::Metal,
            base_hash,
            make_identity("k2", "int4", "per_token"),
        );

        let mut load_count = 0;
        for key in [
            key_kernel,
            key_kernel_changed,
            key_quant_changed,
            key_fusion_changed,
        ] {
            cache
                .get_or_load(&key, || {
                    load_count += 1;
                    Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
                })
                .unwrap();
        }

        assert_eq!(
            load_count, 4,
            "each identity variant should trigger a unique cache load"
        );
        let stats = cache.stats();
        assert_eq!(stats.misses, 4);
        assert_eq!(cache.len(), 4);
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
    fn test_pinned_eviction_returns_budget_error() {
        let cache = ModelHandleCache::new(80);
        let base_key = make_key(BackendType::Metal, b"pinned_base");
        let new_key = make_key(BackendType::Metal, b"new_model");

        cache
            .get_or_load_base_model(&base_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 60])), 60))
            })
            .unwrap();
        assert!(cache.is_pinned(&base_key));

        let err = cache
            .get_or_load(&new_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 40])), 40))
            })
            .unwrap_err();
        assert!(
            err.to_string().contains("Model cache budget exceeded"),
            "should surface budget error"
        );
        assert_eq!(cache.len(), 1, "pinned entry should remain");
    }

    #[test]
    fn test_base_model_load_recovers_after_failure() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"base_recover");

        let err = cache
            .get_or_load_base_model(&key, || Err(AosError::Internal("boom".to_string())))
            .unwrap_err();
        assert!(err.to_string().contains("boom"));
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.pinned_count(), 0);

        let handle = cache
            .get_or_load_base_model(&key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
            })
            .expect("recovery should succeed");
        assert!(matches!(handle, ModelHandle::Metal(_)));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.pinned_count(), 1);
    }

    #[test]
    fn test_adapter_load_recovers_after_failure() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"adapter_recover");

        let err = cache
            .get_or_load(&key, || Err(AosError::Internal("adapter-fail".to_string())))
            .unwrap_err();
        assert!(err.to_string().contains("adapter-fail"));
        assert_eq!(cache.len(), 0);

        let handle = cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![7, 8])), 2)))
            .expect("adapter reload should succeed");
        assert!(matches!(handle, ModelHandle::Metal(_)));
        assert_eq!(cache.len(), 1);

        let stats = cache.stats();
        assert_eq!(
            stats.misses, 1,
            "first successful load should count as miss"
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
    fn test_concurrent_loads_single_flight() {
        let cache = Arc::new(ModelHandleCache::new(1024 * 1024));
        let key = make_key(BackendType::Metal, b"concurrent");
        let load_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));

        let mut threads = Vec::new();
        for _ in 0..8 {
            let cache_cloned = Arc::clone(&cache);
            let key_cloned = key.clone();
            let load_count_cloned = Arc::clone(&load_count);
            let barrier_cloned = Arc::clone(&barrier);
            threads.push(thread::spawn(move || {
                barrier_cloned.wait();
                cache_cloned
                    .get_or_load(&key_cloned, || {
                        load_count_cloned.fetch_add(1, Ordering::SeqCst);
                        Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
                    })
                    .unwrap();
            }));
        }

        for handle in threads {
            handle.join().unwrap();
        }

        assert_eq!(
            load_count.load(Ordering::SeqCst),
            1,
            "loader should execute only once"
        );
        assert_eq!(cache.len(), 1);
        // With SingleFlightSync, waiters receive the handle directly rather than
        // reading from cache, so hits/misses tracking differs from the old impl.
        // The key invariant is: loader called once, cache has 1 entry.
        assert_eq!(
            cache.stats().misses,
            1,
            "should record exactly one cache miss"
        );
    }

    #[test]
    fn test_concurrent_failures_propagate() {
        let cache = Arc::new(ModelHandleCache::new(1024 * 1024));
        let key = make_key(BackendType::Metal, b"fail");
        let load_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(4));

        let mut threads = Vec::new();
        for _ in 0..4 {
            let cache_cloned = Arc::clone(&cache);
            let key_cloned = key.clone();
            let load_count_cloned = Arc::clone(&load_count);
            let barrier_cloned = Arc::clone(&barrier);
            threads.push(thread::spawn(move || {
                barrier_cloned.wait();
                cache_cloned
                    .get_or_load(&key_cloned, || {
                        load_count_cloned.fetch_add(1, Ordering::SeqCst);
                        Err(AosError::Worker("expected failure".to_string()))
                    })
                    .expect_err("all callers should receive failure");
            }));
        }

        for handle in threads {
            handle.join().unwrap();
        }

        // With instant-fail loaders, the SingleFlight entry may be removed
        // before all threads register as waiters, allowing multiple loads.
        // The key invariant is that failures don't poison the cache.
        assert!(
            load_count.load(Ordering::SeqCst) >= 1,
            "at least one loader should run on failure"
        );
        assert_eq!(cache.len(), 0, "failed load must not poison cache");
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn test_retry_after_failure_succeeds_once() {
        let cache = Arc::new(ModelHandleCache::new(1024 * 1024));
        let key = make_key(BackendType::Metal, b"retry");

        // First attempt: deliberate failure
        // Note: With instant-fail loaders, the SingleFlight entry may be removed
        // before all threads register as waiters, allowing multiple loads.
        // The key invariants are: all threads receive error, no cache entry.
        let fail_barrier = Arc::new(Barrier::new(3));
        let fail_count = Arc::new(AtomicUsize::new(0));
        let mut fail_threads = Vec::new();
        for _ in 0..3 {
            let cache_cloned = Arc::clone(&cache);
            let key_cloned = key.clone();
            let count_cloned = Arc::clone(&fail_count);
            let barrier_cloned = Arc::clone(&fail_barrier);
            fail_threads.push(thread::spawn(move || {
                barrier_cloned.wait();
                cache_cloned
                    .get_or_load(&key_cloned, || {
                        count_cloned.fetch_add(1, Ordering::SeqCst);
                        Err(AosError::Worker("first attempt failed".to_string()))
                    })
                    .expect_err("failure should propagate to waiters");
            }));
        }
        for handle in fail_threads {
            handle.join().unwrap();
        }
        // With instant-fail, fail_count may be > 1 due to timing (entry removed before waiters register)
        assert!(
            fail_count.load(Ordering::SeqCst) >= 1,
            "at least one loader should run"
        );
        assert!(cache.is_empty(), "failure must not insert cache entry");

        // Second attempt: success, still single-flight
        let success_barrier = Arc::new(Barrier::new(5));
        let success_count = Arc::new(AtomicUsize::new(0));
        let mut success_threads = Vec::new();
        for _ in 0..5 {
            let cache_cloned = Arc::clone(&cache);
            let key_cloned = key.clone();
            let count_cloned = Arc::clone(&success_count);
            let barrier_cloned = Arc::clone(&success_barrier);
            success_threads.push(thread::spawn(move || {
                barrier_cloned.wait();
                cache_cloned
                    .get_or_load(&key_cloned, || {
                        count_cloned.fetch_add(1, Ordering::SeqCst);
                        Ok((ModelHandle::Metal(Arc::new(vec![9, 9, 9])), 3))
                    })
                    .unwrap();
            }));
        }
        for handle in success_threads {
            handle.join().unwrap();
        }

        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "retry load should run only once"
        );
        assert_eq!(cache.len(), 1, "successful retry must insert cache entry");
        // With SingleFlightSync, waiters receive the handle directly rather than
        // reading from cache, so we only check that misses == 1 (loader ran once).
        assert_eq!(
            cache.stats().misses,
            1,
            "should record exactly one cache miss"
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

    #[test]
    fn test_loader_failure_then_success() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"flaky");
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_fail = attempts.clone();

        let err = cache
            .get_or_load(&key, || {
                attempts_fail.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Worker("first failure".to_string()))
            })
            .unwrap_err();
        assert!(format!("{err}").contains("first failure"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);

        let success = cache
            .get_or_load(&key, || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
            })
            .expect("second attempt should succeed");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(matches!(success, ModelHandle::Metal(_)));
    }

    #[test]
    fn test_single_flight_dedupes_parallel_loads() {
        let cache = Arc::new(ModelHandleCache::new(1024));
        let key = make_key(BackendType::Metal, b"parallel");
        let barrier = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));

        let cache_a = cache.clone();
        let key_a = key.clone();
        let barrier_a = barrier.clone();
        let calls_a = calls.clone();
        let t1 = thread::spawn(move || {
            barrier_a.wait();
            cache_a.get_or_load(&key_a, || {
                calls_a.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
                Ok((ModelHandle::Metal(Arc::new(vec![1])), 1))
            })
        });

        let cache_b = cache.clone();
        let key_b = key.clone();
        let barrier_b = barrier.clone();
        let calls_b = calls.clone();
        let t2 = thread::spawn(move || {
            barrier_b.wait();
            cache_b.get_or_load(&key_b, || {
                calls_b.fetch_add(1, Ordering::SeqCst);
                Ok((ModelHandle::Metal(Arc::new(vec![2])), 1))
            })
        });

        t1.join().expect("thread 1 join").expect("load 1");
        t2.join().expect("thread 2 join").expect("load 2");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "single-flight should allow only one loader"
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_eviction_after_active_guard_released_under_pressure() {
        let cache = ModelHandleCache::new(100);
        let active_key = make_key(BackendType::Metal, b"active-guard");
        let new_key = make_key(BackendType::Metal, b"new-model");

        cache
            .get_or_load(&active_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 80])), 80))
            })
            .unwrap();
        let guard = cache.begin_use(&active_key).expect("guard should start");

        let blocked = cache.get_or_load(&new_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
        });
        assert!(
            blocked.is_err(),
            "active guard should block eviction while held"
        );

        drop(guard); // releases active mark
        let after_release = cache.get_or_load(&new_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
        });
        assert!(
            after_release.is_ok(),
            "eviction should succeed after release"
        );
        assert_eq!(cache.len(), 1, "old entry should evict under pressure");
        assert!(
            cache.stats().evictions >= 1,
            "eviction count should reflect pressure"
        );
    }

    #[test]
    fn test_pinned_keys_returns_all_pinned() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        // Load and pin some models
        cache
            .get_or_load_base_model(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load_base_model(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();
        cache
            .get_or_load(&key3, || Ok((ModelHandle::Metal(Arc::new(vec![3])), 1)))
            .unwrap();

        let pinned = cache.pinned_keys();
        assert_eq!(pinned.len(), 2, "Should have 2 pinned keys");
        assert!(pinned.contains(&key1));
        assert!(pinned.contains(&key2));
        assert!(
            !pinned.contains(&key3),
            "key3 was not pinned via base_model"
        );
    }

    #[test]
    fn test_pinned_memory_bytes() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        cache
            .get_or_load_base_model(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 100])), 100))
            })
            .unwrap();
        cache
            .get_or_load_base_model(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 200])), 200))
            })
            .unwrap();

        assert_eq!(cache.pinned_memory_bytes(), 300);

        // Unpin one
        cache.unpin(&key1);
        assert_eq!(cache.pinned_memory_bytes(), 200);
    }

    #[test]
    fn test_unpin_all() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        cache
            .get_or_load_base_model(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load_base_model(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();

        assert_eq!(cache.pinned_count(), 2);

        let unpinned = cache.unpin_all();
        assert_eq!(unpinned, 2);
        assert_eq!(cache.pinned_count(), 0);
        assert!(!cache.is_pinned(&key1));
        assert!(!cache.is_pinned(&key2));
    }

    #[test]
    fn test_cleanup_all_evicts_everything() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");

        cache
            .get_or_load_base_model(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 100])), 100))
            })
            .unwrap();
        cache
            .get_or_load(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 200])), 200))
            })
            .unwrap();

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.pinned_count(), 1);

        cache.cleanup_all();

        assert_eq!(cache.len(), 0, "All entries should be evicted");
        assert_eq!(cache.pinned_count(), 0, "All pins should be cleared");
        assert_eq!(cache.memory_usage(), 0, "Memory should be zero");
    }

    #[test]
    fn test_stale_pinned_entries_detection() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"model");

        cache
            .get_or_load_base_model(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // With zero threshold, everything is stale
        let stale = cache.stale_pinned_entries(Duration::from_secs(0));
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].0, key);

        // With very long threshold, nothing is stale
        let stale = cache.stale_pinned_entries(Duration::from_secs(3600));
        assert!(stale.is_empty());
    }

    // ========================================
    // Pin limit behavior tests
    // ========================================

    #[test]
    fn test_pin_limit_default() {
        let cache = ModelHandleCache::new(1024);
        assert_eq!(
            cache.max_pinned_entries(),
            super::DEFAULT_MAX_PINNED_ENTRIES
        );
    }

    #[test]
    fn test_pin_limit_configurable() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(4);
        assert_eq!(cache.max_pinned_entries(), 4);
    }

    #[test]
    fn test_pin_limit_enforced_on_pin() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        // Load all models
        cache
            .get_or_load(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();
        cache
            .get_or_load(&key3, || Ok((ModelHandle::Metal(Arc::new(vec![3])), 1)))
            .unwrap();

        // Pin first two - should succeed
        assert!(cache.pin(&key1), "First pin should succeed");
        assert!(cache.pin(&key2), "Second pin should succeed");
        assert_eq!(cache.pinned_count(), 2);

        // Third pin should fail due to limit
        assert!(!cache.pin(&key3), "Third pin should fail due to limit");
        assert_eq!(cache.pinned_count(), 2);
        assert!(!cache.is_pinned(&key3));
    }

    #[test]
    fn test_pin_limit_enforced_on_base_model_load() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        // Load first two as base models - should be pinned
        cache
            .get_or_load_base_model(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load_base_model(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();

        assert_eq!(cache.pinned_count(), 2);
        assert!(cache.is_pinned(&key1));
        assert!(cache.is_pinned(&key2));

        // Third base model load should succeed (model loaded) but NOT be pinned
        let result =
            cache.get_or_load_base_model(&key3, || Ok((ModelHandle::Metal(Arc::new(vec![3])), 1)));
        assert!(
            result.is_ok(),
            "Model should load even when pin limit exceeded"
        );
        assert_eq!(cache.len(), 3, "Model should be in cache");
        assert_eq!(cache.pinned_count(), 2, "Pinned count should not increase");
        assert!(!cache.is_pinned(&key3), "Third model should NOT be pinned");
    }

    #[test]
    fn test_pin_is_idempotent() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        let key = make_key(BackendType::Metal, b"model");

        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // First pin succeeds
        assert!(cache.pin(&key));
        assert_eq!(cache.pinned_count(), 1);

        // Second pin of same key should succeed (idempotent)
        assert!(cache.pin(&key));
        assert_eq!(
            cache.pinned_count(),
            1,
            "Pinning same key twice shouldn't double-count"
        );
    }

    #[test]
    fn test_pin_limit_freed_after_unpin() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        cache
            .get_or_load(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
            .unwrap();
        cache
            .get_or_load(&key3, || Ok((ModelHandle::Metal(Arc::new(vec![3])), 1)))
            .unwrap();

        // Pin first two
        assert!(cache.pin(&key1));
        assert!(cache.pin(&key2));

        // Third fails
        assert!(!cache.pin(&key3));

        // Unpin one
        assert!(cache.unpin(&key1));
        assert_eq!(cache.pinned_count(), 1);

        // Now third should succeed
        assert!(cache.pin(&key3));
        assert_eq!(cache.pinned_count(), 2);
        assert!(!cache.is_pinned(&key1));
        assert!(cache.is_pinned(&key2));
        assert!(cache.is_pinned(&key3));
    }

    #[test]
    fn test_pin_limit_with_metrics() {
        use adapteros_telemetry::metrics::critical_components::CriticalComponentMetrics;

        let metrics = Arc::new(CriticalComponentMetrics::new().expect("metrics"));
        let cache =
            ModelHandleCache::new_with_metrics(1024, metrics.clone()).with_max_pinned_entries(2);

        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        // Verify limit is set in metrics
        assert_eq!(metrics.get_pin_limit(), 2);

        // Load and pin models
        cache
            .get_or_load_base_model(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 100])), 100))
            })
            .unwrap();
        cache
            .get_or_load_base_model(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 200])), 200))
            })
            .unwrap();

        // Verify pinned count and memory in metrics
        assert_eq!(metrics.get_pinned_entries_count(), 2);
        assert_eq!(metrics.get_pinned_memory_bytes(), 300);

        // Load third - should hit limit
        cache
            .get_or_load_base_model(&key3, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 50])), 50))
            })
            .unwrap();

        // Verify rejection was counted
        assert_eq!(
            metrics.get_pin_limit_rejections(),
            1.0,
            "Pin limit rejection should be counted"
        );

        // Pinned count should not have increased
        assert_eq!(metrics.get_pinned_entries_count(), 2);
        // Pinned memory should be unchanged
        assert_eq!(metrics.get_pinned_memory_bytes(), 300);
    }

    #[test]
    fn test_audit_pinned_entries_runs_without_panic() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        let key = make_key(BackendType::Metal, b"model");

        cache
            .get_or_load_base_model(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // Should not panic
        cache.audit_pinned_entries(Duration::from_secs(0));
        cache.audit_pinned_entries(Duration::from_secs(3600));
    }

    #[test]
    fn test_zero_pin_limit_rejects_all() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(0);
        let key = make_key(BackendType::Metal, b"model");

        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // Pin should always fail with limit of 0
        assert!(!cache.pin(&key));
        assert_eq!(cache.pinned_count(), 0);
    }

    #[test]
    fn test_max_pin_limit_allows_many() {
        // With a very high limit, many pins should work
        let cache = ModelHandleCache::new(1024 * 1024).with_max_pinned_entries(usize::MAX);

        for i in 0..100 {
            let key = make_key(BackendType::Metal, format!("model{}", i).as_bytes());
            cache
                .get_or_load_base_model(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
                .unwrap();
        }

        assert_eq!(cache.pinned_count(), 100);
    }
}
