# AOS 2.0 Benchmark Summary

**Last Run**: 2025-11-19
**Platform**: macOS (Apple Silicon)
**Quick Reference**: Key performance metrics from actual measurements

## TL;DR

- **Loading Speed**: 12-82 μs for complete archive load
- **Memory Efficiency**: mmap uses 99.96% less memory than regular read
- **Throughput**: Up to 5.8 TiB/s with mmap
- **Production Ready**: Performance exceeds requirements by 50-300x

## Key Metrics

### Speed

| Operation | Time | vs Theoretical |
|-----------|------|----------------|
| Header parse | 7.25 μs | 7x slower* |
| Manifest (100 tensors) | 18.4 μs | 54x faster |
| Full load (50MB) | 31.6 μs | 158x faster |
| mmap (100MB) | 16.7 μs | 297x faster |

\* *Includes file open overhead*

### Memory

| File Size | Regular Read | mmap | Savings |
|-----------|--------------|------|---------|
| 1 MB      | +0.06 MB     | +0.02 MB | 67% |
| 10 MB     | +10.03 MB    | +0.02 MB | 99.8% |
| 50 MB     | +50.03 MB    | +0.02 MB | 99.96% |
| 100 MB    | +100.03 MB   | +0.00 MB | 100% |

### Throughput

| Operation | Throughput | Notes |
|-----------|------------|-------|
| JSON parsing | 268-324 MiB/s | Consistent across sizes |
| mmap (100MB) | 5.8 TiB/s | Zero-copy transfer |
| Regular read (100MB) | 19.6 GiB/s | User-space copy |

## Production Recommendations

1. **Always use mmap**: 3-300x faster, 99%+ memory savings
2. **Pre-allocate buffers**: 72-100x faster than growing allocation
3. **Cache manifests**: Avoid re-parsing (10-100 μs saved)
4. **Monitor RSS**: Peak usage is predictable and low

## Running Benchmarks

```bash
# Full benchmark suite
cargo bench -p adapteros-aos

# Memory profiler
cargo run --release --example memory_profile --features mmap -p adapteros-aos

# Comprehensive report
./scripts/measure_aos_performance.sh
```

## Detailed Reports

- Full analysis: [/target/performance_reports/aos_performance_2025-11-19.md](/target/performance_reports/aos_performance_2025-11-19.md)
- Interactive charts: `target/criterion/report/index.html`
- Usage guide: [PERFORMANCE.md](PERFORMANCE.md)

---

**Maintainer**: James KC Auchterlonie
