# MLX Memory Management Implementation

## Overview

This document describes the memory tracking and management system for the MLX FFI backend in adapterOS. The implementation provides comprehensive visibility into unified memory allocation patterns and enables proactive memory management for the lifecycle manager.

## Architecture

### Memory Tracking Mechanism

The implementation uses a three-layer approach:

1. **C++ Allocation Tracking** (`mlx_cpp_wrapper_real.cpp`)
   - Thread-safe atomic counters for total memory and allocation count
   - Hash map tracking individual allocations by pointer
   - Automatic updates on array creation/destruction and model weight loading

2. **FFI Function Exports** (`wrapper.h`)
   - Pure C functions for accessing memory statistics
   - Memory-safe query functions (no ownership transfer)
   - Utility functions for GC hints and resets

3. **Rust API Wrapper** (`src/lib.rs`)
   - Safe Rust abstractions over FFI functions
   - Helper functions for unit conversion and formatting
   - Threshold checking utilities for lifecycle management

### Key Components

#### Global Memory State (mlx_cpp_wrapper_real.cpp)

```cpp
static std::atomic<size_t> g_total_memory_used(0);      // Total bytes allocated
static std::atomic<size_t> g_allocation_count(0);        // Total allocations
static std::mutex g_memory_mutex;                         // Lock for tracking updates
static std::unordered_map<uintptr_t, size_t> g_allocation_map;  // Track individual allocations
```

**Thread Safety:**
- Atomic counters for fast non-blocking reads
- Mutex-protected hash map for reliable tracking
- Memory order: `relaxed` (order not critical for memory accounting)

#### MLXArrayWrapper Tracking

Each array wrapper automatically tracks memory:

```cpp
struct MLXArrayWrapper {
    mx::array arr;
    size_t allocated_bytes;  // Track bytes for this array

    explicit MLXArrayWrapper(const mx::array& a) : arr(a) {
        allocated_bytes = calculate_array_memory(arr);
        record_allocation(reinterpret_cast<uintptr_t>(this), allocated_bytes);
    }

    ~MLXArrayWrapper() {
        unrecord_allocation(reinterpret_cast<uintptr_t>(this));
    }
};
```

**Features:**
- Automatic allocation recording on creation
- Automatic deallocation tracking on destruction
- Handles all MLX dtypes (float32, float16, int32, uint32)

#### MLXModelWrapper Tracking

Model weights are tracked on load:

```cpp
bool load_weights() {
    // Load safetensors or create dummy weights
    // ...

    // Calculate and track memory usage for loaded weights
    total_weight_bytes = 0;
    for (const auto& [name, arr] : weights) {
        size_t bytes = calculate_array_memory(arr);
        total_weight_bytes += bytes;
    }
    record_allocation(reinterpret_cast<uintptr_t>(this), total_weight_bytes);

    return true;
}

~MLXModelWrapper() {
    unrecord_allocation(reinterpret_cast<uintptr_t>(this));
}
```

**Features:**
- Tracks all loaded weights (safetensors or dummy)
- Supports graceful fallback to dummy weights if file not found
- Cleans up tracking on model destruction

### C++ Memory Functions

#### `mlx_memory_usage()`

Returns total unified memory allocated in bytes.

```cpp
extern "C" size_t mlx_memory_usage(void) {
    return g_total_memory_used.load(std::memory_order_relaxed);
}
```

**Use Case:** Lifecycle manager queries current memory pressure for eviction decisions.

#### `mlx_allocation_count()`

Returns number of active allocations.

```cpp
extern "C" size_t mlx_allocation_count(void) {
    return g_allocation_count.load(std::memory_order_relaxed);
}
```

**Use Case:** Debugging, profiling, and detecting memory leaks.

#### `mlx_gc_collect()`

Triggers garbage collection hints.

```cpp
extern "C" void mlx_gc_collect(void) {
    try {
        mx::eval(mx::array(0.0f));  // Flush pipeline
        // Hints system to reclaim unused buffers
    } catch (const std::exception& e) {
        g_last_error = std::string("GC hint failed: ") + e.what();
    }
}
```

**Implementation:**
- Flushes pending MLX operations via `mx::eval()`
- Allows unified memory manager to reclaim buffers
- Non-fatal: errors are logged but don't propagate

**Limitations:**
- MLX doesn't expose explicit memory pools
- Hint-based approach only; actual reclamation depends on system scheduler
- Designed for cooperative garbage collection, not forced deallocation

#### `mlx_memory_reset()`

Clears all tracking state (testing/debugging only).

```cpp
extern "C" void mlx_memory_reset(void) {
    std::lock_guard<std::mutex> lock(g_memory_mutex);
    g_allocation_map.clear();
    g_total_memory_used.store(0, std::memory_order_relaxed);
    g_allocation_count.store(0, std::memory_order_relaxed);
}
```

**Use Case:** Unit tests and memory profiling.

#### `mlx_memory_stats()`

Fills output pointers with current statistics.

```cpp
extern "C" void mlx_memory_stats(size_t* out_total_bytes, size_t* out_allocation_count) {
    if (out_total_bytes) {
        *out_total_bytes = g_total_memory_used.load(std::memory_order_relaxed);
    }
    if (out_allocation_count) {
        *out_allocation_count = g_allocation_count.load(std::memory_order_relaxed);
    }
}
```

**Use Case:** Atomic read of both statistics in one call.

### Rust Memory Module

Located in `src/lib.rs::memory`, provides safe abstractions:

#### Basic Queries

```rust
use adapteros_lora_mlx_ffi::memory;

let bytes = memory::memory_usage();          // Total bytes
let count = memory::allocation_count();      // Allocation count
let (total, count) = memory::memory_stats(); // Both at once
```

#### Structured Statistics

```rust
let stats = memory::stats();  // MemoryStats { total_bytes, allocation_count }

println!("{}", memory::format_stats(&stats));
// Output: "MLX Memory: 123.45 MB (42 allocations)"
```

#### Threshold Checking (for Lifecycle Manager)

```rust
if memory::exceeds_threshold(2048.0) {  // 2GB limit
    tracing::warn!("Memory usage exceeded 2GB");
    memory::gc_collect();
}
```

#### Unit Conversion

```rust
let mb = memory::bytes_to_mb(1024 * 1024);
assert_eq!(mb, 1.0);
```

## Integration with Lifecycle Manager

The memory tracking system integrates with `adapteros-lora-lifecycle` for adaptive memory management:

### Lifecycle Manager Integration Points

1. **Memory Pressure Check**
   ```rust
   let current_memory = adapteros_lora_mlx_ffi::memory::memory_usage();
   let threshold = total_available_memory * 0.85;  // 85% threshold
   if current_memory > threshold {
       // Trigger eviction of least-used adapters
   }
   ```

2. **Post-Eviction Cleanup**
   ```rust
   // After evicting adapters
   adapteros_lora_mlx_ffi::memory::gc_collect();

   let freed_bytes = previous_memory - adapteros_lora_mlx_ffi::memory::memory_usage();
   tracing::info!(freed_bytes = freed_bytes, "Memory reclaimed by GC");
   ```

3. **Telemetry Reporting**
   ```rust
   let stats = adapteros_lora_mlx_ffi::memory::stats();
   telemetry::record_event("mlx_memory_checkpoint", {
       "total_mb": memory::bytes_to_mb(stats.total_bytes),
       "allocations": stats.allocation_count,
   });
   ```

## Data Type Sizes

Memory calculation uses accurate dtype sizes:

| Dtype | Size | Used For |
|-------|------|----------|
| float32 | 4 bytes | Model weights, activations |
| float16 | 2 bytes | Quantized weights (future) |
| int32 | 4 bytes | Token IDs, indices |
| uint32 | 4 bytes | Quantized gates (Q15) |

## Implementation Details

### Allocation Tracking Formula

For each array:

```
memory = element_count × dtype_size_bytes
```

Example: 7B model with 4096 hidden dim, float32 weights
```
32000 × 4096 × 4 bytes = ~536 MB (embedding layer alone)
```

### Thread Safety Guarantees

1. **Atomic Counters:** Wait-free reads/writes (no contention)
2. **Hash Map Updates:** Mutex-protected (brief critical sections)
3. **No Deadlocks:** Single mutex, no nested locks
4. **Memory Ordering:** Relaxed atomic ops sufficient (ordering not required)

### Performance Characteristics

- **Memory Query:** O(1) atomic read (~nanoseconds)
- **Allocation Recording:** O(1) atomic add + O(1) hash insert (~microseconds)
- **Deallocation Unrecording:** O(1) hash lookup + O(1) atomic subtract (~microseconds)
- **GC Collection:** O(1) dummy array creation + MLX eval (milliseconds)

Total overhead is negligible compared to actual computation.

## Testing

Unit tests in `/src/lora.rs` and `/src/backend.rs`:

```rust
#[test]
fn test_memory_tracking() {
    memory::reset();

    let data = vec![1.0; 1000];
    let tensor = MLXFFITensor::from_data(&data, vec![10, 100]).unwrap();

    let stats = memory::stats();
    assert!(stats.total_bytes > 0);
    assert_eq!(stats.allocation_count, 1);

    drop(tensor);

    let stats = memory::stats();
    assert_eq!(stats.total_bytes, 0);
    assert_eq!(stats.allocation_count, 0);
}
```

## Debugging

### Enable Memory Tracking Logs

```bash
RUST_LOG=adapteros_lora_mlx_ffi::memory=debug ./your_app
```

### Memory Snapshot During Development

```rust
use adapteros_lora_mlx_ffi::memory;

fn checkpoint(label: &str) {
    let stats = memory::stats();
    println!("[{}] {}", label, memory::format_stats(&stats));
}

checkpoint("before_load");
let model = MLXFFIModel::load("path/to/model")?;
checkpoint("after_load");

// Output:
// [before_load] MLX Memory: 0.00 MB (0 allocations)
// [after_load] MLX Memory: 534.25 MB (1 allocations)
```

### Detect Memory Leaks

```rust
memory::reset();
// ... perform operations ...
let stats = memory::stats();

if stats.allocation_count > 0 && stats.total_bytes > 0 {
    eprintln!("WARNING: {} bytes in {} allocations not cleaned up",
              stats.total_bytes, stats.allocation_count);
}
```

## Limitations and Future Work

### Current Limitations

1. **Unified Memory Only:** Doesn't track CPU-side buffers or host memory
2. **Hint-Based GC:** MLX doesn't expose memory pool compaction
3. **No Per-Layer Tracking:** Coarse-grained totals only
4. **No GPU Memory Separate:** Can't distinguish GPU vs CPU unified regions

### Future Enhancements

1. **Per-Adapter Tracking:** Track memory per loaded adapter
2. **Memory Watermarks:** High/low water mark statistics
3. **Allocation Histograms:** Track allocation size distribution
4. **Weak References:** Lazy cleanup of unused arrays
5. **System Integration:** Query total system memory pressure

## References

- MLX Documentation: https://ml-explore.github.io/mlx/
- Unified Memory: Apple Metal documentation on unified memory model
- adapterOS Lifecycle Manager: `crates/adapteros-lora-lifecycle/`
- FFI Safety: Rust FFI guidelines in adapterOS AGENTS.md
