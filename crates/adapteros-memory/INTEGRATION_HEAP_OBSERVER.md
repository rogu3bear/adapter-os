# Metal Heap Observer - Integration Guide

## Quick Start

The Metal heap observer is now fully implemented and integrated. Use it from Rust via the FFI wrapper functions in `src/heap_observer.rs`.

### Basic Usage Example

```rust
use adapteros_memory::heap_observer::{
    ffi_metal_heap_record_allocation,
    ffi_metal_heap_record_deallocation,
    ffi_metal_heap_get_metrics,
    FFIMetalMemoryMetrics,
};

fn main() {
    // Record an allocation
    ffi_metal_heap_record_allocation(
        heap_id: 1,
        buffer_id: 100,
        size: 1024,
        offset: 0,
        addr: 0x1000,
        storage_mode: 1, // MTLStorageModeShared
    );

    // Record a deallocation
    ffi_metal_heap_record_deallocation(100);

    // Get current metrics
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

    let result = unsafe {
        ffi_metal_heap_get_metrics(&mut metrics)
    };

    if result == 0 {
        println!("Total allocated: {}", metrics.total_allocated);
        println!("Fragmentation: {:.1}%", metrics.overall_fragmentation * 100.0);
    }
}
```

## File Structure

```
crates/adapteros-memory/
├── src/
│   ├── heap_observer.rs            # Rust FFI bindings & wrapper types
│   ├── heap_observer_impl.mm       # Objective-C++ implementation (NEW)
│   ├── lib.rs                      # Library exports
│   └── [other modules]
├── include/
│   └── heap_observer.h             # C FFI header
├── build.rs                        # Build script for Objective-C++ compilation
├── Cargo.toml                      # Dependencies
└── HEAP_OBSERVER_IMPL.md           # Implementation documentation (NEW)
```

## Build Configuration

The build process automatically compiles the Objective-C++ code on macOS:

1. **build.rs** runs during `cargo build`
2. Invokes `cc::Build` to compile `src/heap_observer_impl.mm`
3. Applies framework linking for Metal, Foundation, IOKit, CoreFoundation
4. Creates static library `libheap_observer.a`
5. Rust FFI bindings in `heap_observer.rs` link against it

### Rebuild Triggers

The build script watches for changes to:
- `src/heap_observer_impl.mm`
- `include/heap_observer.h`

If either changes, the build automatically recompiles the Objective-C++ code.

## FFI API Reference

All functions return status codes or counts:
- **Success:** Return value >= 0
- **Error:** Return value < 0 (typically -1 or -2)

### Initialization
```rust
pub fn ffi_metal_heap_observer_init() -> i32;
```
Call once at startup to initialize singleton observer.

### Tracking
```rust
pub fn ffi_metal_heap_record_allocation(
    heap_id: u64, buffer_id: u64, size: u64, offset: u64,
    addr: u64, storage_mode: u32
) -> i32;

pub fn ffi_metal_heap_record_deallocation(buffer_id: u64) -> i32;

pub fn ffi_metal_heap_update_state(
    heap_id: u64, total_size: u64, used_size: u64
) -> i32;
```

### Metrics Queries
```rust
pub fn ffi_metal_heap_get_fragmentation(
    out_metrics: *mut FFIFragmentationMetrics
) -> i32;

pub fn ffi_metal_heap_get_all_states(
    out_heaps: *mut FFIHeapState, max_heaps: u32
) -> i32;

pub fn ffi_metal_heap_get_metrics(
    out_metrics: *mut FFIMetalMemoryMetrics
) -> i32;

pub fn ffi_metal_heap_get_migration_events(
    out_events: *mut FFIPageMigrationEvent, max_events: u32
) -> i32;
```

### Utility
```rust
pub fn ffi_metal_heap_clear() -> i32;

pub fn ffi_metal_heap_get_last_error(
    buffer: *mut i8, buffer_len: usize
) -> usize;
```

## Data Structures

### FFIHeapState
```rust
pub struct FFIHeapState {
    pub heap_id: u64,
    pub total_size: u64,
    pub used_size: u64,
    pub allocation_count: u32,
    pub fragmentation_ratio: f32,
    pub avg_alloc_size: u64,
    pub largest_free_block: u64,
}
```

### FFIMetalMemoryMetrics
```rust
pub struct FFIMetalMemoryMetrics {
    pub total_allocated: u64,
    pub total_heap_size: u64,
    pub total_heap_used: u64,
    pub allocation_count: u32,
    pub heap_count: u32,
    pub overall_fragmentation: f32,
    pub utilization_pct: f32,
    pub migration_event_count: u32,
}
```

### FFIFragmentationMetrics
```rust
pub struct FFIFragmentationMetrics {
    pub fragmentation_ratio: f32,
    pub external_fragmentation: f32,
    pub internal_fragmentation: f32,
    pub free_blocks: u32,
    pub total_free_bytes: u64,
    pub avg_free_block_size: u64,
    pub largest_free_block: u64,
    pub compaction_efficiency: f32,
}
```

### FFIPageMigrationEvent
```rust
pub struct FFIPageMigrationEvent {
    pub event_id_high: u64,
    pub event_id_low: u64,
    pub migration_type: u32,  // 1=PageOut, 2=PageIn, 3=BufferRelocate, etc.
    pub source_addr: u64,
    pub dest_addr: u64,
    pub size_bytes: u64,
    pub timestamp: u64,
}
```

## Thread Safety

All operations are thread-safe:

- **Recordings (async):** Non-blocking, submitted to serial queue
- **Queries (sync):** Block briefly for snapshot consistency
- **No data races:** Serial dispatch queue enforces total order

Safe to call from multiple threads concurrently.

## Platform Support

### macOS (Full Support)
- Uses Metal API directly
- Compiles Objective-C++ code
- Links Metal, Foundation, IOKit frameworks

### Other Platforms (Stub)
- FFI functions return success codes without tracking
- Allows cross-platform compilation
- Actual observatin is macOS-only

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| record_allocation | O(1) | Async, non-blocking |
| record_deallocation | O(1) | Async, non-blocking |
| get_metrics | O(n+m) | n=allocations, m=heaps, brief sync block |
| get_fragmentation | O(n) | n=heaps, brief sync block |
| get_all_states | O(m) | m=heaps, brief sync block |

## Compilation

```bash
# Build everything
cargo build

# Build just the memory crate
cargo build -p adapteros-memory

# Check for compilation errors
cargo check -p adapteros-memory

# Run tests (if available on macOS)
cargo test -p adapteros-memory
```

## Debugging

### Check Compilation
```bash
cargo build -p adapteros-memory --verbose 2>&1 | grep heap_observer
```

### Xcode Integration
If using Xcode for development, the Objective-C++ code is compiled via Cargo's build system. To debug:

```bash
# Build with debug symbols
cargo build -p adapteros-memory --verbose

# View generated object files
ls -la target/debug/deps/ | grep heap_observer
```

### LLDB Debugging
```
(lldb) br set -n "metal_heap_observe_allocation"
(lldb) run
```

## Implementation Details

### Thread-Safe Singleton

```rust
static g_observer: MetalHeapObserverImpl = ... // Objective-C singleton
static g_observer_once: dispatch_once_t = 0;  // Initialization guard
```

Uses `dispatch_once()` to ensure single initialization across all threads.

### Dispatch Queue Pattern

```objc
dispatch_queue_t threadSafeQueue = dispatch_queue_create(
    "com.adapteros.metal.heap.observer",
    DISPATCH_QUEUE_SERIAL
);

// Async recording
dispatch_async(threadSafeQueue, ^{
    // Mutate state
});

// Sync query
dispatch_sync(threadSafeQueue, ^{
    // Read state
});
```

Ensures:
- All mutations happen sequentially
- Queries see consistent snapshots
- No concurrent access to shared state

### Automatic Reference Counting

Compiled with `-fobjc-arc` flag, so:
- No manual `retain`/`release` needed
- Objects automatically cleaned up
- No reference counting bugs

## Known Limitations

1. **Sampling:** Can drop events with `samplingRate < 1.0`
2. **Migration Detection:** Currently heuristic-based (size thresholds)
3. **Event Buffer:** Unbounded (could add max size in future)
4. **Timestamp Resolution:** Uses `mach_absolute_time()` (system ticks, not wall clock)

## Future Enhancements

- [ ] IOKit memory pressure integration
- [ ] Xcode Instruments export
- [ ] Circular event buffer (bounded size)
- [ ] Metal debugging layer integration
- [ ] Per-heap fragmentation queries
- [ ] Allocation source tracking (stack traces)

## Related Documentation

- **Implementation Details:** [ARCHITECTURE_HEAP_OBSERVER.md](ARCHITECTURE_HEAP_OBSERVER.md)
- **FFI Patterns:** [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](docs/OBJECTIVE_CPP_FFI_PATTERNS.md)
- **Rust Interface:** `src/heap_observer.rs`
- **C Header:** `include/heap_observer.h`
- **Build Script:** `build.rs`

## Testing

The Objective-C++ code is tested indirectly through Rust FFI tests in `src/heap_observer.rs`:

```bash
cargo test -p adapteros-memory heap_observer
```

Key test cases:
- FFI null pointer handling
- Metrics calculation accuracy
- Fragmentation detection
- Event recording
- Thread safety (concurrent calls)

## Support

For issues with:
- **Metal API:** Refer to [Apple Metal Programming Guide](https://developer.apple.com/documentation/metal)
- **Objective-C++:** Refer to [Objective-C++ interoperability guide](https://clang.llvm.org/compatibility.html)
- **FFI Safety:** Refer to [Rust FFI Book](https://doc.rust-lang.org/nomicon/ffi.html)
- **Build Issues:** Check `build.rs` configuration and framework paths
