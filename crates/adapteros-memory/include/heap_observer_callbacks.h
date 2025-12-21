//! Metal heap observer callbacks header
//!
//! FFI declarations for MTLHeap callback-based event system
//! Provides C-compatible interface for allocation tracking, statistics,
//! and performance monitoring with thread-safe callback invocation.
//!
//! Compiled: macOS only
//! Header guards prevent multiple inclusion

#ifndef METAL_HEAP_OBSERVER_CALLBACKS_H
#define METAL_HEAP_OBSERVER_CALLBACKS_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// MARK: - Callback Type Definitions

/// Allocation success callback: (heap_id, buffer_id, size_bytes, timestamp_us)
typedef void (*AllocationSuccessCallback)(uint64_t, uint64_t, uint64_t, uint64_t);

/// Allocation failure callback: (heap_id, requested_size, error_code)
typedef void (*AllocationFailureCallback)(uint64_t, uint64_t, int32_t);

/// Deallocation callback: (heap_id, buffer_id, size_bytes, timestamp_us)
typedef void (*DeallocationCallback)(uint64_t, uint64_t, uint64_t, uint64_t);

/// Heap compaction callback: (heap_id, bytes_recovered, blocks_compacted)
typedef void (*CompactionCallback)(uint64_t, uint64_t, uint32_t);

/// Memory pressure callback: (pressure_level, available_bytes)
typedef void (*MemoryPressureCallback)(int32_t, uint64_t);

// MARK: - Data Structures

/// Heap statistics with peak tracking
typedef struct {
    uint64_t current_used_bytes;
    uint64_t peak_used_bytes;
    uint64_t total_heap_size;
    uint64_t current_allocation_count;
    uint64_t peak_allocation_count;
    uint64_t total_allocations_lifetime;
    uint64_t total_deallocations_lifetime;
    float fragmentation_ratio;
    float page_fault_rate;
    uint64_t last_update_us;
} HeapStats;

// MARK: - Callback Registration API

/// Register callback for successful allocations
/// Arguments: heap_id, buffer_id, size_bytes, timestamp_us
void metal_heap_set_allocation_success_callback(AllocationSuccessCallback callback);

/// Register callback for failed allocations
/// Arguments: heap_id, requested_size, error_code
void metal_heap_set_allocation_failure_callback(AllocationFailureCallback callback);

/// Register callback for deallocations
/// Arguments: heap_id, buffer_id, size_bytes, timestamp_us
void metal_heap_set_deallocation_callback(DeallocationCallback callback);

/// Register callback for heap compaction events
/// Arguments: heap_id, bytes_recovered, blocks_compacted
void metal_heap_set_compaction_callback(CompactionCallback callback);

/// Register callback for memory pressure events
/// Arguments: pressure_level, available_bytes
void metal_heap_set_memory_pressure_callback(MemoryPressureCallback callback);

// MARK: - Allocation Tracking API

/// Record successful allocation
/// Returns 0 on success, negative on error
int32_t metal_heap_record_allocation(uint64_t heap_id, uint64_t buffer_id, uint64_t size_bytes,
                                     uint64_t offset_bytes, uint64_t memory_addr, uint32_t storage_mode);

/// Record deallocation
/// Returns 0 on success, negative on error
int32_t metal_heap_record_deallocation(uint64_t heap_id, uint64_t buffer_id);

/// Record allocation failure
/// error_code: Metal allocation error code
/// Returns 0 on success, negative on error
int32_t metal_heap_record_allocation_failure(uint64_t heap_id, uint64_t requested_size, int32_t error_code);

// MARK: - Heap Compaction API

/// Record heap compaction event
/// bytes_recovered: Total bytes freed by compaction
/// bytes_moved: Total bytes moved during compaction
/// blocks_compacted: Number of memory blocks consolidated
/// Returns 0 on success, negative on error
int32_t metal_heap_record_compaction(uint64_t heap_id, uint64_t bytes_recovered,
                                    uint64_t bytes_moved, uint32_t blocks_compacted);

// MARK: - Memory Pressure API

/// Handle memory pressure event
/// pressure_level: Pressure indicator (0=normal, 1=warning, 2=critical)
/// available_bytes: Available memory at the time of pressure
/// Returns 0 on success, negative on error
int32_t metal_heap_on_memory_pressure(int32_t pressure_level, uint64_t available_bytes);

// MARK: - Statistics Collection API

/// Get statistics for specific heap
/// Returns 0 on success, -1 if heap not found
int32_t metal_heap_get_stats(uint64_t heap_id, HeapStats *out_stats);

/// Get aggregated statistics across all heaps
/// Returns 0 on success, negative on error
int32_t metal_heap_get_global_stats(HeapStats *out_stats);

// MARK: - Performance Counters API

/// Get total number of allocations recorded
/// Returns allocation count since observer initialization
uint64_t metal_heap_get_allocation_count(void);

/// Get total number of deallocations recorded
/// Returns deallocation count since observer initialization
uint64_t metal_heap_get_deallocation_count(void);

/// Get number of failed allocations
/// Returns count of allocation failures since observer initialization
uint64_t metal_heap_get_failed_allocations(void);

/// Get number of compaction events
/// Returns compaction event count since observer initialization
uint64_t metal_heap_get_compaction_count(void);

/// Get current allocation rate per second
/// Calculated over a rolling window, may vary based on recent activity
/// Returns allocations per second (floating point)
float metal_heap_get_allocation_rate_per_second(void);

/// Calculate fragmentation percentage for a specific heap
/// Fragmentation = (total_free_bytes / total_heap_size) * 100
/// Returns percentage (0.0 to 100.0)
float metal_heap_get_fragmentation_percentage(uint64_t heap_id);

/// Get total page fault count
/// Page faults indicate memory pressure and potential swapping
/// Returns page fault count since observer initialization
uint64_t metal_heap_get_page_fault_count(void);

/// Record a page fault event
/// Called by system memory pressure handlers
void metal_heap_record_page_fault(uint64_t heap_id);

// MARK: - Error Handling API

/// Get human-readable error message from last failed operation
/// Returns pointer to error string (valid until next operation)
const char *metal_heap_get_last_error(void);

// MARK: - Maintenance API

/// Clear all statistics and recorded data
/// Use with caution as this removes all historical data
void metal_heap_clear_stats(void);

#ifdef __cplusplus
}
#endif

#endif // METAL_HEAP_OBSERVER_CALLBACKS_H
