# Testing Database Retry Logic

## Test Coverage Overview

The database retry logic includes 18+ test cases covering all scenarios. Tests are located in two files:

1. **Inline unit tests** - `/Users/star/Dev/aos/crates/adapteros-server-api/src/db_retry.rs` (lines 228-362)
2. **Integration tests** - `/Users/star/Dev/aos/crates/adapteros-server-api/tests/db_retry_tests.rs` (full file)

## Test Execution

### Running All Retry Tests

```bash
# Run unit tests only
cargo test -p adapteros-server-api db_retry --lib

# Run integration tests only
cargo test -p adapteros-server-api db_retry_tests

# Run both with output
cargo test -p adapteros-server-api db_retry -- --nocapture
```

### Running Specific Tests

```bash
# Test exponential backoff
cargo test -p adapteros-server-api test_exponential_backoff -- --nocapture

# Test transient error retry
cargo test -p adapteros-server-api test_retries_transient_error -- --nocapture

# Test max attempts exhaustion
cargo test -p adapteros-server-api test_exhausts_max_attempts -- --nocapture
```

## Test Categories

### 1. Error Classification Tests

**Test:** `test_is_retryable_error()`
**Verifies:** Error classification logic correctly identifies which errors should trigger retries

**Cases:**
- Network errors → retryable
- Timeout errors → retryable
- IO errors with "connection", "timeout", "deadlock" → retryable
- SQLite "database is locked" → retryable
- Validation errors → NOT retryable
- Config errors → NOT retryable
- Policy violations → NOT retryable

**Expected Outcome:** ✓ All error types classified correctly

---

### 2. Configuration Preset Tests

**Tests:**
- `test_fast_config_preset()` - Verifies fast config: 3 attempts, 50-500ms
- `test_slow_config_preset()` - Verifies slow config: 5 attempts, 500ms-30s
- `test_minimal_config_preset()` - Verifies minimal: 1 attempt, no jitter
- `test_default_config()` - Verifies default: 3 attempts, 100ms-10s

**Expected Outcome:** ✓ All presets have correct settings

---

### 3. Success Scenarios

**Test:** `test_no_retry_on_immediate_success()`
**Scenario:** Operation succeeds on first attempt
**Verifies:**
- No retries triggered
- Attempt count = 1
- Total duration = 0
- Success flag = true

**Expected Outcome:** ✓ Single attempt, no retry

---

### 4. Transient Error Scenarios

**Tests:**

#### Connection Error Retry
**Test:** `test_retries_on_connection_error()`
**Scenario:**
- Attempt 1: "connection refused" error
- Attempt 2: "connection refused" error
- Attempt 3: Success

**Verifies:**
- Retries triggered on connection errors
- Attempt count = 3
- Total duration > 10ms (delays applied)

**Expected Outcome:** ✓ Retried and succeeded

#### Database Locked Retry
**Test:** `test_retries_on_database_locked()`
**Scenario:**
- Attempt 1: "database is locked" error
- Attempt 2: Success

**Verifies:**
- SQLite lock errors trigger retries
- Attempt count = 2

**Expected Outcome:** ✓ Retried and succeeded

#### Timeout Error Retry
**Test:** `test_retries_on_timeout()`
**Scenario:**
- Attempt 1: Timeout error
- Attempt 2: Success

**Verifies:**
- Timeout errors trigger retries
- Attempt count = 2

**Expected Outcome:** ✓ Retried and succeeded

#### Network Error Retry
**Test:** `test_network_error_is_retryable()`
**Scenario:**
- Attempt 1: Network error
- Attempt 2: Success

**Verifies:**
- Network errors trigger retries
- Attempt count = 2

**Expected Outcome:** ✓ Retried and succeeded

#### Multiple Transient Failures
**Test:** `test_multiple_transient_failures_then_success()`
**Scenario:**
- Attempts 1-4: "database is locked" errors
- Attempt 5: Success

**Verifies:**
- Multiple consecutive retries work
- Attempt count = 5
- Eventually succeeds

**Expected Outcome:** ✓ Multiple retries handled

---

### 5. Non-Retryable Error Scenarios

**Tests:**

#### Validation Error
**Test:** `test_no_retry_on_validation_error()`
**Scenario:** Validation error (invalid input)
**Verifies:**
- No retries triggered
- Only 1 attempt
- Immediate failure

**Expected Outcome:** ✓ Failed on first attempt

#### Config Error
**Test:** `test_no_retry_on_config_error()`
**Scenario:** Config error (bad configuration)
**Verifies:**
- No retries triggered
- Only 1 attempt
- Immediate failure

**Expected Outcome:** ✓ Failed on first attempt

---

### 6. Retry Limit Tests

**Test:** `test_exhausts_max_attempts()`
**Scenario:**
- Configured max_attempts = 2
- All attempts fail with Network error

**Verifies:**
- Respects max_attempts limit
- Total attempts = 3 (initial + 2 retries)
- Error message contains "after 3 attempts"

**Expected Outcome:** ✓ Stops at max_attempts

---

### 7. Backoff Behavior Tests

#### Exponential Backoff
**Test:** `test_exponential_backoff()`
**Config:**
- max_attempts: 3
- base_delay: 10ms
- backoff_factor: 2.0
- jitter: disabled

**Scenario:**
- Attempt 1: Immediate
- Attempt 2: After 10ms delay
- Attempt 3: After 20ms delay (10×2.0)
- Attempt 4: After 40ms delay (20×2.0), then succeeds

**Verifies:**
- Exponential backoff progression: 10ms → 20ms → 40ms
- Total elapsed time > 50ms
- Backoff formula: `delay = delay × backoff_factor`

**Expected Outcome:** ✓ Delays follow exponential pattern

#### Jitter Application
**Test:** `test_jitter_application()`
**Config:**
- base_delay: 100ms
- jitter: enabled (±10%)
- backoff_factor: 1.0 (no growth)

**Scenario:**
- Attempt 1: Error
- Attempt 2: Success (after delay with jitter)

**Verifies:**
- Jitter applied (actual delay ≥ base_delay)
- Jitter range: 0-10% of base_delay
- Duration in range [100ms, 110ms)

**Expected Outcome:** ✓ Jitter applied correctly

---

### 8. Wrapper Function Tests

**Test:** `test_retry_db_simple_wrapper()`
**Verifies:** Simple wrapper function works correctly
**Scenario:** Operation succeeds on first attempt
**Expected Outcome:** ✓ Wrapper returns result directly

---

## Test Data & Scenarios

### Configuration Variations

| Test | max_attempts | base_delay | max_delay | backoff | jitter | Description |
|------|-------------|-----------|----------|---------|--------|------------|
| fast | 3 | 50ms | 500ms | 2.0 | ✓ | Quick operations |
| slow | 5 | 500ms | 30s | 1.5 | ✓ | Heavy operations |
| minimal | 1 | 10ms | 50ms | 1.5 | ✗ | Testing only |
| default | 3 | 100ms | 10s | 2.0 | ✓ | Standard |

### Error Scenarios

| Error Type | Retryable | Test Case |
|-----------|-----------|-----------|
| Network/connection | Yes | `test_retries_on_connection_error` |
| Timeout | Yes | `test_retries_on_timeout` |
| Database locked | Yes | `test_retries_on_database_locked` |
| Network error | Yes | `test_network_error_is_retryable` |
| Validation | No | `test_no_retry_on_validation_error` |
| Config | No | `test_no_retry_on_config_error` |

---

## Manual Testing Guide

### Testing in Local Development

1. **Start with minimal config for fast feedback:**
   ```rust
   let config = DbRetryConfig::minimal();  // 1 retry, 10ms base delay
   ```

2. **Simulate transient error:**
   ```rust
   // In your operation, fail first time, succeed second
   let mut first_call = true;
   let result = retry_db_simple(
       &config,
       "test_op",
       || {
           Box::pin(async {
               if first_call {
                   first_call = false;
                   Err(AosError::Io("connection timeout".to_string()))
               } else {
                   Ok("success")
               }
           })
       }
   ).await;
   assert!(result.is_ok());
   ```

3. **Monitor logs to verify:**
   ```
   WARN  Transient database error, retrying...
   DEBUG Retrying database operation
   INFO  Database operation succeeded after retries
   ```

4. **Test with actual database:**
   ```bash
   # While operation is running, kill database connection:
   # - SQLite: delete/lock the database file
   # - Postgres: restart the database

   # Observe operation succeeds after retries
   ```

### Stress Testing

To verify retry logic under load:

```rust
// Create many concurrent operations
let handles: Vec<_> = (0..100)
    .map(|_| {
        tokio::spawn(async {
            retry_db_simple(
                &DbRetryConfig::default(),
                "stress_test",
                || Box::pin(async { db_operation().await })
            ).await
        })
    })
    .collect();

// Monitor retry rates and success counts
let results: Vec<_> = futures::future::join_all(handles).await;
let success_count = results.iter().filter(|r| r.is_ok()).count();
println!("Success rate: {}/100", success_count);
```

---

## Performance Benchmarks

### Typical Retry Timings

| Scenario | Attempts | Total Time | Notes |
|----------|----------|-----------|-------|
| Success on attempt 1 | 1 | ~0ms | No delay |
| Success on attempt 2 | 2 | ~100ms (base) | 1 × base_delay |
| Success on attempt 3 | 3 | ~300ms (base) | 1 × base_delay + 2 × base_delay |
| Exhausted (3 attempts) | 3 | ~300ms (base) | Failed after all attempts |

### With Exponential Backoff (2.0x)

| Scenario | Attempts | Total Time |
|----------|----------|-----------|
| Success on attempt 1 | 1 | ~0ms |
| Success on attempt 2 | 2 | ~100ms |
| Success on attempt 3 | 3 | ~300ms (100 + 200) |
| Success on attempt 4 | 4 | ~700ms (100 + 200 + 400) |

---

## Troubleshooting Failed Tests

### Test Timeout

**Symptom:** Test hangs or times out
**Cause:** Infinite retry loop or sleep duration too long
**Fix:**
- Use `minimal` config in tests
- Set very short delays: `Duration::from_millis(1)`
- Add explicit timeout: `tokio::time::timeout(Duration::from_secs(5), ...)`

### Non-Deterministic Timing Tests

**Symptom:** Jitter tests pass/fail randomly
**Cause:** Jitter adds randomness
**Fix:**
- Disable jitter for timing assertions: `enable_jitter: false`
- Use generous time windows (e.g., 50% margin)
- Test jitter range rather than exact value

### Async Execution Issues

**Symptom:** Test panics with "future not executed" or similar
**Cause:** Missing `#[tokio::test]` or not awaiting properly
**Fix:**
- Ensure test has `#[tokio::test]` attribute
- Ensure all async operations are awaited
- Use `Box::pin()` for dynamic trait objects

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Test Database Retry Logic

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: |
          cargo test -p adapteros-server-api db_retry --lib
          cargo test -p adapteros-server-api db_retry_tests
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

set -e

echo "Running database retry tests..."
cargo test -p adapteros-server-api db_retry -- --test-threads=1

if [ $? -ne 0 ]; then
    echo "Tests failed! Commit aborted."
    exit 1
fi
```

---

## Known Issues & Limitations

1. **Timing Tests Are Flaky** - System load can affect exact timing
   - Use loose time windows (e.g., `> 50ms` instead of `== 75ms`)

2. **Jitter Randomness** - Different seeds on each run
   - Use `fastrand` for consistent seeding in tests if needed

3. **Async Runtime Dependent** - Tests require Tokio runtime
   - Ensure `#[tokio::test]` is used, not `#[test]`

4. **Database Mocking** - Real database may behave differently
   - Consider integration tests against actual database

---

## Next Steps

1. **Run all tests:**
   ```bash
   cargo test -p adapteros-server-api db_retry
   ```

2. **Monitor in production:**
   - Track retry rate by error type
   - Monitor success rate of retry operations
   - Alert if retry rate exceeds threshold

3. **Optimize configuration:**
   - Adjust delays based on observed recovery times
   - Enable/disable jitter based on client patterns
   - Set max_attempts based on acceptable latency

4. **Extend coverage:**
   - Add tests for specific database error codes
   - Test retry behavior under network latency
   - Verify memory usage under sustained retries

---

**Document Version:** 1.0
**Last Updated:** 2025-11-19
