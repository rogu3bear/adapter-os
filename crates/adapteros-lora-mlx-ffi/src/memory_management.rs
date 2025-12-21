//! MLX Memory Management Functions
//!
//! Provides comprehensive memory management capabilities for the MLX backend including:
//! - Garbage collection triggering
//! - Memory usage tracking
//! - Allocation counting and statistics
//! - GPU operation synchronization
//! - Integration with adapteros-memory infrastructure

use adapteros_core::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Memory management statistics snapshot
#[derive(Debug, Clone, Copy)]
pub struct MemoryManagementStats {
    /// Total bytes currently allocated
    pub total_bytes: usize,
    /// Number of active allocations
    pub allocation_count: usize,
    /// Peak memory usage recorded
    pub peak_bytes: u64,
}

impl MemoryManagementStats {
    /// Convert total bytes to megabytes
    pub fn total_mb(&self) -> f32 {
        self.total_bytes as f32 / (1024.0 * 1024.0)
    }

    /// Convert peak bytes to megabytes
    pub fn peak_mb(&self) -> f32 {
        self.peak_bytes as f32 / (1024.0 * 1024.0)
    }

    /// Check if memory usage exceeds a threshold
    pub fn exceeds_mb_threshold(&self, threshold_mb: f32) -> bool {
        self.total_mb() > threshold_mb
    }
}

/// Global memory tracking for MLX backend
///
/// Thread-safe counters for monitoring peak memory usage across all MLX operations
pub struct MemoryTracker {
    peak_memory: AtomicU64,
    current_collections: AtomicU64,
}

impl MemoryTracker {
    /// Create a new memory tracker
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            peak_memory: AtomicU64::new(0),
            current_collections: AtomicU64::new(0),
        })
    }

    /// Record a memory measurement and update peak if needed
    pub fn record_memory(&self, bytes: usize) {
        let bytes_u64 = bytes as u64;
        let mut current_peak = self.peak_memory.load(Ordering::Relaxed);

        while bytes_u64 > current_peak {
            match self.peak_memory.compare_exchange(
                current_peak,
                bytes_u64,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }
    }

    /// Get current peak memory usage
    pub fn peak_memory(&self) -> u64 {
        self.peak_memory.load(Ordering::Acquire)
    }

    /// Record a garbage collection operation
    pub fn record_gc(&self) {
        self.current_collections.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total GC collection count
    pub fn collection_count(&self) -> u64 {
        self.current_collections.load(Ordering::Acquire)
    }

    /// Reset tracking (for testing)
    pub fn reset(&self) {
        self.peak_memory.store(0, Ordering::Release);
        self.current_collections.store(0, Ordering::Release);
    }
}

impl Default for MemoryTracker {
    fn default() -> Self {
        Self {
            peak_memory: AtomicU64::new(0),
            current_collections: AtomicU64::new(0),
        }
    }
}

/// MLX memory management API
///
/// High-level interface for memory operations with integration to existing
/// memory tracking infrastructure
pub struct MLXMemoryManager {
    tracker: Arc<MemoryTracker>,
}

impl MLXMemoryManager {
    /// Create a new memory manager
    pub fn new() -> Self {
        Self {
            tracker: MemoryTracker::new(),
        }
    }

    /// Trigger garbage collection in MLX unified memory
    ///
    /// This function hints the system to reclaim unused buffers by:
    /// 1. Flushing pending operations via mlx_eval
    /// 2. Allowing the memory manager to compact its pools
    /// 3. Marking opportunities for memory reclamation
    ///
    /// # Note
    /// GC is a soft hint and may not immediately reclaim memory. The amount
    /// of reclaimed memory depends on MLX's internal memory manager state.
    ///
    /// # Example
    /// ```ignore
    /// let manager = MLXMemoryManager::new();
    /// manager.gc_collect()?;
    /// ```
    pub fn gc_collect(&self) -> Result<()> {
        debug!("Triggering MLX garbage collection");

        unsafe {
            // Clear any previous error state before operation
            super::mlx_clear_error();

            // Call C FFI to trigger GC
            super::mlx_gc_collect();

            // Check for errors after void function (may fail silently)
            let error_msg = super::mlx_get_last_error();
            if !error_msg.is_null() {
                let error_str = std::ffi::CStr::from_ptr(error_msg)
                    .to_string_lossy()
                    .to_string();
                if !error_str.is_empty() {
                    super::mlx_clear_error();
                    warn!("MLX garbage collection warning: {}", error_str);
                    // Don't fail - GC is advisory
                }
            }
        }

        self.tracker.record_gc();

        debug!(
            collection_count = self.tracker.collection_count(),
            "MLX garbage collection completed"
        );

        Ok(())
    }

    /// Get current memory usage in bytes
    ///
    /// Tracks all array allocations and model weights through the FFI wrapper.
    /// Returns the sum of all currently allocated unified memory buffers.
    ///
    /// # Returns
    /// Total bytes currently allocated to MLX arrays and models
    ///
    /// # Example
    /// ```ignore
    /// let usage = manager.memory_usage()?;
    /// let mb = usage as f32 / (1024.0 * 1024.0);
    /// println!("Current memory usage: {:.2} MB", mb);
    /// ```
    pub fn memory_usage(&self) -> Result<usize> {
        let usage = unsafe { super::mlx_memory_usage() };
        self.tracker.record_memory(usage);
        Ok(usage)
    }

    /// Get the number of tracked allocations
    ///
    /// Useful for debugging and profiling to understand allocation patterns
    /// and detect potential memory leaks or fragmentation.
    ///
    /// # Returns
    /// Number of currently active allocations
    ///
    /// # Example
    /// ```ignore
    /// let count = manager.allocation_count()?;
    /// println!("Active allocations: {}", count);
    /// ```
    pub fn allocation_count(&self) -> Result<usize> {
        let count = unsafe { super::mlx_allocation_count() };
        Ok(count)
    }

    /// Get detailed memory statistics
    ///
    /// Returns a comprehensive snapshot of current memory usage and allocation patterns.
    ///
    /// # Returns
    /// Tuple of (total_bytes, allocation_count)
    ///
    /// # Example
    /// ```ignore
    /// let stats = manager.memory_stats()?;
    /// println!("Total: {} bytes, Allocations: {}", stats.total_bytes, stats.allocation_count);
    /// ```
    pub fn memory_stats(&self) -> Result<MemoryManagementStats> {
        let current_usage = self.memory_usage()?;
        let allocation_count = self.allocation_count()?;

        Ok(MemoryManagementStats {
            total_bytes: current_usage,
            allocation_count,
            peak_bytes: self.tracker.peak_memory(),
        })
    }

    /// Check if memory usage exceeds a threshold and trigger GC if needed
    ///
    /// # Arguments
    /// * `threshold_mb` - Memory threshold in megabytes
    ///
    /// # Returns
    /// true if GC was triggered (memory exceeded threshold)
    ///
    /// # Example
    /// ```ignore
    /// if manager.check_and_gc(2048.0)? {
    ///     tracing::warn!("Memory pressure detected, GC triggered");
    /// }
    /// ```
    pub fn check_and_gc(&self, threshold_mb: f32) -> Result<bool> {
        let usage = self.memory_usage()?;
        let current_mb = usage as f32 / (1024.0 * 1024.0);

        if current_mb > threshold_mb {
            warn!(
                current_mb = current_mb,
                threshold_mb = threshold_mb,
                "Memory threshold exceeded, triggering GC"
            );
            self.gc_collect()?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Synchronize GPU operations
    ///
    /// Forces evaluation of all pending lazy computations on the GPU.
    /// This ensures that:
    /// 1. All queued operations are executed
    /// 2. Memory is committed and visible
    /// 3. Accurate memory measurements can be taken
    ///
    /// # Note
    /// This is a blocking operation and can be expensive. Use sparingly
    /// or only when synchronization is required for correctness.
    ///
    /// # Example
    /// ```ignore
    /// manager.synchronize()?;
    /// let stats = manager.memory_stats()?;
    /// ```
    pub fn synchronize(&self) -> Result<()> {
        debug!("Synchronizing MLX GPU operations");

        unsafe {
            // Call C FFI to synchronize operations
            super::mlx_synchronize();
        }

        debug!("MLX GPU operations synchronized");
        Ok(())
    }

    /// Reset memory tracking
    ///
    /// Clears all tracked allocations and resets memory counters to zero.
    /// Used for testing and debugging purposes only.
    ///
    /// # Safety
    /// This should only be called when no MLX operations are in progress.
    /// Resetting tracking while operations are ongoing will cause
    /// inconsistent statistics.
    ///
    /// # Example
    /// ```ignore
    /// manager.reset();
    /// // ... perform operations ...
    /// let stats = manager.memory_stats()?;
    /// ```
    pub fn reset(&self) -> Result<()> {
        debug!("Resetting MLX memory tracking");

        unsafe {
            // Call C FFI to reset tracking
            super::mlx_memory_reset();
        }

        self.tracker.reset();

        debug!("MLX memory tracking reset");
        Ok(())
    }

    /// Get the memory tracker reference
    pub fn tracker(&self) -> &Arc<MemoryTracker> {
        &self.tracker
    }
}

impl Default for MLXMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory management integration with adapteros-memory infrastructure
///
/// Bridges MLX memory operations with the unified memory management system
pub mod integration {
    use super::*;

    /// Convert MLX stats to unified memory format
    ///
    /// Transforms MLX-specific memory statistics into the format expected by
    /// adapteros-memory infrastructure for unified monitoring and reporting
    pub fn mlx_stats_to_unified(stats: &MemoryManagementStats) -> (u64, usize) {
        (stats.total_bytes as u64, stats.allocation_count)
    }

    /// Check memory pressure and recommend cleanup
    ///
    /// Analyzes current memory usage and returns recommendations for cleanup
    pub fn analyze_memory_pressure(
        stats: &MemoryManagementStats,
        available_memory_mb: usize,
    ) -> MemoryPressureRecommendation {
        let current_mb = stats.total_mb() as usize;
        let utilization = current_mb as f32 / available_memory_mb as f32;

        if utilization > 0.90 {
            MemoryPressureRecommendation::Critical {
                current_mb,
                available_mb: available_memory_mb,
            }
        } else if utilization > 0.75 {
            MemoryPressureRecommendation::High {
                current_mb,
                available_mb: available_memory_mb,
            }
        } else if utilization > 0.60 {
            MemoryPressureRecommendation::Moderate {
                current_mb,
                available_mb: available_memory_mb,
            }
        } else {
            MemoryPressureRecommendation::Normal {
                current_mb,
                available_mb: available_memory_mb,
            }
        }
    }

    /// Memory pressure recommendation
    #[derive(Debug, Clone)]
    pub enum MemoryPressureRecommendation {
        /// Normal operation, no action needed
        Normal {
            current_mb: usize,
            available_mb: usize,
        },
        /// Moderate pressure, consider GC or adapter eviction
        Moderate {
            current_mb: usize,
            available_mb: usize,
        },
        /// High pressure, GC and adapter unloading recommended
        High {
            current_mb: usize,
            available_mb: usize,
        },
        /// Critical pressure, immediate action required
        Critical {
            current_mb: usize,
            available_mb: usize,
        },
    }

    impl MemoryPressureRecommendation {
        /// Check if immediate action is needed
        pub fn requires_immediate_action(&self) -> bool {
            matches!(self, Self::Critical { .. } | Self::High { .. })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tracker_peak() {
        let tracker = MemoryTracker::default();

        tracker.record_memory(1024);
        assert_eq!(tracker.peak_memory(), 1024);

        tracker.record_memory(512);
        assert_eq!(tracker.peak_memory(), 1024); // Peak doesn't decrease

        tracker.record_memory(2048);
        assert_eq!(tracker.peak_memory(), 2048);
    }

    #[test]
    fn test_memory_tracker_gc_count() {
        let tracker = MemoryTracker::default();

        assert_eq!(tracker.collection_count(), 0);
        tracker.record_gc();
        assert_eq!(tracker.collection_count(), 1);
        tracker.record_gc();
        assert_eq!(tracker.collection_count(), 2);
    }

    #[test]
    fn test_memory_stats_mb_conversion() {
        let stats = MemoryManagementStats {
            total_bytes: 1024 * 1024,
            allocation_count: 42,
            peak_bytes: 2 * 1024 * 1024,
        };

        assert_eq!(stats.total_mb(), 1.0);
        assert_eq!(stats.peak_mb(), 2.0);
    }

    #[test]
    fn test_memory_stats_threshold() {
        let stats = MemoryManagementStats {
            total_bytes: 3 * 1024 * 1024,
            allocation_count: 10,
            peak_bytes: 3 * 1024 * 1024,
        };

        assert!(stats.exceeds_mb_threshold(2.0));
        assert!(!stats.exceeds_mb_threshold(4.0));
    }

    #[test]
    fn test_memory_manager_creation() {
        let manager = MLXMemoryManager::new();
        let tracker = manager.tracker();
        assert_eq!(tracker.peak_memory(), 0);
        assert_eq!(tracker.collection_count(), 0);
    }

    #[test]
    fn test_pressure_recommendation_critical() {
        let stats = MemoryManagementStats {
            total_bytes: 9 * 1024 * 1024,
            allocation_count: 10,
            peak_bytes: 9 * 1024 * 1024,
        };

        let rec = integration::analyze_memory_pressure(&stats, 10);
        assert!(rec.requires_immediate_action());
    }

    #[test]
    fn test_pressure_recommendation_normal() {
        let stats = MemoryManagementStats {
            total_bytes: 2 * 1024 * 1024,
            allocation_count: 10,
            peak_bytes: 2 * 1024 * 1024,
        };

        let rec = integration::analyze_memory_pressure(&stats, 10);
        assert!(!rec.requires_immediate_action());
    }
}
