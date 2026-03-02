//! System allocator statistics for internal fragmentation tracking
//!
//! This module provides access to allocator-level memory statistics on macOS
//! using the `malloc_zone_statistics` API. These statistics enable accurate
//! measurement of internal fragmentation (wasted space within allocations due
//! to alignment, padding, and size-class overhead).
//!
//! # Platform Support
//!
//! - macOS: Full support via `malloc/malloc.h` zone APIs
//! - Other platforms: Stub implementation returning estimates
//!
//! # Usage
//!
//! ```no_run
//! use adapteros_memory::allocator_stats::{AllocatorStats, get_allocator_stats};
//!
//! let stats = get_allocator_stats();
//! println!("Internal fragmentation: {:.2}%", stats.internal_fragmentation_ratio * 100.0);
//! ```

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Allocator statistics for fragmentation analysis
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AllocatorStats {
    /// Total bytes allocated by the allocator
    pub bytes_allocated: u64,
    /// Total bytes in use by the application (requested size)
    pub bytes_in_use: u64,
    /// Total bytes reserved by the allocator (including overhead)
    pub bytes_reserved: u64,
    /// Number of free blocks tracked by the allocator
    pub free_block_count: u32,
    /// Total free bytes in the allocator's free lists
    pub free_bytes: u64,
    /// Largest contiguous free block
    pub largest_free_block: u64,
    /// Internal fragmentation ratio (0.0-1.0)
    /// Calculated as: (bytes_allocated - bytes_in_use) / bytes_allocated
    pub internal_fragmentation_ratio: f32,
    /// Whether stats were obtained from the real allocator or estimated
    pub is_real_stats: bool,
}

impl Default for AllocatorStats {
    fn default() -> Self {
        Self {
            bytes_allocated: 0,
            bytes_in_use: 0,
            bytes_reserved: 0,
            free_block_count: 0,
            free_bytes: 0,
            largest_free_block: 0,
            internal_fragmentation_ratio: 0.0,
            is_real_stats: false,
        }
    }
}

impl AllocatorStats {
    /// Create stats with an estimated internal fragmentation ratio
    /// Used when real allocator stats are not available
    pub fn with_estimate(bytes_allocated: u64, estimate_ratio: f32) -> Self {
        let estimated_overhead = (bytes_allocated as f64 * estimate_ratio as f64) as u64;
        let bytes_in_use = bytes_allocated.saturating_sub(estimated_overhead);

        Self {
            bytes_allocated,
            bytes_in_use,
            bytes_reserved: bytes_allocated,
            free_block_count: 0,
            free_bytes: 0,
            largest_free_block: 0,
            internal_fragmentation_ratio: estimate_ratio,
            is_real_stats: false,
        }
    }
}

// ============================================================================
// macOS IMPLEMENTATION
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;

    /// FFI-safe malloc zone statistics structure
    /// Matches the layout of `malloc_statistics_t` from `malloc/malloc.h`
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct MallocStatistics {
        pub blocks_in_use: u32,
        pub size_in_use: usize,
        pub max_size_in_use: usize,
        pub size_allocated: usize,
    }

    /// FFI-safe malloc zone structure (opaque)
    #[repr(C)]
    pub struct MallocZone {
        _private: [u8; 0],
    }

    extern "C" {
        /// Get the default malloc zone
        fn malloc_default_zone() -> *mut MallocZone;

        /// Get statistics for a malloc zone
        fn malloc_zone_statistics(zone: *mut MallocZone, stats: *mut MallocStatistics);

        /// Get all registered malloc zones
        fn malloc_get_all_zones(
            task: u32,
            reader: *const c_void,
            addresses: *mut *mut *mut MallocZone,
            count: *mut u32,
        ) -> i32;
    }

    /// Get the current task port
    #[inline]
    fn mach_task_self() -> u32 {
        extern "C" {
            fn mach_task_self() -> u32;
        }
        unsafe { mach_task_self() }
    }

    /// Query allocator statistics from the macOS malloc zone API
    pub fn get_zone_stats() -> AllocatorStats {
        let mut total_stats = AllocatorStats {
            is_real_stats: true,
            ..Default::default()
        };

        // Get default zone statistics
        let default_zone = unsafe { malloc_default_zone() };
        if !default_zone.is_null() {
            let mut zone_stats = MallocStatistics::default();
            unsafe {
                malloc_zone_statistics(default_zone, &mut zone_stats);
            }

            total_stats.bytes_allocated = zone_stats.size_allocated as u64;
            total_stats.bytes_in_use = zone_stats.size_in_use as u64;
            total_stats.bytes_reserved = zone_stats.max_size_in_use as u64;

            // Estimate free block count from the difference
            // A more accurate count would require zone introspection
            if zone_stats.blocks_in_use > 0 {
                // Rough estimate: each allocation averages some overhead
                let avg_alloc_size = zone_stats.size_in_use / zone_stats.blocks_in_use as usize;
                let overhead_per_block = zone_stats
                    .size_allocated
                    .saturating_sub(zone_stats.size_in_use)
                    / zone_stats.blocks_in_use as usize;
                total_stats.free_block_count = (zone_stats
                    .size_allocated
                    .saturating_sub(zone_stats.size_in_use)
                    / avg_alloc_size.max(1)) as u32;
                total_stats.largest_free_block = overhead_per_block as u64;
            }
        }

        // Try to get all zones for more accurate stats
        let mut zone_addresses: *mut *mut MallocZone = std::ptr::null_mut();
        let mut zone_count: u32 = 0;

        let result = unsafe {
            malloc_get_all_zones(
                mach_task_self(),
                std::ptr::null(),
                &mut zone_addresses,
                &mut zone_count,
            )
        };

        if result == 0 && !zone_addresses.is_null() && zone_count > 0 {
            let mut aggregate_allocated: u64 = 0;
            let mut aggregate_in_use: u64 = 0;
            let mut aggregate_max: u64 = 0;

            for i in 0..zone_count as isize {
                let zone = unsafe { *zone_addresses.offset(i) };
                if !zone.is_null() {
                    let mut zone_stats = MallocStatistics::default();
                    unsafe {
                        malloc_zone_statistics(zone, &mut zone_stats);
                    }
                    aggregate_allocated += zone_stats.size_allocated as u64;
                    aggregate_in_use += zone_stats.size_in_use as u64;
                    aggregate_max += zone_stats.max_size_in_use as u64;
                }
            }

            // Use aggregate stats if available
            if aggregate_allocated > 0 {
                total_stats.bytes_allocated = aggregate_allocated;
                total_stats.bytes_in_use = aggregate_in_use;
                total_stats.bytes_reserved = aggregate_max;
                total_stats.free_bytes = aggregate_allocated.saturating_sub(aggregate_in_use);
            }
        }

        // Calculate internal fragmentation ratio
        if total_stats.bytes_allocated > 0 {
            let overhead = total_stats
                .bytes_allocated
                .saturating_sub(total_stats.bytes_in_use);
            total_stats.internal_fragmentation_ratio =
                (overhead as f64 / total_stats.bytes_allocated as f64) as f32;
        }

        debug!(
            "Allocator stats: allocated={}, in_use={}, fragmentation={:.2}%",
            total_stats.bytes_allocated,
            total_stats.bytes_in_use,
            total_stats.internal_fragmentation_ratio * 100.0
        );

        total_stats
    }
}

// ============================================================================
// NON-macOS STUB IMPLEMENTATION
// ============================================================================

#[cfg(not(target_os = "macos"))]
mod other {
    use super::*;

    /// Stub implementation for non-macOS platforms
    /// Returns estimated statistics based on typical allocator overhead
    pub fn get_zone_stats() -> AllocatorStats {
        debug!("Allocator stats not available on this platform, using estimate");

        // Return an estimate based on typical jemalloc/system allocator overhead
        // Modern allocators typically have 5-15% overhead depending on allocation patterns
        AllocatorStats {
            bytes_allocated: 0,
            bytes_in_use: 0,
            bytes_reserved: 0,
            free_block_count: 0,
            free_bytes: 0,
            largest_free_block: 0,
            internal_fragmentation_ratio: 0.10, // Conservative 10% estimate
            is_real_stats: false,
        }
    }
}

// ============================================================================
// PUBLIC API
// ============================================================================

/// Get current allocator statistics
///
/// On macOS, queries the malloc zone API for accurate statistics.
/// On other platforms, returns estimated values.
pub fn get_allocator_stats() -> AllocatorStats {
    #[cfg(target_os = "macos")]
    {
        macos::get_zone_stats()
    }
    #[cfg(not(target_os = "macos"))]
    {
        other::get_zone_stats()
    }
}

/// Calculate internal fragmentation ratio for a given allocation size
///
/// This function queries the system allocator and calculates the internal
/// fragmentation based on actual zone statistics. If real stats are available,
/// uses them; otherwise falls back to an estimate.
///
/// # Arguments
///
/// * `total_allocated` - Total bytes allocated (for fallback calculation)
///
/// # Returns
///
/// Internal fragmentation ratio between 0.0 and 1.0
pub fn calculate_internal_fragmentation(total_allocated: u64) -> f32 {
    let stats = get_allocator_stats();

    if stats.is_real_stats && stats.bytes_allocated > 0 {
        // Use real allocator stats
        stats.internal_fragmentation_ratio.clamp(0.0, 1.0)
    } else if total_allocated > 0 {
        // For GPU memory, use a slightly higher estimate due to alignment requirements
        // GPU allocations typically have 16-byte to 256-byte alignment requirements
        // which can cause more internal fragmentation than CPU allocations
        const GPU_ALIGNMENT_OVERHEAD_ESTIMATE: f32 = 0.15;
        GPU_ALIGNMENT_OVERHEAD_ESTIMATE
    } else {
        0.0
    }
}

/// Get the estimated overhead for GPU memory allocations
///
/// GPU memory allocators (Metal, CUDA, etc.) have different fragmentation
/// characteristics than CPU allocators due to:
/// - Stricter alignment requirements (often 256-byte or page-aligned)
/// - Coarser allocation granularity
/// - Device-specific memory pools
///
/// This function returns a more accurate estimate for GPU contexts.
pub fn gpu_internal_fragmentation_estimate() -> f32 {
    // GPU memory typically has higher fragmentation due to:
    // 1. Alignment requirements (16-256 bytes)
    // 2. Page-size granularity (4KB-16KB)
    // 3. Memory pool overhead
    //
    // Empirical studies show 10-20% overhead is common
    0.15
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocator_stats_default() {
        let stats = AllocatorStats::default();
        assert_eq!(stats.bytes_allocated, 0);
        assert_eq!(stats.internal_fragmentation_ratio, 0.0);
        assert!(!stats.is_real_stats);
    }

    #[test]
    fn test_allocator_stats_with_estimate() {
        let stats = AllocatorStats::with_estimate(1000, 0.15);
        assert_eq!(stats.bytes_allocated, 1000);
        assert_eq!(stats.bytes_in_use, 850); // 1000 - (1000 * 0.15)
        assert_eq!(stats.internal_fragmentation_ratio, 0.15);
        assert!(!stats.is_real_stats);
    }

    #[test]
    fn test_get_allocator_stats() {
        let stats = get_allocator_stats();

        // On macOS, we should get real stats
        #[cfg(target_os = "macos")]
        {
            // Stats should be real and have reasonable values
            assert!(stats.is_real_stats);
            // Fragmentation should be between 0 and 100%
            assert!(stats.internal_fragmentation_ratio >= 0.0);
            assert!(stats.internal_fragmentation_ratio <= 1.0);
        }

        // On other platforms, we get estimates
        #[cfg(not(target_os = "macos"))]
        {
            assert!(!stats.is_real_stats);
            assert_eq!(stats.internal_fragmentation_ratio, 0.10);
        }
    }

    #[test]
    fn test_calculate_internal_fragmentation() {
        // With zero allocation, should return 0
        let frag = calculate_internal_fragmentation(0);
        assert_eq!(frag, 0.0);

        // With non-zero allocation, should return a reasonable value
        let frag = calculate_internal_fragmentation(1_000_000);
        assert!(frag >= 0.0);
        assert!(frag <= 1.0);
    }

    #[test]
    fn test_gpu_fragmentation_estimate() {
        let estimate = gpu_internal_fragmentation_estimate();
        assert!(estimate > 0.0);
        assert!(estimate < 1.0);
        // Should be around 15%
        assert!((estimate - 0.15).abs() < 0.01);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos_zone_stats_reasonable_values() {
        // Allocate some memory to ensure we have stats
        let allocations: Vec<Vec<u8>> = (0..100).map(|i| vec![0u8; (i + 1) * 1024]).collect();

        let stats = get_allocator_stats();

        // Should have some memory allocated
        assert!(stats.bytes_allocated > 0, "Expected some bytes allocated");
        assert!(stats.bytes_in_use > 0, "Expected some bytes in use");

        // In use should not exceed allocated
        assert!(
            stats.bytes_in_use <= stats.bytes_allocated,
            "bytes_in_use ({}) should not exceed bytes_allocated ({})",
            stats.bytes_in_use,
            stats.bytes_allocated
        );

        // Fragmentation should be reasonable (not 100%)
        assert!(
            stats.internal_fragmentation_ratio < 0.5,
            "Fragmentation ({:.2}%) seems too high",
            stats.internal_fragmentation_ratio * 100.0
        );

        // Keep allocations alive
        drop(allocations);
    }
}
