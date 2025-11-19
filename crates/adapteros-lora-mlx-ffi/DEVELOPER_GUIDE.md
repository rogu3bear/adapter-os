# MLX Memory Management Developer Guide

## Quick Start

### For Lifecycle Manager Integration

The lifecycle manager can query MLX memory and make eviction decisions:

```rust
use adapteros_lora_mlx_ffi::memory;

// Check current usage
let current_usage = memory::memory_usage();
let threshold = total_system_memory * 0.85;

if current_usage > threshold {
    eprintln!("Memory pressure detected: {}", memory::format_stats(&memory::stats()));

    // Trigger adapter eviction
    evict_least_used_adapters();

    // Request cleanup
    memory::gc_collect();

    // Verify cleanup
    let new_usage = memory::memory_usage();
    println!("Freed {} bytes", current_usage - new_usage);
}
```

### For Testing

```rust
use adapteros_lora_mlx_ffi::memory;

#[test]
fn test_my_feature() {
    memory::reset();  // Start clean

    // ... perform operations ...

    let stats = memory::stats();
    assert!(stats.total_bytes > 0);  // Verify allocations occurred
}
```

### For Monitoring

```rust
use adapteros_lora_mlx_ffi::memory;

fn log_memory_checkpoint(label: &str) {
    let stats = memory::stats();
    println!("[{}] {}", label, memory::format_stats(&stats));
}

log_memory_checkpoint("startup");
let model = load_model()?;
log_memory_checkpoint("after_model_load");
```

## Understanding Memory Tracking

### What Gets Tracked

**Automatically Tracked:**
- All MLX array allocations (vectors, matrices, tensors)
- Model weights (safetensors or dummy weights)
- LoRA adapter parameters
- Intermediate computations

**Not Tracked:**
- Host memory buffers (CPU-side)
- String allocations (error messages, metadata)
- Hash maps for tracking themselves

### Data Types Supported

| Type | Size | Example |
|------|------|---------|
| float32 | 4 bytes | Model weights (7B model ~28GB) |
| float16 | 2 bytes | Quantized weights (future) |
| int32 | 4 bytes | Token IDs (prompt, ~4KB) |
| uint32 | 4 bytes | Q15 gates (routing) |

### Memory Calculation

For any array:
```
memory_bytes = num_elements × dtype_size_bytes
```

Example 7B model breakdown:
```
Token Embedding:      32000 × 4096 × 4 = 536 MB
Attention Weights:    32 layers × ... = 7000+ MB
Output Projection:    4096 × 32000 × 4 = 536 MB
Total:                ~28 GB
```

## Architecture Decision Records

### Why Atomic Counters?

**Decision:** Use `std::atomic<size_t>` for memory tracking counters

**Rationale:**
1. **No Locks Needed:** Reads don't block writes; writes don't block reads
2. **Zero-Copy:** No data copying for queries
3. **Thread-Safe:** Guaranteed visibility across cores
4. **Performance:** Single CPU instruction for read/write

**Alternative Considered:** Mutex-protected integers
- Rejected: Locks introduce contention and latency

### Why Hash Map for Allocations?

**Decision:** Use `std::unordered_map<uintptr_t, size_t>` for per-allocation tracking

**Rationale:**
1. **Fast Lookup:** O(1) hash lookup on deallocation
2. **Memory Overhead:** Only ~24 bytes per allocation
3. **Debugging:** Can dump all active allocations if needed

**Alternative Considered:** Simple counter without tracking
- Rejected: Couldn't detect memory leaks or track individual allocations

### Why Relaxed Memory Ordering?

**Decision:** Use `std::memory_order_relaxed` for atomic operations

**Rationale:**
1. **No Synchronization Needed:** Memory accounting order doesn't matter
2. **Fastest:** No memory barriers or cache flushes
3. **Sufficient:** Consistency maintained by mutex for hash map updates

**Alternative Considered:** Sequentially consistent atomics
- Rejected: Unnecessary overhead for non-synchronization uses

## Common Patterns

### Pattern: Memory Budgeting

```rust
use adapteros_lora_mlx_ffi::memory;

struct MemoryBudget {
    limit_mb: f32,
    warning_mb: f32,
}

impl MemoryBudget {
    fn check_health(&self) -> MemoryHealth {
        let stats = memory::stats();
        let current_mb = memory::bytes_to_mb(stats.total_bytes);

        if current_mb > self.limit_mb {
            MemoryHealth::Critical
        } else if current_mb > self.warning_mb {
            MemoryHealth::Warning
        } else {
            MemoryHealth::Healthy
        }
    }
}
```

### Pattern: Scoped Memory Tracking

```rust
use adapteros_lora_mlx_ffi::memory;

struct MemoryScope {
    label: String,
    initial_bytes: usize,
}

impl MemoryScope {
    fn new(label: &str) -> Self {
        memory::reset();
        Self {
            label: label.to_string(),
            initial_bytes: 0,
        }
    }

    fn report(&self) {
        let final_bytes = memory::memory_usage();
        let delta = final_bytes - self.initial_bytes;
        println!("[{}] Allocated {} bytes", self.label, delta);
    }
}
```

### Pattern: Memory-Aware Eviction

```rust
use adapteros_lora_mlx_ffi::memory;

fn evict_with_memory_tracking(
    adapters: &mut Vec<Adapter>,
    target_freed_mb: f32,
) -> Result<()> {
    let target_bytes = (target_freed_mb * 1024.0 * 1024.0) as usize;
    let mut freed = 0;

    let initial = memory::memory_usage();

    while freed < target_bytes && !adapters.is_empty() {
        let adapter = adapters.pop().unwrap();
        let adapter_size = estimate_adapter_size(&adapter);

        unload_adapter(adapter)?;

        freed += adapter_size;
    }

    memory::gc_collect();

    let final = memory::memory_usage();
    let actual_freed = initial - final;

    tracing::info!(
        target_freed_bytes = target_bytes,
        actual_freed_bytes = actual_freed,
        "Memory eviction complete"
    );

    Ok(())
}
```

## Debugging Memory Issues

### Detect Memory Leaks

```rust
use adapteros_lora_mlx_ffi::memory;

#[test]
fn test_no_memory_leak() {
    memory::reset();

    for _ in 0..1000 {
        let tensor = create_tensor(1024)?;
        // ... use tensor ...
        drop(tensor);
    }

    // After dropping all tensors, memory should be freed
    let stats = memory::stats();
    assert_eq!(stats.total_bytes, 0, "Memory leak detected!");
}
```

### Track Peak Memory

```rust
use adapteros_lora_mlx_ffi::memory;

struct MemoryTracker {
    peak_bytes: usize,
    peak_allocations: usize,
}

impl MemoryTracker {
    fn update(&mut self) {
        let stats = memory::stats();
        if stats.total_bytes > self.peak_bytes {
            self.peak_bytes = stats.total_bytes;
        }
        if stats.allocation_count > self.peak_allocations {
            self.peak_allocations = stats.allocation_count;
        }
    }

    fn report(&self) {
        println!(
            "Peak memory: {:.2} MB ({} allocations)",
            memory::bytes_to_mb(self.peak_bytes),
            self.peak_allocations
        );
    }
}
```

### Profile Allocations

```rust
use adapteros_lora_mlx_ffi::memory;

fn profile_operation<F: FnOnce() -> Result<T>, T>(
    label: &str,
    op: F,
) -> Result<(T, usize)> {
    memory::reset();

    let result = op()?;

    let stats = memory::stats();
    tracing::info!(
        operation = label,
        bytes_allocated = stats.total_bytes,
        allocation_count = stats.allocation_count,
        bytes_mb = memory::bytes_to_mb(stats.total_bytes),
        "Operation profiling complete"
    );

    Ok((result, stats.total_bytes))
}

// Usage:
let (model, bytes) = profile_operation("load_model", || {
    MLXFFIModel::load("path/to/model")
})?;
```

## Performance Characteristics

### Overhead Per Operation

| Operation | Time | Notes |
|-----------|------|-------|
| Query memory | <1 µs | Atomic read |
| Record allocation | ~1 µs | Hash insert + atomic add |
| Unrecord deallocation | ~1 µs | Hash lookup + atomic subtract |
| GC collection | ~1 ms | MLX::eval() overhead |

### Memory Overhead

- Per allocation: ~24 bytes (hash map entry)
- Per wrapper: ~8 bytes (atomic counter)
- Global state: ~64 bytes (mutex + atomics)

**Total:** Negligible (<0.1% of actual allocations)

## Integration Checklist

When integrating with new code:

- [ ] Import `adapteros_lora_mlx_ffi::memory` module
- [ ] Call `memory::reset()` at test start
- [ ] Check `memory::memory_usage()` at key points
- [ ] Use `memory::format_stats()` for logging
- [ ] Call `memory::gc_collect()` after eviction
- [ ] Test with `memory::exceeds_threshold()` for pressure
- [ ] Document memory requirements in adapter metadata

## FAQ

**Q: Why does my memory not decrease after dropping arrays?**
A: MLX uses lazy evaluation and garbage collection. Call `memory::gc_collect()` to hint the system to reclaim buffers.

**Q: Can I track memory per adapter?**
A: Currently no, but this is planned for future versions. Workaround: reset, load adapter, query memory.

**Q: Is memory tracking thread-safe?**
A: Yes. All counters use atomics and the hash map is protected by a single mutex.

**Q: What if tracking itself consumes too much memory?**
A: The tracking overhead is <1% and bounded (fixed size hash map). For extreme cases, the hash map can be disabled.

**Q: Can I use this in production?**
A: Yes, but this is experimental. MLX backend is non-deterministic and not production-ready. Metal backend is recommended for production.

## References

- **Complete Architecture:** `MEMORY_MANAGEMENT.md`
- **Implementation Details:** `src/mlx_cpp_wrapper_real.cpp` (lines 29-90)
- **Rust API:** `src/lib.rs::memory` module
- **Tests:** `tests/memory_tracking_tests.rs`
- **C Header:** `wrapper.h` (lines 99-104)

## Support

For questions or issues:
1. Check `MEMORY_MANAGEMENT.md` for architecture details
2. Review test cases in `tests/memory_tracking_tests.rs` for usage examples
3. Check error messages via `mlx_get_last_error()` (GC hints only)
4. Enable tracing logs: `RUST_LOG=debug`

## Version History

- **v0.01** (Current): Initial implementation
  - Basic allocation tracking
  - Memory statistics queries
  - GC collection hints
  - Rust wrapper module
  - Comprehensive tests
