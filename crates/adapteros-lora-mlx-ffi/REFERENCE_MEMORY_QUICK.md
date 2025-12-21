# MLX Memory Tracking - Quick Reference

## Basic Usage

### Get Current Memory Usage
```rust
use adapteros_lora_mlx_ffi::memory;

let bytes = memory::memory_usage();                    // Total bytes
let count = memory::allocation_count();                // Allocation count
let (bytes, count) = memory::memory_stats();           // Both at once
let stats = memory::stats();                           // Structured
```

### Format for Logging
```rust
let stats = memory::stats();
println!("{}", memory::format_stats(&stats));
// Output: "MLX Memory: 123.45 MB (42 allocations)"
```

### Convert to MB
```rust
let mb = memory::bytes_to_mb(1024 * 1024);
assert_eq!(mb, 1.0);
```

### Check Memory Pressure
```rust
if memory::exceeds_threshold(2048.0) {  // 2GB limit
    eprintln!("Memory exceeded 2GB!");
    memory::gc_collect();
}
```

## Lifecycle Manager Integration

### Memory-Aware Eviction
```rust
use adapteros_lora_mlx_ffi::memory;

let threshold = total_system_memory * 0.85;
if memory::memory_usage() > threshold {
    // Trigger eviction
    evict_adapters();

    // Request cleanup
    memory::gc_collect();

    // Verify
    let freed = threshold - memory::memory_usage();
    tracing::info!(freed_bytes = freed, "Memory reclaimed");
}
```

### Periodic Monitoring
```rust
use adapteros_lora_mlx_ffi::memory;

loop {
    let stats = memory::stats();
    tracing::info!("{}", memory::format_stats(&stats));

    std::thread::sleep(std::time::Duration::from_secs(30));
}
```

## Testing

### Reset Before Test
```rust
#[test]
fn test_adapter_loading() {
    memory::reset();

    // ... test code ...

    let stats = memory::stats();
    assert!(stats.total_bytes > 0);
}
```

### Memory Leak Detection
```rust
memory::reset();
for _ in 0..1000 {
    let tensor = create_tensor()?;
    drop(tensor);
}
assert_eq!(memory::memory_usage(), 0, "Memory leak!");
```

## Rust API

### Memory Module Functions
```rust
memory::memory_usage() -> usize                  // Total bytes
memory::allocation_count() -> usize              // Count
memory::memory_stats() -> (usize, usize)         // (bytes, count)
memory::reset()                                  // Clear tracking
memory::gc_collect()                             // GC hint
memory::stats() -> MemoryStats                   // Snapshot
memory::bytes_to_mb(bytes: usize) -> f32        // Convert
memory::format_stats(stats: &MemoryStats) -> String
memory::exceeds_threshold(mb: f32) -> bool
```

### MemoryStats Struct
```rust
pub struct MemoryStats {
    pub total_bytes: usize,
    pub allocation_count: usize,
}
```

## C FFI Level

### Function Signatures
```c
size_t mlx_memory_usage(void);
size_t mlx_allocation_count(void);
void mlx_gc_collect(void);
void mlx_memory_reset(void);
void mlx_memory_stats(size_t* out_total_bytes, size_t* out_allocation_count);
```

### Usage from C
```c
#include "wrapper.h"

size_t bytes = mlx_memory_usage();
size_t count = mlx_allocation_count();

size_t total = 0;
size_t allocs = 0;
mlx_memory_stats(&total, &allocs);

mlx_gc_collect();
mlx_memory_reset();
```

## Common Patterns

### Memory Checkpoint
```rust
fn checkpoint(label: &str) {
    let stats = memory::stats();
    println!("[{}] {}", label, memory::format_stats(&stats));
}

checkpoint("start");
do_work()?;
checkpoint("end");
```

### Scoped Tracking
```rust
struct MemoryScope {
    label: String,
    initial: usize,
}

impl MemoryScope {
    fn new(label: &str) -> Self {
        memory::reset();
        Self {
            label: label.to_string(),
            initial: memory::memory_usage(),
        }
    }
}

impl Drop for MemoryScope {
    fn drop(&mut self) {
        let final_mem = memory::memory_usage();
        let delta = final_mem - self.initial;
        println!("[{}] Used {} bytes", self.label, delta);
    }
}

let _scope = MemoryScope::new("my_operation");
```

### Memory Budgeting
```rust
const MEMORY_LIMIT_MB: f32 = 4096.0;
const WARNING_THRESHOLD_MB: f32 = MEMORY_LIMIT_MB * 0.85;
const CRITICAL_THRESHOLD_MB: f32 = MEMORY_LIMIT_MB * 0.95;

fn check_memory_health() -> Health {
    let current_mb = memory::bytes_to_mb(memory::memory_usage());

    if current_mb > CRITICAL_THRESHOLD_MB {
        Health::Critical
    } else if current_mb > WARNING_THRESHOLD_MB {
        Health::Warning
    } else {
        Health::Healthy
    }
}
```

## Data Types & Sizes

| Type | Size | Example |
|------|------|---------|
| float32 | 4 bytes | Weights: 7B model = 28GB |
| float16 | 2 bytes | Quantized: 7B = 14GB |
| int32 | 4 bytes | Tokens: sequence × 4 |
| uint32 | 4 bytes | Gates: routing gates |

## Performance

| Operation | Time |
|-----------|------|
| Query memory | <1 µs |
| Record allocation | ~1 µs |
| Unrecord allocation | ~1 µs |
| GC collection | ~1 ms |
| Memory overhead | <0.1% |

## Logging

### Enable Debug Logs
```bash
RUST_LOG=debug cargo run
```

### Typical Log Pattern
```
[adapteros_lora_mlx_ffi::memory] Current memory: 1234.56 MB (42 allocations)
[adapteros_lora_mlx_ffi::memory] Memory pressure detected, triggering GC
[adapteros_lora_mlx_ffi::memory] Freed 512.34 MB
```

## Troubleshooting

### Memory Not Decreasing?
Call `memory::gc_collect()` to hint the system.

### Why Different Values?
MLX uses lazy evaluation; memory is freed asynchronously.

### Memory Stuck at High Value?
Check for references being held; use `memory::reset()` in tests.

### Is Tracking Thread-Safe?
Yes. Atomic counters guarantee thread safety.

## Links

- **Full Docs:** `REFERENCE_MEMORY_MANAGEMENT.md`
- **Developer Guide:** `GUIDE_DEVELOPER_MEMORY.md`
- **Implementation:** `src/lib.rs::memory` module
- **C Code:** `src/mlx_cpp_wrapper_real.cpp`
- **Tests:** `tests/memory_tracking_tests.rs`
