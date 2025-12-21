//! MTLHeap observer callbacks implementation
//!
//! Provides callback-based event system for MTLHeap operations:
//! - Allocation success/failure events
//! - Deallocation notifications
//! - Heap compaction tracking
//! - Memory pressure handling
//! - Performance counters collection
//! - Statistics aggregation with peak tracking
//!
//! Compiled: macOS only
//! Thread-safe: Uses os_unfair_lock and atomic operations

#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <os/lock.h>
#import <os/log.h>
#include <atomic>
#include <chrono>
#include <map>
#include <vector>
#include <cstring>
#include <stdint.h>

// MARK: - Callback Type Definitions

/// Allocation success callback: (heap_id, buffer_id, size, timestamp_us)
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

/// Performance counters
typedef struct {
    std::atomic<uint64_t> allocation_count;
    std::atomic<uint64_t> deallocation_count;
    std::atomic<uint64_t> failed_allocations;
    std::atomic<uint64_t> compaction_count;
    std::atomic<uint64_t> page_faults;
    std::atomic<uint64_t> total_bytes_allocated;
    std::atomic<uint64_t> total_bytes_deallocated;
} PerformanceCounters;

/// Allocation record
typedef struct {
    uint64_t buffer_id;
    uint64_t size_bytes;
    uint64_t offset_bytes;
    uint64_t timestamp_us;
    uint64_t memory_addr;
    uint32_t storage_mode;
} AllocationRecord;

/// Compaction event
typedef struct {
    uint64_t timestamp_us;
    uint64_t bytes_recovered;
    uint64_t bytes_moved;
    uint32_t blocks_compacted;
} CompactionEvent;

// MARK: - Global Observer Implementation

class MetalHeapObserverImpl {
private:
    // Thread-safe lock
    os_unfair_lock state_lock = OS_UNFAIR_LOCK_INIT;

    // Heap tracking
    std::map<uint64_t, std::vector<AllocationRecord>> heap_allocations;
    std::map<uint64_t, HeapStats> heap_stats;
    std::map<uint64_t, std::vector<CompactionEvent>> compaction_history;

    // Callbacks (stored outside lock to avoid deadlocks)
    AllocationSuccessCallback on_allocation_success = nullptr;
    AllocationFailureCallback on_allocation_failure = nullptr;
    DeallocationCallback on_deallocation = nullptr;
    CompactionCallback on_compaction = nullptr;
    MemoryPressureCallback on_memory_pressure = nullptr;

    // Performance counters
    PerformanceCounters performance_counters;
    std::atomic<uint64_t> global_peak_memory_bytes = 0;
    std::atomic<uint64_t> current_global_memory_bytes = 0;

    // Error tracking
    char last_error[256];

public:
    MetalHeapObserverImpl() {
        std::memset(last_error, 0, sizeof(last_error));
        performance_counters.allocation_count.store(0, std::memory_order_relaxed);
        performance_counters.deallocation_count.store(0, std::memory_order_relaxed);
        performance_counters.failed_allocations.store(0, std::memory_order_relaxed);
        performance_counters.compaction_count.store(0, std::memory_order_relaxed);
        performance_counters.page_faults.store(0, std::memory_order_relaxed);
        performance_counters.total_bytes_allocated.store(0, std::memory_order_relaxed);
        performance_counters.total_bytes_deallocated.store(0, std::memory_order_relaxed);
    }

    // MARK: - Callback Registration

    void set_allocation_success_callback(AllocationSuccessCallback callback) {
        os_unfair_lock_lock(&state_lock);
        on_allocation_success = callback;
        os_unfair_lock_unlock(&state_lock);
    }

    void set_allocation_failure_callback(AllocationFailureCallback callback) {
        os_unfair_lock_lock(&state_lock);
        on_allocation_failure = callback;
        os_unfair_lock_unlock(&state_lock);
    }

    void set_deallocation_callback(DeallocationCallback callback) {
        os_unfair_lock_lock(&state_lock);
        on_deallocation = callback;
        os_unfair_lock_unlock(&state_lock);
    }

    void set_compaction_callback(CompactionCallback callback) {
        os_unfair_lock_lock(&state_lock);
        on_compaction = callback;
        os_unfair_lock_unlock(&state_lock);
    }

    void set_memory_pressure_callback(MemoryPressureCallback callback) {
        os_unfair_lock_lock(&state_lock);
        on_memory_pressure = callback;
        os_unfair_lock_unlock(&state_lock);
    }

    // MARK: - Allocation Tracking

    int32_t record_allocation(uint64_t heap_id, uint64_t buffer_id, uint64_t size_bytes,
                              uint64_t offset_bytes, uint64_t memory_addr, uint32_t storage_mode) {
        if (size_bytes == 0) {
            snprintf(last_error, sizeof(last_error), "Invalid allocation size: 0");
            return -1;
        }

        uint64_t timestamp_us = get_timestamp_us();

        os_unfair_lock_lock(&state_lock);

        // Initialize heap stats if needed
        if (heap_stats.find(heap_id) == heap_stats.end()) {
            heap_stats[heap_id] = {0, 0, 0, 0, 0, 0, 0, 0.0f, 0.0f, timestamp_us};
        }

        // Record allocation
        AllocationRecord record{buffer_id, size_bytes, offset_bytes, timestamp_us, memory_addr, storage_mode};
        heap_allocations[heap_id].push_back(record);

        // Update statistics
        HeapStats &stats = heap_stats[heap_id];
        stats.current_used_bytes += size_bytes;
        stats.current_allocation_count++;
        stats.total_allocations_lifetime++;
        stats.last_update_us = timestamp_us;

        // Update peak tracking
        if (stats.current_used_bytes > stats.peak_used_bytes) {
            stats.peak_used_bytes = stats.current_used_bytes;
        }
        if (stats.current_allocation_count > stats.peak_allocation_count) {
            stats.peak_allocation_count = stats.current_allocation_count;
        }

        // Update global memory tracking
        current_global_memory_bytes.fetch_add(size_bytes, std::memory_order_relaxed);
        uint64_t global_used = current_global_memory_bytes.load(std::memory_order_relaxed);
        uint64_t current_peak = global_peak_memory_bytes.load(std::memory_order_relaxed);
        if (global_used > current_peak) {
            global_peak_memory_bytes.compare_exchange_strong(
                current_peak, global_used,
                std::memory_order_release,
                std::memory_order_relaxed
            );
        }

        // Update performance counters
        performance_counters.allocation_count.fetch_add(1, std::memory_order_release);
        performance_counters.total_bytes_allocated.fetch_add(size_bytes, std::memory_order_release);

        // Capture callback before releasing lock
        AllocationSuccessCallback callback = on_allocation_success;
        os_unfair_lock_unlock(&state_lock);

        // Invoke callback outside lock to prevent deadlocks
        if (callback) {
            callback(heap_id, buffer_id, size_bytes, timestamp_us);
        }

        return 0;
    }

    int32_t record_deallocation(uint64_t heap_id, uint64_t buffer_id) {
        uint64_t timestamp_us = get_timestamp_us();
        uint64_t freed_size = 0;

        os_unfair_lock_lock(&state_lock);

        // Find and remove allocation
        auto &allocations = heap_allocations[heap_id];
        for (auto it = allocations.begin(); it != allocations.end(); ++it) {
            if (it->buffer_id == buffer_id) {
                freed_size = it->size_bytes;
                allocations.erase(it);
                break;
            }
        }

        if (freed_size == 0) {
            os_unfair_lock_unlock(&state_lock);
            snprintf(last_error, sizeof(last_error), "Allocation not found: %llu", buffer_id);
            return -1;
        }

        // Update statistics
        if (heap_stats.find(heap_id) != heap_stats.end()) {
            HeapStats &stats = heap_stats[heap_id];
            if (stats.current_used_bytes >= freed_size) {
                stats.current_used_bytes -= freed_size;
            }
            if (stats.current_allocation_count > 0) {
                stats.current_allocation_count--;
            }
            stats.total_deallocations_lifetime++;
            stats.last_update_us = timestamp_us;
        }

        // Update global memory tracking
        current_global_memory_bytes.fetch_sub(freed_size, std::memory_order_relaxed);

        // Update performance counters
        performance_counters.deallocation_count.fetch_add(1, std::memory_order_release);
        performance_counters.total_bytes_deallocated.fetch_add(freed_size, std::memory_order_release);

        // Capture callback before releasing lock
        DeallocationCallback callback = on_deallocation;
        os_unfair_lock_unlock(&state_lock);

        // Invoke callback outside lock
        if (callback) {
            callback(heap_id, buffer_id, freed_size, timestamp_us);
        }

        return 0;
    }

    // MARK: - Allocation Failure

    int32_t record_allocation_failure(uint64_t heap_id, uint64_t requested_size, int32_t error_code) {
        uint64_t timestamp_us = get_timestamp_us();

        os_unfair_lock_lock(&state_lock);
        performance_counters.failed_allocations.fetch_add(1, std::memory_order_release);
        AllocationFailureCallback callback = on_allocation_failure;
        os_unfair_lock_unlock(&state_lock);

        if (callback) {
            callback(heap_id, requested_size, error_code);
        }

        return 0;
    }

    // MARK: - Heap Compaction

    int32_t record_heap_compaction(uint64_t heap_id, uint64_t bytes_recovered,
                                    uint64_t bytes_moved, uint32_t blocks_compacted) {
        uint64_t timestamp_us = get_timestamp_us();

        os_unfair_lock_lock(&state_lock);

        if (heap_stats.find(heap_id) != heap_stats.end()) {
            compaction_history[heap_id].push_back({timestamp_us, bytes_recovered, bytes_moved, blocks_compacted});
        }

        performance_counters.compaction_count.fetch_add(1, std::memory_order_release);
        CompactionCallback callback = on_compaction;
        os_unfair_lock_unlock(&state_lock);

        if (callback) {
            callback(heap_id, bytes_recovered, blocks_compacted);
        }

        return 0;
    }

    // MARK: - Memory Pressure

    int32_t on_memory_pressure_event(int32_t pressure_level, uint64_t available_bytes) {
        os_unfair_lock_lock(&state_lock);
        MemoryPressureCallback callback = on_memory_pressure;
        os_unfair_lock_unlock(&state_lock);

        if (callback) {
            callback(pressure_level, available_bytes);
        }

        return 0;
    }

    // MARK: - Statistics Collection

    int32_t get_heap_stats(uint64_t heap_id, HeapStats *out_stats) {
        if (!out_stats) return -1;

        os_unfair_lock_lock(&state_lock);

        auto it = heap_stats.find(heap_id);
        if (it == heap_stats.end()) {
            os_unfair_lock_unlock(&state_lock);
            return -1;
        }

        *out_stats = it->second;
        os_unfair_lock_unlock(&state_lock);
        return 0;
    }

    int32_t get_global_stats(HeapStats *out_stats) {
        if (!out_stats) return -1;

        os_unfair_lock_lock(&state_lock);

        HeapStats global_stats = {0};
        global_stats.current_used_bytes = current_global_memory_bytes.load(std::memory_order_relaxed);
        global_stats.peak_used_bytes = global_peak_memory_bytes.load(std::memory_order_relaxed);

        for (const auto &[heap_id, stats] : heap_stats) {
            global_stats.total_heap_size += stats.total_heap_size;
            global_stats.current_allocation_count += stats.current_allocation_count;
            global_stats.peak_allocation_count += stats.peak_allocation_count;
            global_stats.total_allocations_lifetime += stats.total_allocations_lifetime;
            global_stats.total_deallocations_lifetime += stats.total_deallocations_lifetime;
        }

        os_unfair_lock_unlock(&state_lock);

        *out_stats = global_stats;
        return 0;
    }

    // MARK: - Performance Counters

    uint64_t get_allocation_count() const {
        return performance_counters.allocation_count.load(std::memory_order_acquire);
    }

    uint64_t get_deallocation_count() const {
        return performance_counters.deallocation_count.load(std::memory_order_acquire);
    }

    uint64_t get_failed_allocations() const {
        return performance_counters.failed_allocations.load(std::memory_order_acquire);
    }

    uint64_t get_compaction_count() const {
        return performance_counters.compaction_count.load(std::memory_order_acquire);
    }

    float calculate_allocation_rate_per_second() const {
        static uint64_t last_count = 0;
        static auto last_time = std::chrono::high_resolution_clock::now();

        uint64_t current_count = get_allocation_count();
        auto current_time = std::chrono::high_resolution_clock::now();

        auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(
            current_time - last_time
        ).count();

        if (duration == 0) return 0.0f;

        float rate = ((float)(current_count - last_count) / (float)duration) * 1000.0f;

        last_count = current_count;
        last_time = current_time;

        return rate;
    }

    float calculate_fragmentation_percentage(uint64_t heap_id) {
        os_unfair_lock_lock(&state_lock);

        auto heap_it = heap_stats.find(heap_id);
        if (heap_it == heap_stats.end()) {
            os_unfair_lock_unlock(&state_lock);
            return 0.0f;
        }

        const auto &allocations = heap_allocations[heap_id];
        uint64_t total_allocated = 0;
        for (const auto &alloc : allocations) {
            total_allocated += alloc.size_bytes;
        }

        uint64_t total_heap_size = heap_it->second.total_heap_size;
        os_unfair_lock_unlock(&state_lock);

        if (total_heap_size == 0) return 0.0f;

        float utilization = static_cast<float>(total_allocated) / static_cast<float>(total_heap_size);
        return (1.0f - utilization) * 100.0f;
    }

    uint64_t get_page_fault_count() const {
        return performance_counters.page_faults.load(std::memory_order_acquire);
    }

    void record_page_fault(uint64_t heap_id) {
        performance_counters.page_faults.fetch_add(1, std::memory_order_release);
    }

    // MARK: - Utilities

    const char *get_last_error() const {
        return last_error;
    }

    void clear_stats() {
        os_unfair_lock_lock(&state_lock);
        heap_allocations.clear();
        heap_stats.clear();
        compaction_history.clear();
        os_unfair_lock_unlock(&state_lock);

        performance_counters.allocation_count.store(0, std::memory_order_release);
        performance_counters.deallocation_count.store(0, std::memory_order_release);
        performance_counters.failed_allocations.store(0, std::memory_order_release);
        performance_counters.compaction_count.store(0, std::memory_order_release);
        performance_counters.page_faults.store(0, std::memory_order_release);
        performance_counters.total_bytes_allocated.store(0, std::memory_order_release);
        performance_counters.total_bytes_deallocated.store(0, std::memory_order_release);

        global_peak_memory_bytes.store(0, std::memory_order_release);
        current_global_memory_bytes.store(0, std::memory_order_release);
    }

private:
    static uint64_t get_timestamp_us() {
        auto now = std::chrono::high_resolution_clock::now();
        return std::chrono::duration_cast<std::chrono::microseconds>(
            now.time_since_epoch()
        ).count();
    }
};

// MARK: - Global Instance

static MetalHeapObserverImpl g_metal_heap_observer;

// MARK: - C FFI API

#ifdef __cplusplus
extern "C" {
#endif

// MARK: - Callback Registration

void metal_heap_set_allocation_success_callback(AllocationSuccessCallback callback) {
    g_metal_heap_observer.set_allocation_success_callback(callback);
}

void metal_heap_set_allocation_failure_callback(AllocationFailureCallback callback) {
    g_metal_heap_observer.set_allocation_failure_callback(callback);
}

void metal_heap_set_deallocation_callback(DeallocationCallback callback) {
    g_metal_heap_observer.set_deallocation_callback(callback);
}

void metal_heap_set_compaction_callback(CompactionCallback callback) {
    g_metal_heap_observer.set_compaction_callback(callback);
}

void metal_heap_set_memory_pressure_callback(MemoryPressureCallback callback) {
    g_metal_heap_observer.set_memory_pressure_callback(callback);
}

// MARK: - Allocation Tracking

int32_t metal_heap_record_allocation(uint64_t heap_id, uint64_t buffer_id, uint64_t size_bytes,
                                     uint64_t offset_bytes, uint64_t memory_addr, uint32_t storage_mode) {
    return g_metal_heap_observer.record_allocation(heap_id, buffer_id, size_bytes, offset_bytes, memory_addr, storage_mode);
}

int32_t metal_heap_record_deallocation(uint64_t heap_id, uint64_t buffer_id) {
    return g_metal_heap_observer.record_deallocation(heap_id, buffer_id);
}

int32_t metal_heap_record_allocation_failure(uint64_t heap_id, uint64_t requested_size, int32_t error_code) {
    return g_metal_heap_observer.record_allocation_failure(heap_id, requested_size, error_code);
}

// MARK: - Heap Compaction

int32_t metal_heap_record_compaction(uint64_t heap_id, uint64_t bytes_recovered,
                                    uint64_t bytes_moved, uint32_t blocks_compacted) {
    return g_metal_heap_observer.record_heap_compaction(heap_id, bytes_recovered, bytes_moved, blocks_compacted);
}

// MARK: - Memory Pressure

int32_t metal_heap_on_memory_pressure(int32_t pressure_level, uint64_t available_bytes) {
    return g_metal_heap_observer.on_memory_pressure_event(pressure_level, available_bytes);
}

// MARK: - Statistics

int32_t metal_heap_get_stats(uint64_t heap_id, HeapStats *out_stats) {
    return g_metal_heap_observer.get_heap_stats(heap_id, out_stats);
}

int32_t metal_heap_get_global_stats(HeapStats *out_stats) {
    return g_metal_heap_observer.get_global_stats(out_stats);
}

// MARK: - Performance Counters

uint64_t metal_heap_get_allocation_count(void) {
    return g_metal_heap_observer.get_allocation_count();
}

uint64_t metal_heap_get_deallocation_count(void) {
    return g_metal_heap_observer.get_deallocation_count();
}

uint64_t metal_heap_get_failed_allocations(void) {
    return g_metal_heap_observer.get_failed_allocations();
}

uint64_t metal_heap_get_compaction_count(void) {
    return g_metal_heap_observer.get_compaction_count();
}

float metal_heap_get_allocation_rate_per_second(void) {
    return g_metal_heap_observer.calculate_allocation_rate_per_second();
}

float metal_heap_get_fragmentation_percentage(uint64_t heap_id) {
    return g_metal_heap_observer.calculate_fragmentation_percentage(heap_id);
}

uint64_t metal_heap_get_page_fault_count(void) {
    return g_metal_heap_observer.get_page_fault_count();
}

void metal_heap_record_page_fault(uint64_t heap_id) {
    g_metal_heap_observer.record_page_fault(heap_id);
}

// MARK: - Error Handling

const char *metal_heap_get_last_error(void) {
    return g_metal_heap_observer.get_last_error();
}

// MARK: - Maintenance

void metal_heap_clear_stats(void) {
    g_metal_heap_observer.clear_stats();
}

#ifdef __cplusplus
}
#endif
