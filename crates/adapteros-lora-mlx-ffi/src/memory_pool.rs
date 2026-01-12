//! MLX Memory Pool for GPU Buffer Management
//!
//! Implements efficient GPU buffer pooling and memory management for MLX backend:
//! - Size-bucketed buffer pooling (power of 2 rounding)
//! - Idle timeout cleanup for unused buffers
//! - Memory pressure callbacks for eviction
//! - Per-adapter VRAM usage tracking
//! - Integration with MLX's unified memory system

use adapteros_core::Result;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// MLX memory pool configuration
#[derive(Debug, Clone)]
pub struct MLXMemoryPoolConfig {
    /// Maximum number of buffers per size bucket
    pub max_buffers_per_bucket: usize,
    /// Maximum total pooled memory in bytes (default 512MB)
    pub max_pooled_memory: usize,
    /// Buffer idle timeout before cleanup in seconds (default 60)
    pub idle_timeout_secs: u64,
    /// Memory pressure threshold to trigger cleanup (0.0-1.0, default 0.85)
    pub pressure_threshold: f32,
    /// Minimum buffer size to pool (smaller buffers are not worth pooling)
    pub min_buffer_size: usize,
    /// Maximum buffer size to pool (larger buffers are allocated on demand)
    pub max_buffer_size: usize,
    /// Target headroom percentage after cleanup
    pub target_headroom: f32,
}

impl Default for MLXMemoryPoolConfig {
    fn default() -> Self {
        Self {
            max_buffers_per_bucket: 16,
            max_pooled_memory: 512 * 1024 * 1024, // 512 MB
            idle_timeout_secs: 60,
            pressure_threshold: 0.85,
            min_buffer_size: 4 * 1024,          // 4 KB
            max_buffer_size: 256 * 1024 * 1024, // 256 MB
            target_headroom: 0.15,
        }
    }
}

/// Pooled buffer wrapper for MLX unified memory
///
/// MLX uses unified memory, so this represents a logical buffer allocation
/// rather than a separate GPU buffer. The buffer data is managed by MLX's
/// internal memory system.
pub struct PooledBuffer {
    /// Unique allocation ID
    allocation_id: u64,
    /// Buffer size in bytes
    size: usize,
    /// Raw buffer data (MLX unified memory)
    data: Vec<f32>,
    /// Creation timestamp for metrics
    #[allow(dead_code)]
    created_at: Instant,
}

impl PooledBuffer {
    /// Get the allocation ID
    pub fn allocation_id(&self) -> u64 {
        self.allocation_id
    }

    /// Get the buffer size in bytes
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the buffer data as a slice
    pub fn data(&self) -> &[f32] {
        &self.data
    }

    /// Get mutable access to buffer data
    pub fn data_mut(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// Get the number of elements in the buffer
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Internal pooled buffer metadata
struct PooledBufferEntry {
    /// The buffer data
    data: Vec<f32>,
    /// Buffer size in bytes
    size: usize,
    /// Last access time for idle timeout
    last_accessed: Instant,
    /// Number of times this buffer has been reused
    reuse_count: u32,
    /// Original allocation ID
    allocation_id: u64,
}

/// Memory pressure event for callbacks
#[derive(Debug, Clone)]
pub struct MemoryPressureEvent {
    /// Current memory usage in bytes
    pub current_usage: usize,
    /// Total available memory in bytes
    pub total_available: usize,
    /// Pressure level (0.0-1.0)
    pub pressure_level: f32,
    /// Bytes needed to free to reach target headroom
    pub bytes_to_free: usize,
    /// Event timestamp (Unix seconds)
    pub timestamp: u64,
}

/// Memory pressure callback signature
pub type MemoryPressureCallback = Box<dyn Fn(MemoryPressureEvent) + Send + Sync>;

/// Memory pool statistics for monitoring and telemetry
#[derive(Debug, Clone, Default)]
pub struct MemoryPoolStats {
    /// Total allocations made
    pub total_allocations: u64,
    /// Total deallocations made
    pub total_deallocations: u64,
    /// Current pooled buffer count
    pub pooled_buffer_count: usize,
    /// Total pooled memory in bytes
    pub total_pooled_bytes: usize,
    /// Total active memory in bytes (buffers in use)
    pub total_active_bytes: usize,
    /// Pool hits (reused buffers)
    pub pool_hits: u64,
    /// Pool misses (new allocations)
    pub pool_misses: u64,
    /// Buffers cleaned up due to timeout
    pub timeout_cleanups: u64,
    /// Buffers cleaned up due to pressure
    pub pressure_cleanups: u64,
    /// Peak memory usage in bytes
    pub peak_memory_usage: usize,
}

/// Per-adapter memory tracking entry
#[derive(Debug, Clone, Default)]
struct AdapterMemoryEntry {
    /// Total bytes allocated for this adapter
    bytes: usize,
    /// Number of allocations
    allocation_count: u32,
    /// Last update timestamp
    last_updated: Option<Instant>,
}

/// MLX Memory Pool for unified memory buffer management
///
/// Provides efficient buffer pooling and reuse for MLX operations, reducing
/// allocation overhead and fragmentation. Integrates with MLX's unified
/// memory model where CPU and GPU share the same memory space.
pub struct MLXMemoryPool {
    /// Configuration
    config: MLXMemoryPoolConfig,
    /// Pooled buffers organized by size bucket (power of 2)
    pools: RwLock<HashMap<usize, VecDeque<PooledBufferEntry>>>,
    /// Active buffers currently in use (allocation_id -> size)
    active_buffers: RwLock<HashMap<u64, usize>>,
    /// Statistics for monitoring
    stats: RwLock<MemoryPoolStats>,
    /// Allocation counter for unique IDs
    allocation_counter: AtomicU64,
    /// Memory pressure callbacks
    pressure_callbacks: RwLock<Vec<MemoryPressureCallback>>,
    /// Per-adapter memory tracking
    adapter_memory: RwLock<HashMap<u16, AdapterMemoryEntry>>,
    /// Estimated total device memory (MLX unified memory)
    total_device_memory: usize,
}

impl MLXMemoryPool {
    /// Create a new MLX memory pool with the given configuration
    ///
    /// # Arguments
    /// * `config` - Pool configuration parameters
    ///
    /// # Returns
    /// A new MLXMemoryPool instance
    pub fn new(config: MLXMemoryPoolConfig) -> Self {
        // Get total device memory from system
        // On Apple Silicon, unified memory is shared between CPU and GPU
        let total_device_memory = Self::estimate_device_memory();

        info!(
            max_pooled_memory = config.max_pooled_memory,
            total_device_memory = total_device_memory,
            pressure_threshold = config.pressure_threshold,
            idle_timeout_secs = config.idle_timeout_secs,
            "Created MLX memory pool"
        );

        Self {
            config,
            pools: RwLock::new(HashMap::new()),
            active_buffers: RwLock::new(HashMap::new()),
            stats: RwLock::new(MemoryPoolStats::default()),
            allocation_counter: AtomicU64::new(0),
            pressure_callbacks: RwLock::new(Vec::new()),
            adapter_memory: RwLock::new(HashMap::new()),
            total_device_memory,
        }
    }

    /// Estimate total device memory available for MLX
    ///
    /// On Apple Silicon, this queries the system's unified memory size.
    /// Falls back to a conservative default if unavailable.
    fn estimate_device_memory() -> usize {
        // Try to get system memory info
        // On Apple Silicon, GPU uses unified memory
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            // Try to get physical memory from sysctl
            if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
                if let Ok(mem_str) = String::from_utf8(output.stdout) {
                    if let Ok(mem_bytes) = mem_str.trim().parse::<usize>() {
                        // MLX can use most of unified memory, but we should leave
                        // headroom for system. Use 75% as available GPU memory.
                        return (mem_bytes as f64 * 0.75) as usize;
                    }
                }
            }
        }

        // Conservative default: 8GB
        8 * 1024 * 1024 * 1024
    }

    /// Allocate a buffer from the pool or create a new one
    ///
    /// Buffers are organized by size buckets (power of 2 rounding) for efficient reuse.
    /// If a suitable pooled buffer exists, it will be reused; otherwise a new buffer
    /// is allocated.
    ///
    /// # Arguments
    /// * `size` - Requested buffer size in bytes
    ///
    /// # Returns
    /// A pooled buffer on success, or an error if allocation fails
    pub fn allocate(&self, size: usize) -> Result<PooledBuffer> {
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
                if let Some(mut entry) = bucket_queue.pop_front() {
                    entry.last_accessed = Instant::now();
                    entry.reuse_count += 1;
                    let allocation_id = entry.allocation_id;
                    let buffer_size = entry.size;

                    // Update stats
                    {
                        let mut stats = self.stats.write();
                        stats.pool_hits += 1;
                        stats.total_pooled_bytes =
                            stats.total_pooled_bytes.saturating_sub(buffer_size);
                        stats.pooled_buffer_count = stats.pooled_buffer_count.saturating_sub(1);
                        stats.total_active_bytes += buffer_size;
                    }

                    // Track active buffer
                    self.active_buffers
                        .write()
                        .insert(allocation_id, buffer_size);

                    debug!(
                        allocation_id = allocation_id,
                        size = buffer_size,
                        reuse_count = entry.reuse_count,
                        "Reused buffer from MLX pool"
                    );

                    return Ok(PooledBuffer {
                        allocation_id,
                        size: buffer_size,
                        data: entry.data,
                        created_at: Instant::now(),
                    });
                }
            }
        }

        // No pooled buffer available, allocate new with bucket size
        self.allocate_new(bucket)
    }

    /// Allocate a new buffer (not from pool)
    fn allocate_new(&self, size: usize) -> Result<PooledBuffer> {
        // Check memory pressure before allocation
        self.check_memory_pressure()?;

        // Calculate number of f32 elements needed
        let num_elements = size.div_ceil(std::mem::size_of::<f32>());

        // Allocate the buffer data
        let data = vec![0.0f32; num_elements];
        let actual_size = num_elements * std::mem::size_of::<f32>();

        let allocation_id = self.allocation_counter.fetch_add(1, Ordering::SeqCst);

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_allocations += 1;
            stats.pool_misses += 1;
            stats.total_active_bytes += actual_size;

            let current_usage = stats.total_active_bytes + stats.total_pooled_bytes;
            if current_usage > stats.peak_memory_usage {
                stats.peak_memory_usage = current_usage;
            }
        }

        // Track active buffer
        self.active_buffers
            .write()
            .insert(allocation_id, actual_size);

        debug!(
            allocation_id = allocation_id,
            size = actual_size,
            "Allocated new MLX buffer"
        );

        Ok(PooledBuffer {
            allocation_id,
            size: actual_size,
            data,
            created_at: Instant::now(),
        })
    }

    /// Return a buffer to the pool for potential reuse
    ///
    /// The buffer may be pooled for future reuse if it meets size criteria
    /// and pool limits haven't been reached. Otherwise it will be deallocated.
    ///
    /// # Arguments
    /// * `buffer` - The buffer to return to the pool
    pub fn return_buffer(&self, buffer: PooledBuffer) {
        let allocation_id = buffer.allocation_id;
        let size = buffer.size;

        // Remove from active tracking
        {
            let mut active = self.active_buffers.write();
            if active.remove(&allocation_id).is_none() {
                warn!(
                    allocation_id = allocation_id,
                    "Attempted to return unknown buffer to MLX pool"
                );
                return;
            }
        }

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
                let entry = PooledBufferEntry {
                    data: buffer.data,
                    size,
                    last_accessed: Instant::now(),
                    reuse_count: 0,
                    allocation_id,
                };
                bucket_queue.push_back(entry);

                // Update stats
                {
                    let mut stats = self.stats.write();
                    stats.total_deallocations += 1;
                    stats.total_active_bytes = stats.total_active_bytes.saturating_sub(size);
                    stats.total_pooled_bytes += size;
                    stats.pooled_buffer_count += 1;
                }

                debug!(
                    allocation_id = allocation_id,
                    size = size,
                    bucket = bucket,
                    "Returned buffer to MLX pool"
                );
                return;
            }
        }

        // Buffer not pooled, deallocate
        {
            let mut stats = self.stats.write();
            stats.total_deallocations += 1;
            stats.total_active_bytes = stats.total_active_bytes.saturating_sub(size);
        }

        debug!(
            allocation_id = allocation_id,
            size = size,
            "Deallocated MLX buffer (not pooled)"
        );
        // Buffer is dropped here
    }

    /// Clean up idle buffers that exceed the timeout
    ///
    /// Iterates through all pooled buffers and removes those that haven't
    /// been accessed within the idle timeout period.
    ///
    /// # Returns
    /// Total bytes freed from idle buffers
    pub fn cleanup_idle(&self) -> usize {
        let timeout = Duration::from_secs(self.config.idle_timeout_secs);
        let now = Instant::now();
        let mut total_freed = 0usize;

        let mut pools = self.pools.write();
        for (bucket, queue) in pools.iter_mut() {
            let initial_len = queue.len();

            // Remove buffers that have been idle too long
            queue.retain(|entry| {
                let idle_time = now.duration_since(entry.last_accessed);
                if idle_time > timeout {
                    total_freed += entry.size;
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
                    "Cleaned up idle buffers from MLX pool"
                );
            }
        }

        if total_freed > 0 {
            let mut stats = self.stats.write();
            stats.total_pooled_bytes = stats.total_pooled_bytes.saturating_sub(total_freed);
            stats.timeout_cleanups += 1;
            stats.pooled_buffer_count = pools.values().map(|q| q.len()).sum();

            info!(
                total_freed_bytes = total_freed,
                total_freed_mb = total_freed as f64 / (1024.0 * 1024.0),
                "Cleaned up idle MLX buffers"
            );
        }

        total_freed
    }

    /// Get current memory pool statistics
    ///
    /// # Returns
    /// A snapshot of current memory pool statistics
    pub fn get_stats(&self) -> MemoryPoolStats {
        self.stats.read().clone()
    }

    /// Track memory usage for a specific adapter
    ///
    /// # Arguments
    /// * `adapter_id` - Unique adapter identifier
    /// * `bytes` - Number of bytes to track for this adapter
    pub fn track_adapter(&self, adapter_id: u16, bytes: usize) {
        let mut adapter_memory = self.adapter_memory.write();
        let entry = adapter_memory.entry(adapter_id).or_default();
        entry.bytes = bytes;
        entry.allocation_count += 1;
        entry.last_updated = Some(Instant::now());

        debug!(
            adapter_id = adapter_id,
            bytes = bytes,
            "Tracking adapter memory in MLX pool"
        );
    }

    /// Stop tracking memory for a specific adapter
    ///
    /// # Arguments
    /// * `adapter_id` - Unique adapter identifier to untrack
    pub fn untrack_adapter(&self, adapter_id: u16) {
        let removed = self.adapter_memory.write().remove(&adapter_id);

        if let Some(entry) = removed {
            debug!(
                adapter_id = adapter_id,
                bytes = entry.bytes,
                "Untracked adapter from MLX pool"
            );
        }
    }

    /// Get total memory tracked across all adapters
    ///
    /// # Returns
    /// Total bytes tracked for all adapters
    pub fn total_adapter_memory(&self) -> usize {
        self.adapter_memory
            .read()
            .values()
            .map(|entry| entry.bytes)
            .sum()
    }

    /// Get memory usage for a specific adapter
    ///
    /// # Arguments
    /// * `adapter_id` - Unique adapter identifier
    ///
    /// # Returns
    /// Bytes tracked for the adapter, or None if not tracked
    pub fn get_adapter_memory(&self, adapter_id: u16) -> Option<usize> {
        self.adapter_memory.read().get(&adapter_id).map(|e| e.bytes)
    }

    /// Register a callback for memory pressure events
    ///
    /// The callback will be invoked when memory usage exceeds the pressure threshold.
    ///
    /// # Arguments
    /// * `callback` - Function to call on memory pressure events
    pub fn register_pressure_callback(&self, callback: MemoryPressureCallback) {
        self.pressure_callbacks.write().push(callback);
    }

    /// Handle memory pressure by freeing pooled buffers
    ///
    /// Frees buffers from largest buckets first until the target bytes are freed.
    ///
    /// # Arguments
    /// * `bytes_to_free` - Target number of bytes to free
    ///
    /// # Returns
    /// Actual number of bytes freed
    pub fn handle_memory_pressure(&self, bytes_to_free: usize) -> usize {
        let mut total_freed = 0usize;
        let mut pools = self.pools.write();

        // Sort buckets by size (free larger buffers first for efficiency)
        let mut buckets: Vec<usize> = pools.keys().copied().collect();
        buckets.sort_by(|a, b| b.cmp(a)); // Descending

        for bucket in buckets {
            if total_freed >= bytes_to_free {
                break;
            }

            if let Some(queue) = pools.get_mut(&bucket) {
                while total_freed < bytes_to_free && !queue.is_empty() {
                    if let Some(entry) = queue.pop_back() {
                        total_freed += entry.size;
                        // Buffer data is dropped here
                    }
                }
            }
        }

        if total_freed > 0 {
            let mut stats = self.stats.write();
            stats.total_pooled_bytes = stats.total_pooled_bytes.saturating_sub(total_freed);
            stats.pressure_cleanups += 1;
            stats.pooled_buffer_count = pools.values().map(|q| q.len()).sum();

            info!(
                total_freed_bytes = total_freed,
                target_bytes = bytes_to_free,
                "Freed MLX memory due to pressure"
            );
        }

        total_freed
    }

    /// Check memory pressure and trigger callbacks if needed
    fn check_memory_pressure(&self) -> Result<()> {
        let stats = self.stats.read();
        let current_usage = stats.total_active_bytes + stats.total_pooled_bytes;
        let pressure_level = current_usage as f32 / self.total_device_memory as f32;

        if pressure_level >= self.config.pressure_threshold {
            let target_usage =
                ((1.0 - self.config.target_headroom) * self.total_device_memory as f32) as usize;
            let bytes_to_free = current_usage.saturating_sub(target_usage);

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
                current_usage_mb = current_usage as f64 / (1024.0 * 1024.0),
                "MLX memory pressure detected"
            );
        }

        Ok(())
    }

    /// Convert size to bucket (power of 2 rounding)
    ///
    /// Rounds the size up to the nearest power of 2 for efficient bucket management.
    fn size_to_bucket(&self, size: usize) -> usize {
        let mut bucket = self.config.min_buffer_size;
        while bucket < size {
            bucket *= 2;
        }
        bucket.min(self.config.max_buffer_size)
    }

    /// Get current memory usage
    ///
    /// # Returns
    /// Tuple of (active_bytes, pooled_bytes)
    pub fn current_usage(&self) -> (usize, usize) {
        let stats = self.stats.read();
        (stats.total_active_bytes, stats.total_pooled_bytes)
    }

    /// Clear all pooled buffers
    ///
    /// Immediately frees all pooled buffers. Active buffers are not affected.
    pub fn clear_pool(&self) {
        let mut pools = self.pools.write();
        let total_freed: usize = pools.values().flat_map(|q| q.iter()).map(|e| e.size).sum();

        pools.clear();

        let mut stats = self.stats.write();
        stats.total_pooled_bytes = 0;
        stats.pooled_buffer_count = 0;

        info!(
            total_freed_bytes = total_freed,
            total_freed_mb = total_freed as f64 / (1024.0 * 1024.0),
            "Cleared MLX memory pool"
        );
    }

    /// Get pool information for telemetry
    ///
    /// # Returns
    /// Vector of (bucket_size, buffer_count, total_bytes) tuples
    pub fn pool_info(&self) -> Vec<(usize, usize, usize)> {
        let pools = self.pools.read();
        pools
            .iter()
            .map(|(bucket, queue)| {
                let total_bytes: usize = queue.iter().map(|e| e.size).sum();
                (*bucket, queue.len(), total_bytes)
            })
            .collect()
    }

    /// Get list of tracked adapter IDs
    pub fn tracked_adapters(&self) -> Vec<u16> {
        self.adapter_memory.read().keys().copied().collect()
    }

    /// Get total device memory estimate
    pub fn total_device_memory(&self) -> usize {
        self.total_device_memory
    }
}

/// Get current timestamp in Unix seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = MLXMemoryPoolConfig::default();
        assert_eq!(config.max_buffers_per_bucket, 16);
        assert_eq!(config.max_pooled_memory, 512 * 1024 * 1024);
        assert_eq!(config.idle_timeout_secs, 60);
        assert!((config.pressure_threshold - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_size_to_bucket() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Test power of 2 rounding
        assert_eq!(pool.size_to_bucket(1024), 4096);
        assert_eq!(pool.size_to_bucket(4096), 4096);
        assert_eq!(pool.size_to_bucket(5000), 8192);
        assert_eq!(pool.size_to_bucket(65536), 65536);
        assert_eq!(pool.size_to_bucket(100000), 131072);
    }

    #[test]
    fn test_allocate_and_return() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Allocate a buffer
        let buffer = pool.allocate(8192).expect("Allocation should succeed");
        assert!(buffer.size() >= 8192);

        let stats = pool.get_stats();
        assert_eq!(stats.total_allocations, 1);
        assert_eq!(stats.pool_misses, 1);
        assert!(stats.total_active_bytes > 0);

        // Return buffer to pool
        pool.return_buffer(buffer);

        let stats = pool.get_stats();
        assert_eq!(stats.total_deallocations, 1);
        assert_eq!(stats.pooled_buffer_count, 1);
    }

    #[test]
    fn test_buffer_reuse() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Allocate and return a buffer
        let buffer1 = pool.allocate(8192).expect("Allocation should succeed");
        let size1 = buffer1.size();
        pool.return_buffer(buffer1);

        // Allocate again - should reuse from pool
        let buffer2 = pool.allocate(8192).expect("Allocation should succeed");
        assert_eq!(buffer2.size(), size1);

        let stats = pool.get_stats();
        assert_eq!(stats.pool_hits, 1);
        assert_eq!(stats.pool_misses, 1);
    }

    #[test]
    fn test_adapter_tracking() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Track adapter memory
        pool.track_adapter(1, 1024 * 1024);
        pool.track_adapter(2, 2 * 1024 * 1024);

        assert_eq!(pool.get_adapter_memory(1), Some(1024 * 1024));
        assert_eq!(pool.get_adapter_memory(2), Some(2 * 1024 * 1024));
        assert_eq!(pool.total_adapter_memory(), 3 * 1024 * 1024);

        // Untrack adapter
        pool.untrack_adapter(1);
        assert_eq!(pool.get_adapter_memory(1), None);
        assert_eq!(pool.total_adapter_memory(), 2 * 1024 * 1024);
    }

    #[test]
    fn test_clear_pool() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Allocate and return multiple buffers
        for _ in 0..5 {
            let buffer = pool.allocate(8192).expect("Allocation should succeed");
            pool.return_buffer(buffer);
        }

        let stats = pool.get_stats();
        assert!(stats.pooled_buffer_count > 0);

        // Clear pool
        pool.clear_pool();

        let stats = pool.get_stats();
        assert_eq!(stats.pooled_buffer_count, 0);
        assert_eq!(stats.total_pooled_bytes, 0);
    }

    #[test]
    fn test_stats_default() {
        let stats = MemoryPoolStats::default();
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.pool_hits, 0);
        assert_eq!(stats.pool_misses, 0);
        assert_eq!(stats.pooled_buffer_count, 0);
    }

    #[test]
    fn test_pool_info() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Allocate buffers of different sizes
        let buf1 = pool.allocate(4096).expect("Should allocate");
        let buf2 = pool.allocate(8192).expect("Should allocate");
        let buf3 = pool.allocate(4096).expect("Should allocate");

        pool.return_buffer(buf1);
        pool.return_buffer(buf2);
        pool.return_buffer(buf3);

        let info = pool.pool_info();
        assert!(!info.is_empty());

        // Should have buffers in at least one bucket
        let total_buffers: usize = info.iter().map(|(_, count, _)| count).sum();
        assert!(total_buffers >= 3);
    }

    #[test]
    fn test_handle_memory_pressure() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        // Allocate and pool several buffers
        for _ in 0..10 {
            let buffer = pool.allocate(65536).expect("Should allocate");
            pool.return_buffer(buffer);
        }

        let stats_before = pool.get_stats();
        assert!(stats_before.total_pooled_bytes > 0);

        // Request to free some memory
        let freed = pool.handle_memory_pressure(stats_before.total_pooled_bytes / 2);
        assert!(freed > 0);

        let stats_after = pool.get_stats();
        assert!(stats_after.total_pooled_bytes < stats_before.total_pooled_bytes);
    }

    #[test]
    fn test_pooled_buffer_accessors() {
        let config = MLXMemoryPoolConfig::default();
        let pool = MLXMemoryPool::new(config);

        let mut buffer = pool.allocate(1024).expect("Should allocate");

        assert!(!buffer.is_empty());
        assert!(!buffer.is_empty());
        assert!(buffer.size() > 0);

        // Test mutable access
        if let Some(first) = buffer.data_mut().first_mut() {
            *first = 42.0;
        }
        assert!((buffer.data()[0] - 42.0).abs() < f32::EPSILON);
    }
}
