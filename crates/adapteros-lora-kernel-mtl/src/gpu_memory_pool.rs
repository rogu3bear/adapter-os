//! GPU Memory Pool for Metal Backend
//!
//! Implements efficient GPU buffer pooling and memory management:
//! - Buffer reuse to reduce allocation overhead
//! - Automatic cleanup of unused buffers
//! - Memory pressure callbacks and integration with adapteros-memory
//! - Telemetry and monitoring for memory usage

use adapteros_core::{AosError, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

#[cfg(target_os = "macos")]
use metal::{Buffer, Device, MTLResourceOptions};

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

    /// Handle memory pressure by freeing pooled buffers
    pub fn handle_memory_pressure(&self, bytes_to_free: u64) -> u64 {
        let mut total_freed = 0u64;
        let mut pools = self.pools.write();

        // Sort buckets by size (free larger buffers first)
        let mut buckets: Vec<u64> = pools.keys().copied().collect();
        buckets.sort_by(|a, b| b.cmp(a)); // Descending

        for bucket in buckets {
            if total_freed >= bytes_to_free {
                break;
            }

            if let Some(queue) = pools.get_mut(&bucket) {
                while total_freed < bytes_to_free && !queue.is_empty() {
                    if let Some(pooled) = queue.pop_back() {
                        total_freed += pooled.size;
                        // Buffer is dropped here
                    }
                }
            }
        }

        if total_freed > 0 {
            let mut stats = self.stats.write();
            stats.total_pooled_bytes -= total_freed;
            stats.pressure_cleanups += 1;
            stats.pooled_buffer_count = pools
                .values()
                .map(|q: &VecDeque<PooledGpuBuffer>| q.len())
                .sum();

            info!(
                total_freed = total_freed,
                target = bytes_to_free,
                "Freed GPU memory due to pressure"
            );
        }

        total_freed
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
}

#[cfg(not(target_os = "macos"))]
impl GpuMemoryPool {
    pub fn new(config: GpuMemoryPoolConfig) -> Self {
        Self {
            config,
            stats: parking_lot::RwLock::new(GpuMemoryStats::default()),
        }
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
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
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
}
