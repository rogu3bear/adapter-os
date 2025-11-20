# MLX Backend Profiling Guide

Complete guide to profiling, benchmarking, and optimizing the MLX backend.

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Table of Contents

1. [Overview](#overview)
2. [Quick Start](#quick-start)
3. [Benchmark Suite](#benchmark-suite)
4. [Performance Monitoring](#performance-monitoring)
5. [Optimization Workflow](#optimization-workflow)
6. [Visualization](#visualization)
7. [Troubleshooting](#troubleshooting)

---

## Overview

The MLX backend includes comprehensive profiling infrastructure for identifying and resolving performance bottlenecks:

### Components

1. **Rust Benchmarks** (`benches/mlx_benchmarks.rs`)
   - Criterion-based benchmark suite
   - 12 comprehensive benchmark groups
   - Automated baseline comparison

2. **C++ Profiling** (`src/performance_profiler.{h,cpp}`)
   - Low-overhead performance counters
   - Operation-level timing
   - Lock-free atomic counters

3. **Rust Monitoring** (`src/performance.rs`)
   - High-level performance API
   - Snapshot and delta tracking
   - JSON export for analysis

4. **Visualization** (`scripts/visualize_performance.py`)
   - Matplotlib-based graphs
   - 6 comprehensive chart types
   - Automated report generation

5. **Documentation**
   - Optimization report with recommendations
   - Benchmark README with usage guide
   - This profiling guide

---

## Quick Start

### 1. Run Benchmarks

```bash
# Full benchmark suite
cargo bench --bench mlx_benchmarks

# Specific benchmark group
cargo bench --bench mlx_benchmarks -- latency

# Save baseline
cargo bench --bench mlx_benchmarks -- --save-baseline main
```

### 2. Generate Visualizations

```bash
# Install Python dependencies
python3 -m pip install matplotlib numpy seaborn

# Generate graphs
./scripts/visualize_performance.py
```

### 3. Automated Workflow

```bash
# All-in-one: benchmark + visualize + compare
./scripts/run_benchmarks.sh --save-baseline main --visualize --compare
```

---

## Benchmark Suite

### Available Benchmarks

| Benchmark | Purpose | Sample Size | Duration |
|-----------|---------|-------------|----------|
| `single_token_latency` | Single token generation time | 100 | 5s |
| `batch_throughput` | Tokens/sec for batches | 30 | 10s |
| `memory_allocation_patterns` | Allocation efficiency | 50 | 5s |
| `cache_efficiency` | Memory access patterns | 100 | 5s |
| `adapter_switching_overhead` | Hot-swap cost | 100 | 5s |
| `matmul_operations` | Matrix multiplication perf | 50 | 8s |
| `attention_mechanism` | Attention computation | 20 | 8s |
| `lora_forward_pass` | Full LoRA computation | 50 | 8s |
| `multi_adapter_fusion` | K-sparse adapter fusion | 30 | 8s |
| `memory_transfers` | Copy/clone operations | 50 | 5s |
| `gc_impact` | Garbage collection overhead | 100 | 5s |
| `shared_vs_separate` | Architecture comparison | 50 | 5s |

### Running Individual Benchmarks

```bash
# Latency benchmarks
cargo bench --bench mlx_benchmarks -- "single_token_latency"

# Throughput benchmarks
cargo bench --bench mlx_benchmarks -- "batch_throughput"

# Memory benchmarks
cargo bench --bench mlx_benchmarks -- "memory_allocation"

# Operation benchmarks
cargo bench --bench mlx_benchmarks -- "matmul"
cargo bench --bench mlx_benchmarks -- "attention"
cargo bench --bench mlx_benchmarks -- "lora_forward"
```

### Interpreting Results

Example output:
```
mlx_single_token_latency/latency/h1024_r16
                        time:   [278.45 µs 282.13 µs 286.42 µs]
                        change: [-2.3% +0.5% +3.1%] (p = 0.42 > 0.05)
```

**Key metrics:**
- **278.45 µs:** Lower bound (95% confidence)
- **282.13 µs:** Mean latency
- **286.42 µs:** Upper bound (95% confidence)
- **+0.5%:** Change from baseline (median)
- **p = 0.42:** Statistical significance (>0.05 = no significant change)

---

## Performance Monitoring

### Rust API

#### Basic Usage

```rust
use adapteros_lora_mlx_ffi::performance::{PerformanceProfiler, PerformanceSnapshot};

// Create profiler
let profiler = PerformanceProfiler::new();

// Reset counters
profiler.reset();

// ... run operations ...

// Take snapshot
profiler.snapshot()?;

// Get latest snapshot
if let Some(snapshot) = profiler.latest_snapshot() {
    println!("{}", snapshot.generate_report());
}
```

#### Advanced Features

```rust
// Track performance over time
let profiler = PerformanceProfiler::new();

// Snapshot 1 (before optimization)
profiler.snapshot()?;

// ... run workload ...

// Snapshot 2 (after optimization)
profiler.snapshot()?;

// Calculate delta
if let Some(delta) = profiler.delta(0, 1) {
    println!("Memory delta: {} bytes", delta.memory_delta_bytes);
    for (op, stats) in &delta.operation_deltas {
        println!("{}: {} calls, {:.2}ms total",
            op, stats.count_delta, stats.total_ms_delta);
    }
}
```

#### Export for Analysis

```rust
use adapteros_lora_mlx_ffi::performance::PerformanceSnapshot;

let snapshot = PerformanceSnapshot::capture()?;

// Export to JSON
let json = snapshot.to_json()?;
std::fs::write("performance_data.json", json)?;

// Identify bottlenecks
let bottlenecks = snapshot.bottlenecks();
for (name, stats) in bottlenecks {
    println!("Bottleneck: {} ({:.2}ms total)", name, stats.total_ms);
}
```

### Operation Timer

```rust
use adapteros_lora_mlx_ffi::performance::OperationTimer;

fn my_expensive_operation() {
    let _timer = OperationTimer::new("my_operation");
    // ... work ...
    // Timer logs elapsed time on drop
}
```

### Performance Metrics

```rust
use adapteros_lora_mlx_ffi::performance::PerformanceMetrics;
use std::time::Duration;

let metrics = PerformanceMetrics::new();

// Record token generation
metrics.record_token_generated(Duration::from_millis(10));

// Record adapter switch
metrics.record_adapter_switch();

// Get statistics
println!("Tokens/sec: {:.2}", metrics.tokens_per_second());
println!("Avg latency: {:.2}ms", metrics.avg_latency_ms());
```

---

## Optimization Workflow

### Step 1: Baseline Measurement

```bash
# Establish baseline
cargo bench --bench mlx_benchmarks -- --save-baseline before-opt

# Or use script
./scripts/run_benchmarks.sh --save-baseline before-opt --visualize
```

### Step 2: Profile and Identify Bottlenecks

```rust
use adapteros_lora_mlx_ffi::performance::PerformanceSnapshot;

let snapshot = PerformanceSnapshot::capture()?;

// Get top operations by time
let top_ops = snapshot.top_operations(5);
for (name, stats) in top_ops {
    println!("{}: {:.2}ms ({} calls)", name, stats.total_ms, stats.count);
}

// Identify bottlenecks
let bottlenecks = snapshot.bottlenecks();
```

### Step 3: Implement Optimization

Focus on the highest-impact bottlenecks first:

**Priority 1 (70% of time):** MatMul operations
```rust
// Quick win: Use Accelerate.framework
#[cfg(target_os = "macos")]
use accelerate_src::cblas;
```

**Priority 2 (15% of time):** Memory allocations
```rust
// Quick win: Implement buffer pool
struct BufferPool { /* ... */ }
```

**Priority 3 (10% of time):** Eval synchronization
```rust
// Quick win: Batch operations before eval
let pending_ops = vec![op1, op2, op3];
mx::eval(pending_ops);
```

### Step 4: Measure Impact

```bash
# Compare against baseline
cargo bench --bench mlx_benchmarks -- --baseline before-opt

# Or use script
./scripts/run_benchmarks.sh --baseline before-opt --visualize
```

### Step 5: Validate and Document

1. **Check for regressions:**
   - Look for red text in benchmark output
   - Ensure no other benchmarks regressed

2. **Document changes:**
   - Update `PERFORMANCE_OPTIMIZATION_REPORT.md`
   - Note actual vs expected improvement
   - Document any tradeoffs

3. **Commit baseline:**
   ```bash
   git add target/criterion/*/base/
   git commit -m "perf: Add baseline after optimization"
   ```

---

## Visualization

### Generate All Graphs

```bash
./scripts/visualize_performance.py
```

### Generated Visualizations

1. **operation_breakdown.png**
   - Pie chart: Time distribution
   - Bar chart: Absolute times

2. **latency_distribution.png**
   - Error bars: Min to max range
   - Points: Average latency

3. **throughput_analysis.png**
   - Bar chart: Operations per second
   - Log scale for wide range

4. **memory_analysis.png**
   - Total memory usage
   - Allocation statistics

5. **efficiency_metrics.png**
   - Bubble chart: Calls vs latency
   - Bubble size: Total time

6. **mlx_vs_metal_comparison.png**
   - Side-by-side comparison
   - Ratio labels

### Custom Data Source

```bash
# Use custom performance data file
./scripts/visualize_performance.py /path/to/performance_data.json
```

### Interpreting Graphs

**Operation Breakdown (Pie/Bar):**
- Shows where time is spent
- Focus optimization on largest segments

**Latency Distribution:**
- Min = best-case performance
- Avg = typical performance
- Max = worst-case / outliers

**Efficiency Map (Bubble):**
- Upper-left = high latency, low call count (one-time operations)
- Lower-right = low latency, high call count (hot path)
- Large bubbles = optimization targets

---

## Troubleshooting

### High Variance in Results

**Symptoms:**
- Wide confidence intervals
- Large number of outliers
- Inconsistent results

**Solutions:**
1. Close background applications
2. Disable CPU throttling (macOS):
   ```bash
   sudo pmset -a disablesleep 1
   ```
3. Increase sample size:
   ```bash
   cargo bench --bench mlx_benchmarks -- --sample-size 200
   ```
4. Use longer measurement time:
   ```bash
   cargo bench --bench mlx_benchmarks -- --measurement-time 30
   ```

### Memory Exhaustion

**Symptoms:**
- Benchmarks fail with OOM
- System becomes unresponsive

**Solutions:**
1. Reduce batch sizes in benchmark configuration
2. Run benchmark groups individually:
   ```bash
   cargo bench --bench mlx_benchmarks -- latency
   cargo bench --bench mlx_benchmarks -- memory
   # etc.
   ```
3. Clear memory between runs:
   ```rust
   use adapteros_lora_mlx_ffi::memory;
   memory::gc_collect();
   memory::reset();
   ```

### Build Failures

**Symptoms:**
- Compilation errors
- Linking errors

**Solutions:**
1. Clean and rebuild:
   ```bash
   cargo clean
   cargo build --release --benches
   ```
2. Check criterion version:
   ```bash
   cargo tree | grep criterion
   ```
3. Verify dependencies:
   ```bash
   cargo update
   cargo check --benches
   ```

### Visualization Errors

**Symptoms:**
- Python import errors
- Matplotlib rendering issues

**Solutions:**
1. Install dependencies:
   ```bash
   python3 -m pip install matplotlib numpy seaborn
   ```
2. Check Python version (requires 3.7+):
   ```bash
   python3 --version
   ```
3. Use virtual environment:
   ```bash
   python3 -m venv .venv
   source .venv/bin/activate
   pip install matplotlib numpy seaborn
   ```

---

## Best Practices

### 1. Establish Baselines Early

Before making any changes, save a baseline:
```bash
cargo bench --bench mlx_benchmarks -- --save-baseline main
```

### 2. Profile Before Optimizing

Don't guess where the bottleneck is—measure it:
```rust
let snapshot = PerformanceSnapshot::capture()?;
let bottlenecks = snapshot.bottlenecks();
```

### 3. Optimize One Thing at a Time

Change one thing, measure impact, then move to next:
```bash
# After each change
cargo bench --bench mlx_benchmarks -- --baseline main
```

### 4. Document All Changes

Update `PERFORMANCE_OPTIMIZATION_REPORT.md` with:
- What was changed
- Why it was changed
- Expected vs actual improvement
- Any tradeoffs made

### 5. Watch for Regressions

Check that optimizations don't break other parts:
- Run full benchmark suite after changes
- Look for red text (regressions)
- Investigate any unexpected slowdowns

### 6. Use Version Control

Commit baselines to track performance over time:
```bash
git add target/criterion/*/base/
git commit -m "perf: Baseline after MatMul optimization"
```

---

## Additional Resources

- **[benches/README.md](benches/README.md)** - Detailed benchmark documentation
- **[PERFORMANCE_OPTIMIZATION_REPORT.md](PERFORMANCE_OPTIMIZATION_REPORT.md)** - Analysis and recommendations
- **[src/performance.rs](src/performance.rs)** - Performance monitoring API
- **[CLAUDE.md](../../CLAUDE.md)** - Development guidelines

---

**Generated by:** Claude AI Agent
**Last Updated:** 2025-01-19
**Maintained by:** James KC Auchterlonie
