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
//! ## Pinned Model GC (ANCHOR, AUDIT, RECTIFY)
//!
//! Stale pins can leak memory if `unpin()` fails or is never called. The GC system:
//!
//! - **ANCHOR**: `gc_stale_pins(timeout)` enforces `DEFAULT_STALE_PIN_TIMEOUT` (1 hour)
//! - **AUDIT**: `stale_pin_gc_count` in [`CacheStats`], exposed via `stats()` accessor
//! - **RECTIFY**: Periodic GC unpins stale entries; `audit_pinned_entries()` logs alerts
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

use crate::{
    backend_factory::PinConflictMode, base_model_state::BaseModelState, model_key::ModelKey,
};
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

/// RAII guard for cross-process model load serialization
struct CrossProcessLoadGuard {
    file: Option<std::fs::File>,
    lock_path: std::path::PathBuf,
    key_hex: String,
}

impl CrossProcessLoadGuard {
    fn acquire(key_hex: String) -> Self {
        let var_dir = adapteros_core::resolve_var_dir();
        let lock_dir = var_dir.join("run").join("aos-locks");

        if let Err(e) = std::fs::create_dir_all(&lock_dir) {
            tracing::warn!(error = %e, dir = %lock_dir.display(), "Failed to create lock directory, proceeding without cross-process lock");
            return Self {
                file: None,
                lock_path: std::path::PathBuf::new(),
                key_hex,
            };
        }

        let lock_path = lock_dir.join(format!("model_load_{}.lock", key_hex));

        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&lock_path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = %e, path = %lock_path.display(), "Failed to open lock file, proceeding without cross-process lock");
                return Self {
                    file: None,
                    lock_path,
                    key_hex,
                };
            }
        };

        use fs2::FileExt;
        tracing::info!(key = %key_hex, path = %lock_path.display(), "Acquiring cross-process model load lock...");
        let start_wait = std::time::Instant::now();

        if let Err(e) = file.lock_exclusive() {
            tracing::warn!(error = %e, path = %lock_path.display(), "Failed to acquire exclusive file lock, proceeding anyway");
            return Self {
                file: None,
                lock_path,
                key_hex,
            };
        }

        tracing::info!(key = %key_hex, waited = ?start_wait.elapsed(), "Acquired cross-process model load lock");

        Self {
            file: Some(file),
            lock_path,
            key_hex,
        }
    }
}

impl Drop for CrossProcessLoadGuard {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            use fs2::FileExt;
            if let Err(e) = file.unlock() {
                tracing::warn!(error = %e, path = %self.lock_path.display(), "Failed to unlock cross-process model load lock");
            } else {
                tracing::debug!(key = %self.key_hex, "Released cross-process model load lock");
            }
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
    /// Timestamps for when each key was pinned (for stale pin GC)
    pinned_timestamps: RwLock<HashMap<ModelKey, Instant>>,
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
    /// Count of stale pins cleaned up by GC
    pub stale_pin_gc_count: u64,
}

/// Current cache state snapshot for status reporting
///
/// This provides a read-consistent view of the cache for monitoring,
/// health checks, and graceful shutdown coordination.
#[derive(Debug, Clone)]
pub struct CacheStatus {
    /// Keys of all currently loaded models (as short hex strings)
    pub loaded_model_keys: Vec<String>,
    /// Number of pinned entries that cannot be evicted
    pub pinned_count: usize,
    /// Total memory usage of all cached models in bytes
    pub cache_memory_bytes: u64,
    /// Cache hit ratio (0.0 to 1.0)
    pub hit_ratio: f32,
    /// Number of currently active models (in-flight inference)
    pub active_count: usize,
    /// Number of models eligible for eviction (not pinned, not active)
    pub evictable_count: usize,
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
    /// Conflict behavior when pin limit is reached.
    pub conflict_mode: PinConflictMode,
}

/// Operation label for SingleFlight metrics
const MODEL_LOAD_OPERATION: &str = "model_load";

/// Default maximum number of pinned entries allowed to prevent unbounded cache growth.
/// This is a safety cap - operators can override via `with_max_pinned_entries()`.
pub const DEFAULT_MAX_PINNED_ENTRIES: usize = 16;

/// Default timeout for stale pin garbage collection.
/// Pins older than this duration are eligible for automatic cleanup.
pub const DEFAULT_STALE_PIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3600); // 1 hour

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
            pinned_timestamps: RwLock::new(HashMap::new()),
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
            pinned_timestamps: RwLock::new(HashMap::new()),
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
        conflict_mode: PinConflictMode,
    ) {
        let mut state = self.base_model_pin.write();
        state.enabled = enabled;
        state.budget_bytes = budget_bytes;
        state.model_id = model_id;
        state.conflict_mode = conflict_mode;
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
            tracing::info!(
                existing = %existing.short_hex(),
                next = %key.short_hex(),
                "Base model key changed; switching residency tracking key"
            );
        }
        state.base_model_key = Some(key.clone());
        state.load_count = 0;
        state.evict_count = 0;
    }

    fn select_pin_displacement_candidate(&self, incoming_key: &ModelKey) -> Option<ModelKey> {
        let pinned = self.pinned_keys.read();
        let timestamps = self.pinned_timestamps.read();
        let cache = self.cache.read();
        let active = self.active_counts.read();

        let mut candidates: Vec<(ModelKey, Option<Instant>)> = pinned
            .iter()
            .filter(|key| *key != incoming_key)
            .filter(|key| cache.contains_key(*key))
            .filter(|key| active.get(*key).copied().unwrap_or(0) == 0)
            .map(|key| (key.clone(), timestamps.get(key).copied()))
            .collect();

        candidates.sort_by(|a, b| {
            a.1.is_none()
                .cmp(&b.1.is_none())
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });

        candidates.into_iter().map(|(key, _)| key).next()
    }

    fn pin_displacement_block_reason(&self, incoming_key: &ModelKey) -> &'static str {
        let pinned = self.pinned_keys.read();
        if pinned.is_empty() {
            return "no_pinned_entries";
        }

        let cache = self.cache.read();
        let active = self.active_counts.read();
        let mut has_other = false;
        let mut has_existing_other = false;

        for key in pinned.iter() {
            if key == incoming_key {
                continue;
            }
            has_other = true;
            if !cache.contains_key(key) {
                continue;
            }
            has_existing_other = true;
            if active.get(key).copied().unwrap_or(0) == 0 {
                return "evictable_candidate_available";
            }
        }

        if !has_other {
            "only_incoming_model_pinned"
        } else if !has_existing_other {
            "other_pinned_entries_not_cached"
        } else {
            "all_other_pinned_entries_active"
        }
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

        // Acquire cross-process lock via RAII guard to safely stagger loads
        // The guard will automatically release the lock when it goes out of scope (even on panic)
        let _load_guard = CrossProcessLoadGuard::acquire(key.short_hex());

        // Run the loader
        let loader_result = loader();

        // Explicitly drop guard here (optional, but clarifies scope)
        drop(_load_guard);

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
        let conflict_mode = self.base_model_pin.read().conflict_mode;

        // Auto-pin the base model (respecting the limit and conflict mode)
        {
            let mut pinned = self.pinned_keys.write();

            // Check if already pinned (idempotent)
            if !pinned.contains(key) {
                // Check pin limit before adding new entry
                if pinned.len() >= self.max_pinned_entries {
                    drop(pinned);
                    let candidate = self.select_pin_displacement_candidate(key);
                    let block_reason = if candidate.is_none() {
                        self.pin_displacement_block_reason(key)
                    } else {
                        "displacement_candidate_available"
                    };
                    let candidate_short = candidate.as_ref().map(ModelKey::short_hex);

                    match conflict_mode {
                        PinConflictMode::Shadow => {
                            tracing::warn!(
                                key = %key.short_hex(),
                                current_pinned = self.pinned_count(),
                                max_pinned = self.max_pinned_entries,
                                conflict_mode = %conflict_mode,
                                displacement_candidate = ?candidate_short,
                                block_reason,
                                "Base model loaded but left unpinned because pin limit was reached"
                            );
                            if let Some(ref m) = self.metrics {
                                m.record_pin_limit_rejection();
                            }
                            return Ok(handle);
                        }
                        PinConflictMode::Enforce => {
                            let displaced = candidate.ok_or_else(|| {
                                if let Some(ref m) = self.metrics {
                                    m.record_pin_limit_rejection();
                                }
                                AosError::CacheEntryPinned {
                                    key: key.short_hex(),
                                    reason: format!(
                                        "Pin conflict mode is enforce and no evictable pinned entries are available (reason: {block_reason})"
                                    ),
                                }
                            })?;

                            let displaced_short = displaced.short_hex();
                            if !self.unpin(&displaced) {
                                if let Some(ref m) = self.metrics {
                                    m.record_pin_limit_rejection();
                                }
                                return Err(AosError::CacheEntryPinned {
                                    key: key.short_hex(),
                                    reason: format!(
                                        "Pin conflict mode is enforce and no evictable pinned entries are available (displacement candidate {} could not be unpinned)",
                                        displaced_short
                                    ),
                                });
                            }

                            if let Err(err) = self.unload(&displaced) {
                                if self.cache.read().contains_key(&displaced) {
                                    let _ = self.pin(&displaced);
                                }
                                if let Some(ref m) = self.metrics {
                                    m.record_pin_limit_rejection();
                                }
                                return Err(AosError::CacheEntryPinned {
                                    key: key.short_hex(),
                                    reason: format!(
                                        "Pin conflict mode is enforce and no evictable pinned entries are available after displacement attempt for {}: {}",
                                        displaced_short, err
                                    ),
                                });
                            }

                            if !self.pin(key) {
                                if let Some(ref m) = self.metrics {
                                    m.record_pin_limit_rejection();
                                }
                                return Err(AosError::CacheEntryPinned {
                                    key: key.short_hex(),
                                    reason: "Pin conflict mode is enforce and no evictable pinned entries are available after displacement".to_string(),
                                });
                            }

                            tracing::info!(
                                incoming_key = %key.short_hex(),
                                displaced_key = %displaced_short,
                                conflict_mode = %conflict_mode,
                                "Displaced pinned base model to satisfy enforce pin conflict mode"
                            );
                            return Ok(handle);
                        }
                    }
                }

                if pinned.insert(key.clone()) {
                    // Record when this key was pinned for stale pin GC
                    self.pinned_timestamps
                        .write()
                        .insert(key.clone(), Instant::now());
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
        // Note: Base models are NOT marked active here - being pinned is sufficient
        // to prevent eviction. "Active" means in-flight inference, tracked via begin_use/end_use.

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
            // Record when this key was pinned for stale pin GC
            self.pinned_timestamps
                .write()
                .insert(key.clone(), Instant::now());
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
            // Clean up timestamp
            self.pinned_timestamps.write().remove(key);
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
        let timestamps = self.pinned_timestamps.read();

        timestamps
            .iter()
            .filter_map(|(key, pinned_at)| {
                let age = now.duration_since(*pinned_at);
                if age > threshold {
                    Some((key.clone(), age))
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

    /// Garbage collect stale pinned entries
    ///
    /// Unpins entries that have been pinned for longer than the specified timeout.
    /// This prevents memory leaks when `unpin()` is not called (e.g., after failed
    /// operations or bugs in cleanup paths).
    ///
    /// Returns the number of entries that were unpinned.
    ///
    /// # Default Behavior
    ///
    /// Use `DEFAULT_STALE_PIN_TIMEOUT` (1 hour) as the timeout for typical usage:
    /// ```ignore
    /// let gc_count = cache.gc_stale_pins(DEFAULT_STALE_PIN_TIMEOUT);
    /// ```
    ///
    /// # Integration
    ///
    /// Call this method periodically from the health monitor (e.g., every 5 minutes).
    pub fn gc_stale_pins(&self, timeout: std::time::Duration) -> usize {
        let now = Instant::now();
        let timestamps = self.pinned_timestamps.read();

        // Collect keys that are stale
        let stale_keys: Vec<ModelKey> = timestamps
            .iter()
            .filter_map(|(key, pinned_at)| {
                if now.duration_since(*pinned_at) > timeout {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        drop(timestamps); // Release read lock before unpinning

        if stale_keys.is_empty() {
            return 0;
        }

        let mut gc_count = 0;
        for key in &stale_keys {
            let age = self
                .pinned_timestamps
                .read()
                .get(key)
                .map(|t| now.duration_since(*t));

            if self.unpin(key) {
                gc_count += 1;
                tracing::info!(
                    key = %key.short_hex(),
                    age_secs = age.map(|a| a.as_secs()).unwrap_or(0),
                    timeout_secs = timeout.as_secs(),
                    "Stale pin garbage collected"
                );
            }
        }

        // Update stats
        if gc_count > 0 {
            self.stats.write().stale_pin_gc_count += gc_count as u64;
            tracing::warn!(
                gc_count = gc_count,
                timeout_secs = timeout.as_secs(),
                "Garbage collected stale pins - check for missing unpin() calls"
            );
        }

        gc_count
    }

    /// Explicitly unload a model from the cache
    ///
    /// This is useful for graceful transitions where you want to explicitly
    /// release a model before loading its replacement, or for cleanup during
    /// shutdown sequences.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The model is pinned (must call `unpin()` first)
    /// - The model has active references (in-flight inference)
    /// - The model is not in the cache
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Unpin first if pinned
    /// cache.unpin(&old_key);
    /// // Then unload
    /// cache.unload(&old_key)?;
    /// ```
    pub fn unload(&self, key: &ModelKey) -> Result<()> {
        // Check if pinned
        if self.is_pinned(key) {
            return Err(AosError::CacheEntryPinned {
                key: key.short_hex(),
                reason: "Cannot unload pinned model. Call unpin() first.".to_string(),
            });
        }

        // Check if active
        if self.is_active(key) {
            return Err(AosError::CacheEntryActive {
                key: key.short_hex(),
                reason: "Cannot unload model with active references. Wait for in-flight requests to complete.".to_string(),
            });
        }

        // Acquire write lock and remove
        let mut cache = self.cache.write();

        // Verify entry exists
        let entry = cache
            .remove(key)
            .ok_or_else(|| AosError::CacheEntryNotFound {
                key: key.short_hex(),
            })?;

        // Clean up active counts (defensive - should already be empty)
        self.active_counts.write().remove(key);

        // Notify listeners
        self.notify_evict(key);
        self.listeners.write().remove(key);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.evictions += 1;
            stats.total_memory_bytes = stats.total_memory_bytes.saturating_sub(entry.memory_bytes);
        }

        tracing::info!(
            key = %key.short_hex(),
            memory_mb = entry.memory_bytes / (1024 * 1024),
            "Model explicitly unloaded from cache"
        );

        Ok(())
    }

    /// Prepare for model switch: load new model, then unpin and mark old for eviction
    ///
    /// This method performs a graceful model transition by:
    /// 1. Loading the new model first (so we don't drop the old before replacement is ready)
    /// 2. Unpinning the old model if it was pinned
    /// 3. Marking the old model for eviction (will be evicted on next memory pressure)
    ///
    /// The old model is NOT immediately evicted to allow in-flight requests to complete.
    /// It will be evicted when memory pressure requires it.
    ///
    /// # Arguments
    ///
    /// * `from` - The model key to transition away from
    /// * `to` - The model key to transition to
    /// * `loader` - Function to load the new model (returns handle + memory size)
    ///
    /// # Returns
    ///
    /// The handle for the newly loaded model.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let new_handle = cache.transition_model(&old_key, &new_key, || {
    ///     Ok((ModelHandle::Metal(Arc::new(model_bytes)), size))
    /// })?;
    /// ```
    pub fn transition_model<F>(
        &self,
        from: &ModelKey,
        to: &ModelKey,
        loader: F,
    ) -> Result<ModelHandle>
    where
        F: FnOnce() -> Result<(ModelHandle, u64)>,
    {
        tracing::info!(
            from = %from.short_hex(),
            to = %to.short_hex(),
            "Starting model transition"
        );

        // Step 1: Load new model first (ensures we have replacement before dropping old)
        let new_handle = self.get_or_load(to, loader)?;

        // Step 2: Unpin old model if pinned (allows eviction under memory pressure)
        let was_pinned = self.unpin(from);
        if was_pinned {
            tracing::debug!(
                from = %from.short_hex(),
                "Unpinned old model during transition"
            );
        }

        // Step 3: Mark old model as inactive (if we had marked it active)
        // Note: We don't force-evict here - the old model may still have
        // in-flight requests. It will be evicted on next memory pressure.
        let _ = self.mark_inactive(from);

        tracing::info!(
            from = %from.short_hex(),
            to = %to.short_hex(),
            was_pinned = was_pinned,
            "Model transition complete. Old model marked for eviction."
        );

        Ok(new_handle)
    }

    /// Get current cache state for status reporting
    ///
    /// Returns a consistent snapshot of the cache state including:
    /// - All loaded model keys
    /// - Pinned entry count
    /// - Total memory usage
    /// - Hit ratio
    /// - Active and evictable counts
    ///
    /// This is useful for health checks, monitoring dashboards, and
    /// graceful shutdown coordination.
    pub fn get_cache_status(&self) -> CacheStatus {
        let cache = self.cache.read();
        let pinned = self.pinned_keys.read();
        let active = self.active_counts.read();
        let stats = self.stats.read();

        let loaded_model_keys: Vec<String> = cache.keys().map(|k| k.short_hex()).collect();

        let active_count = active.values().filter(|&&c| c > 0).count();

        let evictable_count = cache
            .keys()
            .filter(|k| !pinned.contains(*k) && active.get(*k).copied().unwrap_or(0) == 0)
            .count();

        let cache_memory_bytes: u64 = cache.values().map(|e| e.memory_bytes).sum();

        let hit_ratio = if stats.hits + stats.misses == 0 {
            0.0
        } else {
            stats.hits as f32 / (stats.hits + stats.misses) as f32
        };

        CacheStatus {
            loaded_model_keys,
            pinned_count: pinned.len(),
            cache_memory_bytes,
            hit_ratio,
            active_count,
            evictable_count,
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
    use std::time::{Duration, Instant};

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
    fn shadow_does_not_displace_and_returns_unpinned_handle() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(1);
        cache.configure_base_model_pinning(
            true,
            None,
            Some("shadow-model".to_string()),
            PinConflictMode::Shadow,
        );
        let key1 = make_key(BackendType::Metal, b"shadow-model-1");
        let key2 = make_key(BackendType::Metal, b"shadow-model-2");

        cache
            .get_or_load_base_model(&key1, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        let result =
            cache.get_or_load_base_model(&key2, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)));

        assert!(result.is_ok(), "Shadow mode should keep serving");
        assert!(cache.is_pinned(&key1), "Existing pin should remain");
        assert!(
            !cache.is_pinned(&key2),
            "Incoming model should remain unpinned"
        );
        assert!(cache.cache.read().contains_key(&key1));
        assert!(cache.cache.read().contains_key(&key2));
    }

    #[test]
    fn enforce_displaces_oldest_non_active_pinned_entry() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(2);
        cache.configure_base_model_pinning(
            true,
            None,
            Some("enforce-model".to_string()),
            PinConflictMode::Enforce,
        );
        let old_key = make_key(BackendType::Metal, b"enforce-old");
        let newer_key = make_key(BackendType::Metal, b"enforce-newer");
        let incoming_key = make_key(BackendType::Metal, b"enforce-incoming");

        cache
            .get_or_load_base_model(&old_key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        cache
            .get_or_load_base_model(&newer_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![2])), 1))
            })
            .unwrap();

        {
            let now = Instant::now();
            let mut timestamps = cache.pinned_timestamps.write();
            timestamps.insert(old_key.clone(), now - Duration::from_secs(20));
            timestamps.insert(newer_key.clone(), now - Duration::from_secs(10));
        }

        let result = cache.get_or_load_base_model(&incoming_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![3])), 1))
        });
        assert!(
            result.is_ok(),
            "Enforce mode should displace the oldest evictable pin"
        );

        assert!(!cache.is_pinned(&old_key), "Old pin should be removed");
        assert!(
            !cache.cache.read().contains_key(&old_key),
            "Displaced model should be unloaded"
        );
        assert!(cache.is_pinned(&newer_key), "Newer pin should remain");
        assert!(
            cache.cache.read().contains_key(&newer_key),
            "Newer pinned model should still be cached"
        );
        assert!(
            cache.is_pinned(&incoming_key),
            "Incoming model should become pinned"
        );
    }

    #[test]
    fn enforce_fails_when_all_pinned_entries_are_active() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(1);
        cache.configure_base_model_pinning(
            true,
            None,
            Some("enforce-blocked".to_string()),
            PinConflictMode::Enforce,
        );
        let old_key = make_key(BackendType::Metal, b"enforce-blocked-old");
        let incoming_key = make_key(BackendType::Metal, b"enforce-blocked-incoming");

        cache
            .get_or_load_base_model(&old_key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();
        let _guard = cache
            .begin_use(&old_key)
            .expect("Old key should be active for conflict test");

        let err = cache
            .get_or_load_base_model(&incoming_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![2])), 1))
            })
            .expect_err("Enforce mode should fail when no pinned candidate is evictable");

        match err {
            AosError::CacheEntryPinned { reason, .. } => {
                assert!(
                    reason.contains("no evictable pinned entries"),
                    "Error reason should explain enforce failure: {}",
                    reason
                );
            }
            other => panic!("Unexpected error variant: {}", other),
        }
        assert!(
            cache.is_pinned(&old_key),
            "Existing pin should remain in place"
        );
        assert!(
            !cache.is_pinned(&incoming_key),
            "Incoming model should not be pinned on enforce failure"
        );
    }

    #[test]
    fn displacement_order_is_deterministic_on_timestamp_tie() {
        let cache = ModelHandleCache::new(1024).with_max_pinned_entries(8);
        cache.configure_base_model_pinning(
            true,
            None,
            Some("candidate-order".to_string()),
            PinConflictMode::Enforce,
        );
        let candidate_a = make_key(BackendType::Metal, b"candidate-a");
        let candidate_b = make_key(BackendType::Metal, b"candidate-b");
        let active_key = make_key(BackendType::Metal, b"candidate-active");
        let missing_key = make_key(BackendType::Metal, b"candidate-missing");
        let incoming_key = make_key(BackendType::Metal, b"candidate-incoming");

        for key in [&candidate_a, &candidate_b, &active_key, &missing_key] {
            cache
                .get_or_load_base_model(key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
                .unwrap();
        }

        let _guard = cache
            .begin_use(&active_key)
            .expect("Active key should be markable active");
        cache.cache.write().remove(&missing_key);

        let tie_timestamp = Instant::now() - Duration::from_secs(10);
        {
            let mut timestamps = cache.pinned_timestamps.write();
            timestamps.insert(candidate_a.clone(), tie_timestamp);
            timestamps.insert(candidate_b.clone(), tie_timestamp);
            timestamps.insert(
                active_key.clone(),
                Instant::now() - Duration::from_secs(100),
            );
            timestamps.insert(
                missing_key.clone(),
                Instant::now() - Duration::from_secs(100),
            );
        }

        let expected = if candidate_a < candidate_b {
            candidate_a.clone()
        } else {
            candidate_b.clone()
        };
        let selected = cache
            .select_pin_displacement_candidate(&incoming_key)
            .expect("A deterministic candidate should be selected");
        assert_eq!(selected, expected);
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

    // ========================================
    // Explicit unload tests
    // ========================================

    #[test]
    fn test_unload_success() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"unload_test");

        cache
            .get_or_load(&key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 100))
            })
            .unwrap();

        assert_eq!(cache.len(), 1);

        // Unload should succeed
        cache.unload(&key).expect("Unload should succeed");

        assert_eq!(cache.len(), 0);
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1, "Unload should count as eviction");
    }

    #[test]
    fn test_unload_fails_when_pinned() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"pinned_unload");

        cache
            .get_or_load_base_model(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 100)))
            .unwrap();

        assert!(cache.is_pinned(&key));

        // Unload should fail
        let err = cache.unload(&key).unwrap_err();
        assert!(
            err.to_string().contains("pinned"),
            "Error should mention pinned: {}",
            err
        );

        // Entry should still be there
        assert_eq!(cache.len(), 1);

        // After unpinning, unload should work
        cache.unpin(&key);
        cache
            .unload(&key)
            .expect("Unload should succeed after unpin");
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_unload_fails_when_active() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"active_unload");

        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 100)))
            .unwrap();

        // Mark as active
        let _guard = cache.begin_use(&key).expect("begin_use should succeed");

        // Unload should fail
        let err = cache.unload(&key).unwrap_err();
        assert!(
            err.to_string().contains("active"),
            "Error should mention active: {}",
            err
        );

        // Entry should still be there
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_unload_not_found() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"not_in_cache");

        let err = cache.unload(&key).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "Error should mention not found: {}",
            err
        );
    }

    // ========================================
    // Model transition tests
    // ========================================

    #[test]
    fn test_transition_model_success() {
        let cache = ModelHandleCache::new(1024);
        let old_key = make_key(BackendType::Metal, b"old_model");
        let new_key = make_key(BackendType::Metal, b"new_model");

        // Load old model and pin it
        cache
            .get_or_load_base_model(&old_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 100))
            })
            .unwrap();

        assert!(cache.is_pinned(&old_key));
        assert_eq!(cache.len(), 1);

        // Transition to new model
        let new_handle = cache
            .transition_model(&old_key, &new_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![4, 5, 6])), 150))
            })
            .expect("Transition should succeed");

        assert!(matches!(new_handle, ModelHandle::Metal(_)));

        // Both models should be in cache (old marked for eviction but not yet evicted)
        assert_eq!(cache.len(), 2);

        // Old model should be unpinned
        assert!(!cache.is_pinned(&old_key));
    }

    #[test]
    fn test_transition_model_new_already_cached() {
        let cache = ModelHandleCache::new(1024);
        let old_key = make_key(BackendType::Metal, b"old_model");
        let new_key = make_key(BackendType::Metal, b"new_model");

        // Load both models
        cache
            .get_or_load_base_model(&old_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 100))
            })
            .unwrap();
        cache
            .get_or_load(&new_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![4, 5, 6])), 150))
            })
            .unwrap();

        assert_eq!(cache.len(), 2);

        let mut load_count = 0;

        // Transition should reuse cached new model
        let _handle = cache
            .transition_model(&old_key, &new_key, || {
                load_count += 1;
                Ok((ModelHandle::Metal(Arc::new(vec![7, 8, 9])), 150))
            })
            .expect("Transition should succeed");

        // Loader should not have been called (cache hit)
        assert_eq!(load_count, 0);

        // Old should be unpinned
        assert!(!cache.is_pinned(&old_key));
    }

    #[test]
    fn test_transition_from_unpinned() {
        let cache = ModelHandleCache::new(1024);
        let old_key = make_key(BackendType::Metal, b"old_unpinned");
        let new_key = make_key(BackendType::Metal, b"new_model");

        // Load old model without pinning
        cache
            .get_or_load(&old_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 100))
            })
            .unwrap();

        assert!(!cache.is_pinned(&old_key));

        // Transition should still work
        let _handle = cache
            .transition_model(&old_key, &new_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![4, 5, 6])), 150))
            })
            .expect("Transition should succeed");

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_set_base_model_key_switches_and_resets_counters() {
        let cache = ModelHandleCache::new(1024);
        let old_key = make_key(BackendType::Metal, b"tracked-old");
        let new_key = make_key(BackendType::Metal, b"tracked-new");

        cache.set_base_model_key(&old_key);
        cache.record_base_model_load(&old_key);
        cache.record_base_model_evict(&old_key);

        let state_before = cache.base_model_pin_state();
        assert_eq!(state_before.base_model_key, Some(old_key.clone()));
        assert_eq!(state_before.load_count, 1);
        assert_eq!(state_before.evict_count, 1);

        cache.set_base_model_key(&new_key);
        let state_after = cache.base_model_pin_state();
        assert_eq!(state_after.base_model_key, Some(new_key));
        assert_eq!(state_after.load_count, 0);
        assert_eq!(state_after.evict_count, 0);
    }

    // ========================================
    // Cache status tests
    // ========================================

    #[test]
    fn test_get_cache_status_empty() {
        let cache = ModelHandleCache::new(1024);
        let status = cache.get_cache_status();

        assert!(status.loaded_model_keys.is_empty());
        assert_eq!(status.pinned_count, 0);
        assert_eq!(status.cache_memory_bytes, 0);
        assert_eq!(status.hit_ratio, 0.0);
        assert_eq!(status.active_count, 0);
        assert_eq!(status.evictable_count, 0);
    }

    #[test]
    fn test_get_cache_status_with_models() {
        let cache = ModelHandleCache::new(1024);
        let key1 = make_key(BackendType::Metal, b"model1");
        let key2 = make_key(BackendType::Metal, b"model2");
        let key3 = make_key(BackendType::Metal, b"model3");

        // Load pinned model
        cache
            .get_or_load_base_model(&key1, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 100])), 100))
            })
            .unwrap();

        // Load unpinned model
        cache
            .get_or_load(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 200])), 200))
            })
            .unwrap();

        // Load and mark active
        cache
            .get_or_load(&key3, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 150])), 150))
            })
            .unwrap();
        let _guard = cache.begin_use(&key3).unwrap();

        // Generate a cache hit
        cache
            .get_or_load(&key2, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0; 200])), 200))
            })
            .unwrap();

        let status = cache.get_cache_status();

        assert_eq!(status.loaded_model_keys.len(), 3);
        assert_eq!(status.pinned_count, 1);
        assert_eq!(status.cache_memory_bytes, 450); // 100 + 200 + 150
        assert!(status.hit_ratio > 0.0); // We had hits and misses
        assert_eq!(status.active_count, 1); // key3 is active
        assert_eq!(status.evictable_count, 1); // only key2 is evictable
    }

    #[test]
    fn test_get_cache_status_hit_ratio() {
        let cache = ModelHandleCache::new(1024);
        let key = make_key(BackendType::Metal, b"model");

        // First load (miss)
        cache
            .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![1])), 1)))
            .unwrap();

        // Three hits
        for _ in 0..3 {
            cache
                .get_or_load(&key, || Ok((ModelHandle::Metal(Arc::new(vec![2])), 1)))
                .unwrap();
        }

        let status = cache.get_cache_status();
        // 3 hits / (3 hits + 1 miss) = 0.75
        assert!((status.hit_ratio - 0.75).abs() < 0.01);
    }
}
