//! Rust FFI bindings for Metal heap observer callbacks
//!
//! Provides safe Rust wrappers for the Objective-C++ callback-based
//! Metal heap observer system. Manages allocation tracking, statistics
//! collection, and performance monitoring.
//!
//! Thread-safe callback registration and invocation through FFI.

use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn, error};
use crate::Result;

// MARK: - FFI Structures

/// Heap statistics snapshot (must match C struct)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    pub current_used_bytes: u64,
    pub peak_used_bytes: u64,
    pub total_heap_size: u64,
    pub current_allocation_count: u64,
    pub peak_allocation_count: u64,
    pub total_allocations_lifetime: u64,
    pub total_deallocations_lifetime: u64,
    pub fragmentation_ratio: f32,
    pub page_fault_rate: f32,
    pub last_update_us: u64,
}

// MARK: - Callback Types

pub type AllocationSuccessCallback = extern "C" fn(u64, u64, u64, u64);
pub type AllocationFailureCallback = extern "C" fn(u64, u64, i32);
pub type DeallocationCallback = extern "C" fn(u64, u64, u64, u64);
pub type CompactionCallback = extern "C" fn(u64, u64, u32);
pub type MemoryPressureCallback = extern "C" fn(i32, u64);

// MARK: - FFI Declarations

#[cfg(target_os = "macos")]
extern "C" {
    // Callback registration
    pub fn metal_heap_set_allocation_success_callback(callback: Option<AllocationSuccessCallback>);
    pub fn metal_heap_set_allocation_failure_callback(callback: Option<AllocationFailureCallback>);
    pub fn metal_heap_set_deallocation_callback(callback: Option<DeallocationCallback>);
    pub fn metal_heap_set_compaction_callback(callback: Option<CompactionCallback>);
    pub fn metal_heap_set_memory_pressure_callback(callback: Option<MemoryPressureCallback>);

    // Allocation tracking
    pub fn metal_heap_record_allocation(
        heap_id: u64,
        buffer_id: u64,
        size_bytes: u64,
        offset_bytes: u64,
        memory_addr: u64,
        storage_mode: u32,
    ) -> i32;

    pub fn metal_heap_record_deallocation(heap_id: u64, buffer_id: u64) -> i32;

    pub fn metal_heap_record_allocation_failure(
        heap_id: u64,
        requested_size: u64,
        error_code: i32,
    ) -> i32;

    // Heap compaction
    pub fn metal_heap_record_compaction(
        heap_id: u64,
        bytes_recovered: u64,
        bytes_moved: u64,
        blocks_compacted: u32,
    ) -> i32;

    // Memory pressure
    pub fn metal_heap_on_memory_pressure(pressure_level: i32, available_bytes: u64) -> i32;

    // Statistics
    pub fn metal_heap_get_stats(heap_id: u64, out_stats: *mut HeapStats) -> i32;
    pub fn metal_heap_get_global_stats(out_stats: *mut HeapStats) -> i32;

    // Performance counters
    pub fn metal_heap_get_allocation_count() -> u64;
    pub fn metal_heap_get_deallocation_count() -> u64;
    pub fn metal_heap_get_failed_allocations() -> u64;
    pub fn metal_heap_get_compaction_count() -> u64;
    pub fn metal_heap_get_allocation_rate_per_second() -> f32;
    pub fn metal_heap_get_fragmentation_percentage(heap_id: u64) -> f32;
    pub fn metal_heap_get_page_fault_count() -> u64;
    pub fn metal_heap_record_page_fault(heap_id: u64);

    // Error handling
    pub fn metal_heap_get_last_error() -> *const i8;

    // Maintenance
    pub fn metal_heap_clear_stats();
}

// MARK: - Stub Implementations (non-macOS)

#[cfg(not(target_os = "macos"))]
pub unsafe fn metal_heap_set_allocation_success_callback(_callback: Option<AllocationSuccessCallback>) {}

#[cfg(not(target_os = "macos"))]
pub unsafe fn metal_heap_set_allocation_failure_callback(_callback: Option<AllocationFailureCallback>) {}

#[cfg(not(target_os = "macos"))]
pub unsafe fn metal_heap_set_deallocation_callback(_callback: Option<DeallocationCallback>) {}

#[cfg(not(target_os = "macos"))]
pub unsafe fn metal_heap_set_compaction_callback(_callback: Option<CompactionCallback>) {}

#[cfg(not(target_os = "macos"))]
pub unsafe fn metal_heap_set_memory_pressure_callback(_callback: Option<MemoryPressureCallback>) {}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_record_allocation(_heap_id: u64, _buffer_id: u64, _size_bytes: u64,
                                    _offset_bytes: u64, _memory_addr: u64, _storage_mode: u32) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_record_deallocation(_heap_id: u64, _buffer_id: u64) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_record_allocation_failure(_heap_id: u64, _requested_size: u64, _error_code: i32) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_record_compaction(_heap_id: u64, _bytes_recovered: u64,
                                   _bytes_moved: u64, _blocks_compacted: u32) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_on_memory_pressure(_pressure_level: i32, _available_bytes: u64) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_stats(_heap_id: u64, _out_stats: *mut HeapStats) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_global_stats(_out_stats: *mut HeapStats) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_allocation_count() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_deallocation_count() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_failed_allocations() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_compaction_count() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_allocation_rate_per_second() -> f32 {
    0.0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_fragmentation_percentage(_heap_id: u64) -> f32 {
    0.0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_page_fault_count() -> u64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_record_page_fault(_heap_id: u64) {}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_get_last_error() -> *const i8 {
    std::ptr::null()
}

#[cfg(not(target_os = "macos"))]
pub fn metal_heap_clear_stats() {}

// MARK: - High-Level API

/// Heap observer callback manager
pub struct HeapObserverCallbackManager {
    allocation_success_handlers: Arc<Mutex<Vec<Box<dyn Fn(u64, u64, u64, u64) + Send>>>>,
    deallocation_handlers: Arc<Mutex<Vec<Box<dyn Fn(u64, u64, u64, u64) + Send>>>>,
    compaction_handlers: Arc<Mutex<Vec<Box<dyn Fn(u64, u64, u32) + Send>>>>,
}

impl HeapObserverCallbackManager {
    /// Create a new callback manager
    pub fn new() -> Self {
        Self {
            allocation_success_handlers: Arc::new(Mutex::new(Vec::new())),
            deallocation_handlers: Arc::new(Mutex::new(Vec::new())),
            compaction_handlers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register allocation success handler
    pub fn on_allocation_success<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(u64, u64, u64, u64) + Send + 'static,
    {
        let mut handlers = self.allocation_success_handlers.lock()
            .map_err(|e| adapteros_core::AosError::Config(format!("Failed to lock handlers: {}", e)))?;
        handlers.push(Box::new(handler));
        debug!("Registered allocation success handler");
        Ok(())
    }

    /// Register deallocation handler
    pub fn on_deallocation<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(u64, u64, u64, u64) + Send + 'static,
    {
        let mut handlers = self.deallocation_handlers.lock()
            .map_err(|e| adapteros_core::AosError::Config(format!("Failed to lock handlers: {}", e)))?;
        handlers.push(Box::new(handler));
        debug!("Registered deallocation handler");
        Ok(())
    }

    /// Register compaction handler
    pub fn on_compaction<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(u64, u64, u32) + Send + 'static,
    {
        let mut handlers = self.compaction_handlers.lock()
            .map_err(|e| adapteros_core::AosError::Config(format!("Failed to lock handlers: {}", e)))?;
        handlers.push(Box::new(handler));
        debug!("Registered compaction handler");
        Ok(())
    }

    /// Invoke allocation success handlers
    pub fn invoke_allocation_success(&self, heap_id: u64, buffer_id: u64, size_bytes: u64, timestamp_us: u64) {
        if let Ok(handlers) = self.allocation_success_handlers.lock() {
            for handler in handlers.iter() {
                handler(heap_id, buffer_id, size_bytes, timestamp_us);
            }
        }
    }

    /// Invoke deallocation handlers
    pub fn invoke_deallocation(&self, heap_id: u64, buffer_id: u64, size_bytes: u64, timestamp_us: u64) {
        if let Ok(handlers) = self.deallocation_handlers.lock() {
            for handler in handlers.iter() {
                handler(heap_id, buffer_id, size_bytes, timestamp_us);
            }
        }
    }

    /// Invoke compaction handlers
    pub fn invoke_compaction(&self, heap_id: u64, bytes_recovered: u64, blocks_compacted: u32) {
        if let Ok(handlers) = self.compaction_handlers.lock() {
            for handler in handlers.iter() {
                handler(heap_id, bytes_recovered, blocks_compacted);
            }
        }
    }
}

impl Default for HeapObserverCallbackManager {
    fn default() -> Self {
        Self::new()
    }
}

// MARK: - Global Performance Statistics

/// Performance metrics snapshot
#[derive(Debug, Clone, Copy)]
pub struct PerformanceMetrics {
    pub allocation_count: u64,
    pub deallocation_count: u64,
    pub failed_allocations: u64,
    pub compaction_count: u64,
    pub allocation_rate_per_second: f32,
    pub page_fault_count: u64,
}

impl PerformanceMetrics {
    /// Collect current performance metrics
    pub fn collect() -> Self {
        Self {
            allocation_count: unsafe { metal_heap_get_allocation_count() },
            deallocation_count: unsafe { metal_heap_get_deallocation_count() },
            failed_allocations: unsafe { metal_heap_get_failed_allocations() },
            compaction_count: unsafe { metal_heap_get_compaction_count() },
            allocation_rate_per_second: unsafe { metal_heap_get_allocation_rate_per_second() },
            page_fault_count: unsafe { metal_heap_get_page_fault_count() },
        }
    }

    /// Calculate success rate percentage
    pub fn allocation_success_rate(&self) -> f32 {
        let total = self.allocation_count.wrapping_add(self.failed_allocations);
        if total == 0 {
            100.0
        } else {
            (self.allocation_count as f32 / total as f32) * 100.0
        }
    }

    /// Calculate net allocations (allocations - deallocations)
    pub fn net_allocations(&self) -> i64 {
        self.allocation_count as i64 - self.deallocation_count as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_metrics_collection() {
        let metrics = PerformanceMetrics::collect();
        assert!(metrics.allocation_rate_per_second >= 0.0);
        assert!(metrics.allocation_success_rate() <= 100.0);
    }

    #[test]
    fn test_callback_manager_creation() {
        let manager = HeapObserverCallbackManager::new();
        assert!(manager.on_allocation_success(|_, _, _, _| {}).is_ok());
    }

    #[test]
    fn test_net_allocations() {
        let metrics = PerformanceMetrics {
            allocation_count: 100,
            deallocation_count: 30,
            failed_allocations: 5,
            compaction_count: 2,
            allocation_rate_per_second: 10.5,
            page_fault_count: 0,
        };

        assert_eq!(metrics.net_allocations(), 70);
    }

    #[test]
    fn test_allocation_success_rate() {
        let metrics = PerformanceMetrics {
            allocation_count: 90,
            deallocation_count: 30,
            failed_allocations: 10,
            compaction_count: 2,
            allocation_rate_per_second: 10.5,
            page_fault_count: 0,
        };

        let rate = metrics.allocation_success_rate();
        assert!(rate > 85.0 && rate < 95.0);
    }
}
