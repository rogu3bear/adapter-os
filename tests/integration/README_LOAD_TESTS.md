# Load Testing Suite for Concurrent Adapter Operations

## Overview

This directory contains comprehensive load tests for adapterOS concurrent adapter operations. The tests validate performance, stability, and reliability under high concurrency scenarios including:

- 100+ concurrent inference requests with different adapters
- Simultaneous hot-swaps from multiple threads
- Adapter load/unload operations under active request load
- Configurable stress testing with custom parameters

## Test Files

### `concurrent_workloads.rs`
Main test suite containing:
- `test_high_concurrent_inference_load` - Tests 100+ concurrent inference requests
- `test_concurrent_adapter_hotswap` - Tests simultaneous hot-swap operations
- `test_adapter_lifecycle_under_load` - Tests load/unload under request load
- `test_configurable_stress_test` - Configurable stress test with environment variables

### Supporting Files
- `test_utils.rs` - Reusable test utilities and helpers
- `fixtures.rs` - Test data and configurations

## Quick Start

### 1. Run All Load Tests
```bash
cd <repo-root>
./scripts/run_load_tests.sh --all
```

### 2. Run Specific Test
```bash
# High concurrent inference
./scripts/run_load_tests.sh concurrent_inference

# Hot-swap operations
./scripts/run_load_tests.sh hotswap

# Lifecycle under load
./scripts/run_load_tests.sh lifecycle

# Stress test
./scripts/run_load_tests.sh stress
```

### 3. Use Predefined Profiles
```bash
# Light load (50 requests, 10 concurrent)
./scripts/run_load_tests.sh --profile light --all

# Medium load (200 requests, 25 concurrent)
./scripts/run_load_tests.sh --profile medium --all

# Heavy load (500 requests, 50 concurrent)
./scripts/run_load_tests.sh --profile heavy --all
```

## Metrics Collected

### Latency Percentiles
- **P50 (Median)**: 50th percentile latency
- **P95**: 95th percentile latency - 95% of requests complete within this time
- **P99**: 99th percentile latency - 99% of requests complete within this time
- **Min/Max/Avg**: Minimum, maximum, and average latencies

### Error Rates
- **Success Rate**: Percentage of requests that completed successfully
- **Error Rate**: Percentage of requests that failed
- **Error Breakdown**: Types of errors encountered

### Performance
- **Throughput**: Operations per second
- **Total Duration**: Time from first request to last completion
- **Concurrency**: Number of simultaneous operations

### Resource Usage
- **Memory Usage**: Min, max, and average memory consumption
- **Memory Stability**: Tracking for memory leaks or excessive growth

## Performance Targets

| Test | Error Rate | P95 Latency | P99 Latency | Notes |
|------|-----------|-------------|-------------|-------|
| Concurrent Inference | < 5% | < 10s | < 15s | 100 requests, 20 concurrent |
| Hot-Swaps | < 1% | < 500ms | < 1s | 50 swaps, 10 concurrent |
| Lifecycle Under Load | < 5% | < 5s | < 5s | Mixed operations |
| Stress Test | < 10% | - | - | Configurable load |

## Configuration

### Environment Variables

```bash
# Server URL
export MPLORA_TEST_URL="http://localhost:18080"

# Tenant tokens (optional)
export TENANT_A_TOKEN="your-token-a"
export TENANT_B_TOKEN="your-token-b"
export TENANT_C_TOKEN="your-token-c"

# Stress test parameters
export AOS_STRESS_REQUESTS=500
export AOS_STRESS_CONCURRENCY=50
```

### Test Configuration in Code

```rust
// In concurrent_workloads.rs
let num_requests = 100;          // Total requests
let concurrency_limit = 20;      // Max concurrent operations
let semaphore = Arc::new(Semaphore::new(concurrency_limit));
```

## Test Architecture

### Latency Tracking
```rust
struct LatencyStats {
    latencies: Vec<Duration>,
}

impl LatencyStats {
    fn calculate(&mut self) -> LatencyMetrics {
        // Calculates P50, P95, P99 percentiles
    }
}
```

### Results Reporting
```rust
struct LoadTestResults {
    total_requests: usize,
    successful_requests: usize,
    failed_requests: usize,
    error_rate: f64,
    latency_metrics: LatencyMetrics,
    throughput: f64,
    memory_samples: Vec<f64>,
}
```

### Resource Monitoring
```rust
struct ResourceMonitor {
    metrics: Arc<Mutex<HashMap<String, Vec<ResourceMetrics>>>>,
}

struct ResourceMetrics {
    memory_mb: f64,
    cpu_percent: f64,
    storage_mb: f64,
    timestamp: Instant,
}
```

## Example Test Execution

```rust
#[tokio::test]
async fn test_high_concurrent_inference_load() -> Result<()> {
    // Setup
    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let monitor = ResourceMonitor::new();

    // Configure
    let num_requests = 100;
    let concurrency_limit = 20;
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    // Execute concurrent requests
    for i in 0..num_requests {
        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            // Execute request and measure latency
        });
        handles.push(handle);
    }

    // Collect results and calculate metrics
    let latency_metrics = latency_stats.calculate();
    let error_rate = failed / num_requests;

    // Assertions
    assert!(error_rate < 0.05);
    assert!(latency_metrics.p95 < Duration::from_secs(10));

    Ok(())
}
```

## Sample Output

```
======================================================================
LOAD TEST RESULTS: 100+ Concurrent Inference Requests
======================================================================
Total Requests:     100
Successful:         98 (98.00%)
Failed:             2 (2.00%)
Error Rate:         2.00%
Total Duration:     12.345s
Throughput:         8.10 req/sec

Request Latency Metrics:
  Requests:  100
  Min:       123ms
  Max:       4.567s
  Avg:       1.234s
  P50:       1.100s
  P95:       2.345s
  P99:       3.456s

Memory Usage:
  Min:       200.00 MB
  Max:       450.00 MB
  Avg:       325.00 MB
======================================================================
✓ High concurrent inference load test PASSED
```

## Troubleshooting

### High Error Rates
**Problem**: Error rate > 10%

**Solutions**:
- Reduce concurrency: `--concurrency 10`
- Use lighter profile: `--profile light`
- Check server logs for errors
- Verify server has sufficient resources

### High Latency
**Problem**: P95/P99 latency exceeds targets

**Solutions**:
- Check disk I/O performance
- Monitor database query times
- Profile adapter loading operations
- Reduce concurrent load

### Memory Issues
**Problem**: Memory usage grows excessively

**Solutions**:
- Check for memory leaks in server
- Verify adapters are being unloaded properly
- Monitor over longer test runs
- Review resource cleanup logic

### Server Connection Issues
**Problem**: "Connection refused" errors

**Solutions**:
- Verify server is running: `cargo run --bin adapteros-server`
- Check server URL: `export MPLORA_TEST_URL=http://localhost:18080`
- Verify network connectivity
- Check firewall settings

## Documentation

- **Full Guide**: `/tests/LOAD_TEST_GUIDE.md` - Comprehensive documentation
- **Quick Start**: `/tests/LOAD_TEST_QUICKSTART.md` - Quick reference
- **Baseline Results**: `/tests/BASELINE_RESULTS.md` - Performance baselines

## CI/CD Integration

### GitHub Actions Example
```yaml
name: Load Tests

on:
  push:
    branches: [main]
  schedule:
    - cron: '0 0 * * 0'  # Weekly

jobs:
  load-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
      - name: Start Server
        run: cargo run --bin adapteros-server &
      - name: Run Load Tests
        run: ./scripts/run_load_tests.sh --profile medium --all
      - name: Upload Results
        uses: actions/upload-artifact@v2
        with:
          name: load-test-results
          path: var/load_test_results/
```

## Performance Monitoring

### Recommended Practices

1. **Establish Baseline**: Run tests regularly to establish performance baseline
2. **Track Trends**: Monitor metrics over time to detect regressions
3. **Set Alerts**: Configure alerts for performance degradation
4. **Document Changes**: Record any configuration or code changes
5. **Compare Results**: Compare before/after metrics for optimizations

### Metrics to Track

- **Request Latency**: P50, P95, P99 over time
- **Error Rate**: Trend analysis
- **Throughput**: Requests per second capacity
- **Memory Usage**: Growth patterns
- **Resource Utilization**: CPU, memory, disk I/O

## Contributing

When adding new load tests:

1. Follow existing test structure and patterns
2. Use `LatencyStats` and `LoadTestResults` for consistency
3. Document test purpose, metrics, and targets
4. Add performance assertions
5. Update documentation

## Support

For issues or questions:
- Check documentation in `/tests/` directory
- Review existing test code for examples
- File issues in the repository
- Contact the adapterOS team

---

**Last Updated**: 2025-12-24
**Maintainers**: adapterOS Team
