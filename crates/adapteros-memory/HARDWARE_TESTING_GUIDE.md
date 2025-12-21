# Metal Heap Observer Hardware Testing Guide

This guide documents how to run hardware-specific tests for Metal heap observation, including CI-safe tests and local hardware tests.

## Overview

The memory crate includes three test suites for Metal heap observation:

1. **metal_heap_tests.rs** - Comprehensive unit and integration tests
2. **metal_heap_benchmarks.rs** - Performance and overhead measurement
3. **heap_pressure_integration.rs** - Integration with memory pressure management

### Test Safety Levels

| Test Suite | CI-Safe | Hardware Required | Purpose |
|-----------|---------|------------------|---------|
| metal_heap_tests.rs (unmocked) | Yes | No | Mock-based testing |
| metal_heap_tests.rs (#[ignore]) | No | Yes (macOS) | Hardware validation |
| metal_heap_benchmarks.rs | Yes | No | Performance measurement |
| heap_pressure_integration.rs | Yes | No | Integration validation |

## CI-Safe Tests (Always Run)

These tests use mock implementations and work on all platforms, including CI environments.

### Running CI-Safe Tests

```bash
# Run all CI-safe memory tests
cargo test --test metal_heap_tests -- --nocapture

# Run specific test
cargo test --test metal_heap_tests test_mock_observer_creation -- --nocapture

# Run with all output
cargo test --test metal_heap_tests -- --nocapture --test-threads=1
```

### CI-Safe Test Coverage

#### Mock Heap Observer Tests (9 tests)
- `test_mock_observer_creation` - Creation and initialization
- `test_mock_observer_single_allocation` - Single allocation tracking
- `test_mock_observer_multiple_allocations` - Multiple allocations
- `test_mock_observer_deallocation` - Deallocation handling
- `test_mock_observer_fragmentation_tracking` - Fragmentation detection
- `test_mock_observer_multi_heap` - Multi-heap support
- `test_mock_observer_memory_stats` - Statistics calculation

#### FFI Binding Tests (5 tests)
- `test_ffi_fragmentation_metrics_structure_size` - Structure sizing
- `test_ffi_heap_state_structure_size` - Heap state sizing
- `test_ffi_metal_memory_metrics_structure_size` - Metrics sizing
- `test_ffi_page_migration_event_structure_size` - Event sizing
- `test_ffi_fragmentation_metrics_initialization` - Initialization
- `test_ffi_heap_state_initialization` - Heap state init
- `test_ffi_page_migration_event_initialization` - Event init
- `test_ffi_metal_memory_metrics_utilization` - Utilization calc

#### Statistics Collection Tests (5 tests)
- `test_memory_stats_calculation_empty` - Empty statistics
- `test_memory_stats_calculation_single_heap` - Single heap stats
- `test_memory_stats_calculation_multi_heap` - Multi-heap stats
- `test_fragmentation_metrics_classification` - Fragmentation types
- `test_utilization_percentage_calculation` - Utilization calc

#### Hardware Detection Tests (2 tests)
- `test_hardware_detection_non_mock` - Device availability detection
- `test_device_optional_creation` - Optional device creation

**Total CI-Safe Tests: 30+**

Expected runtime: < 100ms

### GitHub Actions Configuration

Add to your CI workflow:

```yaml
- name: Run memory tests (CI-safe)
  run: cargo test --test metal_heap_tests -- --nocapture

- name: Run pressure integration tests
  run: cargo test --test heap_pressure_integration -- --nocapture

- name: Run benchmarks (baseline)
  run: cargo bench --bench metal_heap_benchmarks -- --output-format bencher | tee output.txt
  continue-on-error: true
```

## Hardware Tests (Marked #[ignore])

These tests require macOS with Metal support and must be run locally or on Apple Silicon hardware.

### Running Hardware Tests

#### On macOS with Metal Support

```bash
# Run all ignored hardware tests
cargo test --test metal_heap_tests -- --ignored --nocapture

# Run specific hardware test
cargo test --test metal_heap_tests test_real_metal_heap_observer_creation -- --ignored --nocapture

# Run all tests including hardware tests
cargo test --test metal_heap_tests -- --nocapture --include-ignored
```

#### Hardware Test Suite (8 tests)

All marked with `#[ignore]` and require Metal device:

1. **test_metal_device_availability**
   - Validates Metal device detection
   - Panics if no Metal device found
   - Runtime: < 1ms

2. **test_real_metal_heap_observer_creation**
   - Creates MetalHeapObserver with real device
   - Validates initialization
   - Runtime: < 10ms

3. **test_real_metal_heap_observer_sampling_rate**
   - Verifies sampling rate clamping
   - Tests sampling logic
   - Runtime: < 5ms

4. **test_real_metal_heap_observer_memory_stats**
   - Retrieves initial memory statistics
   - Validates stat structure
   - Runtime: < 50ms

5. **test_real_metal_heap_fragmentation_detection**
   - Tests fragmentation algorithm
   - Validates metric ranges
   - Runtime: < 100ms

6. **test_real_metal_heap_state_tracking**
   - Tests heap state management
   - Validates state vectors
   - Runtime: < 10ms

7. **test_real_metal_heap_migration_events**
   - Tests event tracking
   - Validates event structure
   - Runtime: < 10ms

8. **test_real_metal_heap_observer_clear**
   - Tests observer cleanup
   - Validates clear operation
   - Runtime: < 5ms

9. **test_real_metal_heap_observer_performance**
   - Measures observer performance
   - 100 stat retrievals
   - Validates < 1 second completion
   - Runtime: 100-500ms

**Total Hardware Tests: 9**

Expected total runtime: < 1 second (on modern Apple Silicon)

### Prerequisites

- **OS**: macOS 12.0+
- **Hardware**: Apple Silicon (M1/M2/M3+) or Intel with Metal support
- **Xcode**: 13.0+ with Command Line Tools
- **Rust**: 1.70.0+

Verify Metal availability:

```bash
# Check Metal support
system_profiler SPDisplaysDataType | grep Metal

# Verify Rust macOS target
rustc --version --verbose | grep target
# Should show: target: aarch64-apple-darwin (or x86_64-apple-darwin)
```

## Benchmarks

Comprehensive performance benchmarks for overhead measurement.

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench --bench metal_heap_benchmarks

# Run specific benchmark group
cargo bench --bench metal_heap_benchmarks -- allocation_tracking
cargo bench --bench metal_heap_benchmarks -- fragmentation_detection
cargo bench --bench metal_heap_benchmarks -- throughput

# With baseline comparison
cargo bench --bench metal_heap_benchmarks -- --baseline main

# Generate HTML report
cargo bench --bench metal_heap_benchmarks -- --verbose
```

### Benchmark Groups

#### 1. Initialization (1 benchmark)
- `bench_mock_observer_creation` - Observer creation overhead

**Target**: < 1 μs

#### 2. Allocation Tracking (3 benchmarks)
- 10, 100, 1000 allocations
- Measures per-allocation overhead

**Target**: < 10 μs per allocation

#### 3. Memory Stats Collection (3 benchmarks)
- 10, 100, 1000 allocations
- Measures stats retrieval performance

**Target**: < 100 μs (100 allocations)

#### 4. Fragmentation Detection (3 benchmarks)
- Contiguous, fragmented, sparse patterns
- Measures fragmentation algorithm

**Target**: < 500 μs (100 allocations)

#### 5. FFI Structure Creation (1 benchmark)
- FFI structure initialization

**Target**: < 1 μs

#### 6. Throughput (1 benchmark)
- 10,000 allocations per iteration

**Target**: > 100k allocations/sec

#### 7. Multi-Heap Scalability (3 benchmarks)
- 1, 10, 100 heaps
- Tests scalability with multiple heaps

**Target**: Linear scaling < 1ms per heap

### Interpreting Results

Benchmark output format:
```
allocation_tracking/10               time:   [X.XXX ms X.XXX ms X.XXX ms]
```

If benchmarks regress:

1. **Check allocation tracking**: May indicate heap observer overhead
2. **Review fragmentation detection**: Algorithm complexity issue
3. **Verify statistics collection**: Excessive iteration?
4. **Profile with Instruments**: Use `Instruments.app` for detailed analysis

### Setting Baseline

After optimizations, establish new baseline:

```bash
cargo bench --bench metal_heap_benchmarks -- --save-baseline optimized
cargo bench --bench metal_heap_benchmarks -- --baseline optimized
```

## Integration Tests

Test interaction between heap observer and memory pressure manager.

### Running Integration Tests

```bash
# Run all integration tests
cargo test --test heap_pressure_integration -- --nocapture

# Run specific integration test
cargo test --test heap_pressure_integration test_pressure_manager_basic_allocation -- --nocapture
```

### Integration Test Categories

#### Memory Pressure Manager (8 tests)
- Basic allocation/deallocation
- Budget enforcement
- Threshold detection
- Eviction strategy

#### Heap Observer Integration (3 tests)
- Tracking correlation
- Memory pressure correlation
- Eviction cleanup

#### Multi-Adapter Scenarios (3 tests)
- Load balancing
- Selective eviction
- Fragmentation tracking

**Total Integration Tests: 14**

Expected runtime: < 200ms

## Complete Test Run

Run all test suites comprehensively:

```bash
# Run all tests (skip hardware tests)
cargo test --workspace -- --nocapture

# Run all tests including hardware (macOS only)
cargo test --workspace -- --nocapture --include-ignored

# Run with detailed output
RUST_LOG=debug cargo test --test metal_heap_tests -- --nocapture --test-threads=1

# Generate coverage report (requires tarpaulin)
cargo tarpaulin --test metal_heap_tests --test heap_pressure_integration --out Html
```

### Full Test Summary

```
metal_heap_tests.rs
├── CI-Safe Tests (30+)
│   ├── Mock Observer (7 tests)
│   ├── FFI Bindings (8 tests)
│   ├── Statistics (5 tests)
│   └── Hardware Detection (2 tests)
└── Hardware Tests (9, #[ignore])

metal_heap_benchmarks.rs
├── Initialization
├── Allocation Tracking
├── Memory Stats
├── Fragmentation Detection
├── FFI Structures
├── Throughput
└── Multi-Heap Scalability

heap_pressure_integration.rs (14 tests)
├── Pressure Manager
├── Heap Integration
└── Multi-Adapter

Total Test Cases: 50+
Total Benchmarks: 15+
Expected CI Time: < 1 second
Expected Hardware Time: < 2 seconds
```

## Troubleshooting

### Common Issues

#### Metal Device Not Found

```
thread 'test_real_metal_heap_observer_creation' panicked at 'Metal device not found'
```

**Solution**: Ensure running on macOS with Metal support. Check:
```bash
system_profiler SPDisplaysDataType | grep Metal
```

#### Test Timeout

```
test timed out
```

**Solution**: Increase timeout for hardware tests:
```bash
cargo test --test metal_heap_tests -- --ignored --nocapture --test-threads=1
```

#### FFI Structure Size Mismatch

```
assertion failed: size >= 32
```

**Solution**: Verify FFI structure definitions match C ABI. Check alignment:
```bash
cargo test --test metal_heap_tests test_ffi_fragmentation_metrics_structure_size -- --nocapture
```

#### Benchmark Variance

If benchmark results vary significantly between runs:

1. Close other applications
2. Disable system activities
3. Run on stable power source (not battery)
4. Use `--sample-size 500` for more samples

```bash
cargo bench --bench metal_heap_benchmarks -- --sample-size 500
```

## Performance Targets

### Memory Overhead

| Operation | Target | Actual |
|-----------|--------|--------|
| Observer creation | < 1 μs | ??? |
| Allocation tracking | < 10 μs | ??? |
| Deallocation | < 5 μs | ??? |
| Stats retrieval (100 allocs) | < 100 μs | ??? |
| Fragmentation detection (100 allocs) | < 500 μs | ??? |

### Memory Footprint

| Component | Target | Actual |
|-----------|--------|--------|
| Observer struct | < 1 KB | ??? |
| Per allocation | < 256 B | ??? |
| Per heap | < 512 B | ??? |
| Total 1000 allocations | < 256 KB | ??? |

## Advanced Testing

### Stress Testing

```rust
// In your own test file
#[test]
fn test_observer_stress() {
    let mut observer = BenchHeapObserver::new();

    // 100,000 allocations
    for i in 0..100_000 {
        observer.record_allocation(1, 1024, (i as u64) * 1024);
    }

    // Verify stats
    let (total, count) = observer.get_memory_stats();
    assert_eq!(count, 100_000);
    assert_eq!(total, 100_000 * 1024);
}
```

### Custom Allocation Patterns

```rust
#[test]
fn test_custom_allocation_pattern() {
    let mut observer = BenchHeapObserver::new();

    // Your custom pattern
    for i in 0..10 {
        observer.record_allocation(1, (i as u64 + 1) * 1024, 0);
    }

    let metrics = observer.detect_fragmentation();
    println!("Fragmentation: {:.1}%", metrics.fragmentation_ratio * 100.0);
}
```

## Continuous Integration

### Recommended CI Configuration

```yaml
name: Memory Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --test metal_heap_tests -- --nocapture
      - run: cargo test --test heap_pressure_integration -- --nocapture

  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo bench --bench metal_heap_benchmarks -- --output-format bencher | tee output.txt
      - uses: benchmark-action/github-action@v1
        with:
          tool: 'cargo'
          output-file-path: output.txt
        continue-on-error: true
```

### macOS Hardware CI (if available)

```yaml
  hardware-test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --test metal_heap_tests -- --ignored --nocapture
```

## Documentation

For detailed implementation documentation, see:

- `src/heap_observer.rs` - Implementation details
- `src/lib.rs` - Public API
- `docs/` - Architecture documentation

## Contributing

When adding new tests:

1. Mark CI-safe tests with no attribute
2. Mark hardware tests with `#[ignore]`
3. Add benchmarks for performance-critical paths
4. Update this guide with new test categories
5. Ensure tests work on both Metal and non-Metal systems

Example:

```rust
#[test]
fn test_new_feature() {
    // CI-safe test
}

#[test]
#[ignore]
fn test_new_feature_hardware() {
    // Hardware-only test
}
```
