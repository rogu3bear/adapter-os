# Test Execution and Integration Guide

## Overview

This guide covers running, verifying, and integrating the KV Cache and Attention verification tests into your development workflow.

## File Structure

```
crates/adapteros-lora-mlx-ffi/tests/
├── kv_cache_attention_verification.rs       # 45+ main tests
├── attention_debug_utilities.rs             # 15+ debug utilities
├── ffi_verification_examples.rs             # 6 reference examples
├── REFERENCE_KV_CACHE_ATTENTION.md          # Complete test guide
├── STATUS_VERIFICATION_SUMMARY.md           # Executive summary
├── REFERENCE_TESTS_QUICK.md                 # Quick lookup
└── GUIDE_TEST_EXECUTION.md                  # This file
```

## Prerequisites

- Rust 1.70+
- Cargo with test support
- MLX FFI bindings installed
- 2GB+ RAM for test execution

## Quick Start (3 Steps)

```bash
# Step 1: Navigate to crate
cd <repo-root>/crates/adapteros-lora-mlx-ffi

# Step 2: Compile tests
cargo test --test kv_cache_attention_verification --no-run

# Step 3: Run tests
cargo test --test kv_cache_attention_verification --lib
```

## Comprehensive Test Execution

### Execute All Tests by Category

```bash
# ============ KV Cache Tests ============
# 5 initialization tests
cargo test test_kv_cache_ffi

# 5 operation tests  
cargo test test_kv_cache_clear test_kv_cache_hit test_kv_cache_validation

# 2 memory tracking tests
cargo test test_kv_cache_memory

# ============ RoPE Tests ============
# 7 RoPE verification tests
cargo test test_rope

# ============ SDPA Tests ============
# 8 attention computation tests
cargo test test_sdpa test_attention

# ============ Integration Tests ============
# End-to-end scenarios
cargo test test_integration test_kv_cache_with_attention

# ============ Debug Utilities ============
# 15+ debugging and visualization tests
cargo test attention_debug
```

## Individual Test Execution

### Single Test with Output

```bash
# Run with output capture
cargo test test_rope_orthogonality -- --nocapture

# Run with logging
RUST_LOG=debug cargo test test_sdpa_basic_attention -- --nocapture

# Run with thread details
cargo test test_kv_cache_update -- --nocapture --test-threads=1
```

### Test Groups

```bash
# Run all tests matching pattern
cargo test kv_cache

# Run tests in specific module
cargo test crate::kv_cache::tests::

# Run tests excluding pattern
cargo test --lib -- --skip test_rope
```

## Validation Workflow

### Step 1: Verify Compilation
```bash
# Check all tests compile without errors
cargo test --test kv_cache_attention_verification --no-run 2>&1 | \
    grep -E "(error|warning:|Finished)"
# Expected: "Finished `test` profile"
```

### Step 2: Run Core Verification
```bash
# Run the 45+ main tests
cargo test --test kv_cache_attention_verification --lib -- --test-threads=4

# Expected output:
# test result: ok. XX passed; 0 failed; 0 ignored
```

### Step 3: Run Debug Utilities
```bash
# Test visualization and analysis tools
cargo test --test attention_debug_utilities --lib

# Expected: All utilities compile and run
```

### Step 4: Run Examples
```bash
# Test reference implementations
cargo test --test ffi_verification_examples --lib

# Expected: All 6 examples execute successfully
```

## Test Results Interpretation

### Success Indicators
```
test test_kv_cache_ffi_initialization ... ok
test test_rope_orthogonality ... ok
test test_sdpa_basic_attention ... ok

test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured
```

### Common Test Output

#### Pass
```
running 45 tests

test test_kv_cache_clear_all ... ok
...

test result: ok. 45 passed; 0 failed
```

#### Failure
```
test test_rope_dimension_mismatch ... FAILED

thread 'test_rope_dimension_mismatch' panicked at 'assertion failed'
```

#### Timeout
```
test test_kv_cache_large_operation ... timeout

(Increase timeout with: RUST_TEST_TIME_THREADS=2)
```

## Performance Testing

### Memory Usage During Tests
```bash
# Monitor memory with /usr/bin/time
/usr/bin/time -v cargo test --test kv_cache_attention_verification --lib 2>&1 | \
    grep -E "(Maximum|User|System)"
```

### Test Execution Time
```bash
# Run with timing
cargo test --test kv_cache_attention_verification --lib -- --nocapture | \
    grep -E "(test.*ok|test result)"
```

### Cache Statistics
```bash
# Enable debug logging to see cache stats
RUST_LOG=trace cargo test test_kv_cache_with_attention_pipeline -- --nocapture 2>&1 | \
    grep "KV Cache"
```

## Continuous Integration Setup

### For GitHub Actions

```yaml
name: KV Cache and Attention Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run KV Cache tests
        run: |
          cargo test -p adapteros-lora-mlx-ffi \
            --test kv_cache_attention_verification --lib
      - name: Run debug utilities
        run: |
          cargo test -p adapteros-lora-mlx-ffi \
            --test attention_debug_utilities --lib
```

### For Local Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

cargo test -p adapteros-lora-mlx-ffi \
    --test kv_cache_attention_verification --lib \
    --quiet

if [ $? -ne 0 ]; then
    echo "KV Cache tests failed. Commit aborted."
    exit 1
fi
```

## Debugging Failed Tests

### Step 1: Identify Failure
```bash
# Run test to capture error
cargo test test_name -- --nocapture 2>&1 | tee test_output.log
```

### Step 2: Enable Logging
```bash
# Run with debug logs
RUST_LOG=debug cargo test test_name -- --nocapture
```

### Step 3: Check Specific Component
```rust
// Add debug prints to test
println!("Cache state: {:?}", cache.get_status());
println!("Output shape: {:?}", output.shape());
println!("Values: {:?}", output.to_float_vec()?);
```

### Step 4: Validate Assumptions
```bash
# Test in isolation with minimal setup
cargo test test_rope_position_zero_identity --lib -- --nocapture
```

## Advanced Test Scenarios

### Test with Thread Limit
```bash
# Single-threaded execution for determinism
cargo test --test kv_cache_attention_verification --lib -- --test-threads=1
```

### Test with Timeout
```bash
# 60-second timeout per test
RUST_TEST_TIME_THREADS=60 cargo test --test kv_cache_attention_verification --lib
```

### Test Specific Features
```bash
# Run tests for specific FFI feature
cargo test --test kv_cache_attention_verification --features mlx-gpu --lib
```

### Benchmark Tests
```bash
# Run as benchmarks (if configured)
cargo bench --bench kv_cache_attention_verification
```

## Test Maintenance

### Update Tests After Code Changes
```bash
# Recompile after modifying implementation
cargo clean
cargo test --test kv_cache_attention_verification --no-run

# Run all tests
cargo test --test kv_cache_attention_verification --lib
```

### Add New Tests
```rust
#[test]
fn test_new_feature() {
    // Arrange
    let cache = MLXKVCache::new(config);
    
    // Act
    let result = cache.some_operation();
    
    // Assert
    assert!(result.is_ok());
}
```

### Remove Broken Tests
```bash
# Comment out and mark with TODO
#[test]
#[ignore]  // TODO: Fix after issue #123
fn test_broken_feature() { ... }
```

## Integration with Development Workflow

### Pre-Push Validation
```bash
#!/bin/bash
# Before pushing to remote

echo "Running comprehensive test suite..."
cargo test -p adapteros-lora-mlx-ffi --lib
if [ $? -ne 0 ]; then
    echo "Tests failed!"
    exit 1
fi

echo "All tests passed. Ready to push."
```

### Regression Testing
```bash
# Save test output as baseline
cargo test --test kv_cache_attention_verification --lib > baseline.txt 2>&1

# After changes, compare
cargo test --test kv_cache_attention_verification --lib > current.txt 2>&1
diff baseline.txt current.txt
```

### Performance Monitoring
```bash
# Track test execution time over commits
for commit in HEAD~5..HEAD; do
    git checkout $commit
    echo "Testing $commit:"
    time cargo test --test kv_cache_attention_verification --lib --quiet
done
```

## Troubleshooting

### Issue: Tests Won't Compile
```bash
# Clean build
cargo clean
cargo test --test kv_cache_attention_verification --no-run

# Check for missing dependencies
cargo tree | grep adapteros-lora-mlx-ffi
```

### Issue: Tests Timeout
```bash
# Increase timeout
RUST_TEST_TIME_THREADS=300 cargo test --test kv_cache_attention_verification --lib

# Or run specific fast tests
cargo test test_kv_cache_ffi --lib
```

### Issue: Intermittent Failures
```bash
# Run repeatedly to find flaky tests
for i in {1..10}; do
    echo "Run $i:"
    cargo test --test kv_cache_attention_verification --lib --quiet
done
```

### Issue: Memory Leaks During Tests
```bash
# Use valgrind to detect leaks (Linux)
valgrind --leak-check=full \
    cargo test --test kv_cache_attention_verification --lib

# Or use LLVM's leak detector (macOS/Linux)
LSAN_OPTIONS=verbosity=1 cargo test --test kv_cache_attention_verification --lib
```

## Reporting Test Results

### Generate Test Report
```bash
# Run with JSON output
cargo test --test kv_cache_attention_verification --lib -- --format json > results.json

# Parse and display
jq '.test_result | group_by(.event) | map({event: .[0].event, count: length})' results.json
```

### Generate Coverage Report
```bash
# Install coverage tool
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin -p adapteros-lora-mlx-ffi \
    --out Html --output-dir coverage_report
```

## Documentation

For more details, see:
- `REFERENCE_TESTS_QUICK.md` - Command reference
- `STATUS_VERIFICATION_SUMMARY.md` - Test overview
- `REFERENCE_KV_CACHE_ATTENTION.md` - Detailed test guide

## Key Commands Summary

```bash
# Compile tests
cargo test --test kv_cache_attention_verification --no-run

# Run all tests
cargo test --test kv_cache_attention_verification --lib

# Run specific test category
cargo test test_kv_cache --lib
cargo test test_rope --lib
cargo test test_sdpa --lib

# Run with output
cargo test test_name -- --nocapture

# Run with logging
RUST_LOG=debug cargo test test_name -- --nocapture

# Run specific test only
cargo test test_rope_orthogonality --lib -- --exact --nocapture
```

---

**Last Updated:** 2025-11-22
**Status:** Ready for Integration
