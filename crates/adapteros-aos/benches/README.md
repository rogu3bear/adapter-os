# AOS 2.0 Benchmarks

Criterion-based performance benchmarks for the AOS 2.0 archive format.

## Benchmark Files

### `aos_benchmarks.rs`

Main benchmark suite with six test categories:

1. **Header Parsing** - Measures 8-byte header read/parse performance
2. **Manifest Loading** - JSON deserialization across different archive sizes (10-500 tensors)
3. **mmap vs Read** - Compares memory-mapped I/O vs traditional file reading (1-100 MB)
4. **Full Archive Load** - End-to-end loading time (header + manifest + weights)
5. **JSON Parsing** - Isolates manifest parsing overhead (10-1000 tensors)
6. **Memory Allocation** - Pre-allocated vs growing Vec strategies (1-100 MB)

### `load_benchmark.rs`

Legacy benchmark for write/read cycles. Use `aos_benchmarks.rs` instead.

## Running Benchmarks

### Quick Run

```bash
# Run all benchmarks
cargo bench -p adapteros-aos

# Run specific benchmark group
cargo bench -p adapteros-aos -- header_parsing
cargo bench -p adapteros-aos -- manifest_loading
cargo bench -p adapteros-aos -- mmap_vs_read
cargo bench -p adapteros-aos -- full_archive_load
cargo bench -p adapteros-aos -- json_parsing
cargo bench -p adapteros-aos -- memory_allocation
```

### With Filters

```bash
# Test only 10MB files
cargo bench -p adapteros-aos -- 10MB

# Test only mmap operations
cargo bench -p adapteros-aos -- mmap
```

## Output

### Terminal Output

Real-time progress and summary statistics:

```
header_parsing/parse_header
                        time:   [7.2259 µs 7.2529 µs 7.2893 µs]
```

### HTML Reports

Interactive charts and detailed analysis:

```
open target/criterion/report/index.html
```

### JSON Data

Raw benchmark data for programmatic analysis:

```
target/criterion/*/base/estimates.json
```

## Benchmark Configuration

### Criterion Settings

- **Sample size**: 100 iterations (default)
- **Warm-up time**: 3 seconds
- **Measurement time**: 5 seconds
- **Statistical analysis**: Mean, median, std dev, outlier detection

### Test Data

Benchmarks create synthetic .aos archives with:
- Configurable tensor counts (10, 50, 100, 500, 1000)
- Configurable weights sizes (1MB, 10MB, 50MB, 100MB)
- Realistic manifest structure (tensor shapes, metadata)

## Memory Profiling

For memory usage analysis, use the dedicated profiler:

```bash
cargo run --release --example memory_profile --features mmap -p adapteros-aos
```

Output includes:
- RSS (Resident Set Size) snapshots
- Memory delta from baseline
- Peak memory usage
- Comparison of mmap vs regular read

## Continuous Performance Tracking

### Baseline Comparison

```bash
# Save current performance as baseline
cargo bench -p adapteros-aos -- --save-baseline main

# Compare against baseline
cargo bench -p adapteros-aos -- --baseline main
```

### CI Integration

Add to CI pipeline to detect performance regressions:

```bash
cargo bench -p adapteros-aos -- --save-baseline pr-123
```

## Interpreting Results

### Time Metrics

- **μs (microseconds)**: 1/1,000,000 second
- **ms (milliseconds)**: 1/1,000 second
- Lower is better

### Throughput Metrics

- **MiB/s**: Megabytes per second (2^20 bytes)
- **GiB/s**: Gigabytes per second (2^30 bytes)
- **TiB/s**: Terabytes per second (2^40 bytes)
- Higher is better

### Memory Metrics

- **RSS**: Resident Set Size (actual RAM usage)
- **Delta**: Change from baseline
- Lower is better

## Benchmark Results

Latest results: [BENCHMARK_SUMMARY.md](../BENCHMARK_SUMMARY.md)

Detailed analysis: [/target/performance_reports/aos_performance_2025-11-19.md](/target/performance_reports/aos_performance_2025-11-19.md)

---

**Maintainer**: James KC Auchterlonie
