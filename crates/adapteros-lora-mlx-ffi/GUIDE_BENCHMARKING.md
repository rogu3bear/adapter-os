# MLX FFI Backend Benchmarking Guide

## Overview

This guide explains how to run and interpret performance benchmarks for the AdapterOS MLX FFI backend.

**Contents:**
1. Running benchmarks
2. Interpreting results
3. Regression testing
4. Performance optimization workflow

## 1. Running Benchmarks

### 1.1 Comprehensive Performance Benchmarks

Run the full benchmark suite with detailed metrics:

```bash
# All benchmarks
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance

# Specific benchmark group
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance forward_pass_latency

# With verbose output and HTML reports
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --verbose
```

**Output:**
- Terminal: Real-time results with comparisons to baseline
- HTML Reports: `target/criterion/report/index.html`

### 1.2 Original MLX Performance Benchmarks

Run the standard benchmark suite:

```bash
# All original benchmarks
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_performance

# Single benchmark
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_performance inference_step
```

### 1.3 Regression Tests

Run automated regression tests with performance assertions:

```bash
# All regression tests with output
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture

# Single test
cargo test -p adapteros-lora-mlx-ffi test_inference_step_latency_baseline -- --nocapture

# With exact output (no parallelization)
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture --test-threads=1
```

## 2. Benchmark Groups

### 2.1 Forward Pass Latency (`forward_pass_latency`)

Measures end-to-end inference latency for varying sequence lengths and vocabulary sizes.

**Parameters:**
- Sequence length: 1, 4, 8, 16 tokens
- Vocabulary size: 8K, 32K, 128K

**Expected Results:**
```
Sequence Length 1:    ~1-2ms
Sequence Length 4:    ~1.8-3ms
Sequence Length 8:    ~2.5-4ms
Sequence Length 16:   ~3.8-5.2ms
```

**Interpretation:**
- Linear scaling with sequence length = good for batching
- Sub-linear scaling with vocabulary = optimized

### 2.2 FFI Overhead (`ffi_overhead`)

Isolates FFI boundary overhead from compute overhead.

**Comparisons:**
- Rust-only vector operations (baseline)
- FFI tensor allocation (includes marshalling)
- FFI array data extraction (zero-copy)

**Expected Overhead:**
```
1KB:    ~125% (FFI is 2.25x slower)
4KB:    ~50% (FFI is 1.5x slower)
16KB:   ~28% (FFI is 1.28x slower)
64KB:   ~13% (FFI is 1.13x slower)
```

**Key Insight:** FFI overhead amortizes with larger buffers. Batching reduces relative cost.

### 2.3 Generation Throughput (`generation_throughput`)

Measures tokens generated per second.

**Metrics:**
- TTFT: Speed-to-first-token (latency for first generated token)
- Throughput: Sustained tokens/sec after first token

**Expected Results (Stub):**
```
10 tokens:   ~12ms (833 tokens/sec)
50 tokens:   ~55ms (909 tokens/sec)
100 tokens:  ~110ms (909 tokens/sec)
```

**Production Expectation:** Real MLX + GPU can achieve 500-2000 tokens/sec depending on model size.

### 2.4 Memory Patterns (`memory_patterns`)

Analyzes memory allocation efficiency and fragmentation.

**Tests:**
- Single large allocation (1MB)
- Repeated small allocations (100x, 10KB each)
- Mixed-size allocations
- Adapter lifecycle (load/unload)

**Expected Results:**
```
Single 1MB allocation:     ~600μs
100x 10KB allocations:     ~45ms (fragmentation cost)
Mixed sizes:               ~15ms
Adapter lifecycle:         ~3ms total
```

### 2.5 Batch Operations (`batch_operations`)

Compares single operation vs batch efficiency.

**Tests:**
- Single matrix multiply
- Batch of 4 matrix multiplies

**Expected Results (Stub):**
```
Single 256x256: ~8μs
Batch 4x:       ~32μs (8μs each, no batching benefit in stub)
```

**Production Expectation:** With real GPU, batch of 4 should complete faster than 1 sequential (parallelism).

### 2.6 Regression Baselines

Establishes baseline metrics to detect performance degradation.

**Key Baselines:**
- Standard inference step: ~1.8ms
- Baseline adapter load: ~2.1ms
- K=4 routing (32 adapters): ~2ms

**Thresholds:**
- Latency regression >10% = FAIL (requires investigation)
- Memory regression >15% = FAIL (requires optimization)
- Throughput regression >5% = FAIL (root cause analysis needed)

## 3. Regression Testing Workflow

### 3.1 Establish Baseline

Before making changes:

```bash
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --save-baseline main
```

### 3.2 Make Changes

Optimize code, implement features, etc.

### 3.3 Compare Against Baseline

After changes:

```bash
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --baseline main
```

**Output Example:**
```
Inference Step (vocab=32K):
    time:   [1.79 ms 1.83 ms 1.87 ms]
            change: [-5.21% -2.74% -0.15%] (likely improved)

Standard Inference Step:
    time:   [1.80 ms 1.85 ms 1.90 ms]
            change: [-5.00% +1.00% +7.00%] (regression detected!)
```

### 3.4 Automated Regression Tests

Run tests with performance assertions:

```bash
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture
```

**Tests that fail indicate regression:**
- `test_inference_step_latency_baseline` - inference slower than 2.5ms
- `test_tensor_allocation_latency_baseline` - allocation slower than thresholds
- `test_adapter_load_latency_baseline` - adapter load slower than 3.5ms
- `test_routing_scaling_baseline` - routing doesn't scale linearly with K

## 4. Performance Optimization Workflow

### 4.1 Identify Bottleneck

Use benchmarks to identify where time is spent:

```bash
# Run FFI overhead benchmark
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance ffi_overhead -- --verbose

# Analyze HTML report
open target/criterion/report/index.html
```

**Common Bottlenecks:**
1. FFI marshalling overhead (FFI boundary)
2. Memory allocation/deallocation (allocation patterns)
3. Tensor operations (compute)
4. LoRA forward pass (multi-adapter routing)

### 4.2 Measure Before Optimization

```bash
# Run regression tests to establish baseline
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture

# Save benchmark baseline
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --save-baseline before_opt
```

### 4.3 Implement Optimization

Example optimizations documented in `GUIDE_PERFORMANCE.md`:
1. **Memory pooling** - Reuse buffers
2. **Zero-copy FFI** - Avoid data copies
3. **Fused kernels** - Combine FFI calls
4. **Batch processing** - Process multiple sequences
5. **Quantization** - Reduce precision for gates

### 4.4 Validate Optimization

```bash
# Run regression tests
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture

# Compare benchmarks
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --baseline before_opt

# Expected: >10% improvement in target metric
```

### 4.5 Document Results

Create or update performance documentation:
- What was optimized
- Expected improvement
- Actual results
- Trade-offs (memory vs latency, etc.)

## 5. Interpreting Criterion Output

### 5.1 Criterion Report Example

```
forward_pass_latency/inference/seq1_8192
    time:   [1.18 ms 1.20 ms 1.22 ms]
    change: [-3.45% -1.23% +1.05%] (regression)

    Slope  [1.2000 ms/iter]
    R-squared: 0.9982 (excellent fit)
```

**Fields:**
- `time`: Measured latency with 95% confidence interval
- `change`: Percentage change vs baseline
- `Slope`: Time per iteration (primary metric)
- `R-squared`: Fit quality (0.99+ = excellent)

### 5.2 Regression/Improvement Interpretation

| Change | Status | Action |
|--------|--------|--------|
| -5% to +5% | Noise | No action needed |
| +5% to +15% | Minor regression | Investigate if consistent |
| +15% to +50% | Significant regression | Revert or optimize |
| >+50% | Critical regression | Stop, diagnose immediately |
| -5% to -20% | Minor improvement | Document |
| <-20% | Major improvement | Validate, document |

### 5.3 HTML Reports

Generate detailed HTML reports:

```bash
# Generate reports
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance

# View in browser
open target/criterion/report/index.html
```

**Report includes:**
- Performance charts over time
- Statistical analysis (mean, std dev, outliers)
- Comparisons between baseline and current
- Scatter plots of individual measurements

## 6. Common Performance Issues

### Issue: High FFI Overhead

**Symptoms:**
- FFI operations >2x slower than Rust equivalent
- Allocation latency >500μs for 1KB

**Solutions:**
1. Batch FFI calls (combine operations)
2. Use larger buffers (amortize overhead)
3. Cache converted types
4. Consider reducing FFI call frequency

### Issue: Memory Fragmentation

**Symptoms:**
- Allocation latency increases with iteration count
- Many small allocations slow down dramatically

**Solutions:**
1. Implement memory pool (pre-allocate buffers)
2. Use larger buffers (reduce allocation count)
3. Add garbage collection between passes
4. Profile with memory profiler (valgrind/heaptrack)

### Issue: Poor Scaling with K

**Symptoms:**
- Latency doesn't scale linearly with K adapters
- K=4 is 5x slower than K=1 (should be ~1.5x)

**Solutions:**
1. Profile individual adapter application
2. Check if gates are pre-computed
3. Look for unnecessary data copies
4. Implement fused multi-adapter kernels

### Issue: Generation Throughput Too Low

**Symptoms:**
- <100 tokens/sec (should be 500+)
- High TTFT (>100ms)

**Solutions:**
1. Implement KV cache for recurrent computation
2. Use continuous batching for multiple sequences
3. Enable speculative decoding
4. Profile forward pass latency

## 7. Performance Testing Checklist

Before submitting PR with performance-critical changes:

- [ ] Run regression tests: `cargo test --test performance_regression`
- [ ] Run comprehensive benchmarks
- [ ] Check HTML report for regressions
- [ ] Compare against baseline
- [ ] Document any changes >5%
- [ ] Verify no memory leaks (run stress test)
- [ ] Test on release build: `cargo bench --release`

## 8. CI/CD Integration

### 8.1 GitHub Actions Example

```yaml
- name: Run performance regression tests
  run: cargo test -p adapteros-lora-mlx-ffi --test performance_regression

- name: Run benchmarks
  run: cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --baseline main
  continue-on-error: true

- name: Upload benchmark results
  uses: actions/upload-artifact@v2
  with:
    name: benchmark-report
    path: target/criterion/report/
```

### 8.2 Comment on PR

Add comment to PR if regressions detected:

```
Performance Regression Detected:
- forward_pass_latency: +12% (1.80ms → 2.02ms)
- memory_patterns: +8% (1.2MB → 1.3MB)

Please investigate and optimize before merging.
```

## 9. Performance Targets

### Current (Stub Implementation)

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Inference latency | <2.5ms | 1.8-2.1ms | ✓ OK |
| Adapter load | <3.5ms | 2.1ms | ✓ OK |
| Allocation (4KB) | <1.5ms | ~1.0ms | ✓ OK |
| Memory pool overhead | <15% | ~10% | ✓ OK |

### Real MLX (Target)

| Metric | Target | Notes |
|--------|--------|-------|
| Inference latency | <100μs | GPU accelerated |
| Throughput | 500+ tokens/sec | With KV cache |
| Memory | <2GB per model | 7B parameter model |
| FFI overhead | <5% | Batched operations |

## 10. References

- **Criterion Documentation:** https://bheisler.github.io/criterion.rs/book/
- **Performance Guide:** `/crates/adapteros-lora-mlx-ffi/GUIDE_PERFORMANCE.md`
- **Architecture:** `/docs/ARCHITECTURE_INDEX.md`
- **MLX Benchmarks:** https://github.com/ml-explore/mlx/tree/main/benchmarks

## 11. Quick Commands

```bash
# Run all tests and benchmarks
cargo test -p adapteros-lora-mlx-ffi --features multi-backend,mlx
cargo bench -p adapteros-lora-mlx-ffi --features multi-backend,mlx

# Run specific benchmark
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance forward_pass_latency

# Baseline establishment
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --save-baseline main

# Regression check
cargo bench -p adapteros-lora-mlx-ffi --bench comprehensive_performance -- --baseline main

# Stress test
cargo test -p adapteros-lora-mlx-ffi test_stress_ -- --nocapture

# Full regression suite
cargo test -p adapteros-lora-mlx-ffi --test performance_regression -- --nocapture --test-threads=1
```

## 12. Contact & Support

For performance-related questions:
- See: `GUIDE_PERFORMANCE.md` (detailed optimization guide)
- File issue: GitHub Issues with `performance` label
- Code: `/crates/adapteros-lora-mlx-ffi/`
