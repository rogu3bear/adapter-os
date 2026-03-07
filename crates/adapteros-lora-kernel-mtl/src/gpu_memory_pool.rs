//! GPU Memory Pool for Metal Backend
//!
//! Implements efficient GPU buffer pooling and memory management:
//! - Buffer reuse to reduce allocation overhead
//! - Automatic cleanup of unused buffers
//! - Memory pressure callbacks and integration with adapteros-memory
//! - Telemetry and monitoring for memory usage
//! - Residency-aware eviction policy for KV cache management

use adapteros_core::{AosError, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

#[cfg(target_os = "macos")]
use super::purgeable::{PurgeableBuffer, PurgeableState};

// Import telemetry metrics for KV residency event emission
use adapteros_telemetry::CriticalComponentMetrics;

#[cfg(target_os = "macos")]
use metal::{Buffer, Device, MTLResourceOptions};

/// KV cache residency classification for memory management.
///
/// HOT entries are actively in use or frequently accessed and should be protected
/// from OS-level memory purgeing. COLD entries can be reclaimed under memory pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KvResidency {
    /// Active or frequently-used entry.
    Hot,
    /// Idle entry - can be evicted under memory pressure.
    #[default]
    Cold,
}

impl std::fmt::Display for KvResidency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hot => write!(f, "HOT"),
            Self::Cold => write!(f, "COLD"),
        }
    }
}

/// Eviction policy for memory pressure handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EvictionPolicy {
    /// Only evict COLD entries (never evict HOT entries)
    ColdOnly,
    /// Evict COLD entries first, then HOT entries if needed
    #[default]
    ColdThenHot,
}

/// GPU memory pool configuration
#[derive(Debug, Clone)]
pub struct GpuMemoryPoolConfig {
    /// Maximum number of buffers per size bucket
    pub max_buffers_per_bucket: usize,
    /// Maximum total pooled memory (bytes)
    pub max_pooled_memory: u64,
    /// Buffer idle timeout before cleanup (seconds)
    pub idle_timeout_secs: u64,
    /// Minimum buffer size to pool (smaller buffers are not worth pooling)
    pub min_buffer_size: u64,
    /// Maximum buffer size to pool (larger buffers are allocated on demand)
    pub max_buffer_size: u64,
    /// Memory pressure threshold to trigger cleanup (0.0-1.0)
    pub pressure_threshold: f32,
    /// Target headroom percentage after cleanup
    pub target_headroom: f32,
    /// Eviction policy for memory pressure handling
    pub eviction_policy: EvictionPolicy,
}

impl Default for GpuMemoryPoolConfig {
    fn default() -> Self {
        Self {
            max_buffers_per_bucket: 16,
            max_pooled_memory: 512 * 1024 * 1024, // 512 MB
            idle_timeout_secs: 60,
            min_buffer_size: 4 * 1024,          // 4 KB
            max_buffer_size: 256 * 1024 * 1024, // 256 MB
            pressure_threshold: 0.85,
            target_headroom: 0.15,
            eviction_policy: EvictionPolicy::default(),
        }
    }
}

/// Pooled GPU buffer metadata
#[cfg(target_os = "macos")]
struct PooledGpuBuffer {
    /// The Metal buffer
    buffer: Buffer,
    /// Buffer size in bytes
    size: u64,
    /// Last access time
    last_accessed: Instant,
    /// Number of times reused
    reuse_count: u32,
    /// Allocation ID for tracking
    allocation_id: u64,
    /// Residency classification for eviction policy
    residency: KvResidency,
    /// Whether this buffer is currently in-flight (cannot be evicted)
    in_flight: bool,
}

/// Memory pressure callback signature
pub type MemoryPressureCallback = Box<dyn Fn(MemoryPressureEvent) + Send + Sync>;

/// Memory pressure event
#[derive(Debug, Clone)]
pub struct MemoryPressureEvent {
    /// Current memory usage (bytes)
    pub current_usage: u64,
    /// Total available memory (bytes)
    pub total_available: u64,
    /// Pressure level (0.0-1.0)
    pub pressure_level: f32,
    /// Bytes to free to reach target headroom
    pub bytes_to_free: u64,
    /// Timestamp
    pub timestamp: u64,
}

/// GPU memory allocation stats for telemetry
#[derive(Debug, Clone, Default)]
pub struct GpuMemoryStats {
    /// Total allocations made
    pub total_allocations: u64,
    /// Total deallocations made
    pub total_deallocations: u64,
    /// Current pooled buffer count
    pub pooled_buffer_count: usize,
    /// Total pooled memory (bytes)
    pub total_pooled_bytes: u64,
    /// Total active memory (bytes) - buffers in use
    pub total_active_bytes: u64,
    /// Pool hits (reused buffers)
    pub pool_hits: u64,
    /// Pool misses (new allocations)
    pub pool_misses: u64,
    /// Buffers cleaned up due to timeout
    pub timeout_cleanups: u64,
    /// Buffers cleaned up due to pressure
    pub pressure_cleanups: u64,
    /// Peak memory usage (bytes)
    pub peak_memory_usage: u64,
    /// COLD entries evicted
    pub cold_evictions: u64,
    /// HOT entries evicted
    pub hot_evictions: u64,
}

/// GPU Memory Pool for Metal buffers
#[cfg(target_os = "macos")]
pub struct GpuMemoryPool {
    /// Metal device
    device: Arc<Device>,
    /// Configuration
    config: GpuMemoryPoolConfig,
    /// Pooled buffers by size bucket
    pools: parking_lot::RwLock<HashMap<u64, VecDeque<PooledGpuBuffer>>>,
    /// Active buffers (in use)
    active_buffers: parking_lot::RwLock<HashMap<u64, u64>>, // allocation_id -> size
    /// Statistics
    stats: parking_lot::RwLock<GpuMemoryStats>,
    /// Allocation counter
    allocation_counter: AtomicU64,
    /// Memory pressure callbacks
    pressure_callbacks: parking_lot::RwLock<Vec<MemoryPressureCallback>>,
    /// Total device memory (estimated)
    total_device_memory: u64,
    /// Optional telemetry metrics handle for event emission
    metrics: Option<Arc<CriticalComponentMetrics>>,
    /// KV quota limit in bytes (None means unlimited)
    kv_quota_limit: Option<u64>,
    /// Current reserved quota in bytes
    kv_quota_reserved: AtomicU64,
}

#[cfg(target_os = "macos")]
impl GpuMemoryPool {
    /// Create a new GPU memory pool
    pub fn new(device: Arc<Device>, config: GpuMemoryPoolConfig) -> Self {
        // Estimate total device memory (Metal doesn't provide direct API)
        // Use recommended working set size as approximation
        let total_device_memory = device.recommended_max_working_set_size();

        info!(
            max_pooled_memory = config.max_pooled_memory,
            total_device_memory = total_device_memory,
            "Created GPU memory pool"
        );

        Self {
            device,
            config,
            pools: parking_lot::RwLock::new(HashMap::new()),
            active_buffers: parking_lot::RwLock::new(HashMap::new()),
            stats: parking_lot::RwLock::new(GpuMemoryStats::default()),
            allocation_counter: AtomicU64::new(0),
            pressure_callbacks: parking_lot::RwLock::new(Vec::new()),
            total_device_memory,
            metrics: None,
            kv_quota_limit: None,
            kv_quota_reserved: AtomicU64::new(0),
        }
    }

    /// Set telemetry metrics handle for event emission
    pub fn set_metrics(&mut self, metrics: Arc<CriticalComponentMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Set KV quota limit in bytes
    ///
    /// When set, allocations will fail with QuotaExceeded if they would exceed this limit.
    /// Set to None for unlimited allocations.
    pub fn set_kv_quota(&mut self, limit: Option<u64>) {
        self.kv_quota_limit = limit;
        if let Some(limit) = limit {
            info!(limit_bytes = limit, "KV quota limit set");
        } else {
            info!("KV quota limit disabled");
        }
    }

    /// Get current quota usage
    pub fn quota_usage(&self) -> (u64, Option<u64>) {
        (
            self.kv_quota_reserved.load(Ordering::SeqCst),
            self.kv_quota_limit,
        )
    }

    /// Reserve quota for an allocation
    ///
    /// Returns true if quota was reserved, false if it would exceed limit.
    fn reserve_quota(&self, size: u64) -> bool {
        let Some(limit) = self.kv_quota_limit else {
            return true; // No quota limit = unlimited
        };

        // Use compare-exchange loop for atomic reservation
        loop {
            let current = self.kv_quota_reserved.load(Ordering::SeqCst);
            if current + size > limit {
                debug!(
                    current = current,
                    requested = size,
                    limit = limit,
                    "Quota reservation would exceed limit"
                );
                return false;
            }
            if self
                .kv_quota_reserved
                .compare_exchange(current, current + size, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
            // Retry if another thread modified the value
        }
    }

    /// Release reserved quota
    fn release_quota(&self, size: u64) {
        if self.kv_quota_limit.is_some() {
            self.kv_quota_reserved.fetch_sub(size, Ordering::SeqCst);
        }
    }

    /// Allocate a GPU buffer (from pool or new)
    pub fn allocate(&self, size: u64) -> Result<(Buffer, u64)> {
        // Check if buffer is too large for pooling
        if size > self.config.max_buffer_size {
            return self.allocate_new(size);
        }

        // Check if buffer is too small for pooling
        if size < self.config.min_buffer_size {
            return self.allocate_new(size);
        }

        let bucket = self.size_to_bucket(size);

        // Try to get from pool
        {
            let mut pools = self.pools.write();
            if let Some(bucket_queue) = pools.get_mut(&bucket) {
                if let Some(mut pooled) = bucket_queue.pop_front() {
                    pooled.last_accessed = Instant::now();
                    pooled.reuse_count += 1;
                    let allocation_id = pooled.allocation_id;
                    let buffer = pooled.buffer;
                    let buffer_size = pooled.size;

                    // Update stats
                    {
                        let mut stats = self.stats.write();
                        stats.pool_hits += 1;
                        stats.total_pooled_bytes -= buffer_size;
                        stats.pooled_buffer_count -= 1;
                        stats.total_active_bytes += buffer_size;
                    }

                    // Track active buffer
                    self.active_buffers
                        .write()
                        .insert(allocation_id, buffer_size);

                    debug!(
                        allocation_id = allocation_id,
                        size = buffer_size,
                        reuse_count = pooled.reuse_count,
                        "Reused buffer from pool"
                    );

                    return Ok((buffer, allocation_id));
                }
            }
        }

        // No pooled buffer available, allocate new
        self.allocate_new(bucket)
    }

    /// Allocate a new buffer (not from pool)
    fn allocate_new(&self, size: u64) -> Result<(Buffer, u64)> {
        // KV quota enforcement
        if !self.reserve_quota(size) {
            if let Some(ref metrics) = self.metrics {
                metrics.record_kv_quota_exceeded();
            }
            return Err(AosError::QuotaExceeded {
                resource: "kv_cache".to_string(),
                failure_code: Some("KV_QUOTA_EXCEEDED".to_string()),
            });
        }

        // Check memory pressure before allocation
        self.check_memory_pressure()?;

        let buffer = self
            .device
            .new_buffer(size, MTLResourceOptions::StorageModeShared);

        let allocation_id = self.allocation_counter.fetch_add(1, Ordering::SeqCst);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_allocations += 1;
            stats.pool_misses += 1;
            stats.total_active_bytes += size;

            let current_usage = stats.total_active_bytes + stats.total_pooled_bytes;
            if current_usage > stats.peak_memory_usage {
                stats.peak_memory_usage = current_usage;
            }
        }

        // Track active buffer
        self.active_buffers.write().insert(allocation_id, size);

        debug!(
            allocation_id = allocation_id,
            size = size,
            "Allocated new GPU buffer"
        );

        Ok((buffer, allocation_id))
    }

    /// Release a buffer back to the pool or deallocate
    pub fn release(&self, buffer: Buffer, allocation_id: u64) {
        let size = {
            let mut active = self.active_buffers.write();
            match active.remove(&allocation_id) {
                Some(s) => s,
                None => {
                    warn!(
                        allocation_id = allocation_id,
                        "Attempted to release unknown buffer"
                    );
                    return;
                }
            }
        };

        // Check if buffer should be pooled
        if size >= self.config.min_buffer_size && size <= self.config.max_buffer_size {
            let bucket = self.size_to_bucket(size);

            let mut pools = self.pools.write();
            let bucket_queue = pools.entry(bucket).or_default();

            // Check pool limits
            let should_pool = {
                let stats = self.stats.read();
                bucket_queue.len() < self.config.max_buffers_per_bucket
                    && stats.total_pooled_bytes + size <= self.config.max_pooled_memory
            };

            if should_pool {
                let pooled = PooledGpuBuffer {
                    buffer,
                    size,
                    last_accessed: Instant::now(),
                    reuse_count: 0,
                    allocation_id,
                    residency: KvResidency::Cold, // Default to COLD
                    in_flight: false,
                };
                bucket_queue.push_back(pooled);

                // Update stats
                {
                    let mut stats = self.stats.write();
                    stats.total_deallocations += 1;
                    stats.total_active_bytes -= size;
                    stats.total_pooled_bytes += size;
                    stats.pooled_buffer_count += 1;
                }

                debug!(
                    allocation_id = allocation_id,
                    size = size,
                    bucket = bucket,
                    "Released buffer to pool"
                );
                return;
            }
        }

        // Buffer not pooled, just deallocate
        {
            let mut stats = self.stats.write();
            stats.total_deallocations += 1;
            stats.total_active_bytes -= size;
        }

        // Release quota when buffer is actually dropped
        self.release_quota(size);

        debug!(
            allocation_id = allocation_id,
            size = size,
            "Deallocated buffer (not pooled)"
        );
        // Buffer is dropped here
    }

    /// Clean up idle buffers that exceed timeout
    pub fn cleanup_idle_buffers(&self) -> u64 {
        let timeout = Duration::from_secs(self.config.idle_timeout_secs);
        let now = Instant::now();
        let mut total_freed = 0u64;

        let mut pools = self.pools.write();
        for (bucket, queue) in pools.iter_mut() {
            let initial_len: usize = queue.len();

            // Remove buffers that have been idle too long
            queue.retain(|pooled| {
                let idle_time = now.duration_since(pooled.last_accessed);
                if idle_time > timeout {
                    total_freed += pooled.size;
                    false
                } else {
                    true
                }
            });

            let removed = initial_len - queue.len();
            if removed > 0 {
                debug!(
                    bucket = bucket,
                    removed = removed,
                    "Cleaned up idle buffers"
                );
            }
        }

        if total_freed > 0 {
            let mut stats = self.stats.write();
            stats.total_pooled_bytes -= total_freed;
            stats.timeout_cleanups += 1;
            stats.pooled_buffer_count = pools
                .values()
                .map(|q: &VecDeque<PooledGpuBuffer>| q.len())
                .sum();

            // Release quota for cleaned up buffers
            self.release_quota(total_freed);

            info!(total_freed = total_freed, "Cleaned up idle GPU buffers");
        }

        total_freed
    }

    /// Check memory pressure and trigger callbacks if needed
    fn check_memory_pressure(&self) -> Result<()> {
        let stats = self.stats.read();
        let current_usage = stats.total_active_bytes + stats.total_pooled_bytes;
        let pressure_level = current_usage as f32 / self.total_device_memory as f32;

        if pressure_level >= self.config.pressure_threshold {
            let bytes_to_free = current_usage
                - ((1.0 - self.config.target_headroom) * self.total_device_memory as f32) as u64;

            let event = MemoryPressureEvent {
                current_usage,
                total_available: self.total_device_memory,
                pressure_level,
                bytes_to_free,
                timestamp: current_timestamp(),
            };

            // Trigger callbacks
            let callbacks = self.pressure_callbacks.read();
            for callback in callbacks.iter() {
                callback(event.clone());
            }

            warn!(
                pressure_level = pressure_level,
                bytes_to_free = bytes_to_free,
                "GPU memory pressure detected"
            );
        }

        Ok(())
    }

    /// Handle memory pressure by freeing pooled buffers with residency awareness
    ///
    /// This implements the residency-aware eviction policy:
    /// 1. Never evict in-flight buffers
    /// 2. For ColdOnly: only evict COLD entries (sorted by LRU)
    /// 3. For ColdThenHot: evict COLD first, then HOT if needed (both sorted by LRU)
    pub fn handle_memory_pressure(&self, bytes_to_free: u64) -> u64 {
        let mut total_freed = 0u64;
        let mut cold_freed = 0u64;
        let mut hot_freed = 0u64;
        let mut pools = self.pools.write();

        // Sort buckets by size (free larger buffers first for efficiency)
        let mut buckets: Vec<u64> = pools.keys().copied().collect();
        buckets.sort_by(|a, b| b.cmp(a)); // Descending

        // Phase 1: Evict COLD entries only (sorted by LRU - oldest first)
        for bucket in &buckets {
            if total_freed >= bytes_to_free {
                break;
            }

            if let Some(queue) = pools.get_mut(bucket) {
                // Build list of evictable COLD buffers with their indices
                let mut cold_candidates: Vec<(usize, u64, Instant)> = queue
                    .iter()
                    .enumerate()
                    .filter(|(_, pooled)| {
                        !pooled.in_flight && pooled.residency == KvResidency::Cold
                    })
                    .map(|(idx, pooled)| (idx, pooled.size, pooled.last_accessed))
                    .collect();

                // Sort by LRU (oldest first)
                cold_candidates.sort_by_key(|(_, _, last_accessed)| *last_accessed);

                // Evict COLD buffers
                let mut indices_to_remove = Vec::new();
                for (idx, size, _) in cold_candidates {
                    if total_freed >= bytes_to_free {
                        break;
                    }
                    indices_to_remove.push(idx);
                    total_freed += size;
                    cold_freed += size;
                }

                // Remove from back to front to preserve indices
                indices_to_remove.sort_by(|a, b| b.cmp(a));
                for idx in indices_to_remove {
                    queue.remove(idx);
                    // Buffer is dropped here
                }
            }
        }

        // Phase 2: If policy allows and still need more memory, evict HOT entries
        if total_freed < bytes_to_free && self.config.eviction_policy == EvictionPolicy::ColdThenHot
        {
            for bucket in &buckets {
                if total_freed >= bytes_to_free {
                    break;
                }

                if let Some(queue) = pools.get_mut(bucket) {
                    // Build list of evictable HOT buffers with their indices
                    let mut hot_candidates: Vec<(usize, u64, Instant)> = queue
                        .iter()
                        .enumerate()
                        .filter(|(_, pooled)| {
                            !pooled.in_flight && pooled.residency == KvResidency::Hot
                        })
                        .map(|(idx, pooled)| (idx, pooled.size, pooled.last_accessed))
                        .collect();

                    // Sort by LRU (oldest first)
                    hot_candidates.sort_by_key(|(_, _, last_accessed)| *last_accessed);

                    // Evict HOT buffers
                    let mut indices_to_remove = Vec::new();
                    for (idx, size, _) in hot_candidates {
                        if total_freed >= bytes_to_free {
                            break;
                        }
                        indices_to_remove.push(idx);
                        total_freed += size;
                        hot_freed += size;
                    }

                    // Remove from back to front to preserve indices
                    indices_to_remove.sort_by(|a, b| b.cmp(a));
                    for idx in indices_to_remove {
                        queue.remove(idx);
                        // Buffer is dropped here
                    }
                }
            }
        }

        if total_freed > 0 {
            let mut stats = self.stats.write();
            stats.total_pooled_bytes -= total_freed;
            stats.pressure_cleanups += 1;
            stats.cold_evictions += if cold_freed > 0 { 1 } else { 0 };
            stats.hot_evictions += if hot_freed > 0 { 1 } else { 0 };
            stats.pooled_buffer_count = pools
                .values()
                .map(|q: &VecDeque<PooledGpuBuffer>| q.len())
                .sum();

            info!(
                total_freed = total_freed,
                cold_freed = cold_freed,
                hot_freed = hot_freed,
                target = bytes_to_free,
                policy = ?self.config.eviction_policy,
                "Freed GPU memory due to pressure"
            );

            // Emit telemetry events for evictions
            if let Some(ref metrics) = self.metrics {
                if cold_freed > 0 {
                    metrics.record_kv_eviction(CriticalComponentMetrics::kv_residency_cold());
                }
                if hot_freed > 0 {
                    metrics.record_kv_eviction(CriticalComponentMetrics::kv_residency_hot());
                }
            }

            // Release quota for evicted buffers
            self.release_quota(total_freed);
        }

        total_freed
    }

    /// Set residency classification for a pooled buffer
    ///
    /// This allows marking buffers as HOT (actively used) or COLD (evictable).
    /// Only affects buffers currently in the pool, not active buffers.
    pub fn set_buffer_residency(&self, allocation_id: u64, residency: KvResidency) -> bool {
        let mut pools = self.pools.write();
        for queue in pools.values_mut() {
            for pooled in queue.iter_mut() {
                if pooled.allocation_id == allocation_id {
                    pooled.residency = residency;
                    debug!(
                        allocation_id = allocation_id,
                        residency = %residency,
                        "Updated buffer residency"
                    );

                    // Apply purgeable state to Metal buffer based on residency
                    let purgeable_state = match residency {
                        KvResidency::Hot => PurgeableState::NonVolatile,
                        KvResidency::Cold => PurgeableState::Volatile,
                    };
                    match pooled.buffer.set_purgeable_state(purgeable_state) {
                        Ok(result) if result.was_purged => {
                            warn!(
                                allocation_id = allocation_id,
                                "Buffer contents were purged by OS before state change"
                            );
                        }
                        Ok(_) => {
                            debug!(
                                allocation_id = allocation_id,
                                purgeable_state = ?purgeable_state,
                                "Applied purgeable state to buffer"
                            );
                        }
                        Err(e) => {
                            warn!(
                                allocation_id = allocation_id,
                                error = %e,
                                "Failed to set purgeable state"
                            );
                            if let Some(ref metrics) = self.metrics {
                                metrics.record_kv_purgeable_failure();
                            }
                        }
                    }

                    return true;
                }
            }
        }
        false
    }

    /// Mark buffer as in-flight (prevents eviction)
    pub fn mark_in_flight(&self, allocation_id: u64, in_flight: bool) -> bool {
        let mut pools = self.pools.write();
        for queue in pools.values_mut() {
            for pooled in queue.iter_mut() {
                if pooled.allocation_id == allocation_id {
                    pooled.in_flight = in_flight;
                    debug!(
                        allocation_id = allocation_id,
                        in_flight = in_flight,
                        "Updated buffer in-flight status"
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Get residency statistics for pooled buffers
    pub fn residency_stats(&self) -> (usize, usize, u64, u64) {
        let pools = self.pools.read();
        let mut hot_count = 0usize;
        let mut cold_count = 0usize;
        let mut hot_bytes = 0u64;
        let mut cold_bytes = 0u64;

        for queue in pools.values() {
            for pooled in queue.iter() {
                match pooled.residency {
                    KvResidency::Hot => {
                        hot_count += 1;
                        hot_bytes += pooled.size;
                    }
                    KvResidency::Cold => {
                        cold_count += 1;
                        cold_bytes += pooled.size;
                    }
                }
            }
        }

        (hot_count, cold_count, hot_bytes, cold_bytes)
    }

    /// Register a memory pressure callback
    pub fn register_pressure_callback(&self, callback: MemoryPressureCallback) {
        self.pressure_callbacks.write().push(callback);
    }

    /// Get current memory statistics
    pub fn stats(&self) -> GpuMemoryStats {
        self.stats.read().clone()
    }

    /// Get current memory usage
    pub fn current_usage(&self) -> (u64, u64) {
        let stats = self.stats.read();
        (stats.total_active_bytes, stats.total_pooled_bytes)
    }

    /// Clear all pooled buffers
    pub fn clear_pool(&self) {
        let mut pools = self.pools.write();
        let total_freed: u64 = pools
            .values()
            .flat_map(|q: &VecDeque<PooledGpuBuffer>| q.iter())
            .map(|p| p.size)
            .sum();

        pools.clear();

        let mut stats = self.stats.write();
        stats.total_pooled_bytes = 0;
        stats.pooled_buffer_count = 0;

        // Release quota for all cleared buffers
        self.release_quota(total_freed);

        info!(total_freed = total_freed, "Cleared GPU memory pool");
    }

    /// Convert size to bucket (power of 2 rounding)
    fn size_to_bucket(&self, size: u64) -> u64 {
        // Round up to next power of 2 for efficient bucketing
        let mut bucket = self.config.min_buffer_size;
        while bucket < size {
            bucket *= 2;
        }
        bucket.min(self.config.max_buffer_size)
    }

    /// Get pool information for telemetry
    pub fn pool_info(&self) -> Vec<(u64, usize, u64)> {
        let pools = self.pools.read();
        pools
            .iter()
            .map(|(bucket, queue): (&u64, &VecDeque<PooledGpuBuffer>)| {
                let total_bytes: u64 = queue.iter().map(|p| p.size).sum();
                (*bucket, queue.len(), total_bytes)
            })
            .collect()
    }
}

/// Non-macOS stub implementation
#[cfg(not(target_os = "macos"))]
pub struct GpuMemoryPool {
    config: GpuMemoryPoolConfig,
    stats: parking_lot::RwLock<GpuMemoryStats>,
    kv_quota_limit: Option<u64>,
}

#[cfg(not(target_os = "macos"))]
impl GpuMemoryPool {
    pub fn new<T>(_device: T, config: GpuMemoryPoolConfig) -> Self {
        Self {
            config,
            stats: parking_lot::RwLock::new(GpuMemoryStats::default()),
            kv_quota_limit: None,
        }
    }

    pub fn set_kv_quota(&mut self, limit: Option<u64>) {
        self.kv_quota_limit = limit;
    }

    pub fn quota_usage(&self) -> (u64, Option<u64>) {
        (0, self.kv_quota_limit)
    }

    pub fn stats(&self) -> GpuMemoryStats {
        self.stats.read().clone()
    }

    pub fn cleanup_idle_buffers(&self) -> u64 {
        0
    }

    pub fn handle_memory_pressure(&self, _bytes_to_free: u64) -> u64 {
        0
    }

    pub fn clear_pool(&self) {
        // No-op on non-macOS
    }

    pub fn pool_info(&self) -> Vec<(u64, usize, u64)> {
        Vec::new()
    }
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_bucket_calculation() {
        let config = GpuMemoryPoolConfig::default();

        // Mock bucket calculation
        let size_to_bucket = |size: u64| -> u64 {
            let mut bucket = config.min_buffer_size;
            while bucket < size {
                bucket *= 2;
            }
            bucket.min(config.max_buffer_size)
        };

        assert_eq!(size_to_bucket(1024), 4096);
        assert_eq!(size_to_bucket(4096), 4096);
        assert_eq!(size_to_bucket(5000), 8192);
        assert_eq!(size_to_bucket(65536), 65536);
    }

    #[test]
    fn test_config_defaults() {
        let config = GpuMemoryPoolConfig::default();
        assert_eq!(config.max_buffers_per_bucket, 16);
        assert_eq!(config.idle_timeout_secs, 60);
        assert!(config.pressure_threshold > 0.0 && config.pressure_threshold < 1.0);
    }

    #[test]
    fn test_stats_default() {
        let stats = GpuMemoryStats::default();
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.pool_hits, 0);
        assert_eq!(stats.pool_misses, 0);
    }

    #[test]
    fn test_quota_reserve_release() {
        // Test the quota reservation logic directly using atomics
        let quota_limit = Some(1024u64);
        let quota_reserved = AtomicU64::new(0);

        // Helper to simulate reserve_quota logic
        let reserve = |size: u64| -> bool {
            let Some(limit) = quota_limit else {
                return true;
            };
            loop {
                let current = quota_reserved.load(Ordering::SeqCst);
                if current + size > limit {
                    return false;
                }
                if quota_reserved
                    .compare_exchange(current, current + size, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return true;
                }
            }
        };

        // Should succeed: 512 < 1024
        assert!(reserve(512));
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 512);

        // Should succeed: 512 + 256 = 768 < 1024
        assert!(reserve(256));
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 768);

        // Should fail: 768 + 512 = 1280 > 1024
        assert!(!reserve(512));
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 768);

        // Should succeed: 768 + 256 = 1024 == 1024
        assert!(reserve(256));
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 1024);

        // Release and verify
        quota_reserved.fetch_sub(512, Ordering::SeqCst);
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 512);

        // Now 512 + 512 = 1024, should succeed
        assert!(reserve(512));
        assert_eq!(quota_reserved.load(Ordering::SeqCst), 1024);
    }

    #[test]
    fn test_quota_unlimited() {
        // Test that None quota means unlimited
        let quota_limit: Option<u64> = None;
        let reserve = |_size: u64| -> bool { quota_limit.is_none() };

        assert!(reserve(u64::MAX));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_pool_quota_enforcement() {
        use metal::Device;

        let device = Device::system_default().expect("Metal device should be available");
        let config = GpuMemoryPoolConfig {
            max_pooled_memory: 1024 * 1024, // 1 MB
            ..Default::default()
        };
        let mut pool = GpuMemoryPool::new(Arc::new(device), config);

        // Set a 256 KB quota (must account for power-of-2 bucket rounding)
        // min_buffer_size is 4KB, so allocations are rounded up to next power of 2
        pool.set_kv_quota(Some(256 * 1024));

        // First allocation: 32KB -> bucket = 32KB
        let result1 = pool.allocate(32 * 1024);
        assert!(result1.is_ok(), "First 32KB allocation should succeed");

        // Second allocation: 64KB -> bucket = 64KB (total = 96KB)
        let result2 = pool.allocate(64 * 1024);
        assert!(result2.is_ok(), "Second 64KB allocation should succeed");

        // Third allocation: 128KB -> bucket = 128KB (total would = 224KB, still under 256KB)
        let result3 = pool.allocate(128 * 1024);
        assert!(result3.is_ok(), "Third 128KB allocation should succeed");

        // Fourth allocation: 64KB -> would exceed quota (224 + 64 = 288KB > 256KB)
        let result4 = pool.allocate(64 * 1024);
        assert!(
            result4.is_err(),
            "Fourth allocation should fail due to quota"
        );

        // Verify quota usage
        let (used, limit) = pool.quota_usage();
        assert_eq!(limit, Some(256 * 1024));
        assert!(used <= 256 * 1024);
    }
}
