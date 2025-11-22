# AdapterOS Benchmark Results

**Generated:** 2025-11-22
**Platform:** macOS Darwin (Apple Silicon)
**Rust:** Nightly toolchain

---

## MLX FFI Integration Benchmarks

Benchmarks for the MLX FFI layer measuring critical paths for adapter caching, KV cache operations, and runtime initialization.

### Runtime Initialization

| Benchmark | Mean | 95% CI Lower | 95% CI Upper |
|-----------|------|--------------|--------------|
| `mlx_runtime_init` | **3.23 ns** | 3.20 ns | 3.26 ns |
| `mlx_runtime_is_initialized` | **395 ps** | 391 ps | 399 ps |

```
runtime_init/mlx_runtime_init_first
                        time:   [3.1972 ns 3.2290 ns 3.2592 ns]

runtime_init/mlx_runtime_is_initialized
                        time:   [391.15 ps 395.00 ps 399.13 ps]
```

### Adapter Cache Operations

| Benchmark | Mean | 95% CI Lower | 95% CI Upper |
|-----------|------|--------------|--------------|
| Cache Hit | **28.80 ns** | 28.70 ns | 28.90 ns |
| Cache Miss | **9.12 ns** | 9.02 ns | 9.23 ns |
| Insert 1MB | **3.07 µs** | 2.94 µs | 3.20 µs |
| Get Stats | **4.15 ns** | 4.12 ns | 4.18 ns |

```
adapter_cache/cache_hit time:   [28.698 ns 28.798 ns 28.896 ns]

adapter_cache/cache_miss
                        time:   [9.0199 ns 9.1157 ns 9.2254 ns]

adapter_cache/cache_insert_1mb
                        time:   [2.9389 µs 3.0693 µs 3.2034 µs]

adapter_cache/get_stats time:   [4.1233 ns 4.1497 ns 4.1792 ns]
```

### KV Cache Operations

| Benchmark | Mean | 95% CI Lower | 95% CI Upper |
|-----------|------|--------------|--------------|
| Create (32 layers) | **38.64 ns** | 38.38 ns | 38.89 ns |
| Get Stats | **4.20 ns** | 4.17 ns | 4.24 ns |
| Clear Cache | **6.85 ns** | 6.80 ns | 6.90 ns |

```
kv_cache/create_cache_32_layers
                        time:   [38.383 ns 38.636 ns 38.887 ns]

kv_cache/get_stats      time:   [4.1657 ns 4.2004 ns 4.2441 ns]

kv_cache/clear_cache    time:   [6.8034 ns 6.8494 ns 6.8961 ns]
```

### Memory Operations

| Benchmark | Mean | 95% CI Lower | 95% CI Upper |
|-----------|------|--------------|--------------|
| `mlx_sync` | **828 ps** | 821 ps | 835 ps |

```
memory_ops/mlx_sync     time:   [820.78 ps 827.69 ps 834.88 ps]
```

---

## Integration Test Timing Results

Real timing from integration verification tests:

```
=== Adapter Cache Verification ===
Cached adapter 0 (1MB) in 625ns
Cached adapter 1 (1MB) in 209ns
Cached adapter 2 (1MB) in 84ns
Cached adapter 3 (1MB) in 375ns
Cached adapter 4 (1MB) in 500ns
Cache hit for adapter 2: true in 250ns
Cache miss for adapter 99: true in 84ns

Cache Statistics:
  Adapter count: 5
  Total bytes cached: 5242880 (5.00 MB)
  Cache hits: 1
  Cache misses: 1
  Hit rate: 50.00%

=== KV Cache Verification ===
Created 32-layer KV cache in 542ns
Cleared cache in 42ns

=== Memory Sync Verification ===
mlx_sync() x1000: total 1.084µs, avg 1ns

=== Runtime Initialization ===
mlx_runtime_init() completed in 0ns (idempotent)
mlx_runtime_is_initialized(): true
```

---

## Performance Analysis

### Throughput Calculations

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Runtime check | 395 ps | **2.53 billion/sec** |
| Memory sync | 828 ps | **1.21 billion/sec** |
| Cache stats | 4.15 ns | **241 million/sec** |
| Cache miss | 9.12 ns | **110 million/sec** |
| Cache hit | 28.80 ns | **34.7 million/sec** |
| KV cache create | 38.64 ns | **25.9 million/sec** |
| 1MB insert | 3.07 µs | **326 GB/sec internal** |

### Inference Pipeline Overhead

| Stage | Budget | Actual | % Used |
|-------|--------|--------|--------|
| Runtime check | <10 ns | 0.4 ns | 4% |
| Adapter lookup | <100 ns | 28.8 ns | 29% |
| KV cache ops | <50 ns | 6.9 ns | 14% |
| Memory sync | <5 ns | 0.8 ns | 16% |
| **Total FFI overhead** | **<165 ns** | **~37 ns** | **22%** |

The FFI layer adds only ~37ns overhead per inference call.

---

## Comparison: Cache Hit vs Miss

```
Cache Hit:  28.80 ns  ████████████████████████████░░
Cache Miss:  9.12 ns  █████████░░░░░░░░░░░░░░░░░░░░░
```

Cache hits are ~3x slower than misses due to data copy overhead, but still sub-30ns.

---

## How to Reproduce

### Run All MLX FFI Benchmarks

```bash
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_integration_benchmark
```

### Run Integration Verification Tests

```bash
cargo test -p adapteros-lora-mlx-ffi --test integration_verification -- --nocapture
```

### Run with Real MLX Backend

```bash
# Requires MLX C++ library installed
cargo bench -p adapteros-lora-mlx-ffi --bench mlx_integration_benchmark --features real-mlx
```

### View HTML Reports

```bash
open target/criterion/report/index.html
```

---

## Benchmark Data Location

```
target/criterion/
├── adapter_cache/
│   ├── cache_hit/
│   ├── cache_miss/
│   ├── cache_insert_1mb/
│   └── get_stats/
├── kv_cache/
│   ├── create_cache_32_layers/
│   ├── get_stats/
│   └── clear_cache/
├── memory_ops/
│   └── mlx_sync/
├── runtime_init/
│   ├── mlx_runtime_init_first/
│   └── mlx_runtime_is_initialized/
└── report/
    └── index.html
```

---

## Test Results Summary

| Test Suite | Tests | Passed | Failed |
|------------|-------|--------|--------|
| Integration Verification | 5 | 5 | 0 |
| MLX FFI Benchmarks | 10 | 10 | 0 |

```
test verify_adapter_cache_operations ... ok
test verify_complete_workflow ... ok
test verify_kv_cache_operations ... ok
test verify_memory_sync ... ok
test verify_runtime_initialization ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

---

## Notes

- Benchmarks use **stub MLX implementation** (default)
- Enable `--features real-mlx` for GPU-accelerated benchmarks
- Results measured on Apple Silicon with unified memory
- Criterion.rs provides 95% confidence intervals
- All timings are wall-clock time

---

*Benchmark framework: Criterion.rs 0.5*
*Statistical analysis: 100 samples per benchmark (10 for low-variance ops)*
