# Metal Heap Observer - Code Examples

## Quick Reference Examples

### Example 1: Basic Initialization and Allocation Tracking

```rust
use adapteros_memory::heap_observer::{
    ffi_metal_heap_record_allocation,
    ffi_metal_heap_record_deallocation,
    ffi_metal_heap_update_state,
    ffi_metal_heap_get_metrics,
    FFIMetalMemoryMetrics,
};

fn main() {
    // Initialize the Metal heap observer
    unsafe {
        let result = ffi_metal_heap_observer_init();
        if result == 0 {
            eprintln!("Failed to initialize Metal heap observer");
            return;
        }
    }

    // Simulate heap 1 with allocations
    let heap_id = 1u64;

    // Record multiple allocations
    let allocations = vec![
        (100u64, 1024u64, 0u64, 0x1000u64, 1u32),    // 1KB
        (101u64, 2048u64, 1024u64, 0x1400u64, 1u32), // 2KB
        (102u64, 512u64, 3072u64, 0x1c00u64, 1u32),  // 512B
    ];

    for (buffer_id, size, offset, addr, storage_mode) in allocations {
        unsafe {
            let result = ffi_metal_heap_record_allocation(
                heap_id,
                buffer_id,
                size,
                offset,
                addr,
                storage_mode,
            );

            if result > 0 {
                println!("Recorded allocation {} ({} bytes)", buffer_id, size);
            }
        }
    }

    // Update heap state
    unsafe {
        ffi_metal_heap_update_state(heap_id, 65536, 3584);
    }

    // Query metrics
    unsafe {
        let mut metrics = FFIMetalMemoryMetrics {
            total_allocated: 0,
            total_heap_size: 0,
            total_heap_used: 0,
            allocation_count: 0,
            heap_count: 0,
            overall_fragmentation: 0.0,
            utilization_pct: 0.0,
            migration_event_count: 0,
        };

        let result = ffi_metal_heap_get_metrics(&mut metrics);
        if result == 0 {
            println!("\n=== Metal Heap Metrics ===");
            println!("Total allocated: {} bytes", metrics.total_allocated);
            println!("Total heap size: {} bytes", metrics.total_heap_size);
            println!("Total heap used: {} bytes", metrics.total_heap_used);
            println!("Active allocations: {}", metrics.allocation_count);
            println!("Active heaps: {}", metrics.heap_count);
            println!("Fragmentation: {:.2}%", metrics.overall_fragmentation * 100.0);
            println!("Utilization: {:.2}%", metrics.utilization_pct);
        }
    }

    // Record deallocations
    for (buffer_id, _, _, _, _) in allocations {
        unsafe {
            ffi_metal_heap_record_deallocation(buffer_id);
        }
    }
}
```

### Example 2: Fragmentation Analysis

```rust
use adapteros_memory::heap_observer::{
    ffi_metal_heap_get_fragmentation,
    FFIFragmentationMetrics,
};

fn analyze_fragmentation() {
    unsafe {
        let mut metrics = FFIFragmentationMetrics {
            fragmentation_ratio: 0.0,
            external_fragmentation: 0.0,
            internal_fragmentation: 0.0,
            free_blocks: 0,
            total_free_bytes: 0,
            avg_free_block_size: 0,
            largest_free_block: 0,
            compaction_efficiency: 0.0,
        };

        let result = ffi_metal_heap_get_fragmentation(&mut metrics);

        if result == 0 {
            println!("\n=== Fragmentation Metrics ===");
            println!("Overall ratio: {:.2}%", metrics.fragmentation_ratio * 100.0);
            println!("External frag: {:.2}%", metrics.external_fragmentation * 100.0);
            println!("Internal frag: {:.2}%", metrics.internal_fragmentation * 100.0);
            println!("Free blocks: {}", metrics.free_blocks);
            println!("Total free: {} bytes", metrics.total_free_bytes);
            println!("Avg free block: {} bytes", metrics.avg_free_block_size);
            println!("Largest free block: {} bytes", metrics.largest_free_block);
            println!("Compaction efficiency: {:.2}%", metrics.compaction_efficiency * 100.0);

            if metrics.fragmentation_ratio > 0.8 {
                println!("WARNING: Critical fragmentation detected!");
            } else if metrics.fragmentation_ratio > 0.5 {
                println!("WARNING: High fragmentation detected.");
            }
        }
    }
}
```

### Example 3: Heap State Enumeration

```rust
use adapteros_memory::heap_observer::{
    ffi_metal_heap_get_all_states,
    FFIHeapState,
};

fn list_all_heaps() {
    unsafe {
        let max_heaps = 64u32;
        let mut heaps: Vec<FFIHeapState> = vec![std::mem::zeroed(); max_heaps as usize];

        let count = ffi_metal_heap_get_all_states(heaps.as_mut_ptr(), max_heaps);

        if count > 0 {
            println!("\n=== Heap States ===");
            println!("{:>8} {:>12} {:>12} {:>8} {:>6} {:>8}",
                     "Heap ID", "Total", "Used", "Allocs", "Frag%", "LargestFree");
            println!("{}", "-".repeat(70));

            for i in 0..(count as usize) {
                let heap = heaps[i];
                let frag_pct = heap.fragmentation_ratio * 100.0;
                println!("{:08x} {:12} {:12} {:8} {:6.2} {:8}",
                         heap.heap_id,
                         heap.total_size,
                         heap.used_size,
                         heap.allocation_count,
                         frag_pct,
                         heap.largest_free_block);
            }
        } else if count < 0 {
            eprintln!("Error querying heap states: {}", count);
        } else {
            println!("No heaps found.");
        }
    }
}
```

### Example 4: Page Migration Event Tracking

```rust
use adapteros_memory::heap_observer::{
    ffi_metal_heap_get_migration_events,
    FFIPageMigrationEvent,
};

fn track_migration_events() {
    unsafe {
        let max_events = 256u32;
        let mut events: Vec<FFIPageMigrationEvent> = vec![std::mem::zeroed(); max_events as usize];

        let count = ffi_metal_heap_get_migration_events(events.as_mut_ptr(), max_events);

        if count > 0 {
            println!("\n=== Page Migration Events ===");
            println!("{:>4} {:>8} {:>20} {:>20} {:>10}",
                     "Type", "Count", "Source", "Dest", "Size");
            println!("{}", "-".repeat(70));

            for i in 0..(count as usize) {
                let event = events[i];
                let migration_type = match event.migration_type {
                    1 => "PageOut",
                    2 => "PageIn",
                    3 => "Relocate",
                    4 => "Compact",
                    5 => "Evict",
                    _ => "Unknown",
                };

                println!("{:>4} {:08x} {:20x} {:20x} {:>10}",
                         migration_type,
                         event.event_id_high,
                         event.source_addr,
                         event.dest_addr,
                         event.size_bytes);
            }
        } else {
            println!("No migration events recorded.");
        }
    }
}
```

### Example 5: Continuous Monitoring Loop

```rust
use std::time::Duration;
use std::thread;
use adapteros_memory::heap_observer::{
    ffi_metal_heap_get_metrics,
    FFIMetalMemoryMetrics,
};

fn monitor_heap(interval: Duration, duration: Duration) {
    let start = std::time::Instant::now();
    let mut iteration = 0;

    while start.elapsed() < duration {
        unsafe {
            let mut metrics = FFIMetalMemoryMetrics {
                total_allocated: 0,
                total_heap_size: 0,
                total_heap_used: 0,
                allocation_count: 0,
                heap_count: 0,
                overall_fragmentation: 0.0,
                utilization_pct: 0.0,
                migration_event_count: 0,
            };

            if ffi_metal_heap_get_metrics(&mut metrics) == 0 {
                iteration += 1;
                println!("[{:3}] Allocs: {:3} | Total: {:8} | Used: {:8} | Frag: {:5.1}% | Events: {}",
                         iteration,
                         metrics.allocation_count,
                         metrics.total_allocated,
                         metrics.total_heap_used,
                         metrics.overall_fragmentation * 100.0,
                         metrics.migration_event_count);
            }
        }

        thread::sleep(interval);
    }
}
```

### Example 6: Error Handling with Error Messages

```rust
use adapteros_memory::heap_observer::ffi_metal_heap_get_last_error;

fn get_error_message() -> String {
    unsafe {
        let mut buffer = vec![0i8; 256];
        let bytes_written = ffi_metal_heap_get_last_error(buffer.as_mut_ptr(), buffer.len());

        if bytes_written > 0 {
            let cstr = std::ffi::CStr::from_ptr(buffer.as_ptr());
            cstr.to_string_lossy().into_owned()
        } else {
            "Unknown error".to_string()
        }
    }
}

fn main_with_error_handling() {
    unsafe {
        let result = ffi_metal_heap_observer_init();
        if result == 0 {
            eprintln!("Init failed: {}", get_error_message());
            return;
        }
    }

    // ... do work ...

    // Check for errors periodically
    let error_msg = get_error_message();
    if !error_msg.is_empty() && error_msg != "No error" {
        eprintln!("Last error: {}", error_msg);
    }
}
```

### Example 7: Safe FFI Wrapper

```rust
use adapteros_memory::heap_observer::*;
use std::sync::OnceLock;

/// Safe wrapper around Metal heap observer
pub struct MetalHeapMonitor {
    initialized: bool,
}

impl MetalHeapMonitor {
    /// Initialize the monitor (safe wrapper)
    pub fn new() -> Result<Self, String> {
        unsafe {
            if ffi_metal_heap_observer_init() != 0 {
                Ok(MetalHeapMonitor { initialized: true })
            } else {
                Err("Failed to initialize Metal heap observer".to_string())
            }
        }
    }

    /// Record an allocation (safe wrapper)
    pub fn record_allocation(
        &self,
        heap_id: u64,
        buffer_id: u64,
        size: u64,
        offset: u64,
        addr: u64,
        storage_mode: u32,
    ) -> Result<(), String> {
        if !self.initialized {
            return Err("Observer not initialized".to_string());
        }

        unsafe {
            if ffi_metal_heap_record_allocation(
                heap_id,
                buffer_id,
                size,
                offset,
                addr,
                storage_mode,
            ) > 0 {
                Ok(())
            } else {
                Err("Failed to record allocation".to_string())
            }
        }
    }

    /// Get current metrics (safe wrapper)
    pub fn get_metrics(&self) -> Result<FFIMetalMemoryMetrics, String> {
        if !self.initialized {
            return Err("Observer not initialized".to_string());
        }

        unsafe {
            let mut metrics = FFIMetalMemoryMetrics {
                total_allocated: 0,
                total_heap_size: 0,
                total_heap_used: 0,
                allocation_count: 0,
                heap_count: 0,
                overall_fragmentation: 0.0,
                utilization_pct: 0.0,
                migration_event_count: 0,
            };

            if ffi_metal_heap_get_metrics(&mut metrics) == 0 {
                Ok(metrics)
            } else {
                Err("Failed to get metrics".to_string())
            }
        }
    }

    /// Clear all observation data (safe wrapper)
    pub fn clear(&self) -> Result<(), String> {
        if !self.initialized {
            return Err("Observer not initialized".to_string());
        }

        unsafe {
            if ffi_metal_heap_clear() != 0 {
                Ok(())
            } else {
                Err("Failed to clear observer data".to_string())
            }
        }
    }
}

// Usage
fn example_safe_wrapper() {
    let monitor = match MetalHeapMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to create monitor: {}", e);
            return;
        }
    };

    // Record allocations safely
    if let Err(e) = monitor.record_allocation(1, 100, 1024, 0, 0x1000, 1) {
        eprintln!("Failed to record: {}", e);
    }

    // Query metrics safely
    match monitor.get_metrics() {
        Ok(metrics) => {
            println!("Allocations: {}", metrics.allocation_count);
        }
        Err(e) => {
            eprintln!("Failed to get metrics: {}", e);
        }
    }

    // Clear safely
    if let Err(e) = monitor.clear() {
        eprintln!("Failed to clear: {}", e);
    }
}
```

### Example 8: Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_heap_observer_initialization() {
        unsafe {
            let result = ffi_metal_heap_observer_init();
            assert_ne!(result, 0, "Observer initialization failed");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_allocation_recording() {
        unsafe {
            ffi_metal_heap_observer_init();

            let result = ffi_metal_heap_record_allocation(
                1,    // heap_id
                100,  // buffer_id
                1024, // size
                0,    // offset
                0x1000, // addr
                1,    // storage_mode
            );

            assert!(result > 0, "Failed to record allocation");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_metrics_retrieval() {
        unsafe {
            ffi_metal_heap_observer_init();

            let mut metrics = FFIMetalMemoryMetrics {
                total_allocated: 0,
                total_heap_size: 0,
                total_heap_used: 0,
                allocation_count: 0,
                heap_count: 0,
                overall_fragmentation: 0.0,
                utilization_pct: 0.0,
                migration_event_count: 0,
            };

            let result = ffi_metal_heap_get_metrics(&mut metrics);
            assert_eq!(result, 0, "Failed to get metrics");
            assert!(metrics.fragmentation_ratio >= 0.0);
            assert!(metrics.fragmentation_ratio <= 1.0);
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_null_pointer_safety() {
        unsafe {
            let result = ffi_metal_heap_get_metrics(std::ptr::null_mut());
            assert!(result < 0, "Should return error for null pointer");
        }
    }
}
```

## Key Patterns

### Pattern 1: Initialization Guard

```rust
use std::sync::OnceLock;

static MONITOR: OnceLock<MetalHeapMonitor> = OnceLock::new();

fn get_monitor() -> &'static MetalHeapMonitor {
    MONITOR.get_or_init(|| {
        MetalHeapMonitor::new().expect("Failed to initialize monitor")
    })
}
```

### Pattern 2: Metrics Polling

```rust
fn check_heap_health() -> bool {
    unsafe {
        let mut metrics = FFIMetalMemoryMetrics::default();
        if ffi_metal_heap_get_metrics(&mut metrics) == 0 {
            metrics.overall_fragmentation < 0.75
        } else {
            true // Assume healthy on error
        }
    }
}
```

### Pattern 3: Adaptive Sampling

```rust
fn should_record() -> bool {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);

    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    count % 10 == 0  // Record every 10th allocation
}
```

## Building and Testing

```bash
# Build the memory crate
cargo build -p adapteros-memory

# Run examples
cargo run --example heap_observer

# Run tests
cargo test -p adapteros-memory heap_observer

# Run with output
cargo test -p adapteros-memory -- --nocapture
```

## Performance Tips

1. **Minimize Sync Queries:** Batch queries when possible
2. **Use Sampling:** Enable sampling for high-frequency operations
3. **Async Recording:** Allocations/deallocations are async (non-blocking)
4. **Periodic Polling:** Check metrics every 100ms, not every microsecond
5. **Clear Data:** Call clear() periodically to manage event buffer size
