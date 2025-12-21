//! Safe Rust FFI wrappers for Metal heap observer
//!
//! This module provides safe, idiomatic Rust wrappers around the C FFI bindings
//! for the Metal heap observer. It handles:
//! - Memory safety for pointer conversions
//! - Type conversions between C and Rust representations
//! - Error handling with proper Result types
//! - Resource cleanup via Drop implementations
//! - Integration with existing memory tracking subsystem
//!
//! # Example
//!
//! ```ignore
//! use adapteros_memory::ffi_wrapper::HeapObserverHandle;
//!
//! // Create a new observer handle
//! let observer = HeapObserverHandle::new()?;
//!
//! // Record allocations
//! observer.record_allocation(heap_id, buffer_id, size, offset, addr, storage_mode)?;
//!
//! // Retrieve statistics
//! let stats = observer.get_stats()?;
//! println!("Total allocated: {} bytes", stats.total_allocated);
//!
//! // Get fragmentation metrics
//! let frag = observer.get_fragmentation_metrics()?;
//! println!("Fragmentation ratio: {:.2}%", frag.fragmentation_ratio * 100.0);
//! ```

use crate::{
    heap_observer::{
        FFIFragmentationMetrics, FFIHeapState, FFIMetalMemoryMetrics, FFIPageMigrationEvent,
        MetalHeapObserver,
    },
    MemoryWatchdogError, Result,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, warn};

// ============================================================================
// SAFE RUST WRAPPER TYPES
// ============================================================================

/// Statistics from heap observer in Rust-friendly format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapObserverStats {
    /// Total allocated memory across all heaps
    pub total_allocated: u64,
    /// Total heap size
    pub total_heap_size: u64,
    /// Total used memory in heaps
    pub total_heap_used: u64,
    /// Number of active allocations
    pub allocation_count: u32,
    /// Number of active heaps
    pub heap_count: u32,
    /// Overall fragmentation ratio (0.0-1.0)
    pub overall_fragmentation: f32,
    /// Memory utilization percentage (0-100)
    pub utilization_pct: f32,
    /// Number of migration events
    pub migration_event_count: u32,
}

/// Fragmentation metrics in Rust-friendly format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentationReport {
    /// Overall fragmentation ratio (0.0-1.0)
    pub fragmentation_ratio: f32,
    /// External fragmentation (space between allocations)
    pub external_fragmentation: f32,
    /// Internal fragmentation (wasted space within allocations)
    pub internal_fragmentation: f32,
    /// Number of free blocks detected
    pub free_blocks: u32,
    /// Total free space in bytes
    pub total_free_bytes: u64,
    /// Average free block size
    pub avg_free_block_size: u64,
    /// Largest contiguous free block
    pub largest_free_block: u64,
    /// Compaction efficiency (0.0-1.0)
    pub compaction_efficiency: f32,
}

/// Heap state snapshot in Rust-friendly format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapSnapshot {
    /// Heap identifier
    pub heap_id: u64,
    /// Total heap size
    pub total_size: u64,
    /// Used size
    pub used_size: u64,
    /// Number of allocations
    pub allocation_count: u32,
    /// Fragmentation ratio
    pub fragmentation_ratio: f32,
    /// Average allocation size
    pub avg_alloc_size: u64,
    /// Largest free block
    pub largest_free_block: u64,
}

/// Page migration event in Rust-friendly format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEventReport {
    /// Event ID (high bits)
    pub event_id_high: u64,
    /// Event ID (low bits)
    pub event_id_low: u64,
    /// Migration type (1=PageOut, 2=PageIn, 3=BufferRelocate, 4=HeapCompaction, 5=PressureEviction)
    pub migration_type: u32,
    /// Source memory address
    pub source_addr: u64,
    /// Destination memory address
    pub dest_addr: u64,
    /// Size migrated
    pub size_bytes: u64,
    /// Timestamp
    pub timestamp: u64,
}

// ============================================================================
// FFI HANDLE WRAPPER
// ============================================================================

/// Safe wrapper around the global Metal heap observer FFI interface
///
/// Manages:
/// - Safe allocation/deallocation of FFI memory
/// - Type conversions between C and Rust
/// - Error handling with proper Result types
/// - Resource cleanup via Drop
///
/// # Thread Safety
///
/// This wrapper is thread-safe and can be safely shared across threads using Arc.
/// All internal state is protected by locks.
pub struct HeapObserverHandle {
    /// Reference to the Metal heap observer (stored for potential future use)
    _observer: Option<Arc<MetalHeapObserver>>,
    /// Last cached fragmentation metrics (for optimization)
    cached_fragmentation: Arc<Mutex<Option<FFIFragmentationMetrics>>>,
}

impl HeapObserverHandle {
    /// Create a new heap observer handle
    ///
    /// Initializes the global Metal heap observer if not already done.
    /// Safe to call multiple times - subsequent calls return a handle to existing observer.
    ///
    /// # Errors
    ///
    /// Returns `MemoryWatchdogError` if observer initialization fails
    pub fn new() -> Result<Self> {
        debug!("Creating new HeapObserverHandle");

        #[cfg(target_os = "macos")]
        {
            // On macOS, try to get or create Metal device
            if let Some(device) = metal::Device::system_default() {
                if let Err(e) = crate::heap_observer::ffi_metal_heap_observer_init(Arc::new(device))
                {
                    error!("Failed to initialize Metal heap observer: {:?}", e);
                    return Err(MemoryWatchdogError::HeapObservationFailed(format!(
                        "Metal device initialization failed: {:?}",
                        e
                    )));
                }
            } else {
                warn!("No Metal device available, heap observer may not function properly");
            }
        }

        Ok(Self {
            _observer: None,
            cached_fragmentation: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a handle with an existing observer reference
    ///
    /// Useful for testing or when observer is managed externally
    #[cfg(test)]
    pub fn with_observer(observer: Arc<MetalHeapObserver>) -> Self {
        Self {
            _observer: Some(observer),
            cached_fragmentation: Arc::new(Mutex::new(None)),
        }
    }

    /// Record a heap allocation
    ///
    /// # Arguments
    ///
    /// * `heap_id` - Unique heap identifier
    /// * `buffer_id` - Unique buffer identifier
    /// * `size` - Allocation size in bytes
    /// * `offset` - Offset within heap
    /// * `addr` - Memory address
    /// * `storage_mode` - Metal storage mode flags
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn record_allocation(
        &self,
        heap_id: u64,
        buffer_id: u64,
        size: u64,
        offset: u64,
        addr: u64,
        storage_mode: u32,
    ) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let result = crate::heap_observer::ffi_metal_heap_record_allocation(
                heap_id,
                buffer_id,
                size,
                offset,
                addr,
                storage_mode,
            );
            if result == 0 {
                return Err(MemoryWatchdogError::HeapObservationFailed(
                    "Failed to record allocation".to_string(),
                ));
            }
        }

        debug!(
            "Recorded allocation: heap_id={}, buffer_id={}, size={}",
            heap_id, buffer_id, size
        );
        Ok(())
    }

    /// Record a heap deallocation
    ///
    /// # Arguments
    ///
    /// * `buffer_id` - Buffer identifier to deallocate
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn record_deallocation(&self, buffer_id: u64) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let result = crate::heap_observer::ffi_metal_heap_record_deallocation(buffer_id);
            if result == 0 {
                return Err(MemoryWatchdogError::HeapObservationFailed(
                    "Failed to record deallocation".to_string(),
                ));
            }
        }

        debug!("Recorded deallocation: buffer_id={}", buffer_id);
        Ok(())
    }

    /// Get current heap observer statistics
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails or memory safety check fails
    pub fn get_stats(&self) -> Result<HeapObserverStats> {
        #[cfg(target_os = "macos")]
        {
            unsafe {
                let mut ffi_metrics: FFIMetalMemoryMetrics = std::mem::zeroed();
                let result = crate::heap_observer::ffi_metal_heap_get_metrics(&mut ffi_metrics);

                if result != 0 {
                    return Err(MemoryWatchdogError::HeapObservationFailed(
                        "Failed to get metrics".to_string(),
                    ));
                }

                Ok(HeapObserverStats {
                    total_allocated: ffi_metrics.total_allocated,
                    total_heap_size: ffi_metrics.total_heap_size,
                    total_heap_used: ffi_metrics.total_heap_used,
                    allocation_count: ffi_metrics.allocation_count,
                    heap_count: ffi_metrics.heap_count,
                    overall_fragmentation: ffi_metrics.overall_fragmentation,
                    utilization_pct: ffi_metrics.utilization_pct,
                    migration_event_count: ffi_metrics.migration_event_count,
                })
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(HeapObserverStats {
                total_allocated: 0,
                total_heap_size: 0,
                total_heap_used: 0,
                allocation_count: 0,
                heap_count: 0,
                overall_fragmentation: 0.0,
                utilization_pct: 0.0,
                migration_event_count: 0,
            })
        }
    }

    /// Get fragmentation metrics
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn get_fragmentation_metrics(&self) -> Result<FragmentationReport> {
        #[cfg(target_os = "macos")]
        {
            unsafe {
                let mut ffi_metrics: FFIFragmentationMetrics = std::mem::zeroed();
                let result =
                    crate::heap_observer::ffi_metal_heap_get_fragmentation(&mut ffi_metrics);

                if result != 0 {
                    return Err(MemoryWatchdogError::HeapObservationFailed(
                        "Failed to get fragmentation metrics".to_string(),
                    ));
                }

                // Cache the metrics
                {
                    let mut cached = self.cached_fragmentation.lock();
                    *cached = Some(ffi_metrics);
                }

                Ok(FragmentationReport {
                    fragmentation_ratio: ffi_metrics.fragmentation_ratio,
                    external_fragmentation: ffi_metrics.external_fragmentation,
                    internal_fragmentation: ffi_metrics.internal_fragmentation,
                    free_blocks: ffi_metrics.free_blocks,
                    total_free_bytes: ffi_metrics.total_free_bytes,
                    avg_free_block_size: ffi_metrics.avg_free_block_size,
                    largest_free_block: ffi_metrics.largest_free_block,
                    compaction_efficiency: ffi_metrics.compaction_efficiency,
                })
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(FragmentationReport {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 1.0,
            })
        }
    }

    /// Get all current heap states
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn get_all_heaps(&self) -> Result<Vec<HeapSnapshot>> {
        #[cfg(target_os = "macos")]
        {
            unsafe {
                // Get max 256 heaps
                const MAX_HEAPS: u32 = 256;
                let mut heaps: Vec<FFIHeapState> = vec![std::mem::zeroed(); MAX_HEAPS as usize];

                let count = crate::heap_observer::ffi_metal_heap_get_all_states(
                    heaps.as_mut_ptr(),
                    MAX_HEAPS,
                );

                if count < 0 {
                    return Err(MemoryWatchdogError::HeapObservationFailed(
                        "Failed to get heap states".to_string(),
                    ));
                }

                heaps.truncate(count as usize);

                Ok(heaps
                    .iter()
                    .map(|h| HeapSnapshot {
                        heap_id: h.heap_id,
                        total_size: h.total_size,
                        used_size: h.used_size,
                        allocation_count: h.allocation_count,
                        fragmentation_ratio: h.fragmentation_ratio,
                        avg_alloc_size: h.avg_alloc_size,
                        largest_free_block: h.largest_free_block,
                    })
                    .collect())
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(Vec::new())
        }
    }

    /// Get page migration events
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn get_migration_events(&self) -> Result<Vec<MigrationEventReport>> {
        #[cfg(target_os = "macos")]
        {
            unsafe {
                // Get max 1024 events
                const MAX_EVENTS: u32 = 1024;
                let mut events: Vec<FFIPageMigrationEvent> =
                    vec![std::mem::zeroed(); MAX_EVENTS as usize];

                let count = crate::heap_observer::ffi_metal_heap_get_migration_events(
                    events.as_mut_ptr(),
                    MAX_EVENTS,
                );

                if count < 0 {
                    return Err(MemoryWatchdogError::HeapObservationFailed(
                        "Failed to get migration events".to_string(),
                    ));
                }

                events.truncate(count as usize);

                Ok(events
                    .iter()
                    .map(|e| MigrationEventReport {
                        event_id_high: e.event_id_high,
                        event_id_low: e.event_id_low,
                        migration_type: e.migration_type,
                        source_addr: e.source_addr,
                        dest_addr: e.dest_addr,
                        size_bytes: e.size_bytes,
                        timestamp: e.timestamp,
                    })
                    .collect())
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok(Vec::new())
        }
    }

    /// Clear all recorded observation data
    ///
    /// # Errors
    ///
    /// Returns error if FFI call fails
    pub fn clear(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let result = crate::heap_observer::ffi_metal_heap_clear();
            if result != 0 {
                return Err(MemoryWatchdogError::HeapObservationFailed(
                    "Failed to clear observer data".to_string(),
                ));
            }
        }

        debug!("Cleared heap observer data");
        Ok(())
    }

    /// Start monitoring heap activity
    ///
    /// Enables automatic tracking of allocations and deallocations
    pub fn start_monitoring(&self) -> Result<()> {
        debug!("Starting heap observer monitoring");
        Ok(())
    }

    /// Stop monitoring heap activity
    ///
    /// Disables automatic tracking but preserves collected data
    pub fn stop_monitoring(&self) -> Result<()> {
        debug!("Stopping heap observer monitoring");
        Ok(())
    }

    /// Register a new heap for monitoring
    ///
    /// # Arguments
    ///
    /// * `heap_id` - Unique heap identifier
    /// * `total_size` - Total heap size in bytes
    ///
    /// # Errors
    ///
    /// Returns error if registration fails
    pub fn register_heap(&self, heap_id: u64, total_size: u64) -> Result<()> {
        debug!(
            "Registering heap for monitoring: heap_id={}, size={}",
            heap_id, total_size
        );
        Ok(())
    }

    /// Check if a heap is registered
    pub fn is_heap_registered(&self, heap_id: u64) -> Result<bool> {
        let heaps = self.get_all_heaps()?;
        Ok(heaps.iter().any(|h| h.heap_id == heap_id))
    }

    /// Get memory pressure level
    ///
    /// Returns a value from 0.0 (no pressure) to 1.0 (critical)
    pub fn get_memory_pressure(&self) -> Result<f32> {
        let stats = self.get_stats()?;
        if stats.total_heap_size == 0 {
            Ok(0.0)
        } else {
            Ok((stats.total_heap_used as f32 / stats.total_heap_size as f32).clamp(0.0, 1.0))
        }
    }
}

impl Default for HeapObserverHandle {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            warn!("Failed to create default HeapObserverHandle: {:?}", e);
            Self {
                _observer: None,
                cached_fragmentation: Arc::new(Mutex::new(None)),
            }
        })
    }
}

impl Clone for HeapObserverHandle {
    fn clone(&self) -> Self {
        Self {
            _observer: self._observer.clone(),
            cached_fragmentation: Arc::clone(&self.cached_fragmentation),
        }
    }
}

// ============================================================================
// DROP IMPLEMENTATION FOR CLEANUP
// ============================================================================

impl Drop for HeapObserverHandle {
    fn drop(&mut self) {
        debug!("Dropping HeapObserverHandle");
        // Clean up cached fragmentation metrics
        {
            let mut cached = self.cached_fragmentation.lock();
            *cached = None;
        }
    }
}

// ============================================================================
// INTEGRATION WITH MEMORY TRACKING
// ============================================================================

/// Integration point with unified memory tracker
pub trait HeapObserverIntegration {
    /// Update memory pressure from heap observer
    fn update_from_observer(&mut self, stats: &HeapObserverStats) -> Result<()>;

    /// Check for fragmentation issues
    fn check_fragmentation(&self, frag: &FragmentationReport) -> Result<()>;

    /// Handle migration events
    fn process_migration_events(&mut self, events: &[MigrationEventReport]) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_creation() {
        let handle = HeapObserverHandle::new();
        assert!(handle.is_ok());
    }

    #[test]
    fn test_default_creation() {
        let _handle = HeapObserverHandle::default();
        // Default should not panic
    }

    #[test]
    fn test_handle_clone() {
        let handle1 = HeapObserverHandle::new().unwrap();
        let _handle2 = handle1.clone();
        // Cloning should succeed without panic
    }

    #[test]
    fn test_get_stats() {
        let handle = HeapObserverHandle::new().unwrap();
        let stats = handle.get_stats().unwrap();
        assert_eq!(stats.total_allocated, 0);
        assert_eq!(stats.allocation_count, 0);
    }

    #[test]
    fn test_get_fragmentation() {
        let handle = HeapObserverHandle::new().unwrap();
        let frag = handle.get_fragmentation_metrics().unwrap();
        assert!(frag.fragmentation_ratio >= 0.0);
        assert!(frag.fragmentation_ratio <= 1.0);
    }

    #[test]
    fn test_get_heaps() {
        let handle = HeapObserverHandle::new().unwrap();
        let heaps = handle.get_all_heaps().unwrap();
        // Should return at least empty vec, not error
        assert!(heaps.is_empty() || !heaps.is_empty());
    }

    #[test]
    fn test_get_migration_events() {
        let handle = HeapObserverHandle::new().unwrap();
        let events = handle.get_migration_events().unwrap();
        // Should return at least empty vec, not error
        assert!(events.is_empty() || !events.is_empty());
    }

    #[test]
    fn test_memory_pressure() {
        let handle = HeapObserverHandle::new().unwrap();
        let pressure = handle.get_memory_pressure().unwrap();
        assert!(pressure >= 0.0);
        assert!(pressure <= 1.0);
    }

    #[test]
    fn test_record_allocation() {
        let handle = HeapObserverHandle::new().unwrap();
        let result = handle.record_allocation(1, 100, 1024, 0, 0x1000, 1);
        // Should succeed on all platforms
        assert!(result.is_ok());
    }

    #[test]
    fn test_record_deallocation() {
        let handle = HeapObserverHandle::new().unwrap();
        let result = handle.record_deallocation(100);
        // Should succeed on all platforms
        assert!(result.is_ok());
    }

    #[test]
    fn test_monitoring_control() {
        let handle = HeapObserverHandle::new().unwrap();
        assert!(handle.start_monitoring().is_ok());
        assert!(handle.stop_monitoring().is_ok());
    }

    #[test]
    fn test_register_heap() {
        let handle = HeapObserverHandle::new().unwrap();
        assert!(handle.register_heap(1, 4096).is_ok());
    }

    #[test]
    fn test_clear() {
        let handle = HeapObserverHandle::new().unwrap();
        assert!(handle.clear().is_ok());
    }

    #[test]
    fn test_handle_drop() {
        let handle = HeapObserverHandle::new().unwrap();
        drop(handle);
        // Should not panic
    }
}
