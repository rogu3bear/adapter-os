# IOKit Page Migration Tracking for AdapterOS

## Overview

IOKit-based page migration tracking provides real-time monitoring of VM page migrations, memory pressure events, and unified memory transitions on macOS. This system tracks when the OS moves memory pages between physical RAM and disk, and detects GPU/CPU memory transfers on Apple Silicon processors.

**Location:** `crates/adapteros-memory/src/page_migration_iokit.rs` (Rust)
**Location:** `crates/adapteros-memory/src/page_migration_iokit_impl.mm` (IOKit implementation)

## Key Features

### 1. Page Migration Detection

- **Page-In Events:** Track when pages are loaded from disk into physical RAM
- **Page-Out Events:** Monitor when pages are evicted from RAM to disk
- **Real-time Deltas:** Calculate incremental changes since last query
- **Detailed Statistics:** Full VM statistics including active, inactive, free, and wired pages

### 2. Memory Pressure Monitoring

- **Pressure Levels:** Track three levels of memory pressure:
  - `Normal` - Free memory > 15% of total
  - `Warning` - Free memory 5-15% of total
  - `Critical` - Free memory < 5% of total (active eviction)
- **Automatic Detection:** Memory pressure calculated from free page ratio
- **Callbacks:** Hook for memory pressure event notifications

### 3. Unified Memory Tracking (Apple Silicon M1/M2/M3)

- **GPU/CPU Transitions:** Track memory transfers between GPU and CPU memory spaces
- **Shared Memory Pools:** Monitor MTLSharedHeap allocations
- **ANE Support:** Track Apple Neural Engine memory usage
- **Automatic Detection:** System detection for Apple Silicon vs Intel

### 4. VM Region Information

- **Memory Mapping:** Scan and analyze all process VM regions
- **Region Properties:** Track protection, inheritance, and share mode
- **Resident Pages:** Count physically resident pages per region
- **Pagination Events:** Detect when regions cross paging thresholds

## API Reference

### Initialization

```rust
// Create a new page migration tracker
let tracker = PageMigrationTracker::new()?;

// Update VM statistics
let stats = tracker.update_vm_stats()?;

// Update unified memory info (if available on Apple Silicon)
if let Some(unified_info) = tracker.update_unified_memory_info()? {
    println!("GPU memory: {} bytes", unified_info.gpu_memory_in_use);
}
```

### VM Statistics

```rust
// Get current VM statistics
let stats = tracker.update_vm_stats()?;
println!("Page-ins: {}", stats.page_ins);
println!("Page-outs: {}", stats.page_outs);
println!("Free pages: {}", stats.free_pages);
println!("Pagein delta: {}", stats.pagein_delta);
println!("Pageout delta: {}", stats.pageout_delta);
```

### Memory Pressure

```rust
use adapteros_memory::MemoryPressureLevel;

// Get current memory pressure level
let detailed_stats = tracker.get_detailed_stats();
match detailed_stats.current_pressure {
    MemoryPressureLevel::Normal => println!("Normal pressure"),
    MemoryPressureLevel::Warning => println!("Memory pressure warning"),
    MemoryPressureLevel::Critical => println!("Critical memory pressure!"),
}
```

### Migration Events

```rust
// Get recent migration events
let recent_events = tracker.get_recent_migrations(100); // Last 100 events

for event in recent_events {
    println!("Migration: {:?} {} bytes", event.migration_type, event.size_bytes);
    println!("  Source: {:?}", event.source_addr);
    println!("  Dest: {:?}", event.dest_addr);
    println!("  Pressure: {:?}", event.pressure_level);
}

// Clear event log
tracker.clear_events();
```

### Detailed Statistics

```rust
let stats = tracker.get_detailed_stats();
println!("Total migration events: {}", stats.total_migration_events);
println!("Page-in events: {}", stats.pagein_events);
println!("Page-out events: {}", stats.pageout_events);
println!("GPU↔CPU migrations: {}", stats.gpu_cpu_migrations);
println!("Total bytes migrated: {}", stats.total_bytes_migrated);
```

## Data Structures

### PageMigrationEvent

Represents a single memory migration event:

```rust
pub struct PageMigrationEvent {
    pub event_id: Uuid,
    pub migration_type: PageMigrationType,
    pub source_addr: Option<u64>,        // Source memory address
    pub dest_addr: Option<u64>,          // Destination address
    pub size_bytes: u64,                 // Migrated size
    pub timestamp: u128,                 // Microseconds since epoch
    pub pressure_level: MemoryPressureLevel,
    pub context: serde_json::Value,      // Additional metadata
}
```

### VMStatistics

VM page statistics snapshot:

```rust
pub struct VMStatistics {
    pub page_ins: u64,              // Total page-ins since boot
    pub page_outs: u64,             // Total page-outs since boot
    pub pagein_delta: u64,          // Delta since last query
    pub pageout_delta: u64,         // Delta since last query
    pub free_pages: u64,            // Currently free pages
    pub active_pages: u64,          // Active (in-use) pages
    pub inactive_pages: u64,        // Inactive pages
    pub timestamp: u128,            // Timestamp of snapshot
}
```

### UnifiedMemoryInfo

Apple Silicon unified memory information:

```rust
pub struct UnifiedMemoryInfo {
    pub gpu_memory_in_use: u64,
    pub gpu_memory_available: u64,
    pub shared_memory_pool: u64,
    pub gpu_to_cpu_migrations: u64,
    pub cpu_to_gpu_migrations: u64,
    pub ane_memory_in_use: u64,
    pub timestamp: u128,
}
```

## Implementation Details

### C/C++ Bridge Layer

The implementation uses an Objective-C++ bridge (`page_migration_iokit_impl.mm`) that interfaces with macOS kernel APIs:

**Key Functions:**
- `iokit_vm_init()` - Initialize IOKit monitoring
- `iokit_vm_get_stats()` - Fetch VM statistics via Mach APIs
- `iokit_vm_get_pagein_delta()` / `iokit_vm_get_pageout_delta()` - Get incremental changes
- `iokit_memory_pressure_level()` - Get memory pressure level
- `iokit_unified_memory_supported()` - Check Apple Silicon support
- `iokit_unified_memory_info()` - Fetch unified memory stats
- `iokit_vm_region_info()` - Get details for specific VM region
- `iokit_vm_scan_regions()` - Scan all VM regions

**Key Macros:**
- `VM_REGION_BASIC_INFO_64` - VM region info type for 64-bit systems
- `HOST_VM_INFO64_COUNT` - VM statistics array size
- `MAX_MIGRATION_EVENTS` - Circular buffer size (256 events)

### Thread Safety

- **Lock-based:** Uses `os_unfair_lock` for atomic state access
- **Lock-free reads:** Statistics cached and updated atomically
- **Circular buffer:** Migration events stored in fixed-size circular buffer

### Memory Management

- **Allocation:** VM regions allocated via Mach kernel APIs
- **Deallocation:** Automatic cleanup on tracker drop
- **Bounds:** Fixed-size event buffer (256 events max)
- **Cleanup:** `iokit_vm_cleanup()` called on teardown

## Platform Support

### macOS Requirements

- **Minimum:** macOS 10.13 (High Sierra)
- **Tested:** macOS 13+ (Ventura, Sonoma)
- **Frameworks:** IOKit, Foundation, CoreFoundation

### Architecture Support

- **Apple Silicon:** M1, M2, M3 (full unified memory support)
- **Intel:** Supported (page-in/out detection only)

### Non-macOS Fallback

On non-macOS platforms, the module compiles but returns empty data:

```rust
#[cfg(not(target_os = "macos"))]
pub fn new() -> crate::Result<Self> {
    warn!("Page migration tracking not available on this platform");
    // ... returns stub implementation
}
```

## Integration Points

### With Adapter Lifecycle

Page migration tracking integrates with the adapter lifecycle system:

1. **Eviction Detection:** High pageout deltas trigger eviction analysis
2. **Memory Pressure:** Critical pressure triggers priority-based eviction
3. **Cold/Warm States:** Page-out patterns inform state transitions
4. **Pinning System:** Critical adapters protected from eviction

### With Telemetry

Migration events are logged as telemetry:

```rust
info!(
    migration_type = %event.migration_type,
    size_bytes = event.size_bytes,
    pressure = ?event.pressure_level,
    "Page migration detected"
);
```

### With Memory Watchdog

The `MemoryWatchdog` coordinates with page migration tracker:

```rust
// In memory_watchdog.rs
let page_tracker = PageMigrationTracker::new()?;
page_tracker.update_vm_stats()?;

// Check for memory pressure
let stats = page_tracker.get_detailed_stats();
if stats.current_pressure == MemoryPressureLevel::Critical {
    // Trigger emergency eviction
}
```

## Performance Considerations

### Sampling Rate

- **VM Statistics:** ~1ms to fetch (kernel call overhead)
- **Memory Pressure:** ~0.1ms (cached calculation)
- **Region Scanning:** ~10-100ms (depends on region count)
- **Recommended Frequency:** 100-1000ms intervals

### CPU Impact

- **Minimal:** Uses efficient kernel APIs
- **Lock Contention:** Minimal (short lock durations)
- **Background Safe:** Can run on background threads

### Memory Overhead

- **Fixed:** ~256 * 64 bytes = 16KB for event buffer
- **Per-Tracker:** ~1KB for state structures
- **Negligible:** <50KB total system impact

## Troubleshooting

### Missing Memory Statistics

```
Failed to get VM stats: -1
```

**Cause:** Kernel API unavailable (non-macOS or sandbox restrictions)
**Fix:** Check platform and enable necessary entitlements

### Memory Pressure Not Updating

```
Memory pressure monitoring not available
```

**Cause:** IOKit unavailable or permissions issue
**Fix:** Run with elevated privileges or use sandbox entitlements

### Event Buffer Full

Migration event buffer has 256-event limit. If full:
- Oldest events are overwritten
- Call `clear_events()` periodically to free space

## Example: Full Integration

```rust
use adapteros_memory::{PageMigrationTracker, MemoryPressureLevel};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracker
    let tracker = PageMigrationTracker::new()?;

    // Monitor loop
    loop {
        // Update statistics
        let vm_stats = tracker.update_vm_stats()?;
        tracker.update_unified_memory_info()?;
        tracker.detect_migrations()?;

        // Check memory pressure
        let detailed = tracker.get_detailed_stats();
        match detailed.current_pressure {
            MemoryPressureLevel::Normal => {
                info!("Normal memory: {} free pages", vm_stats.free_pages);
            }
            MemoryPressureLevel::Warning => {
                warn!("Memory warning: {} free pages", vm_stats.free_pages);
            }
            MemoryPressureLevel::Critical => {
                error!("CRITICAL memory: {} free pages - triggering eviction",
                       vm_stats.free_pages);
                // Trigger emergency eviction
            }
        }

        // Log recent migrations
        let recent = tracker.get_recent_migrations(10);
        for event in recent {
            info!(
                "Migration: {:?} {} bytes @ {:?}",
                event.migration_type, event.size_bytes, event.pressure_level
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}
```

## Testing

Run tests:

```bash
cargo test -p adapteros-memory page_migration
```

Specific tests:

```bash
cargo test -p adapteros-memory test_page_migration_type_conversion
cargo test -p adapteros-memory test_detailed_stats
cargo test -p adapteros-memory test_event_clearing
```

## References

- **Mach VM:** `/usr/include/mach/vm_statistics.h`
- **IOKit:** `/System/Library/Frameworks/IOKit.framework`
- **Apple Silicon:** See `COREML_INTEGRATION.md` for unified memory details
- **Kernel:** `xnu/osfmk/kern/kern_memorystatus.h` (memory pressure)

## Related Documentation

- [docs/MEMORY_WATCHDOG.md](MEMORY_WATCHDOG.md) - Overall memory monitoring
- [docs/LIFECYCLE.md](LIFECYCLE.md) - Adapter lifecycle states
- [docs/DETERMINISTIC_EXECUTION.md](DETERMINISTIC_EXECUTION.md) - Determinism requirements
- [docs/PINNING_TTL.md](PINNING_TTL.md) - Adapter pinning system
