# MLX Backend Performance Benchmarks

Comprehensive benchmark suite for profiling and optimizing the MLX backend performance.

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Overview

This benchmark suite profiles the MLX backend across multiple dimensions:

- **Single Token Latency:** Time to generate one token (critical for interactive use)
- **Batch Throughput:** Tokens per second for various batch sizes
- **Memory Allocation:** Allocation patterns and memory efficiency
- **Cache Efficiency:** Impact of different memory access patterns
- **Adapter Switching:** Overhead of hot-swapping adapters
- **Operation-Level:** Individual operation profiling (matmul, attention, etc.)

---

## Quick Start

### Run All Benchmarks

```bash
# Run entire benchmark suite
cargo bench --bench mlx_benchmarks

# Run with detailed output
cargo bench --bench mlx_benchmarks -- --verbose
```

### Run Specific Benchmark Groups

```bash
# Latency benchmarks only
cargo bench --bench mlx_benchmarks -- latency

# Throughput benchmarks
cargo bench --bench mlx_benchmarks -- throughput

# Memory benchmarks
cargo bench --bench mlx_benchmarks -- memory

# Operation-level benchmarks
cargo bench --bench mlx_benchmarks -- operation

# Cache efficiency benchmarks
cargo bench --bench mlx_benchmarks -- cache

# Comparison benchmarks
cargo bench --bench mlx_benchmarks -- comparison
```

### Run Individual Benchmarks

```bash
# Single token latency
cargo bench --bench mlx_benchmarks -- "single_token_latency"

# Batch throughput
cargo bench --bench mlx_benchmarks -- "batch_throughput"

# MatMul operations
cargo bench --bench mlx_benchmarks -- "matmul"

# Attention mechanism
cargo bench --bench mlx_benchmarks -- "attention"
```

---

## Benchmark Configuration

### Sample Sizes

- **Latency benchmarks:** 100 samples, 5s measurement time
- **Throughput benchmarks:** 30 samples, 10s measurement time
- **Memory benchmarks:** 50 samples, 5s measurement time
- **Operation benchmarks:** 50 samples, 8s measurement time

### Parameters

The benchmarks test various configurations:

- **Vocab sizes:** 8,192 | 32,000 | 152,064
- **Sequence lengths:** 1 | 8 | 32 | 128 | 512
- **Batch sizes:** 1 | 4 | 8 | 16
- **Adapter counts (K):** 1 | 2 | 4 | 8
- **Hidden dimensions:** 768 | 1024 | 2048 | 4096
- **LoRA ranks:** 4 | 8 | 16 | 32 | 64

---

## Interpreting Results

### Example Output

```
mlx_single_token_latency/latency/h1024_r16
                        time:   [278.45 µs 282.13 µs 286.42 µs]
                        change: [-2.3% +0.5% +3.1%] (p = 0.42 > 0.05)
                        No change in performance detected.
Found 8 outliers among 100 measurements (8.00%)
  4 (4.00%) high mild
  4 (4.00%) high severe
```

**Interpretation:**
- **Mean time:** 282.13µs (average latency)
- **Confidence interval:** [278.45µs, 286.42µs] (95% confidence)
- **Change:** +0.5% (compared to previous baseline)
- **Outliers:** 8% of samples were outliers (normal)

### Performance Targets

| Benchmark | Current | Target | Status |
|-----------|---------|--------|---------|
| Single token (h=1024, r=16) | ~280µs | <200µs | ⚠️ |
| Batch throughput (b=4, s=32) | ~75 tok/s | >100 tok/s | ⚠️ |
| Memory usage (k=4) | 115 MB | <150 MB | ✅ |
| Adapter switch | ~1.8µs | <5µs | ✅ |

---

## Baseline Management

### Save Baseline

Save current performance as baseline for future comparison:

```bash
cargo bench --bench mlx_benchmarks -- --save-baseline main
```

### Compare Against Baseline

Compare current performance against saved baseline:

```bash
cargo bench --bench mlx_benchmarks -- --baseline main
```

### Multiple Baselines

Track multiple baselines for different branches:

```bash
# Save baseline for feature branch
cargo bench --bench mlx_benchmarks -- --save-baseline feature-xyz

# Compare against feature baseline
cargo bench --bench mlx_benchmarks -- --baseline feature-xyz
```

---

## Profiling with Performance Counters

The benchmark suite includes C++ performance profiling instrumentation:

### Enable Profiling

```rust
use adapteros_lora_mlx_ffi::performance::PerformanceProfiler;

// Create profiler
let profiler = PerformanceProfiler::new();

// Reset counters
profiler.reset();

// Run operations...

// Take snapshot
profiler.snapshot()?;

// Generate report
let report = profiler.summary_report();
println!("{}", report);
```

### Export Performance Data

```rust
use adapteros_lora_mlx_ffi::performance::PerformanceSnapshot;

// Capture snapshot
let snapshot = PerformanceSnapshot::capture()?;

// Export to JSON
let json = snapshot.to_json()?;
std::fs::write("performance_data.json", json)?;
```

---

## Visualization

### Generate Performance Graphs

```bash
# Install dependencies
python3 -m pip install matplotlib numpy seaborn

# Generate visualizations
./scripts/visualize_performance.py

# Or specify custom data file
./scripts/visualize_performance.py target/criterion/performance_data.json
```

### Generated Visualizations

1. **operation_breakdown.png** - Pie/bar chart of time by operation
2. **latency_distribution.png** - Min/avg/max latency for each operation
3. **throughput_analysis.png** - Operations per second
4. **memory_analysis.png** - Memory usage and allocation statistics
5. **efficiency_metrics.png** - Bubble chart of calls vs latency
6. **mlx_vs_metal_comparison.png** - Backend comparison

---

## Continuous Integration

### Automated Performance Testing

Add to CI pipeline:

```yaml
# .github/workflows/performance.yml
name: Performance Benchmarks

on:
  push:
    branches: [main]
  pull_request:

jobs:
  benchmark:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3

      - name: Run benchmarks
        run: cargo bench --bench mlx_benchmarks -- --save-baseline ci-${{ github.sha }}

      - name: Compare with main
        if: github.event_name == 'pull_request'
        run: |
          git fetch origin main
          git checkout origin/main
          cargo bench --bench mlx_benchmarks -- --save-baseline main
          git checkout -
          cargo bench --bench mlx_benchmarks -- --baseline main

      - name: Upload results
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-results
          path: target/criterion/
```

### Regression Detection

Set up alerts for performance regressions:

- **Latency regression:** > 10% increase
- **Throughput regression:** > 10% decrease
- **Memory leak:** Continuous growth over 1 hour

---

## Troubleshooting

### Benchmark Instability

If you see high variance in results:

1. **Close background applications** that may interfere
2. **Disable CPU throttling:**
   ```bash
   # macOS
   sudo pmset -a disablesleep 1
   ```
3. **Increase sample size:**
   ```bash
   cargo bench --bench mlx_benchmarks -- --sample-size 200
   ```
4. **Use longer measurement time:**
   ```bash
   cargo bench --bench mlx_benchmarks -- --measurement-time 30
   ```

### Memory Issues

If benchmarks fail due to memory:

1. **Reduce batch sizes** in test parameters
2. **Clear memory between runs:**
   ```rust
   use adapteros_lora_mlx_ffi::memory;
   memory::gc_collect();
   ```
3. **Run benchmarks individually** rather than all at once

### Build Errors

If the benchmark doesn't compile:

1. **Check Criterion version:**
   ```bash
   cargo tree | grep criterion
   ```
2. **Clean build:**
   ```bash
   cargo clean
   cargo build --release --benches
   ```

---

## Optimization Workflow

### 1. Identify Bottlenecks

Run full benchmark suite and identify slow operations:

```bash
cargo bench --bench mlx_benchmarks -- --save-baseline before-opt
```

### 2. Implement Optimization

Make code changes targeting the bottleneck.

### 3. Measure Impact

Compare performance:

```bash
cargo bench --bench mlx_benchmarks -- --baseline before-opt
```

### 4. Validate No Regressions

Ensure other benchmarks didn't regress:

```bash
# Look for red text (regressions)
# Green text indicates improvements
# White text indicates no change
```

### 5. Document Changes

Update `PERFORMANCE_OPTIMIZATION_REPORT.md` with:
- What was optimized
- Expected vs actual improvement
- Any tradeoffs

---

## Advanced Usage

### Custom Benchmark Parameters

Modify benchmark parameters in `mlx_benchmarks.rs`:

```rust
const VOCAB_SIZES: &[usize] = &[8_192, 32_000, 152_064];  // Add your sizes
const SEQUENCE_LENGTHS: &[usize] = &[1, 8, 32, 128, 512];  // Add your lengths
```

### Integration with External Tools

#### Flamegraph Profiling

```bash
# Install cargo-flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bench mlx_benchmarks
```

#### Heaptrack Memory Profiling

```bash
# Install heaptrack (macOS)
brew install heaptrack

# Profile memory
heaptrack cargo bench --bench mlx_benchmarks
heaptrack_gui heaptrack.cargo.*.gz
```

#### Instruments (macOS)

```bash
# Build with symbols
cargo bench --bench mlx_benchmarks --no-run

# Profile with Instruments
instruments -t "Time Profiler" ./target/release/deps/mlx_benchmarks-*
```

---

## Related Documentation

- **[PERFORMANCE_OPTIMIZATION_REPORT.md](../PERFORMANCE_OPTIMIZATION_REPORT.md)** - Detailed analysis and recommendations
- **[/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/src/performance.rs](../src/performance.rs)** - Performance monitoring API
- **[CLAUDE.md](../../../CLAUDE.md)** - Development guidelines

---

## Contact

For questions or issues with the benchmark suite:
- Review the optimization report
- Check existing documentation
- Examine benchmark source code for implementation details

---

**Last Updated:** 2025-01-19
**Maintained by:** James KC Auchterlonie
