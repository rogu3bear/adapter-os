/// Metal Heap Observer FFI Header
/// Provides C-compatible interface for Metal heap monitoring
/// Compiled from heap_observer_impl.mm

#ifndef HEAP_OBSERVER_H
#define HEAP_OBSERVER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// FFI-SAFE STRUCTURES FOR C/C++/OBJECTIVE-C INTEROP
// ============================================================================

/// FFI-safe representation of heap allocation info
typedef struct {
    /// Allocation size in bytes
    uint64_t size_bytes;
    /// Allocation offset within heap
    uint64_t offset_bytes;
    /// Memory address (if available)
    uint64_t memory_addr;
    /// Allocation timestamp in microseconds since epoch
    uint64_t timestamp;
    /// Storage mode flags (MTLStorageModeShared=1, MTLStorageModeManaged=2, etc.)
    uint32_t storage_mode;
} FFIHeapAllocation;

/// FFI-safe representation of heap state snapshot
typedef struct {
    /// Heap identifier (pointer as u64)
    uint64_t heap_id;
    /// Total heap size in bytes
    uint64_t total_size;
    /// Used size in bytes
    uint64_t used_size;
    /// Number of active allocations
    uint32_t allocation_count;
    /// Heap fragmentation ratio (0.0-1.0)
    float fragmentation_ratio;
    /// Average allocation size in bytes
    uint64_t avg_alloc_size;
    /// Largest free block in bytes (if known)
    uint64_t largest_free_block;
} FFIHeapState;

/// FFI-safe representation of fragmentation metrics
typedef struct {
    /// Fragmentation ratio (0.0=no fragmentation, 1.0=maximum)
    float fragmentation_ratio;
    /// External fragmentation (wasted space between allocations)
    float external_fragmentation;
    /// Internal fragmentation (wasted space within allocations)
    float internal_fragmentation;
    /// Number of free blocks detected
    uint32_t free_blocks;
    /// Total free space in bytes
    uint64_t total_free_bytes;
    /// Average free block size in bytes
    uint64_t avg_free_block_size;
    /// Largest contiguous free block in bytes
    uint64_t largest_free_block;
    /// Compaction efficiency (0.0-1.0, higher = more efficient)
    float compaction_efficiency;
} FFIFragmentationMetrics;

/// FFI-safe representation of Metal memory metrics
typedef struct {
    /// Total allocated memory across all heaps
    uint64_t total_allocated;
    /// Total heap size across all heaps
    uint64_t total_heap_size;
    /// Total used memory across all heaps
    uint64_t total_heap_used;
    /// Number of active allocations
    uint32_t allocation_count;
    /// Number of active heaps
    uint32_t heap_count;
    /// Overall fragmentation (0.0-1.0)
    float overall_fragmentation;
    /// Memory utilization percentage (0-100)
    float utilization_pct;
    /// Number of migration events recorded
    uint32_t migration_event_count;
} FFIMetalMemoryMetrics;

/// FFI-safe representation of page migration event
typedef struct {
    /// Event ID (first 8 bytes of UUID)
    uint64_t event_id_high;
    /// Event ID (last 8 bytes of UUID)
    uint64_t event_id_low;
    /// Migration type (1=PageOut, 2=PageIn, 3=BufferRelocate, 4=HeapCompaction, 5=PressureEviction)
    uint32_t migration_type;
    /// Source memory address
    uint64_t source_addr;
    /// Destination memory address
    uint64_t dest_addr;
    /// Size of migrated memory in bytes
    uint64_t size_bytes;
    /// Timestamp in microseconds since epoch
    uint64_t timestamp;
} FFIPageMigrationEvent;

// ============================================================================
// METAL HEAP OBSERVER FFI FUNCTIONS
// ============================================================================

/// Initialize Metal heap observation
/// Returns non-zero on success, 0 on failure
int32_t metal_heap_observer_init(void);

/// Observe a heap allocation
/// Arguments:
///   - heap_id: Opaque heap identifier
///   - buffer_id: Unique buffer identifier
///   - size: Allocation size in bytes
///   - offset: Offset within heap
///   - addr: Memory address
///   - storage_mode: MTL storage mode enum
/// Returns non-zero on success
int32_t metal_heap_observe_allocation(
    uint64_t heap_id,
    uint64_t buffer_id,
    uint64_t size,
    uint64_t offset,
    uint64_t addr,
    uint32_t storage_mode
);

/// Observe a heap deallocation
/// Arguments:
///   - buffer_id: Buffer identifier to deallocate
/// Returns non-zero on success
int32_t metal_heap_observe_deallocation(uint64_t buffer_id);

/// Update heap state after operations
/// Arguments:
///   - heap_id: Heap identifier
///   - total_size: Total heap size in bytes
///   - used_size: Used size in bytes
/// Returns non-zero on success
int32_t metal_heap_update_state(uint64_t heap_id, uint64_t total_size, uint64_t used_size);

/// Calculate heap fragmentation metrics
/// Arguments:
///   - heap_id: Heap identifier
///   - out_metrics: Pointer to metrics struct to fill
/// Returns non-zero on success
int32_t metal_heap_get_fragmentation(
    uint64_t heap_id,
    FFIFragmentationMetrics* out_metrics
);

/// Get all current heap states
/// Arguments:
///   - out_heaps: Pointer to array where heap states will be written
///   - max_heaps: Maximum number of heaps that can fit in array
/// Returns number of heaps written, or negative on error
int32_t metal_heap_get_all_states(FFIHeapState* out_heaps, uint32_t max_heaps);

/// Get current Metal memory metrics
/// Arguments:
///   - out_metrics: Pointer to metrics struct to fill
/// Returns non-zero on success
int32_t metal_heap_get_metrics(FFIMetalMemoryMetrics* out_metrics);

/// Get page migration events
/// Arguments:
///   - out_events: Pointer to array where events will be written
///   - max_events: Maximum number of events that can fit in array
/// Returns number of events written
int32_t metal_heap_get_migration_events(
    FFIPageMigrationEvent* out_events,
    uint32_t max_events
);

/// Clear all recorded observation data
/// Returns non-zero on success
int32_t metal_heap_clear(void);

/// Get last error message
/// Arguments:
///   - buffer: Pointer to character buffer
///   - buffer_len: Size of buffer
/// Returns number of bytes written (including null terminator)
size_t metal_heap_get_last_error(char* buffer, size_t buffer_len);

#ifdef __cplusplus
}
#endif

#endif // HEAP_OBSERVER_H
