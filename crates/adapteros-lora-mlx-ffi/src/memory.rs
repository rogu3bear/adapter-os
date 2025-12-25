//! Memory management API for MLX backend
//!
//! Provides functions for monitoring and managing memory usage in the MLX unified memory system.

use crate::*;

/// Trigger garbage collection in MLX unified memory
///
/// This hints the system to reclaim unused buffers by flushing pending operations
/// and allowing the memory manager to compact its pools.
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::memory;
///
/// memory::gc_collect();
/// ```
pub fn gc_collect() {
    unsafe {
        mlx_gc_collect();
    }
}

/// Get total memory usage in bytes
///
/// Tracks all array allocations and model weights through the FFI wrapper.
/// Returns the sum of all currently allocated unified memory buffers.
///
/// # Example
/// ```ignore
/// let bytes = memory::memory_usage();
/// let mb = bytes as f32 / (1024.0 * 1024.0);
/// println!("Memory usage: {:.2} MB", mb);
/// ```
pub fn memory_usage() -> usize {
    unsafe { mlx_memory_usage() }
}

/// Get the number of tracked allocations
///
/// Useful for debugging and profiling to understand allocation patterns
/// and detect potential memory leaks.
///
/// # Example
/// ```ignore
/// let count = memory::allocation_count();
/// println!("Active allocations: {}", count);
/// ```
pub fn allocation_count() -> usize {
    unsafe { mlx_allocation_count() }
}

/// Get detailed memory statistics
///
/// Returns a tuple of (total_bytes, allocation_count)
///
/// # Example
/// ```ignore
/// let (total, count) = memory::memory_stats();
/// println!("Total: {} bytes, Allocations: {}", total, count);
/// ```
pub fn memory_stats() -> (usize, usize) {
    let mut total_bytes = 0;
    let mut allocation_count = 0;
    unsafe {
        mlx_memory_stats(&mut total_bytes, &mut allocation_count);
    }
    (total_bytes, allocation_count)
}

/// Reset memory tracking
///
/// Clears all tracked allocations and resets counters to zero.
/// Used for testing and debugging purposes.
///
/// # Example
/// ```ignore
/// use adapteros_lora_mlx_ffi::memory;
///
/// memory::reset();
/// // ... perform operations ...
/// let stats = memory::stats();
/// println!("Memory used in this scope: {}", stats.total_bytes);
/// ```
pub fn reset() {
    unsafe {
        mlx_memory_reset();
    }
}

/// Memory statistics snapshot
///
/// A structured representation of memory usage at a point in time
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// Total bytes allocated
    pub total_bytes: usize,
    /// Number of allocations
    pub allocation_count: usize,
}

/// Get memory statistics as a structured snapshot
///
/// # Example
/// ```ignore
/// let stats = memory::stats();
/// println!("{}", memory::format_stats(&stats));
/// ```
pub fn stats() -> MemoryStats {
    let (total_bytes, allocation_count) = memory_stats();
    MemoryStats {
        total_bytes,
        allocation_count,
    }
}

/// Convert bytes to megabytes
///
/// # Example
/// ```ignore
/// let mb = memory::bytes_to_mb(1024 * 1024);
/// assert_eq!(mb, 1.0);
/// ```
pub fn bytes_to_mb(bytes: usize) -> f32 {
    bytes as f32 / (1024.0 * 1024.0)
}

/// Format memory statistics for logging or display
///
/// # Example
/// ```ignore
/// let stats = memory::stats();
/// tracing::info!("{}", memory::format_stats(&stats));
/// // Output: "MLX Memory: 123.45 MB (42 allocations)"
/// ```
pub fn format_stats(stats: &MemoryStats) -> String {
    let mb = bytes_to_mb(stats.total_bytes);
    format!(
        "MLX Memory: {:.2} MB ({} allocations)",
        mb, stats.allocation_count
    )
}

/// Check if memory usage exceeds a threshold
///
/// # Arguments
/// * `threshold_mb` - Memory threshold in megabytes
///
/// # Returns
/// true if current memory usage exceeds the threshold
///
/// # Example
/// ```ignore
/// if memory::exceeds_threshold(2048.0) {
///     tracing::warn!("Memory usage exceeded 2GB");
///     memory::gc_collect();
/// }
/// ```
pub fn exceeds_threshold(threshold_mb: f32) -> bool {
    let stats = stats();
    bytes_to_mb(stats.total_bytes) > threshold_mb
}
