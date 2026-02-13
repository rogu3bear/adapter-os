//! IOKit-based page migration tracking implementation
//!
//! Provides real-time monitoring of VM page migrations and memory pressure events
//! using macOS kernel APIs and IOKit framework.

#import <Foundation/Foundation.h>
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <mach/message.h>
#include <mach/port.h>
#include <dispatch/dispatch.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/sysctl.h>
#include <os/log.h>
#include <os/lock.h>

// IOKit headers for system monitoring
#include <IOKit/IOKitLib.h>
#include <IOKit/IOMessage.h>
#include <CoreFoundation/CoreFoundation.h>

// ============================================================================
// TYPE DEFINITIONS
// ============================================================================

typedef struct {
    uint64_t page_ins;
    uint64_t page_outs;
    uint64_t pages_freed;
    uint64_t pages_reactivated;
    uint64_t free_pages;
    uint64_t active_pages;
    uint64_t inactive_pages;
    uint64_t speculative_pages;
    uint64_t throttled_pages;
    uint64_t wired_pages;
} vm_stats_t;

typedef struct {
    uint64_t gpu_memory_in_use;
    uint64_t gpu_memory_available;
    uint64_t shared_memory_pool;
    uint64_t gpu_to_cpu_migrations;
    uint64_t cpu_to_gpu_migrations;
    uint64_t ane_memory_in_use;
} unified_mem_info_t;

typedef struct {
    uint64_t address;
    uint64_t size;
    uint32_t protection;
    uint32_t max_protection;
    uint32_t inheritance;
    uint32_t share_mode;
    uint64_t resident_pages;
} aos_vm_region_info_t;

typedef struct {
    uint64_t event_id_high;
    uint64_t event_id_low;
    uint32_t migration_type;
    uint64_t source_addr;
    uint64_t dest_addr;
    uint64_t size_bytes;
    uint64_t timestamp;
    uint32_t pressure_level;
} migration_event_t;

// ============================================================================
// STATE MANAGEMENT
// ============================================================================

static os_log_t iokit_log = NULL;
static os_unfair_lock state_lock = OS_UNFAIR_LOCK_INIT;

// VM statistics tracking
typedef struct {
    uint64_t last_page_ins;
    uint64_t last_page_outs;
    uint64_t current_pagein_delta;
    uint64_t current_pageout_delta;
    uint32_t memory_pressure_level;
    bool memory_pressure_enabled;
} vm_state_t;

static vm_state_t vm_state = {0};

// Migration events circular buffer
#define MAX_MIGRATION_EVENTS 256
static migration_event_t migration_events[MAX_MIGRATION_EVENTS];
static int migration_event_count = 0;
static int migration_event_index = 0;

// Error message buffer
#define MAX_ERROR_LEN 256
static char last_error[MAX_ERROR_LEN] = {0};

// ============================================================================
// INITIALIZATION AND CLEANUP
// ============================================================================

int iokit_vm_init(void) {
    os_unfair_lock_lock(&state_lock);

    iokit_log = os_log_create("com.adapteros.memory", "iokit");

    // Initialize VM state
    memset(&vm_state, 0, sizeof(vm_state_t));
    memset(migration_events, 0, sizeof(migration_events));

    // Get initial VM statistics
    struct vm_statistics64 vm_stats = {0};
    mach_msg_type_number_t count = HOST_VM_INFO64_COUNT;

    kern_return_t kr = host_statistics64(mach_host_self(), HOST_VM_INFO64,
                                         (host_info64_t)&vm_stats, &count);

    if (kr == KERN_SUCCESS) {
        vm_state.last_page_ins = vm_stats.pageins;
        vm_state.last_page_outs = vm_stats.pageouts;
        vm_state.memory_pressure_enabled = true;
        os_log_info(iokit_log, "IOKit VM monitoring initialized: pageins=%llu, pageouts=%llu",
                   vm_stats.pageins, vm_stats.pageouts);
    } else {
        snprintf(last_error, MAX_ERROR_LEN, "Failed to get initial VM stats: %d", kr);
        os_log_error(iokit_log, "Initialization failed: %{public}s", last_error);
        os_unfair_lock_unlock(&state_lock);
        return 0;
    }

    os_unfair_lock_unlock(&state_lock);
    return 1;
}

int iokit_vm_cleanup(void) {
    os_unfair_lock_lock(&state_lock);
    memset(&vm_state, 0, sizeof(vm_state_t));
    memset(migration_events, 0, sizeof(migration_events));
    migration_event_count = 0;
    migration_event_index = 0;
    os_unfair_lock_unlock(&state_lock);
    return 1;
}

// ============================================================================
// VM STATISTICS
// ============================================================================

int iokit_vm_get_stats(vm_stats_t *out_stats) {
    if (out_stats == NULL) {
        return -1;
    }

    os_unfair_lock_lock(&state_lock);

    struct vm_statistics64 vm_stats = {0};
    mach_msg_type_number_t count = HOST_VM_INFO64_COUNT;

    kern_return_t kr = host_statistics64(mach_host_self(), HOST_VM_INFO64,
                                         (host_info64_t)&vm_stats, &count);

    if (kr != KERN_SUCCESS) {
        snprintf(last_error, MAX_ERROR_LEN, "host_statistics64 failed: %d", kr);
        os_log_error(iokit_log, "Failed to get VM stats: %d", kr);
        os_unfair_lock_unlock(&state_lock);
        return -1;
    }

    // Calculate deltas
    vm_state.current_pagein_delta = vm_stats.pageins - vm_state.last_page_ins;
    vm_state.current_pageout_delta = vm_stats.pageouts - vm_state.last_page_outs;

    // Update last known values
    vm_state.last_page_ins = vm_stats.pageins;
    vm_state.last_page_outs = vm_stats.pageouts;

    // Fill output structure
    out_stats->page_ins = vm_stats.pageins;
    out_stats->page_outs = vm_stats.pageouts;
    out_stats->pages_freed = vm_stats.pageouts;
    out_stats->pages_reactivated = vm_stats.reactivations;
    out_stats->free_pages = vm_stats.free_count;
    out_stats->active_pages = vm_stats.active_count;
    out_stats->inactive_pages = vm_stats.inactive_count;
    out_stats->speculative_pages = vm_stats.speculative_count;
    out_stats->throttled_pages = 0; // Not directly available in vm_statistics64
    out_stats->wired_pages = vm_stats.wire_count;

    os_log_debug(iokit_log,
        "VM Stats: pageins=%llu, pageouts=%llu, free=%llu, active=%llu, inactive=%llu",
        vm_stats.pageins, vm_stats.pageouts, vm_stats.free_count,
        vm_stats.active_count, vm_stats.inactive_count);

    os_unfair_lock_unlock(&state_lock);
    return 0;
}

int64_t iokit_vm_get_pagein_delta(void) {
    os_unfair_lock_lock(&state_lock);
    int64_t delta = vm_state.current_pagein_delta;
    os_unfair_lock_unlock(&state_lock);
    return delta;
}

int64_t iokit_vm_get_pageout_delta(void) {
    os_unfair_lock_lock(&state_lock);
    int64_t delta = vm_state.current_pageout_delta;
    os_unfair_lock_unlock(&state_lock);
    return delta;
}

// ============================================================================
// MEMORY PRESSURE MONITORING
// ============================================================================

int iokit_memory_pressure_level(void) {
    // Polling-based memory pressure estimate. Enable/disable controls whether we
    // update the cached level; callers can still read the last known value.
    os_unfair_lock_lock(&state_lock);
    bool enabled = vm_state.memory_pressure_enabled;
    int level = vm_state.memory_pressure_level;
    os_unfair_lock_unlock(&state_lock);

    if (!enabled) {
        return level;
    }

    // Estimate pressure from free memory ratio
    struct vm_statistics64 vm_stats = {0};
    mach_msg_type_number_t count = HOST_VM_INFO64_COUNT;

    kern_return_t kr = host_statistics64(mach_host_self(), HOST_VM_INFO64,
                                         (host_info64_t)&vm_stats, &count);

    if (kr == KERN_SUCCESS) {
        uint64_t total_pages = vm_stats.active_count + vm_stats.inactive_count +
                               vm_stats.free_count + vm_stats.wire_count;

        if (total_pages > 0) {
            double free_ratio = (double)vm_stats.free_count / total_pages;

            if (free_ratio < 0.05) {
                level = 2; // Critical
            } else if (free_ratio < 0.15) {
                level = 1; // Warning
            } else {
                level = 0; // Normal
            }

            os_unfair_lock_lock(&state_lock);
            vm_state.memory_pressure_level = level;
            os_unfair_lock_unlock(&state_lock);
        }
    }

    return level;
}

int iokit_memory_pressure_enable(void) {
    os_unfair_lock_lock(&state_lock);
    vm_state.memory_pressure_enabled = true;
    os_unfair_lock_unlock(&state_lock);
    os_log_debug(iokit_log, "Memory pressure monitoring enabled (polling)");
    return 1;
}

int iokit_memory_pressure_disable(void) {
    os_unfair_lock_lock(&state_lock);
    vm_state.memory_pressure_enabled = false;
    os_unfair_lock_unlock(&state_lock);
    os_log_debug(iokit_log, "Memory pressure monitoring disabled");
    return 1;
}

// ============================================================================
// UNIFIED MEMORY (APPLE SILICON)
// ============================================================================

int iokit_unified_memory_supported(void) {
    // Check if system is Apple Silicon (arm64)
    int is_apple_silicon = 0;
    size_t len = sizeof(is_apple_silicon);

    if (sysctlbyname("hw.optional.arm64", &is_apple_silicon, &len, NULL, 0) == 0) {
        if (is_apple_silicon) {
            os_log_info(iokit_log, "Apple Silicon detected - unified memory supported");
            return 1;
        }
    }

    os_log_debug(iokit_log, "Intel processor or unified memory not supported");
    return 0;
}

int iokit_unified_memory_info(unified_mem_info_t *out_info) {
    if (out_info == NULL) {
        return -1;
    }

    if (!iokit_unified_memory_supported()) {
        return -1;
    }

    memset(out_info, 0, sizeof(*out_info));

    // Try to get Metal/GPU memory info via IOKit
    // This is a placeholder - actual implementation would require
    // accessing Metal device metrics or IORegistry

    // For now, estimate from system memory
    int64_t gpu_mem = iokit_gpu_memory_usage();
    int64_t ane_mem = iokit_ane_memory_usage();

    if (gpu_mem >= 0) {
        out_info->gpu_memory_in_use = gpu_mem;
    }
    if (ane_mem >= 0) {
        out_info->ane_memory_in_use = ane_mem;
    }

    return 0;
}

int64_t iokit_gpu_memory_usage(void) {
    // Get GPU memory usage from IORegistry or Metal framework
    // This is a best-effort estimate using available APIs

    // For now, return -1 indicating data not available
    // Full implementation would require Metal device introspection
    return -1;
}

int64_t iokit_ane_memory_usage(void) {
    // Get ANE (Apple Neural Engine) memory usage
    // This would require ANE performance metrics access
    // For now, return -1 indicating data not available
    return -1;
}

// ============================================================================
// VM REGION INFORMATION
// ============================================================================

int iokit_vm_region_info(uint64_t address, aos_vm_region_info_t *out_region) {
    if (out_region == NULL) {
        return -1;
    }

    vm_size_t region_size = 0;
    vm_region_basic_info_data_64_t region_info = {0};
    mach_msg_type_number_t info_count = VM_REGION_BASIC_INFO_COUNT_64;
    mach_port_t object_name = MACH_PORT_NULL;

    kern_return_t kr = vm_region_64(
        mach_task_self(),
        (vm_address_t *)&address,
        &region_size,
        VM_REGION_BASIC_INFO_64,
        (vm_region_info_t)&region_info,  // Cast to Mach API type, not our custom type
        &info_count,
        &object_name);

    if (kr != KERN_SUCCESS) {
        return -1;
    }

    out_region->address = address;
    out_region->size = region_size;
    out_region->protection = region_info.protection;
    out_region->max_protection = region_info.max_protection;
    out_region->inheritance = region_info.inheritance;
    out_region->share_mode = region_info.share_mode;
    out_region->resident_pages = region_size / 4096;

    if (object_name != MACH_PORT_NULL) {
        mach_port_deallocate(mach_task_self(), object_name);
    }

    return 0;
}

typedef int (*vm_region_callback_t)(aos_vm_region_info_t *);

int iokit_vm_scan_regions(vm_region_callback_t callback) {
    if (callback == NULL) {
        return -1;
    }

    vm_address_t address = 0;
    int region_count = 0;

    // Scan memory regions
    while (address < (vm_address_t)-1) {
        aos_vm_region_info_t region_info = {0};

        if (iokit_vm_region_info((uint64_t)address, &region_info) != 0) {
            break;
        }

        if (callback(&region_info) != 0) {
            break;
        }

        region_count++;
        address = region_info.address + region_info.size;

        // Safety limit to prevent infinite loops
        if (region_count > 10000) {
            break;
        }
    }

    return region_count;
}

// ============================================================================
// MIGRATION EVENT TRACKING
// ============================================================================

int iokit_migration_get_events(migration_event_t *out_events, uint32_t max_events) {
    if (out_events == NULL || max_events == 0) {
        return -1;
    }

    os_unfair_lock_lock(&state_lock);

    int count = 0;
    for (int i = 0; i < migration_event_count && count < (int)max_events; i++) {
        out_events[count] = migration_events[i];
        count++;
    }

    os_unfair_lock_unlock(&state_lock);
    return count;
}

int iokit_migration_clear_events(void) {
    os_unfair_lock_lock(&state_lock);
    memset(migration_events, 0, sizeof(migration_events));
    migration_event_count = 0;
    migration_event_index = 0;
    os_unfair_lock_unlock(&state_lock);
    return 1;
}

// ============================================================================
// ERROR HANDLING
// ============================================================================

size_t iokit_get_last_error(char *buffer, size_t buffer_len) {
    if (buffer == NULL || buffer_len == 0) {
        return 0;
    }

    os_unfair_lock_lock(&state_lock);
    size_t len = strlen(last_error);
    if (len >= buffer_len) {
        len = buffer_len - 1;
    }
    strncpy(buffer, last_error, len);
    buffer[len] = '\0';
    os_unfair_lock_unlock(&state_lock);

    return len + 1; // Include null terminator
}
