# adapterOS Performance Benchmark Suite

A comprehensive performance benchmarking suite for adapterOS components, focusing on the unique performance characteristics of a deterministic inference runtime with Metal kernels, multi-tenant isolation, and evidence-grounded responses.

## Overview

This benchmark suite provides detailed performance measurements across six key categories:

- **Kernel Performance**: Metal kernel operations, matrix multiplications, attention mechanisms
- **Memory Management**: Allocation patterns, memory pressure, garbage collection
- **Throughput**: Inference throughput, request processing, concurrent operations
- **System Metrics**: Determinism overhead, isolation boundaries, evidence processing
- **Multi-tenant Isolation**: Resource isolation, security boundaries, performance isolation
- **Evidence Processing**: Response grounding, evidence validation, latency characteristics

## Quick Start

### Running All Benchmarks

```bash
# From the project root
./scripts/run_benchmarks.sh
```

### Running Specific Benchmark Categories

```bash
# Run only kernel benchmarks
./scripts/run_benchmarks.sh --kernel-only

# Run only memory benchmarks
./scripts/run_benchmarks.sh --memory-only

# Run only throughput benchmarks
./scripts/run_benchmarks.sh --throughput-only
```

### Using the Benchmark CLI

```bash
# Build the benchmark binary
cargo build --manifest-path=tests/benchmark/Cargo.toml --release --bin adapteros-benchmarks

# Run all benchmarks
cargo run --manifest-path=tests/benchmark/Cargo.toml --release --bin adapteros-benchmarks -- run

# Compare with baseline
cargo run --manifest-path=tests/benchmark/Cargo.toml --release --bin adapteros-benchmarks -- \
  compare current_results.json baseline_results.json

# List available benchmarks
cargo run --manifest-path=tests/benchmark/Cargo.toml --release --bin adapteros-benchmarks -- list
```

## Benchmark Categories

### Kernel Performance Benchmarks

- **Metal Kernel Inference**: End-to-end inference step performance
- **Matrix Operations**: 1024×1024 matrix multiplication performance
- **Attention Mechanisms**: Multi-head attention with 512 sequence length
- **LoRA Adapter Fusion**: Performance of fusing multiple LoRA adapters

### Memory Benchmarks

- **Allocation Patterns**: Memory allocation from 64B to 16MB
- **Access Patterns**: Sequential, random, and strided memory access
- **Memory Pressure**: Fragmentation and garbage collection simulation
- **Concurrent Operations**: Multi-threaded memory operations
- **Memory-mapped Files**: File I/O performance comparison

### Throughput Benchmarks

- **Inference Throughput**: Batched inference performance (1-32 batch sizes)
- **Concurrent Requests**: Multi-worker request processing (1-32 workers)
- **Request Queues**: Asynchronous queue processing performance
- **Adapter Routing**: LoRA adapter selection and routing
- **End-to-End Latency**: Complete request processing pipeline

### System Metrics Benchmarks

- **System Monitoring**: CPU, memory, disk, and network metrics collection
- **Telemetry Processing**: Event collection and aggregation
- **Policy Evaluation**: Security and performance policy checking
- **Deterministic Execution**: Overhead of deterministic guarantees
- **Alerting Engine**: Condition evaluation and alert generation

### Isolation Benchmarks

- **Tenant Context Switching**: Multi-tenant environment switching
- **Resource Quotas**: Per-tenant resource limit enforcement
- **Data Isolation**: Ensuring tenant data separation
- **Security Boundaries**: Access control performance
- **Tenant Cleanup**: Resource reclamation performance

### Evidence Benchmarks

- **Evidence Collection**: Gathering and scoring evidence items
- **Response Grounding**: Evidence-based response generation
- **Evidence Caching**: Retrieval and storage performance
- **Decision Making**: Evidence-weighted decision processes
- **Processing Latency**: Evidence pipeline performance impact

## Configuration

### Environment Variables

- `RESULTS_DIR`: Output directory for benchmark results (default: `benchmark_results`)
- `BASELINE_FILE`: Path to baseline results for comparison
- `FAIL_ON_REGRESSION`: Whether to fail CI on regressions (default: `true`)
- `GENERATE_HTML`: Generate HTML reports (default: `true`)

### Benchmark Configuration

The benchmark runner can be configured via `RunnerConfig`:

```rust
use adapteros_benchmarks::runner::RunnerConfig;

let config = RunnerConfig {
    output_dir: "my_results".to_string(),
    baseline_file: Some("baseline.json".to_string()),
    comparison_threshold: 0.05, // 5% regression threshold
    fail_on_regression: true,
    generate_html_report: true,
    // ... category enables
};
```

## Results and Reporting

### Output Files

- `benchmark_report.json`: Detailed JSON results
- `benchmark_report.html`: Human-readable HTML report
- `benchmark_comparison.html`: Comparison with baseline (if provided)

### Performance Regression Detection

The suite automatically detects performance regressions by comparing against baseline results:

- **Regressions**: >5% performance degradation (configurable)
- **Improvements**: >5% performance improvement (configurable)
- **Unchanged**: Performance within threshold

### CI/CD Integration

The benchmark suite integrates with GitHub Actions for automated performance monitoring:

- **Pull Request Checks**: Compare PR performance against main branch
- **Nightly Runs**: Regular performance regression detection
- **On-Demand Runs**: Manual benchmark execution

## Interpreting Results

### Key Metrics

- **Mean Time**: Average execution time per operation
- **Standard Deviation**: Variability in execution time
- **Throughput**: Operations per second
- **Memory Usage**: Peak memory consumption
- **Regression Ratio**: Performance change vs baseline

### Common Patterns

- **Metal Kernel Benchmarks**: Focus on GPU utilization and memory bandwidth
- **Memory Benchmarks**: Identify allocation hotspots and fragmentation issues
- **Throughput Benchmarks**: Measure system capacity and scaling characteristics
- **System Benchmarks**: Monitor resource usage and system-level overhead
- **Isolation Benchmarks**: Ensure tenant performance isolation
- **Evidence Benchmarks**: Balance grounding quality with latency requirements

## Development

### Adding New Benchmarks

1. Create a new benchmark file in `benches/`
2. Use the Criterion benchmarking framework
3. Follow the naming convention: `{category}_benchmarks.rs`
4. Add the benchmark to `Cargo.toml` under `[[bench]]`
5. Update the runner configuration if needed

### Benchmark Best Practices

- **Warm-up**: Allow sufficient warm-up iterations
- **Sample Size**: Use appropriate sample sizes for statistical significance
- **Isolation**: Ensure benchmarks don't interfere with each other
- **Determinism**: Use deterministic inputs for reproducible results
- **Resource Cleanup**: Properly clean up resources between benchmarks

## Troubleshooting

### Common Issues

- **Metal Not Available**: Benchmarks require macOS with Metal support
- **Memory Issues**: Reduce benchmark sizes if running out of memory
- **Long Runtime**: Use `--kernel-only` or similar flags for faster iteration
- **Inconsistent Results**: Ensure system is idle during benchmarking

### Performance Tips

- Run benchmarks on dedicated hardware
- Disable CPU frequency scaling
- Ensure adequate cooling
- Use SSD storage for result files
- Consider memory-mapped files for large datasets

## Contributing

When adding new benchmarks:

1. Follow the existing code structure and naming conventions
2. Include comprehensive documentation
3. Add appropriate error handling
4. Test on multiple configurations
5. Update this README with new benchmark descriptions

## License

This benchmark suite is part of adapterOS and follows the same license terms (MIT OR Apache-2.0).