# MLX Backend: Profiling & Optimization Quick Start Guide

**Document Purpose:** Step-by-step instructions for profiling, identifying bottlenecks, and implementing optimizations

**Target Audience:** Developers optimizing MLX backend performance

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Quick Reference Commands

### Profiling

```bash
# Capture baseline performance metrics
cd /Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi
cargo bench --bench mlx_benchmarks -- --save-baseline phase0_baseline

# Compare against baseline
cargo bench --bench mlx_benchmarks -- --baseline phase0_baseline

# Full profiling with all details
cargo bench --bench mlx_benchmarks -- --verbose --plotting-backend gnuplot

# Individual benchmark groups
cargo bench --bench mlx_benchmarks -- bench_single_token_latency
cargo bench --bench mlx_benchmarks -- bench_batch_throughput
cargo bench --bench mlx_benchmarks -- bench_matmul_operations
```

### Testing

```bash
# Run determinism tests (validates no performance regressions)
cargo test --test deterministic_mode_tests -- --nocapture
cargo test --test determinism_test -- --nocapture

# Performance-specific tests
cargo test -p adapteros-lora-mlx-ffi --lib performance

# Full integration
cargo test -p adapteros-lora-mlx-ffi
```

### Visualization

```bash
# Generate performance charts
./crates/adapteros-lora-mlx-ffi/scripts/visualize_performance.py

# View results
open target/criterion/mlx_single_token_latency/report/index.html
```

---

## Part 1: Understanding the Profiler Infrastructure

### 1.1 C++ Performance Profiler (`performance_profiler.{h,cpp}`)

**What it does:**
- Tracks 14 operation types with nanosecond precision
- Lock-free atomic counters (zero contention overhead)
- Automatically aggregates: count, avg, min, max, total time

**Available Counters:**
```
matmul                 → Matrix multiplication
add, subtract          → Element-wise arithmetic
multiply, divide       → Element-wise operations
attention              → Attention mechanism
lora_forward           → Single LoRA application
multi_lora_forward     → Multi-adapter fusion
model_forward          → Full forward pass
array_creation         → Array allocation
memory_transfer        → Copy/clone operations
eval                   → GPU evaluation/sync
softmax, activation    → Non-linear operations
```

**Usage in C++:**
```cpp
#include "performance_profiler.h"

void mlx_matmul_example() {
    {
        ScopedTimer timer(g_perf_counters.matmul);
        // ... do matrix multiplication ...
    } // Timer destructor records duration
}

// Get stats
const char* stats_json = mlx_get_performance_stats();
// Returns: {"matmul": {"count": 100, "avg_us": 45.2, ...}}
```

### 1.2 Rust Performance API (`performance.rs`)

**Three-tier API:**

```rust
// Tier 1: Automatic snapshots
use adapteros_lora_mlx_ffi::performance::PerformanceSnapshot;

let snapshot = PerformanceSnapshot::capture()?;
println!("{}", snapshot.generate_report());

// Tier 2: Time-series tracking
use adapteros_lora_mlx_ffi::performance::PerformanceProfiler;

let profiler = PerformanceProfiler::new();
profiler.reset();
// ... run workload ...
profiler.snapshot()?;
println!("{}", profiler.summary_report());

// Tier 3: Custom metrics
use adapteros_lora_mlx_ffi::performance::PerformanceMetrics;

let metrics = PerformanceMetrics::new();
metrics.record_token_generated(Duration::from_millis(10));
println!("Throughput: {:.2} tok/sec", metrics.tokens_per_second());
```

### 1.3 Criterion Benchmarks

**What it provides:**
- 12 benchmark groups with 57 configurations
- Automatic statistical analysis
- HTML reports with graphs
- Baseline comparison
- Configurable sample sizes and measurement times

**Benchmark categories:**

| Category | Count | Purpose |
|----------|-------|---------|
| Latency | 2 groups | Single-token and adapter-switch timing |
| Throughput | 2 groups | Batch processing and multi-adapter scaling |
| Memory | 3 groups | Allocation patterns, transfers, GC |
| Operations | 3 groups | MatMul, attention, LoRA |
| Cache | 1 group | Access pattern efficiency |
| Comparison | 1 group | Architecture variants |

---

## Part 2: Profiling Workflow

### Step 1: Establish Baseline

```bash
# Generate initial baseline
cd /Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi

cargo bench --bench mlx_benchmarks -- \
    --save-baseline "before_optimization" \
    --measurement-time 10 \
    --sample-size 100

# Output: target/criterion/
# Key files:
# ├─ mlx_single_token_latency/report/index.html
# ├─ mlx_batch_throughput/report/index.html
# └─ mlx_matmul_operations/report/index.html
```

**What to look for in baseline:**
- Single token latency: 280µs (measure of interactive responsiveness)
- Batch throughput: 75 tok/sec (measure of batch processing)
- MatMul GFLOPS: 41.2 (compute efficiency)
- Memory per token: ~2.8 KB (memory footprint)

### Step 2: Profile Performance Counters

```rust
// Create profiler
use adapteros_lora_mlx_ffi::performance::PerformanceSnapshot;

// Reset counters
unsafe {
    adapteros_lora_mlx_ffi::performance::mlx_reset_performance_counters();
}

// Run test workload (e.g., 100 tokens)
run_inference_pipeline(100)?;

// Capture snapshot
let snapshot = PerformanceSnapshot::capture()?;

// Analyze
for (name, stats) in snapshot.top_operations(10) {
    println!("{}: {:.2}ms total, {:.2}µs avg, {:.0} ops/sec",
        name,
        stats.total_ms,
        stats.avg_us,
        stats.throughput_ops_per_sec()
    );
}

// Identify bottlenecks
for (name, stats) in snapshot.operations.iter() {
    if stats.is_bottleneck(10.0) { // 10ms threshold
        println!("⚠️  Bottleneck: {}: {:.2}ms", name, stats.total_ms);
    }
}
```

**Sample output:**
```
=== MLX Backend Performance Report ===

Memory Usage: 125.43 MB (3421 allocations)

Top Operations by Total Time:
  matmul: 280.54ms total, 45.22µs avg (6200 calls, 13.7M ops/sec)
  memory_transfer: 65.32ms total, 10.54µs avg (6200 calls, 94.9M ops/sec)
  eval: 45.12ms total, 7.28µs avg (6200 calls, 137.4M ops/sec)
  attention: 18.65ms total, 3.01µs avg (6200 calls, 332.2M ops/sec)
  lora_forward: 12.34ms total, 1.99µs avg (6200 calls, 502.5M ops/sec)

Performance Bottlenecks:
  matmul: 280.54ms total, 45.22µs avg, 89.34µs max
  memory_transfer: 65.32ms total, 10.54µs avg, 32.15µs max
```

### Step 3: Identify Deterministic Mode Overhead

```bash
# Run determinism tests with profiling
cd /Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi
cargo test --test deterministic_mode_tests -- \
    test_deterministic_mode_performance_overhead \
    --nocapture
```

**Expected output:**
```
test_deterministic_mode_performance_overhead ... ok
GPU mode:        2.05µs per operation
CPU mode:        2.62µs per operation
Overhead:        27.8% (0.57µs additional)

Breakdown:
├─ Softmax overhead:     ~0.23µs (40%)
├─ LayerNorm overhead:   ~0.30µs (53%)
└─ Sync overhead:        ~0.04µs (7%)
```

---

## Part 3: Implementing Optimizations

### Optimization 1A: Hybrid GPU-CPU Validation (Priority: #1)

**Expected Improvement:** 50% reduction in softmax overhead

**Implementation Steps:**

1. **Modify `src/backend.rs`:**

```rust
impl MLXFFIBackend {
    /// Apply softmax with optional GPU-CPU validation
    pub fn softmax_with_validation(
        &self,
        input: &[f32],
    ) -> Result<Vec<f32>> {
        // GPU computation (fast)
        let gpu_result = self.mlx_softmax(input)?;

        // Deterministic validation (if enabled)
        if self.deterministic_mode {
            // Spot-check: validate sample indices instead of full array
            let sample_indices = self.get_sample_indices(input.len());

            for &idx in &sample_indices {
                let cpu_val = Self::softmax_element(input, idx)?;
                let tolerance = 1e-5;

                assert!(
                    (gpu_result[idx] - cpu_val).abs() < tolerance,
                    "GPU softmax diverged at index {}: {} vs {}",
                    idx,
                    gpu_result[idx],
                    cpu_val
                );
            }
        }

        Ok(gpu_result) // Return GPU result (fast!)
    }

    /// Get sample indices for validation (5-point sample)
    fn get_sample_indices(&self, len: usize) -> Vec<usize> {
        vec![
            0,
            len / 4,
            len / 2,
            3 * len / 4,
            len - 1,
        ]
    }

    /// Compute softmax for single element (CPU reference)
    fn softmax_element(input: &[f32], idx: usize) -> Result<f32> {
        // Find max for numerical stability
        let max = input.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        // Compute exp(input[i] - max)
        let exp_val = (input[idx] - max).exp();

        // Compute sum of all exps
        let sum_exp: f32 = input
            .iter()
            .map(|&x| (x - max).exp())
            .sum();

        Ok(exp_val / sum_exp)
    }
}
```

2. **Test the implementation:**

```bash
cargo test -p adapteros-lora-mlx-ffi -- softmax_with_validation --nocapture
```

3. **Benchmark before/after:**

```bash
# Before
cargo bench --bench mlx_benchmarks -- --save-baseline before_1a

# Apply optimization...

# After
cargo bench --bench mlx_benchmarks -- --baseline before_1a
```

4. **Verify determinism is maintained:**

```bash
cargo test --test deterministic_mode_tests -- --nocapture
```

### Optimization 1B: Batch Softmax (Priority: #2)

**Expected Improvement:** 40% reduction in eval sync overhead

**Implementation Steps:**

1. **Add C FFI in `mlx_cpp_wrapper_real.cpp`:**

```cpp
// C API for batched softmax computation
extern "C" {
    /// Compute softmax for multiple arrays with single eval
    void mlx_softmax_batch(
        const float** inputs,           // Array of input pointers
        int num_inputs,                 // Number of inputs
        const int* input_sizes,         // Size of each input
        float** outputs,                // Output pointers (pre-allocated)
        mlx_error_t* error_out
    ) {
        try {
            std::vector<mx::array> pending_ops;

            // Queue all softmax operations (deferred evaluation)
            for (int i = 0; i < num_inputs; ++i) {
                mx::array input = mx::array(
                    inputs[i],
                    {static_cast<size_t>(input_sizes[i])},
                    mx::float32
                );

                pending_ops.push_back(mx::softmax(input));
            }

            // Single batch evaluation (saves 90% of sync cost!)
            mx::eval(pending_ops);

            // Copy results
            for (int i = 0; i < num_inputs; ++i) {
                auto data = mx::data(pending_ops[i]);
                std::memcpy(outputs[i], data, input_sizes[i] * sizeof(float));
            }
        } catch (const std::exception& e) {
            *error_out = strdup(e.what());
        }
    }
}
```

2. **Add Rust wrapper in `lib.rs`:**

```rust
extern "C" {
    fn mlx_softmax_batch(
        inputs: *const *const f32,
        num_inputs: i32,
        input_sizes: *const i32,
        outputs: *mut *mut f32,
        error_out: *mut *mut std::os::raw::c_char,
    );
}

pub fn softmax_batch(inputs: &[&[f32]]) -> Result<Vec<Vec<f32>>> {
    let num_inputs = inputs.len();
    let input_sizes: Vec<i32> = inputs.iter().map(|i| i.len() as i32).collect();

    // Pre-allocate outputs
    let mut outputs: Vec<Vec<f32>> = inputs
        .iter()
        .map(|i| vec![0.0; i.len()])
        .collect();

    let input_ptrs: Vec<*const f32> = inputs.iter().map(|i| i.as_ptr()).collect();
    let output_ptrs: Vec<*mut f32> = outputs
        .iter_mut()
        .map(|o| o.as_mut_ptr())
        .collect();

    unsafe {
        let mut error_out = std::ptr::null_mut();
        mlx_softmax_batch(
            input_ptrs.as_ptr(),
            num_inputs as i32,
            input_sizes.as_ptr(),
            output_ptrs.as_ptr() as *mut *mut f32,
            &mut error_out,
        );

        if !error_out.is_null() {
            let error_str = std::ffi::CStr::from_ptr(error_out)
                .to_string_lossy()
                .to_string();
            libc::free(error_out as *mut std::ffi::c_void);
            return Err(AosError::Mlx(format!("Batch softmax error: {}", error_str)));
        }
    }

    Ok(outputs)
}
```

3. **Integrate into attention computation:**

```rust
// Before: Per-token softmax with individual eval
for seq_idx in 0..seq_len {
    let scores = &attention_scores[seq_idx];
    let softmax_scores = mlx_softmax(scores)?; // Individual eval!
    // ... use softmax_scores ...
}

// After: Batch all softmax operations
let softmax_results = softmax_batch(&attention_scores)?; // Single eval!
for (seq_idx, softmax_scores) in softmax_results.iter().enumerate() {
    // ... use softmax_scores ...
}
```

### Optimization 1C: Buffer Pooling (Priority: #3)

**Expected Improvement:** 20-30% reduction in allocation overhead

**Create `src/buffer_pool.rs`:**

```rust
//! Buffer pooling for reducing allocation overhead
//!
//! Reuses temporary buffers across LoRA forward passes,
//! avoiding repeated allocation/deallocation of temporary vectors.

use std::collections::HashMap;
use parking_lot::Mutex;
use std::sync::Arc;

/// Pooled buffer manager for temporary allocations
pub struct BufferPool {
    // Size → list of available buffers
    pools: Mutex<HashMap<usize, Vec<Vec<f32>>>>,
    // Maximum buffers to keep per size
    max_per_size: usize,
}

impl BufferPool {
    /// Create new buffer pool
    pub fn new(max_per_size: usize) -> Self {
        Self {
            pools: Mutex::new(HashMap::new()),
            max_per_size,
        }
    }

    /// Acquire or allocate a buffer
    pub fn acquire(&self, size: usize) -> Vec<f32> {
        let mut pools = self.pools.lock();

        pools
            .entry(size)
            .or_insert_with(Vec::new)
            .pop()
            .unwrap_or_else(|| vec![0.0; size])
    }

    /// Return a buffer to the pool
    pub fn release(&self, mut buf: Vec<f32>) {
        let size = buf.capacity();
        let mut pools = self.pools.lock();

        let pool = pools.entry(size).or_insert_with(Vec::new);

        if pool.len() < self.max_per_size {
            buf.clear(); // Reset but keep capacity
            pool.push(buf);
        }
        // Otherwise, drop and deallocate
    }

    /// Get current pool statistics
    pub fn stats(&self) -> PoolStats {
        let pools = self.pools.lock();
        let mut total_bytes = 0;
        let mut total_buffers = 0;

        for (size, bufs) in pools.iter() {
            total_bytes += size * bufs.len();
            total_buffers += bufs.len();
        }

        PoolStats {
            total_bytes,
            total_buffers,
            num_sizes: pools.len(),
        }
    }

    /// Clear all pooled buffers
    pub fn clear(&self) {
        self.pools.lock().clear();
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new(4) // Keep up to 4 buffers per size
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub total_bytes: usize,
    pub total_buffers: usize,
    pub num_sizes: usize,
}

// Thread-local global pool
thread_local! {
    static GLOBAL_BUFFER_POOL: BufferPool = BufferPool::default();
}

/// Acquire buffer from thread-local pool
pub fn acquire(size: usize) -> Vec<f32> {
    GLOBAL_BUFFER_POOL.with(|pool| pool.acquire(size))
}

/// Return buffer to thread-local pool
pub fn release(buf: Vec<f32>) {
    GLOBAL_BUFFER_POOL.with(|pool| pool.release(buf));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_basic() {
        let pool = BufferPool::new(2);

        let buf1 = pool.acquire(100);
        assert_eq!(buf1.capacity(), 100);

        pool.release(buf1);

        // Acquire again - should reuse
        let buf2 = pool.acquire(100);
        assert_eq!(buf2.capacity(), 100);
    }

    #[test]
    fn test_buffer_pool_multiple_sizes() {
        let pool = BufferPool::new(3);

        let buf100 = pool.acquire(100);
        let buf200 = pool.acquire(200);
        let buf300 = pool.acquire(300);

        pool.release(buf100);
        pool.release(buf200);
        pool.release(buf300);

        let stats = pool.stats();
        assert_eq!(stats.num_sizes, 3);
        assert_eq!(stats.total_buffers, 3);
    }
}
```

**Add to `src/lib.rs`:**
```rust
pub mod buffer_pool;
pub use buffer_pool::{acquire, release};
```

**Usage in LoRA forward:**
```rust
use crate::buffer_pool;

pub fn forward(&self, input: &[f32]) -> Result<Vec<f32>> {
    let rank = self.config.rank;
    let hidden = input.len();

    // Acquire temporary buffer from pool
    let mut intermediate = buffer_pool::acquire(rank);

    // ... do computation ...

    // Return buffer to pool
    buffer_pool::release(intermediate);

    Ok(output)
}
```

---

## Part 4: Benchmarking & Validation

### Running Benchmarks

```bash
# Before optimization
cargo bench --bench mlx_benchmarks -- \
    --save-baseline before_phase1a \
    --sample-size 100

# Implement Phase 1A optimization...

# After Phase 1A
cargo bench --bench mlx_benchmarks -- \
    --save-baseline after_phase1a \
    --baseline before_phase1a \
    --sample-size 100

# Compare results
echo "=== Phase 1A Improvement ==="
echo "Softmax overhead reduction target: 50%"
echo "Check: target/criterion/mlx_attention/report/index.html"
```

### Validating Determinism

```bash
# Run 10 iterations to verify consistency
for i in {1..10}; do
    echo "Run $i..."
    cargo test --test deterministic_mode_tests -- --nocapture | \
        grep "Deterministic mode attestation"
done

# Output should be identical all 10 times
```

### Memory Leak Detection

```bash
# Use valgrind or heaptrack
cargo build --release -p adapteros-lora-mlx-ffi --bin benchmark

valgrind --leak-check=full \
    ./target/release/examples/generation_example \
    --tokens 100

# Or with heaptrack (macOS-friendly)
heaptrack ./target/release/examples/generation_example
heaptrack_gui heaptrack.target.release.examples.generation_example.*.gz
```

---

## Part 5: Troubleshooting

### Issue: Benchmark shows no improvement

**Diagnosis:**
```bash
# Check if optimization was actually compiled
cargo build --release -p adapteros-lora-mlx-ffi

# Add debug output
cargo bench --bench mlx_benchmarks -- --verbose 2>&1 | grep "optimization_point"
```

**Solution:** Verify the optimization code path is being executed

### Issue: Determinism test fails after optimization

**Diagnosis:**
```bash
cargo test --test deterministic_mode_tests -- --nocapture | grep -A 5 "FAIL"
```

**Solution:** Likely introduced floating-point variance, check tolerance levels

### Issue: Buffer pool causes memory leak

**Diagnosis:**
```rust
// In test
let stats = buffer_pool::stats();
println!("Pool has {} buffers, {} bytes",
    stats.total_buffers,
    stats.total_bytes
);

// Should decrease after clear()
buffer_pool::clear();
let stats = buffer_pool::stats();
assert_eq!(stats.total_buffers, 0);
```

**Solution:** Ensure `release()` is called for all acquired buffers

---

## Part 6: Performance Report Template

### Before Optimization

```
=== MLX Backend Performance (Baseline) ===

Single Token Latency:
  h=1024, r=16:  280µs
  h=4096, r=16:  520µs

MatMul GFLOPS:
  2048×64:  41.2 GFLOPS
  4096×64:  38.7 GFLOPS

Deterministic Mode Overhead:
  GPU mode:      2.05µs per op
  CPU mode:      2.62µs per op
  Overhead:      27.8%

Memory Profile:
  Peak usage:    128 MB
  Allocations:   3,421
  Per-token:     2.8 KB
```

### After Optimization

```
=== MLX Backend Performance (After Phase 1) ===

Single Token Latency:
  h=1024, r=16:  240µs  (-14% improvement) ✓
  h=4096, r=16:  460µs  (-12% improvement) ✓

MatMul GFLOPS:
  2048×64:  62.5 GFLOPS  (+51% improvement) ✓
  4096×64:  58.2 GFLOPS  (+50% improvement) ✓

Deterministic Mode Overhead:
  GPU mode:      2.05µs per op
  CPU mode:      2.32µs per op  (reduced from 2.62µs)
  Overhead:      13.2%  (target: <12%) ✓

Memory Profile:
  Peak usage:    118 MB  (-8% improvement)
  Allocations:   2,154  (-37% improvement) ✓
  Per-token:     2.1 KB  (-25% improvement) ✓

Cumulative Improvement:
  Overall latency:       -15%
  Deterministic overhead: -65%
  Memory efficiency:     +25%
  Gap to Metal:          3.3x → 2.8x (15% progress)
```

---

## Summary Checklist

- [ ] Baseline captured: `cargo bench -- --save-baseline phase0`
- [ ] Performance counters analyzed: `PerformanceSnapshot::capture()`
- [ ] Deterministic overhead measured: `test_deterministic_mode_performance_overhead`
- [ ] Phase 1A implemented: `softmax_with_validation()`
- [ ] Phase 1A benchmarked: Verify 50% softmax improvement
- [ ] Phase 1A validated: `cargo test --test deterministic_mode_tests`
- [ ] Phase 1B implemented: `mlx_softmax_batch()`
- [ ] Phase 1B benchmarked: Verify 40% eval overhead reduction
- [ ] Phase 1B validated: Determinism tests pass
- [ ] Combined improvement measured: Target 60-70% overhead reduction
- [ ] Memory profiling complete: Buffer pool benefits verified
- [ ] Final report generated: Document all improvements
- [ ] Optimizations committed: Push to main branch

---

**Last Updated:** 2025-11-19
**Status:** Ready for Implementation
**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
