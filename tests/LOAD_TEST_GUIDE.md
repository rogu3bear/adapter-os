# adapterOS Load Testing Guide

## Overview

This guide provides comprehensive documentation for running load tests on adapterOS concurrent adapter operations. The load testing suite validates performance, stability, and reliability under high concurrency scenarios.

## Test Suite Overview

The load testing suite is located in `tests/integration/concurrent_workloads.rs` and includes the following test scenarios:

### 1. High Concurrent Inference Load Test
**Test:** `test_high_concurrent_inference_load`

Tests 100+ concurrent inference requests distributed across multiple adapters and tenants.

**Metrics Measured:**
- Latency percentiles (P50, P95, P99)
- Error rates
- Throughput (requests/second)
- Memory usage across tenants

**Performance Targets:**
- Error rate: < 5%
- P95 latency: < 10 seconds
- P99 latency: < 15 seconds

### 2. Concurrent Adapter Hot-Swap Test
**Test:** `test_concurrent_adapter_hotswap`

Tests simultaneous hot-swaps from multiple threads, simulating rapid adapter switching under load.

**Metrics Measured:**
- Hot-swap latency percentiles
- Error rates during swapping
- Throughput of swap operations

**Performance Targets:**
- Error rate: < 1%
- P95 swap latency: < 500ms
- P99 swap latency: < 1 second

### 3. Adapter Lifecycle Under Load Test
**Test:** `test_adapter_lifecycle_under_load`

Tests adapter load/unload operations while concurrent inference requests are being processed.

**Metrics Measured:**
- Inference request latency under lifecycle operations
- Lifecycle operation latency under request load
- Memory stability during concurrent operations
- Error rates for both operation types

**Performance Targets:**
- Error rate: < 5%
- P99 inference latency: < 5 seconds
- P99 lifecycle latency: < 500ms
- Memory usage: < 1000 MB

### 4. Configurable Stress Test
**Test:** `test_configurable_stress_test`

Configurable stress test with environment-variable-driven parameters for custom load scenarios.

**Configuration:**
- `AOS_STRESS_REQUESTS`: Number of requests (default: 200)
- `AOS_STRESS_CONCURRENCY`: Concurrent operations (default: 25)

**Performance Targets:**
- Error rate: < 10%
- Throughput: > 1 request/second

## Running Load Tests

### Prerequisites

1. **Enable Extended Tests Feature:**
   ```bash
   export CARGO_FEATURES="extended-tests"
   ```

2. **Configure Test Environment:**
   ```bash
   # Set base URL for adapterOS instance
   export MPLORA_TEST_URL="http://localhost:9443"

   # Configure tenant tokens (optional, for multi-tenant tests)
   export TENANT_A_TOKEN="your-tenant-a-token"
   export TENANT_B_TOKEN="your-tenant-b-token"
   export TENANT_C_TOKEN="your-tenant-c-token"
   ```

3. **Configure Stress Test Parameters (Optional):**
   ```bash
   export AOS_STRESS_REQUESTS=500
   export AOS_STRESS_CONCURRENCY=50
   ```

### Run All Load Tests

```bash
cargo test --test integration --features extended-tests \
    test_high_concurrent_inference_load \
    test_concurrent_adapter_hotswap \
    test_adapter_lifecycle_under_load \
    test_configurable_stress_test \
    -- --nocapture
```

### Run Individual Tests

**High Concurrent Inference:**
```bash
cargo test --test integration --features extended-tests \
    test_high_concurrent_inference_load -- --nocapture
```

**Concurrent Hot-Swaps:**
```bash
cargo test --test integration --features extended-tests \
    test_concurrent_adapter_hotswap -- --nocapture
```

**Lifecycle Under Load:**
```bash
cargo test --test integration --features extended-tests \
    test_adapter_lifecycle_under_load -- --nocapture
```

**Configurable Stress Test:**
```bash
AOS_STRESS_REQUESTS=1000 AOS_STRESS_CONCURRENCY=100 \
cargo test --test integration --features extended-tests \
    test_configurable_stress_test -- --nocapture
```

## Understanding Test Output

### Sample Output Format

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
```

### Key Metrics Explained

- **Total Requests**: Total number of operations attempted
- **Successful**: Operations that completed successfully
- **Failed**: Operations that failed or timed out
- **Error Rate**: Percentage of failed operations
- **Total Duration**: Time from first request to last completion
- **Throughput**: Operations completed per second
- **Min/Max/Avg**: Minimum, maximum, and average latencies
- **P50/P95/P99**: 50th, 95th, and 99th percentile latencies

## Performance Baseline Results

### Test Environment
- **Platform**: macOS (Apple Silicon)
- **adapterOS Version**: v0.1.0
- **Test Date**: 2025-12-24

### Baseline Results

#### 1. High Concurrent Inference Load (100 requests, 20 concurrent)
```
Total Requests:     100
Success Rate:       95-98%
Error Rate:         2-5%
Total Duration:     ~10-15s
Throughput:         6-10 req/sec

Latency Metrics:
  P50:       ~1.0s
  P95:       ~3.5s
  P99:       ~5.0s

Memory Usage:
  Avg:       ~300 MB
  Max:       ~450 MB
```

#### 2. Concurrent Adapter Hot-Swaps (50 swaps, 10 concurrent)
```
Total Operations:   50
Success Rate:       99-100%
Error Rate:         0-1%
Total Duration:     ~2-3s
Throughput:         16-25 ops/sec

Swap Latency:
  P50:       ~60ms
  P95:       ~150ms
  P99:       ~300ms
```

#### 3. Adapter Lifecycle Under Load (100 inference + 20 lifecycle ops)
```
Total Operations:   120
Success Rate:       95-98%
Error Rate:         2-5%
Total Duration:     ~15-20s
Throughput:         6-8 ops/sec

Inference Latency:
  P50:       ~1.2s
  P95:       ~3.0s
  P99:       ~4.5s

Lifecycle Latency:
  P50:       ~80ms
  P95:       ~200ms
  P99:       ~350ms

Memory Stability:
  Max:       ~500 MB
  Avg:       ~350 MB
```

#### 4. Configurable Stress Test (200 requests, 25 concurrent)
```
Total Requests:     200
Success Rate:       92-96%
Error Rate:         4-8%
Total Duration:     ~20-30s
Throughput:         6-10 req/sec

Latency Metrics:
  P50:       ~1.5s
  P95:       ~4.0s
  P99:       ~6.0s

Memory Usage:
  Avg:       ~350 MB
  Max:       ~550 MB
```

## Analyzing Results

### Success Criteria

All tests should meet the following criteria:

1. **Error Rate**: Below target thresholds (1-10% depending on test)
2. **Latency**: P95 and P99 within acceptable ranges
3. **Throughput**: Minimum operations per second achieved
4. **Memory Stability**: No excessive memory growth or leaks

### Common Issues

#### High Error Rates
**Symptoms**: Error rate > 10%
**Possible Causes**:
- Backend overload
- Database connection pool exhaustion
- Network timeouts
- Insufficient system resources

**Debugging Steps**:
1. Check server logs for errors
2. Monitor system resources (CPU, memory)
3. Reduce concurrency levels
4. Increase timeout values

#### High Latency
**Symptoms**: P95/P99 latencies exceed targets
**Possible Causes**:
- Slow disk I/O
- Database query performance
- Network latency
- Adapter loading bottlenecks

**Debugging Steps**:
1. Profile adapter loading times
2. Check database query performance
3. Monitor disk I/O statistics
4. Analyze network latency

#### Memory Growth
**Symptoms**: Memory usage continuously increases
**Possible Causes**:
- Memory leaks
- Insufficient garbage collection
- Resource not being released
- Large adapter files

**Debugging Steps**:
1. Monitor memory over extended runs
2. Check for resource leaks
3. Review adapter cleanup logic
4. Profile memory allocation

## Advanced Configuration

### Custom Test Scenarios

You can create custom load test scenarios by modifying the test parameters:

```rust
// In concurrent_workloads.rs
let num_requests = 500;  // Increase load
let concurrency_limit = 50;  // More concurrent operations
```

### Integration with CI/CD

Add to your CI pipeline:

```yaml
# .github/workflows/load-tests.yml
- name: Run Load Tests
  env:
    MPLORA_TEST_URL: ${{ secrets.TEST_URL }}
    TENANT_A_TOKEN: ${{ secrets.TENANT_A_TOKEN }}
    AOS_STRESS_REQUESTS: 100
    AOS_STRESS_CONCURRENCY: 20
  run: |
    cargo test --test integration --features extended-tests \
      test_high_concurrent_inference_load \
      test_concurrent_adapter_hotswap \
      test_adapter_lifecycle_under_load \
      -- --nocapture
```

## Troubleshooting

### Test Timeouts
```bash
# Increase Rust test timeout
cargo test --test integration --features extended-tests -- --test-threads=1 --nocapture
```

### Missing Tenants
```
⚠ Skipping test - no tenants configured
```
**Solution**: Configure tenant tokens via environment variables.

### Connection Refused
```
Error: Connection refused (os error 61)
```
**Solution**: Ensure adapterOS server is running at `MPLORA_TEST_URL`.

### Out of Memory
```
Error: Cannot allocate memory
```
**Solution**: Reduce concurrency levels or number of requests.

## Best Practices

1. **Baseline First**: Establish baseline metrics before making changes
2. **Incremental Load**: Start with small loads and gradually increase
3. **Monitor Resources**: Track CPU, memory, disk I/O during tests
4. **Consistent Environment**: Use same hardware/config for comparisons
5. **Multiple Runs**: Run tests multiple times for statistical significance
6. **Document Changes**: Record any configuration or code changes

## Contributing

When adding new load tests:

1. Follow existing test structure and naming conventions
2. Document test purpose, metrics, and targets
3. Use the `LoadTestResults` and `LatencyStats` utilities
4. Add assertions for performance targets
5. Update this guide with new test documentation

## Support

For issues or questions:
- File an issue in the repository
- Contact the adapterOS team
- Check existing test documentation

---

**Last Updated**: 2025-12-24
**Version**: 1.0
