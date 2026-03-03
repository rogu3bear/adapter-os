# Load Testing Quick Start Guide

## Quick Run

### Run All Tests (Default Settings)
```bash
./scripts/run_load_tests.sh --all
```

### Run Specific Test
```bash
# Concurrent inference requests
./scripts/run_load_tests.sh concurrent_inference

# Hot-swap operations
./scripts/run_load_tests.sh hotswap

# Lifecycle operations under load
./scripts/run_load_tests.sh lifecycle

# Configurable stress test
./scripts/run_load_tests.sh stress
```

### Use Predefined Profiles

**Light Load** (50 requests, 10 concurrent)
```bash
./scripts/run_load_tests.sh --profile light --all
```

**Medium Load** (200 requests, 25 concurrent)
```bash
./scripts/run_load_tests.sh --profile medium --all
```

**Heavy Load** (500 requests, 50 concurrent)
```bash
./scripts/run_load_tests.sh --profile heavy --all
```

**Extreme Load** (1000 requests, 100 concurrent)
```bash
./scripts/run_load_tests.sh --profile extreme --all
```

### Custom Configuration

```bash
./scripts/run_load_tests.sh \
    --requests 1000 \
    --concurrency 50 \
    --url http://localhost:18080 \
    --all
```

## Prerequisites

1. **adapterOS Server Running**
   ```bash
   # Start the server first
   cargo run --bin adapteros-server
   ```

2. **Rust Toolchain**
   ```bash
   rustc --version  # Should be 1.70+
   ```

3. **Extended Tests Feature**
   - Automatically enabled by the script
   - Manual: `cargo test --features extended-tests`

## Environment Variables

```bash
# Optional: Configure test environment
export MPLORA_TEST_URL="http://localhost:18080"
export TENANT_A_TOKEN="your-token-here"
export TENANT_B_TOKEN="your-token-here"
export TENANT_C_TOKEN="your-token-here"

# Optional: Configure stress test parameters
export AOS_STRESS_REQUESTS=500
export AOS_STRESS_CONCURRENCY=50
```

## Understanding Results

### Success Output
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
  P50:       1.100s
  P95:       2.345s
  P99:       3.456s

✓ High concurrent inference load test PASSED
```

### Results Location
- **Logs**: `var/load_test_results/load_test_TIMESTAMP.log`
- **Summary**: `var/load_test_results/results_TIMESTAMP.txt`

## Troubleshooting

### Server Not Running
```
Warning: Cannot reach adapterOS server at http://localhost:18080
```
**Fix**: Start the server with `cargo run --bin adapteros-server`

### High Error Rates
```
Error Rate: 15.00%
```
**Fix**: Reduce concurrency with `--concurrency 10` or check server logs

### Tests Timeout
**Fix**: Use lighter profile `--profile light` or reduce requests

## Performance Targets

| Test | Error Rate | P95 Latency | P99 Latency |
|------|-----------|-------------|-------------|
| Concurrent Inference | < 5% | < 10s | < 15s |
| Hot-Swaps | < 1% | < 500ms | < 1s |
| Lifecycle Under Load | < 5% | < 5s | < 5s |
| Stress Test | < 10% | - | - |

## CI/CD Integration

Add to your pipeline:

```yaml
- name: Run Load Tests
  run: ./scripts/run_load_tests.sh --profile medium --all
```

## Next Steps

- Read full guide: `tests/LOAD_TEST_GUIDE.md`
- View test code: `tests/integration/concurrent_workloads.rs`
- Customize tests for your needs
- Establish baseline metrics for your environment

## Help

```bash
./scripts/run_load_tests.sh --help
```
