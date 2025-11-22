# Metal Heap Observer Callbacks

Comprehensive allocation tracking and memory event system for MTLHeap operations on macOS.

## Overview

The heap observer provides real-time monitoring of Metal heap allocations with callback-based event handling. Features:

- **Allocation tracking** - Observe successful/failed allocations with full context
- **Memory events** - Deallocation, compaction, and pressure notifications
- **Statistics collection** - Current usage, peak tracking, lifetime metrics
- **Performance counters** - Allocation rate, fragmentation, page faults

## Architecture

### Components

| Component | File | Purpose |
|-----------|------|---------|
| **C++ Implementation** | `heap_observer_callbacks.mm` | Core observer with atomic counters |
| **C Header** | `include/heap_observer_callbacks.h` | FFI declarations |
| **Rust FFI Bindings** | `src/heap_observer_ffi.rs` | Safe Rust wrapper |

### Callback Types

```c
// Allocation success: (heap_id, buffer_id, size_bytes, timestamp_us)
typedef void (*AllocationSuccessCallback)(uint64_t, uint64_t, uint64_t, uint64_t);

// Allocation failure: (heap_id, requested_size, error_code)
typedef void (*AllocationFailureCallback)(uint64_t, uint64_t, int32_t);

// Deallocation: (heap_id, buffer_id, size_bytes, timestamp_us)
typedef void (*DeallocationCallback)(uint64_t, uint64_t, uint64_t, uint64_t);

// Compaction: (heap_id, bytes_recovered, blocks_compacted)
typedef void (*CompactionCallback)(uint64_t, uint64_t, uint32_t);

// Memory pressure: (pressure_level, available_bytes)
typedef void (*MemoryPressureCallback)(int32_t, uint64_t);
```

### Thread Safety

- **Allocation tracking**: Protected by `os_unfair_lock`
- **Callback invocation**: Callbacks invoked **outside** lock to prevent deadlocks
- **Performance counters**: Atomic operations with memory ordering guarantees
- **Statistics**: Lock-free reads for global counters, locked access for detailed stats

## Usage

### 1. Registering Callbacks

```rust
use adapteros_memory::heap_observer_ffi;

// Register allocation success callback
heap_observer_ffi::metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
    println!("Allocated {} bytes on heap {}", size, heap_id);
}));

// Register deallocation callback
heap_observer_ffi::metal_heap_set_deallocation_callback(Some(|heap_id, buffer_id, size, ts| {
    println!("Deallocated {} bytes from heap {}", size, heap_id);
}));

// Register compaction callback
heap_observer_ffi::metal_heap_set_compaction_callback(Some(|heap_id, recovered, blocks| {
    println!("Heap {} compacted: {} bytes recovered from {} blocks", heap_id, recovered, blocks);
}));
```

### 2. Recording Allocations

```rust
let heap_id = 0x1234u64;
let buffer_id = 1u64;
let size_bytes = 1024u64;

// Record allocation (triggers allocation_success callback)
let result = heap_observer_ffi::metal_heap_record_allocation(
    heap_id,
    buffer_id,
    size_bytes,
    0,              // offset
    0x4000,         // memory address
    1               // storage mode
);

assert_eq!(result, 0);
```

### 3. Collecting Statistics

```rust
use adapteros_memory::heap_observer_ffi::{HeapStats, metal_heap_get_global_stats};

// Get global statistics
let mut stats: HeapStats = unsafe { std::mem::zeroed() };
let result = unsafe { metal_heap_get_global_stats(&mut stats) };

if result == 0 {
    println!("Current memory: {} bytes", stats.current_used_bytes);
    println!("Peak memory: {} bytes", stats.peak_used_bytes);
    println!("Allocations: {}", stats.current_allocation_count);
}
```

### 4. Performance Monitoring

```rust
use adapteros_memory::heap_observer_ffi::PerformanceMetrics;

let metrics = PerformanceMetrics::collect();

println!("Allocation rate: {:.2} ops/sec", metrics.allocation_rate_per_second);
println!("Success rate: {:.1}%", metrics.allocation_success_rate());
println!("Net allocations: {}", metrics.net_allocations());
println!("Page faults: {}", metrics.page_fault_count);
```

## Allocation Tracking

### Success Flow

1. Call `metal_heap_record_allocation()`
2. Update internal tracking (thread-safe)
3. Update peak memory statistics
4. Invoke `AllocationSuccessCallback` outside lock
5. Return 0 (success)

### Failure Flow

1. Call `metal_heap_record_allocation_failure()`
2. Increment failure counter
3. Invoke `AllocationFailureCallback` outside lock
4. Return error code

### Deallocation Flow

1. Call `metal_heap_record_deallocation()`
2. Find and remove allocation from tracking
3. Update statistics
4. Invoke `DeallocationCallback` outside lock
5. Return 0 (success) or error

## Memory Event Callbacks

### Allocation Success

```c
void on_allocation_success(uint64_t heap_id, uint64_t buffer_id,
                          uint64_t size_bytes, uint64_t timestamp_us)
```

- Invoked after successful allocation
- Safe to perform I/O (not in critical path)
- Can update application metrics

### Allocation Failure

```c
void on_allocation_failure(uint64_t heap_id, uint64_t requested_size,
                          int32_t error_code)
```

- Invoked when allocation fails
- `error_code` provides Metal error details
- Good place for retry logic or memory cleanup

### Deallocation

```c
void on_deallocation(uint64_t heap_id, uint64_t buffer_id,
                    uint64_t size_bytes, uint64_t timestamp_us)
```

- Invoked on successful deallocation
- Useful for cache invalidation
- Tracks lifetime of allocations

### Heap Compaction

```c
void on_compaction(uint64_t heap_id, uint64_t bytes_recovered,
                  uint32_t blocks_compacted)
```

- Invoked when heap is compacted
- `bytes_recovered`: memory freed by consolidation
- `blocks_compacted`: number of fragments combined

### Memory Pressure

```c
void on_memory_pressure(int32_t pressure_level, uint64_t available_bytes)
```

- Pressure levels: 0 (normal), 1 (warning), 2 (critical)
- `available_bytes`: system memory remaining
- Called by memory pressure monitor

## Heap Statistics

### HeapStats Structure

```c
typedef struct {
    uint64_t current_used_bytes;           // Currently allocated
    uint64_t peak_used_bytes;              // Maximum ever used
    uint64_t total_heap_size;              // Total heap capacity
    uint64_t current_allocation_count;     // Active allocations
    uint64_t peak_allocation_count;        // Maximum concurrent
    uint64_t total_allocations_lifetime;   // Total allocated (count)
    uint64_t total_deallocations_lifetime; // Total deallocated (count)
    float fragmentation_ratio;             // 0.0 (none) to 1.0 (extreme)
    float page_fault_rate;                 // Faults per operation
    uint64_t last_update_us;               // Last update timestamp
} HeapStats;
```

### Per-Heap Statistics

```rust
// Get stats for specific heap
let mut stats: HeapStats = unsafe { std::mem::zeroed() };
unsafe { metal_heap_get_stats(heap_id, &mut stats); }
```

### Global Statistics

```rust
// Get aggregated stats across all heaps
let mut global_stats: HeapStats = unsafe { std::mem::zeroed() };
unsafe { metal_heap_get_global_stats(&mut global_stats); }
```

## Performance Counters

### Allocation Rate

```rust
let rate_per_sec = unsafe { metal_heap_get_allocation_rate_per_second() };
println!("Allocations: {:.1}/sec", rate_per_sec);
```

Calculated over rolling window, reflects recent activity.

### Fragmentation Percentage

```rust
let frag_pct = unsafe { metal_heap_get_fragmentation_percentage(heap_id) };
println!("Fragmentation: {:.1}%", frag_pct);
```

- 0% = fully packed
- 100% = completely empty
- >50% = consider compaction

### Page Fault Tracking

```rust
let faults = unsafe { metal_heap_get_page_fault_count() };
println!("Page faults: {}", faults);
```

Indicates memory pressure and potential swapping.

## Integration Patterns

### 1. Real-Time Monitoring

```rust
metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
    // Update metrics dashboard
    // Log allocation if large
    // Check memory pressure
}));
```

### 2. Memory Pressure Response

```rust
metal_heap_set_memory_pressure_callback(Some(|pressure, available| {
    match pressure {
        0 => println!("Normal"),
        1 => evict_cold_adapters(),
        2 => emergency_cleanup(),
        _ => ()
    }
}));
```

### 3. Compaction Tracking

```rust
metal_heap_set_compaction_callback(Some(|heap_id, recovered, blocks| {
    if recovered > 10 * 1024 * 1024 {  // >10MB
        info!("Significant compaction: {} bytes recovered", recovered);
    }
}));
```

### 4. Failure Analysis

```rust
metal_heap_set_allocation_failure_callback(Some(|heap_id, size, error| {
    error!("Allocation failed: size={}, error={}", size, error);
    collect_memory_dump();
}));
```

## Performance Considerations

### Lock Contention

- Callbacks invoked **outside** lock
- Fast-path for statistics (atomic loads)
- No nested locking

### Memory Overhead

- Per-allocation record: ~48 bytes
- Per-heap record: ~64 bytes
- Reasonable for tracking < 10k allocations

### CPU Overhead

- Atomic increments: negligible
- Lock contention: minimal (short critical sections)
- Callback invocation: depends on handler complexity

## Error Handling

```rust
// Check for errors
let result = unsafe { metal_heap_record_allocation(...) };
if result < 0 {
    let error_msg = unsafe {
        let ptr = metal_heap_get_last_error();
        std::ffi::CStr::from_ptr(ptr).to_string_lossy()
    };
    eprintln!("Allocation tracking failed: {}", error_msg);
}
```

Error codes:
- `0` = success
- `-1` = invalid input
- `-2` = allocation not found
- `-3` = internal error

## Testing

### Unit Tests

```bash
cargo test -p adapteros-memory --lib heap_observer_ffi
```

### Integration Tests

```bash
cargo test -p adapteros-memory --test '*' --features metal
```

### Performance Tests

```bash
cargo bench -p adapteros-memory --bench heap_observer
```

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| macOS 12+ | Supported | Requires Metal framework |
| macOS 11 | Supported | May have limited ANE support |
| iOS/tvOS | Not supported | No Metal heap API |
| Non-Apple | No-op implementations | Safe stubs |

## Debugging

### Enable Tracing

```rust
// Set RUST_LOG=debug for detailed logging
RUST_LOG=debug ./target/release/app
```

### Memory Dumps

```rust
let stats = unsafe {
    let mut s: HeapStats = std::mem::zeroed();
    metal_heap_get_global_stats(&mut s);
    s
};

println!("Memory snapshot: {:#?}", stats);
```

### Callback Inspection

```rust
metal_heap_set_allocation_success_callback(Some(|heap_id, buffer_id, size, ts| {
    eprintln!("ALLOC heap={} buf={} size={} ts={}", heap_id, buffer_id, size, ts);
}));
```

## References

- [MTLHeap documentation](https://developer.apple.com/documentation/metal/mtlheap)
- [Memory management best practices](../docs/MEMORY_MANAGEMENT.md)
- [Performance tuning guide](../docs/PERFORMANCE_TUNING.md)
