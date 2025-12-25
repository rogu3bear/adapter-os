# AdapterOS Performance Documentation

This directory contains performance baselines, benchmarks, and optimization guides for AdapterOS components.

## Performance Baselines

### K-sparse Router Baseline

**[K_SPARSE_ROUTER_BASELINE.md](K_SPARSE_ROUTER_BASELINE.md)**

Comprehensive performance baseline for the K-sparse adapter routing system.

**Key Results**:
- **Average routing latency**: 1-3μs (target: < 100μs) ✓ PASS
- **Routing overhead**: < 0.01% of inference time (target: < 5%) ✓ PASS
- **Scaling**: Linear to 100 adapters with excellent performance

**Quick Test**:
```bash
cargo run --release --example perf_test -p adapteros-lora-router
```

## Benchmark Suite

### Location

Full benchmark suite: `tests/benchmark/`

### Available Benchmarks

| Benchmark | Description | Status |
|-----------|-------------|--------|
| `router_performance.rs` | K-sparse routing performance | ✓ Complete |
| `kernel_performance.rs` | Metal kernel operations | Available |
| `memory_benchmarks.rs` | Memory allocation patterns | Available |
| `throughput_benchmarks.rs` | Inference throughput | Available |
| `system_metrics.rs` | Telemetry overhead | Available |
| `isolation_benchmarks.rs` | Multi-tenant isolation | Available |
| `evidence_benchmarks.rs` | Evidence processing | Available |

### Running Benchmarks

#### Quick Performance Test (Router)

```bash
# Simple router performance test (no dependencies)
cargo run --release --example perf_test -p adapteros-lora-router
```

#### Full Criterion Benchmarks

```bash
# Run all router benchmarks
cargo bench --package adapteros-benchmarks --bench router_performance

# Run specific benchmark category
cargo bench --package adapteros-benchmarks --bench router_performance -- router_latency_by_k

# Generate HTML reports
cargo bench --package adapteros-benchmarks --bench router_performance -- --save-baseline main
```

#### Benchmark Categories

Router benchmarks include:

1. **router_latency_by_k**: Measure latency for K=1, 3, 5, 8
2. **router_latency_by_adapter_count**: Measure latency for 10, 50, 100 adapters
3. **router_overhead**: Verify routing overhead < 5% of inference time
4. **router_with_policy_mask**: Performance with policy constraints
5. **router_determinism_modes**: Deterministic vs adaptive routing
6. **router_entropy_floors**: Impact of entropy floor settings
7. **router_with_policy_config**: Policy-configured router
8. **e2e_routing_pipeline**: End-to-end routing pipeline

## Performance Requirements

### Ruleset #11: Router Performance Budgets

| Metric | Target | Acceptable | Failure |
|--------|--------|------------|---------|
| Router overhead | < 5% | < 8% | ≥ 8% |
| Decision latency | < 100μs | < 1ms | ≥ 1ms |
| Per-adapter p95 | < 24ms | < 50ms | ≥ 50ms |
| Throughput | ≥ 40 tok/s | ≥ 30 tok/s | < 30 tok/s |

### Current Performance (Baseline)

| Metric | Measured | Status |
|--------|----------|--------|
| Router overhead | 0.001% | ✓ PASS (500x better) |
| Decision latency | 1-3μs | ✓ PASS (33-100x better) |
| Throughput | TBD | - |

## Interpreting Results

### Criterion Output

Criterion provides statistical analysis including:
- **Mean**: Average execution time
- **Std Dev**: Variability in measurements
- **Median**: Middle value (less affected by outliers)
- **MAD**: Median Absolute Deviation (robust measure of variance)

Example output:
```
router_latency_by_k/3   time:   [1.2μs 1.3μs 1.4μs]
                        change: [-2.5% +0.8% +3.9%] (p = 0.25 > 0.05)
                        No change in performance detected.
```

### Performance Regression Detection

Criterion automatically detects regressions:
- **Green**: No significant change
- **Yellow**: Possible regression (p < 0.10)
- **Red**: Likely regression (p < 0.05)

### HTML Reports

View detailed results:
```bash
# Reports generated in target/criterion/
open target/criterion/router_latency_by_k/report/index.html
```

## CI/CD Integration

### GitHub Actions

Performance benchmarks run on:
- **Pull requests**: Compare against main branch
- **Nightly**: Track performance trends
- **On-demand**: Manual trigger for deep analysis

### Regression Thresholds

| Severity | Threshold | Action |
|----------|-----------|--------|
| Warning | > 10x baseline | Log warning |
| Error | > 100x baseline | Fail CI |
| Critical | > target (100μs) | Block merge |

## Profiling and Optimization

### CPU Profiling

```bash
# Install cargo-flamegraph
cargo install flamegraph

# Profile router
cargo flamegraph --example perf_test -p adapteros-lora-router
```

### Memory Profiling

```bash
# Install Valgrind/heaptrack
# macOS: brew install valgrind (limited support)
# Linux: apt-get install valgrind heaptrack

# Profile memory usage
valgrind --tool=massif cargo run --example perf_test -p adapteros-lora-router
```

### Performance Analysis Tools

- **Instruments** (macOS): CPU/GPU/Memory profiling
- **perf** (Linux): CPU performance counters
- **cargo-asm**: View generated assembly
- **criterion-compare**: Compare benchmark results

## Adding New Benchmarks

### 1. Create Benchmark File

```bash
# In tests/benchmark/benches/
touch my_benchmark.rs
```

### 2. Add to Cargo.toml

```toml
[[bench]]
name = "my_benchmark"
harness = false
```

### 3. Implement Benchmark

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_my_feature(c: &mut Criterion) {
    c.bench_function("my_feature", |b| {
        b.iter(|| {
            // Code to benchmark
            black_box(my_function());
        });
    });
}

criterion_group!(benches, bench_my_feature);
criterion_main!(benches);
```

### 4. Run and Baseline

```bash
# Run benchmark
cargo bench --bench my_benchmark

# Save baseline
cargo bench --bench my_benchmark -- --save-baseline main

# Compare future runs
cargo bench --bench my_benchmark -- --baseline main
```

## Best Practices

### Benchmark Design

1. **Warmup**: Allow sufficient warmup iterations
2. **Sample size**: Use appropriate sample sizes (50-100)
3. **Isolation**: Ensure benchmarks don't interfere
4. **Determinism**: Use deterministic inputs
5. **Cleanup**: Properly clean up resources

### System Configuration

For reliable benchmarks:

1. **Disable CPU frequency scaling**:
   ```bash
   # macOS: Not easily configurable
   # Linux: sudo cpupower frequency-set --governor performance
   ```

2. **Close background apps**: Minimize system noise

3. **Run on dedicated hardware**: Avoid shared CI runners for baseline

4. **Consistent environment**: Same OS, compiler, hardware

### Statistical Significance

- Use Criterion's statistical analysis
- Look for p-values < 0.05 for significance
- Check for outliers (MAD vs Std Dev)
- Run multiple iterations for stability

## Troubleshooting

### Common Issues

**Benchmark doesn't compile**:
- Ensure all dependencies are available
- Check for feature flags
- Verify workspace configuration

**Inconsistent results**:
- Increase sample size
- Check for system activity
- Verify deterministic inputs

**Very slow benchmarks**:
- Reduce sample size for iteration
- Use `--quick` for faster iteration
- Profile to find bottlenecks

## Resources

### Internal Documentation

- [K-sparse Router Baseline](K_SPARSE_ROUTER_BASELINE.md)
- [Benchmark README](../../tests/benchmark/README.md)
- [Router Metrics](../../crates/adapteros-lora-router/src/metrics.rs)

### External References

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Flamegraph Guide](https://www.brendangregg.com/flamegraphs.html)

## Contributing

When adding new performance optimizations:

1. Run benchmarks before changes (baseline)
2. Implement optimization
3. Run benchmarks after changes
4. Compare results with statistical significance
5. Document performance improvements
6. Update baseline documentation
7. Add regression tests to CI

## License

Performance documentation follows the same license as AdapterOS (MIT OR Apache-2.0).
