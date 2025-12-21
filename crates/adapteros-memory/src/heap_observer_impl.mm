/// Metal Heap Observer Implementation
/// Objective-C++ implementation for Metal heap monitoring and page migration tracking
/// Compiled by build.rs for macOS targets

#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <IOKit/IOKitLib.h>
#import <IOKit/IOTypes.h>

#include "../include/heap_observer.h"
#include <map>
#include <memory>
#include <mutex>
#include <cstring>
#include <atomic>

// ============================================================================
// INTERNAL STATE MANAGEMENT
// ============================================================================

namespace {
    /// Thread-safe error message storage
    thread_local char last_error_buffer[256] = {0};

    /// Heap allocation tracking
    struct AllocationRecord {
        uint64_t heap_id;
        uint64_t size;
        uint64_t offset;
        uint64_t addr;
        uint32_t storage_mode;
        uint64_t timestamp;
    };

    /// Global state for heap tracking
    struct HeapObserverState {
        std::mutex allocations_lock;
        std::map<uint64_t, AllocationRecord> allocations; // buffer_id -> allocation

        std::mutex heaps_lock;
        std::map<uint64_t, FFIHeapState> heaps; // heap_id -> state

        std::atomic<bool> initialized{false};

        /// Safely store error message
        void set_error(const char* format, ...) {
            va_list args;
            va_start(args, format);
            vsnprintf(last_error_buffer, sizeof(last_error_buffer), format, args);
            va_end(args);
        }

        /// Get reference to global state
        static HeapObserverState& instance() {
            static HeapObserverState state;
            return state;
        }
    };
}

// ============================================================================
// FFI IMPLEMENTATION
// ============================================================================

int32_t metal_heap_observer_init(void) {
    @autoreleasepool {
        try {
            auto& state = HeapObserverState::instance();

            // Check if Metal is available
            id<MTLDevice> device = MTLCreateSystemDefaultDevice();
            if (!device) {
                state.set_error("No Metal-capable device found");
                return 0;
            }

            // Mark as initialized
            state.initialized = true;

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Initialization failed: %s", e.what());
            return 0;
        }
    }
}

int32_t metal_heap_observe_allocation(
    uint64_t heap_id,
    uint64_t buffer_id,
    uint64_t size,
    uint64_t offset,
    uint64_t addr,
    uint32_t storage_mode
) {
    @autoreleasepool {
        try {
            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                state.set_error("Heap observer not initialized");
                return 0;
            }

            {
                std::lock_guard<std::mutex> lock(state.allocations_lock);

                // Record allocation
                AllocationRecord record = {
                    .heap_id = heap_id,
                    .size = size,
                    .offset = offset,
                    .addr = addr,
                    .storage_mode = storage_mode,
                    .timestamp = (uint64_t)([[NSDate date] timeIntervalSince1970] * 1e6)
                };

                state.allocations[buffer_id] = record;
            }

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Allocation observation failed: %s", e.what());
            return 0;
        }
    }
}

int32_t metal_heap_observe_deallocation(uint64_t buffer_id) {
    @autoreleasepool {
        try {
            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                state.set_error("Heap observer not initialized");
                return 0;
            }

            {
                std::lock_guard<std::mutex> lock(state.allocations_lock);

                // Remove allocation record
                auto it = state.allocations.find(buffer_id);
                if (it != state.allocations.end()) {
                    state.allocations.erase(it);
                }
            }

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Deallocation observation failed: %s", e.what());
            return 0;
        }
    }
}

int32_t metal_heap_update_state(uint64_t heap_id, uint64_t total_size, uint64_t used_size) {
    @autoreleasepool {
        try {
            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                state.set_error("Heap observer not initialized");
                return 0;
            }

            {
                std::lock_guard<std::mutex> lock(state.heaps_lock);

                // Create or update heap state
                FFIHeapState heap_state = {
                    .heap_id = heap_id,
                    .total_size = total_size,
                    .used_size = used_size,
                    .allocation_count = 0,
                    .fragmentation_ratio = 0.0f,
                    .avg_alloc_size = 0,
                    .largest_free_block = total_size - used_size
                };

                // Count allocations for this heap
                {
                    std::lock_guard<std::mutex> alloc_lock(state.allocations_lock);
                    for (const auto& [buffer_id, record] : state.allocations) {
                        if (record.heap_id == heap_id) {
                            heap_state.allocation_count++;
                        }
                    }
                }

                if (heap_state.allocation_count > 0) {
                    heap_state.avg_alloc_size = used_size / heap_state.allocation_count;
                }

                state.heaps[heap_id] = heap_state;
            }

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("State update failed: %s", e.what());
            return 0;
        }
    }
}

int32_t metal_heap_get_fragmentation(
    uint64_t heap_id,
    FFIFragmentationMetrics* out_metrics
) {
    @autoreleasepool {
        try {
            if (!out_metrics) {
                HeapObserverState::instance().set_error("Null output pointer");
                return 0;
            }

            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                state.set_error("Heap observer not initialized");
                return 0;
            }

            {
                std::lock_guard<std::mutex> lock(state.heaps_lock);

                auto it = state.heaps.find(heap_id);
                if (it == state.heaps.end()) {
                    // Heap not found, return empty metrics
                    memset(out_metrics, 0, sizeof(FFIFragmentationMetrics));
                    return 1;
                }

                const auto& heap_state = it->second;

                // Calculate fragmentation
                // External fragmentation: (total_size - used_size) / total_size
                float external_frag = 0.0f;
                if (heap_state.total_size > 0) {
                    uint64_t free_space = heap_state.total_size - heap_state.used_size;
                    external_frag = (float)free_space / heap_state.total_size;
                }

                // Internal fragmentation estimate: 5% of used space
                float internal_frag = 0.05f;

                out_metrics->fragmentation_ratio = (external_frag + internal_frag) / 2.0f;
                out_metrics->external_fragmentation = external_frag;
                out_metrics->internal_fragmentation = internal_frag;
                out_metrics->free_blocks = (out_metrics->fragmentation_ratio > 0.0f) ? 1 : 0;
                out_metrics->total_free_bytes = heap_state.total_size - heap_state.used_size;
                out_metrics->avg_free_block_size = out_metrics->total_free_bytes;
                out_metrics->largest_free_block = heap_state.largest_free_block;
                out_metrics->compaction_efficiency = 1.0f - out_metrics->fragmentation_ratio;
            }

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Fragmentation calculation failed: %s", e.what());
            return 0;
        }
    }
}

int32_t metal_heap_get_all_states(FFIHeapState* out_heaps, uint32_t max_heaps) {
    @autoreleasepool {
        try {
            if (!out_heaps || max_heaps == 0) {
                HeapObserverState::instance().set_error("Invalid parameters");
                return -1;
            }

            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                HeapObserverState::instance().set_error("Heap observer not initialized");
                return -1;
            }

            {
                std::lock_guard<std::mutex> lock(state.heaps_lock);

                int32_t count = 0;
                for (const auto& [heap_id, heap_state] : state.heaps) {
                    if (count >= (int32_t)max_heaps) {
                        break;
                    }
                    out_heaps[count++] = heap_state;
                }

                return count;
            }
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Get states failed: %s", e.what());
            return -1;
        }
    }
}

int32_t metal_heap_get_metrics(FFIMetalMemoryMetrics* out_metrics) {
    @autoreleasepool {
        try {
            if (!out_metrics) {
                HeapObserverState::instance().set_error("Null output pointer");
                return 0;
            }

            auto& state = HeapObserverState::instance();

            if (!state.initialized) {
                state.set_error("Heap observer not initialized");
                return 0;
            }

            uint64_t total_allocated = 0;
            uint64_t total_heap_size = 0;
            uint64_t total_heap_used = 0;
            uint32_t allocation_count = 0;
            uint32_t heap_count = 0;

            {
                std::lock_guard<std::mutex> lock(state.heaps_lock);
                heap_count = state.heaps.size();

                for (const auto& [heap_id, heap_state] : state.heaps) {
                    total_heap_size += heap_state.total_size;
                    total_heap_used += heap_state.used_size;
                    allocation_count += heap_state.allocation_count;
                }
            }

            {
                std::lock_guard<std::mutex> lock(state.allocations_lock);
                total_allocated = state.allocations.size();

                for (const auto& [buffer_id, record] : state.allocations) {
                    total_allocated += record.size;
                }
            }

            float utilization_pct = 0.0f;
            if (total_heap_size > 0) {
                utilization_pct = (float)total_heap_used / total_heap_size * 100.0f;
            }

            out_metrics->total_allocated = total_allocated;
            out_metrics->total_heap_size = total_heap_size;
            out_metrics->total_heap_used = total_heap_used;
            out_metrics->allocation_count = allocation_count;
            out_metrics->heap_count = heap_count;
            out_metrics->overall_fragmentation = 0.0f; // Placeholder
            out_metrics->utilization_pct = utilization_pct;
            out_metrics->migration_event_count = 0; // Placeholder

            return 0; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Get metrics failed: %s", e.what());
            return -1;
        }
    }
}

int32_t metal_heap_get_migration_events(
    FFIPageMigrationEvent* out_events,
    uint32_t max_events
) {
    @autoreleasepool {
        try {
            if (!out_events || max_events == 0) {
                return 0; // No events
            }

            // Placeholder: Return 0 events
            // Full implementation would track page migrations via IOKit
            return 0;
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Get migration events failed: %s", e.what());
            return -1;
        }
    }
}

int32_t metal_heap_clear(void) {
    @autoreleasepool {
        try {
            auto& state = HeapObserverState::instance();

            {
                std::lock_guard<std::mutex> lock(state.allocations_lock);
                state.allocations.clear();
            }

            {
                std::lock_guard<std::mutex> lock(state.heaps_lock);
                state.heaps.clear();
            }

            return 1; // Success
        } catch (const std::exception& e) {
            HeapObserverState::instance().set_error("Clear failed: %s", e.what());
            return 0;
        }
    }
}

size_t metal_heap_get_last_error(char* buffer, size_t buffer_len) {
    if (!buffer || buffer_len == 0) {
        return 0;
    }

    size_t len = strlen(last_error_buffer);
    if (len >= buffer_len) {
        len = buffer_len - 1;
    }

    strncpy(buffer, last_error_buffer, len);
    buffer[len] = '\0';

    return len + 1; // Include null terminator
}
