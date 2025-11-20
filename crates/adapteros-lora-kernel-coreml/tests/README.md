# CoreML Backend Test Suite

Comprehensive testing infrastructure for the CoreML backend with Apple Neural Engine (ANE) acceleration.

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

---

## Test Structure

### Unit Tests (`unit_tests.rs`)

Core functionality tests without hardware dependencies:

- **ANE Detection**: Validate ANE availability detection
- **Model Loading**: Path validation, extension checks
- **Tensor Operations**: Size alignment, batch processing
- **Memory Management**: Buffer initialization, fingerprinting
- **Quantization**: Q15 gate quantization, FP16/INT8 conversion
- **Error Handling**: Edge cases, NaN/Inf handling

**Run:**
```bash
cargo test --test unit_tests
```

---

### Integration Tests (`integration_tests.rs`)

End-to-end pipeline tests:

- **Inference Pipeline**: Multi-step execution, state consistency
- **Adapter Hot-Swap**: Runtime adapter replacement
- **Router Integration**: K-sparse routing (K=1,2,4,8)
- **Memory Pressure**: Eviction under memory constraints
- **Thermal Throttling**: Behavior under thermal load
- **Error Recovery**: Fault tolerance and recovery
- **Concurrency**: Thread-safe execution

**Run:**
```bash
cargo test --test integration_tests
```

---

### Performance Benchmarks

#### `benches/coreml_inference.rs`

Throughput and latency benchmarks:

- **Single Token Inference**: Latency per token (1-128 input length)
- **Tokens/Second**: Throughput (128-8192 sequence lengths)
- **Latency Distribution**: Min/avg/max/p50/p95/p99 percentiles
- **Memory Usage**: Small/medium/large adapter configs
- **K-Sparse Routing**: Overhead for K=1,2,4,8
- **Adapter Hot-Swap**: Load time (128KB-8MB)
- **Batch Processing**: Batch=1,2,4,8 (ANE optimized for batch=1)
- **Q15 Quantization**: Quantization overhead
- **Long Sequences**: 512-8192 token sequences

**Run:**
```bash
cargo bench --bench coreml_inference
```

**Output:** `target/criterion/report/index.html`

---

#### `benches/ane_comparison.rs`

ANE vs GPU comparison benchmarks:

- **Throughput Comparison**: ANE vs GPU tokens/sec
- **Power Consumption**: Estimated power draw (requires `--features power-metrics`)
- **Memory Bandwidth**: Transfer rates
- **Thermal Impact**: Sustained workload heat generation
- **Cold Start Latency**: Model initialization time
- **Batch Size Impact**: ANE penalty for batch > 1
- **Precision Modes**: FP16 vs FP32 performance

**Run:**
```bash
cargo bench --bench ane_comparison
cargo bench --bench ane_comparison --features power-metrics
```

---

### Accuracy Tests (`accuracy_tests.rs`)

Numerical correctness and stability:

- **MLX Backend Comparison**: Cross-backend output validation
- **Metal Backend Comparison**: CoreML vs Metal accuracy
- **Quantization Error**: FP16/INT8 error analysis
- **Numerical Stability**: Small/large value handling
- **Edge Cases**: Zero input, negative values, NaN/Inf
- **Softmax Stability**: Numerically stable softmax
- **Cross-Entropy**: Stable loss computation
- **Gradient Analysis**: Vanishing/exploding gradient detection
- **Relative Error**: Scale-independent error metrics
- **ULP Distance**: Floating-point precision validation

**Run:**
```bash
cargo test --test accuracy_tests
```

**Accuracy Thresholds:**
- MAE (Mean Absolute Error): < 1e-4
- MSE (Mean Squared Error): < 1e-8
- Cosine Similarity: > 0.999
- FP16 Quantization Error: < 1e-3
- INT8 Quantization Error: < 1/255

---

### Device-Specific Tests (`device_specific_tests.rs`)

Apple Silicon device validation:

- **Device Detection**: Identify M1/M2/M3/M4/Intel
- **M1 Capabilities**: 16 ANE cores, 15.8 TOPS
- **M2 Capabilities**: 16 ANE cores, 17.0 TOPS
- **M3 Capabilities**: 16 ANE cores, 17.0 TOPS, INT4 support
- **M4 Capabilities**: 16 ANE cores, 17.0+ TOPS, improved efficiency
- **Intel Fallback**: GPU/CPU fallback (no ANE)
- **Performance Expectations**: Device-specific throughput baselines
- **Power Estimates**: ANE vs GPU power consumption
- **Thermal Headroom**: Sustained performance scores
- **Memory Bandwidth**: Unified memory bandwidth (68-120 GB/s)
- **Quantization Support**: FP16/INT8/INT4 by generation

**Run:**
```bash
cargo test --test device_specific_tests
```

**Device Detection:**
```bash
sysctl -n machdep.cpu.brand_string
```

---

### Mock Infrastructure (`common/mod.rs`)

Reusable test utilities:

- **MockCoreMLBackend**: Simulates CoreML backend without hardware
- **Test Data Generators**: Deterministic input/gate/logit generation
- **Assertion Helpers**: Approximate equality, range checks, probability validation
- **CI/CD Utilities**: Detect CI environment, skip tests without hardware

**Usage:**
```rust
use common::{MockCoreMLBackend, generators, assertions, ci};

// Create mock backend
let backend = MockCoreMLBackend::new(ane_available: true);

// Generate test data
let input_ids = generators::generate_input_ids(128, seed: 42);
let gates = generators::generate_adapter_gates(k: 4, seed: 42);

// Assertions
assertions::assert_approx_eq(&output1, &output2, tolerance: 1e-5, "outputs match");
assertions::assert_all_finite(&logits, "logits are finite");

// CI detection
if ci::is_ci() && !ci::has_real_hardware() {
    ci::skip_if_no_hardware("test_ane_inference");
}
```

---

## Running Tests

### All Tests
```bash
cargo test --workspace
```

### Unit Tests Only
```bash
cargo test --test unit_tests
```

### Integration Tests Only
```bash
cargo test --test integration_tests
```

### Accuracy Tests Only
```bash
cargo test --test accuracy_tests
```

### Device-Specific Tests Only
```bash
cargo test --test device_specific_tests
```

### All Benchmarks
```bash
cargo bench --workspace
```

### Specific Benchmark
```bash
cargo bench --bench coreml_inference
cargo bench --bench ane_comparison
```

### With Power Metrics
```bash
cargo bench --bench ane_comparison --features power-metrics
```

### Verbose Output
```bash
cargo test -- --nocapture
```

### Single Test
```bash
cargo test test_ane_detection -- --nocapture
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: CoreML Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest # Required for CoreML
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run unit tests
        run: cargo test --test unit_tests

      - name: Run integration tests
        run: cargo test --test integration_tests

      - name: Run accuracy tests
        run: cargo test --test accuracy_tests

      - name: Run device-specific tests (M1+)
        run: cargo test --test device_specific_tests
        continue-on-error: true # May not have ANE in CI

  benchmark:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run benchmarks
        run: cargo bench --workspace

      - name: Upload benchmark results
        uses: actions/upload-artifact@v3
        with:
          name: criterion-reports
          path: target/criterion/
```

---

## Test Coverage

### Coverage by Category

| Category | Test Count | Coverage |
|----------|------------|----------|
| Unit Tests | 30+ | Core functionality |
| Integration Tests | 15+ | End-to-end pipelines |
| Accuracy Tests | 20+ | Numerical correctness |
| Device Tests | 15+ | Hardware-specific |
| Benchmarks | 15+ | Performance metrics |

### Critical Paths Covered

- ✅ ANE detection and fallback
- ✅ Model loading (.mlpackage)
- ✅ Single token inference
- ✅ Multi-token sequences
- ✅ Adapter hot-swap
- ✅ K-sparse routing (K=1-8)
- ✅ Memory management
- ✅ Thermal throttling
- ✅ Error recovery
- ✅ Quantization (FP16/INT8)
- ✅ Cross-backend accuracy
- ✅ Device-specific optimizations

---

## Performance Baselines

### M1 (ANE)
- Throughput: ~60 tokens/sec (7B model)
- Latency: ~16ms per token
- Power: ~8-10W
- Memory: 256MB per adapter (rank=16)

### M2 (ANE)
- Throughput: ~70 tokens/sec
- Latency: ~14ms per token
- Power: ~8-10W
- Memory: Same as M1

### M3/M4 (ANE)
- Throughput: ~75-80 tokens/sec
- Latency: ~12-13ms per token
- Power: ~9-10W
- Memory: Same as M1/M2

### GPU Fallback (Intel/No ANE)
- Throughput: ~30 tokens/sec
- Latency: ~33ms per token
- Power: ~15-20W

---

## Troubleshooting

### Test Failures

**Issue:** `ANE not available` errors
```bash
# Check device
sysctl -n machdep.cpu.brand_string

# Should show "Apple M1/M2/M3/M4" for ANE support
```

**Issue:** Model loading fails
```bash
# Verify .mlpackage path
ls -la models/

# Should have .mlpackage bundle
```

**Issue:** Benchmarks timeout
```bash
# Increase sample size or measurement time
cargo bench -- --sample-size 10
```

**Issue:** Accuracy tests fail
```bash
# Check floating-point precision
cargo test test_quantization_error_fp16 -- --nocapture

# May need to adjust tolerance for device
```

---

## References

- [CoreML Documentation](https://developer.apple.com/documentation/coreml)
- [ANE Performance Guide](https://developer.apple.com/documentation/coreml/optimizing_model_accuracy)
- [docs/COREML_INTEGRATION.md](../../../docs/COREML_INTEGRATION.md)
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](../../../docs/ADR_MULTI_BACKEND_STRATEGY.md)
- [crates/adapteros-lora-kernel-api/src/lib.rs](../../adapteros-lora-kernel-api/src/lib.rs)

---

**Signed:** James KC Auchterlonie
**Date:** 2025-01-19
