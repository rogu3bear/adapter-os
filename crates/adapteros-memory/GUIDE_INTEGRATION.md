# MTLHeap Observer Callbacks - Integration Guide

## Quick Start

### Step 1: Enable the Module

Add to `src/lib.rs`:

```rust
#[cfg(target_os = "macos")]
pub mod heap_observer_ffi;
```

### Step 2: Register Callbacks

```rust
use adapteros_memory::heap_observer_ffi;

fn setup_memory_monitoring() {
    // Register allocation success callback
    heap_observer_ffi::metal_heap_set_allocation_success_callback(
        Some(|heap_id, buffer_id, size, timestamp| {
            debug!("Allocated {} bytes on heap {}", size, heap_id);
        })
    );

    // Register deallocation callback
    heap_observer_ffi::metal_heap_set_deallocation_callback(
        Some(|heap_id, buffer_id, size, timestamp| {
            debug!("Deallocated {} bytes from heap {}", size, heap_id);
        })
    );

    // Register compaction callback
    heap_observer_ffi::metal_heap_set_compaction_callback(
        Some(|heap_id, recovered, blocks| {
            info!("Heap {} compacted: {} bytes recovered", heap_id, recovered);
        })
    );
}
```

### Step 3: Record Allocations

```rust
use adapteros_memory::heap_observer_ffi;

fn track_metal_allocation(heap_id: u64, buffer_id: u64, size: u64, addr: u64) {
    let result = unsafe {
        heap_observer_ffi::metal_heap_record_allocation(
            heap_id,
            buffer_id,
            size,
            0,      // offset
            addr,   // memory address
            1       // storage mode
        )
    };

    if result != 0 {
        error!("Failed to track allocation");
    }
}

fn track_metal_deallocation(heap_id: u64, buffer_id: u64) {
    let result = unsafe {
        heap_observer_ffi::metal_heap_record_deallocation(heap_id, buffer_id)
    };

    if result != 0 {
        error!("Failed to track deallocation");
    }
}
```

### Step 4: Collect Statistics

```rust
use adapteros_memory::heap_observer_ffi::{HeapStats, PerformanceMetrics};

fn print_memory_stats() {
    // Get global statistics
    let mut stats: HeapStats = unsafe { std::mem::zeroed() };
    let result = unsafe {
        heap_observer_ffi::metal_heap_get_global_stats(&mut stats)
    };

    if result == 0 {
        println!("Current memory: {} bytes", stats.current_used_bytes);
        println!("Peak memory: {} bytes", stats.peak_used_bytes);
        println!("Allocations: {}", stats.current_allocation_count);
        println!("Allocations lifetime: {}", stats.total_allocations_lifetime);
    }

    // Get performance metrics
    let metrics = PerformanceMetrics::collect();
    println!("Allocation rate: {:.2} ops/sec", metrics.allocation_rate_per_second);
    println!("Success rate: {:.1}%", metrics.allocation_success_rate());
}
```

## Integration Scenarios

### Scenario 1: Memory Watchdog

```rust
// In a background task that runs periodically
async fn memory_watchdog() {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let metrics = PerformanceMetrics::collect();

        // Alert if allocation rate is too high
        if metrics.allocation_rate_per_second > 1000.0 {
            warn!("High allocation rate: {:.0} ops/sec", metrics.allocation_rate_per_second);
        }

        // Alert if too many failures
        if metrics.failed_allocations > 10 {
            error!("Allocation failures: {}", metrics.failed_allocations);
        }

        // Check fragmentation
        let frag = unsafe {
            heap_observer_ffi::metal_heap_get_fragmentation_percentage(1)
        };
        if frag > 50.0 {
            warn!("High fragmentation: {:.1}%", frag);
            // Trigger compaction
        }
    }
}
```

### Scenario 2: Telemetry Integration

```rust
// Send metrics to telemetry system
fn report_memory_metrics() {
    let metrics = PerformanceMetrics::collect();

    telemetry::counter("memory.allocations", metrics.allocation_count);
    telemetry::counter("memory.deallocations", metrics.deallocation_count);
    telemetry::counter("memory.failures", metrics.failed_allocations);
    telemetry::gauge("memory.allocation_rate", metrics.allocation_rate_per_second);

    let mut stats: HeapStats = unsafe { std::mem::zeroed() };
    if unsafe { heap_observer_ffi::metal_heap_get_global_stats(&mut stats) } == 0 {
        telemetry::gauge("memory.current_bytes", stats.current_used_bytes as f64);
        telemetry::gauge("memory.peak_bytes", stats.peak_used_bytes as f64);
    }
}
```

### Scenario 3: Memory Pressure Handler

```rust
fn setup_memory_pressure_handler() {
    heap_observer_ffi::metal_heap_set_memory_pressure_callback(
        Some(|pressure_level, available_bytes| {
            match pressure_level {
                0 => debug!("Memory pressure: normal"),
                1 => {
                    warn!("Memory pressure: warning ({}MB available)", available_bytes / 1_000_000);
                    // Evict cold adapters
                    evict_cold_adapters();
                }
                2 => {
                    error!("Memory pressure: critical ({}MB available)", available_bytes / 1_000_000);
                    // Emergency cleanup
                    emergency_memory_cleanup();
                    // Notify users
                    notify_memory_crisis();
                }
                _ => {}
            }
        })
    );
}
```

### Scenario 4: Allocation Failure Recovery

```rust
fn setup_failure_handling() {
    heap_observer_ffi::metal_heap_set_allocation_failure_callback(
        Some(|heap_id, requested_size, error_code| {
            error!("Allocation failed: heap={}, size={}, error={}", heap_id, requested_size, error_code);

            // Collect diagnostics
            let metrics = PerformanceMetrics::collect();
            let mut stats: HeapStats = unsafe { std::mem::zeroed() };
            unsafe { heap_observer_ffi::metal_heap_get_stats(heap_id, &mut stats) };

            // Log detailed information
            error!("Memory state: used={}, peak={}, allocations={}",
                   stats.current_used_bytes,
                   stats.peak_used_bytes,
                   stats.current_allocation_count);

            // Try recovery
            if metrics.compaction_count == 0 {
                // Haven't compacted yet, try that
                trigger_heap_compaction(heap_id);
            } else {
                // Already compacted, try garbage collection
                trigger_garbage_collection();
            }
        })
    );
}
```

## Advanced Usage

### Multi-Handler Pattern

```rust
use adapteros_memory::heap_observer_ffi::HeapObserverCallbackManager;

let manager = HeapObserverCallbackManager::new();

// Register multiple handlers for same event
manager.on_allocation_success(|heap_id, buffer_id, size, ts| {
    // Handler 1: Update metrics
    update_metrics(size);
})?;

manager.on_allocation_success(|heap_id, buffer_id, size, ts| {
    // Handler 2: Update dashboard
    update_dashboard(heap_id, size);
})?;

manager.on_allocation_success(|heap_id, buffer_id, size, ts| {
    // Handler 3: Check thresholds
    if size > 100 * 1024 * 1024 {
        warn!("Large allocation: {} bytes", size);
    }
})?;
```

### Periodic Health Check

```rust
async fn memory_health_check() {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let metrics = PerformanceMetrics::collect();
        let mut stats: HeapStats = unsafe { std::mem::zeroed() };
        let result = unsafe { heap_observer_ffi::metal_heap_get_global_stats(&mut stats) };

        if result != 0 {
            error!("Failed to get memory stats");
            continue;
        }

        // Calculate metrics
        let utilization = (stats.current_used_bytes as f64 / stats.total_heap_size as f64) * 100.0;
        let success_rate = metrics.allocation_success_rate();

        println!("Memory Health Check:");
        println!("  Utilization: {:.1}%", utilization);
        println!("  Success rate: {:.1}%", success_rate);
        println!("  Allocation rate: {:.1} ops/sec", metrics.allocation_rate_per_second);
        println!("  Net allocations: {}", metrics.net_allocations());
        println!("  Page faults: {}", metrics.page_fault_count);

        // Check health
        if utilization > 90.0 {
            error!("High memory utilization!");
        }
        if success_rate < 99.0 {
            error!("High allocation failure rate!");
        }
    }
}
```

## Debugging Tips

### Enable Detailed Logging

```rust
// Set environment variable before running
// RUST_LOG=debug cargo run

// Or programmatically
tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();

// Then check logs for detailed memory events
```

### Memory Snapshot for Analysis

```rust
fn dump_memory_snapshot() {
    let mut stats: HeapStats = unsafe { std::mem::zeroed() };
    let result = unsafe { heap_observer_ffi::metal_heap_get_global_stats(&mut stats) };

    if result == 0 {
        println!("=== Memory Snapshot ===");
        println!("Current: {} MB", stats.current_used_bytes / 1_000_000);
        println!("Peak: {} MB", stats.peak_used_bytes / 1_000_000);
        println!("Capacity: {} MB", stats.total_heap_size / 1_000_000);
        println!("Allocations: {} (peak: {})",
                 stats.current_allocation_count,
                 stats.peak_allocation_count);
        println!("Lifetime: {} allocations, {} deallocations",
                 stats.total_allocations_lifetime,
                 stats.total_deallocations_lifetime);
    }

    let error_msg = unsafe {
        let ptr = heap_observer_ffi::metal_heap_get_last_error();
        if !ptr.is_null() {
            std::ffi::CStr::from_ptr(ptr).to_string_lossy()
        } else {
            "No error".into()
        }
    };
    println!("Last error: {}", error_msg);
}
```

## Performance Best Practices

1. **Avoid Heavy Operations in Callbacks**
   ```rust
   // Bad: Don't do I/O in callback
   metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
       write_to_disk(); // Too slow!
   }));

   // Good: Queue for async handling
   metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
       EVENT_QUEUE.push((heap_id, buffer_id, size));
   }));
   ```

2. **Sample High-Frequency Events**
   ```rust
   // Track only large allocations
   metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
       if size > 10_000_000 { // >10MB
           handle_large_allocation(heap_id, size);
       }
   }));
   ```

3. **Batch Statistics Collection**
   ```rust
   // Collect once, don't poll continuously
   let metrics = PerformanceMetrics::collect();
   // Use metrics multiple times before collecting again
   ```

## Testing Your Integration

```bash
# Run with memory observer enabled
RUST_LOG=debug cargo run

# Check that callbacks are being invoked
cargo test test_memory_callbacks --lib -- --nocapture

# Monitor allocation patterns
cargo test test_allocation_tracking -- --nocapture
```

## Troubleshooting

### Callbacks Not Invoked
- Ensure `metal_heap_record_allocation()` is called before expecting callback
- Check that callback pointer is not null
- Verify memory observer is initialized

### High Memory Usage
- Enable `RUST_LOG=debug` to see allocation events
- Call `metal_heap_get_global_stats()` to check peak memory
- Look for large allocations via callback logging

### Compilation Errors
- Ensure `-framework Metal` is linked on macOS
- Verify C++ compilation flags include `-std=c++17`
- Check that header paths are correct

## Support

For issues or questions:
1. Check [REFERENCE_HEAP_OBSERVER_CALLBACKS.md](docs/REFERENCE_HEAP_OBSERVER_CALLBACKS.md)
2. Review example code in this file
3. Enable debug logging
4. Check error messages from `metal_heap_get_last_error()`
