# Agent 4: Database Retry Logic Implementation (PRD-2 Continuation)

## Mission Summary

Add retry logic for transient database failures in the adapter upload handler, enabling resilience against temporary connection issues, locks, and timeouts without permanently failing user operations.

**Status:** ✅ COMPLETE

---

## Deliverables

### 1. Core Retry Module

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/db_retry.rs` (362 lines)

**Components:**
- `DbRetryConfig` struct with configurable parameters
  - `max_attempts`: Retry count (default 3)
  - `base_delay`: Initial backoff duration (default 100ms)
  - `max_delay`: Maximum delay cap (default 10s)
  - `backoff_factor`: Exponential growth multiplier (default 2.0)
  - `enable_jitter`: Randomized delays to prevent thundering herd

- `RetryStats` struct tracking:
  - `attempts`: Total attempts made
  - `total_duration`: Time spent waiting for retries
  - `succeeded`: Operation success status
  - `final_error`: Error if operation failed

- `retry_db_operation()` function with:
  - Intelligent error classification (retryable vs permanent)
  - Exponential backoff with configurable jitter
  - Proper logging at all levels (info/debug/warn/error)
  - Statistics collection

- `retry_db_simple()` convenience wrapper
  - Simplified API for operations not needing statistics

- Preset configurations:
  - `DbRetryConfig::fast()` - 3 attempts, 50-500ms
  - `DbRetryConfig::slow()` - 5 attempts, 500ms-30s
  - `DbRetryConfig::minimal()` - 1 attempt, no jitter (testing)
  - `DbRetryConfig::default()` - 3 attempts, 100ms-10s

**Key Features:**
- Only retries transient errors (network, timeout, locks)
- Never retries validation/policy/config errors
- Exponential backoff: delays = base × 2^N (capped at max)
- Jitter reduces thundering herd effect
- Comprehensive logging for observability

---

### 2. Handler Integration

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/aos_upload.rs` (modified)

**Changes:**
- Added imports:
  ```rust
  use crate::db_retry::{DbRetryConfig, retry_db_simple};
  use std::time::Duration;
  ```

- Wrapped `register_adapter_with_aos()` call with retry logic:
  ```rust
  let retry_config = DbRetryConfig {
      max_attempts: 3,
      base_delay: Duration::from_millis(200),
      max_delay: Duration::from_secs(10),
      backoff_factor: 1.5,
      enable_jitter: true,
  };

  let id = retry_db_simple(
      &retry_config,
      "register_adapter_with_aos",
      || {
          let db = db.clone();
          let params = params_clone.clone();
          Box::pin(async move {
              db.register_adapter_with_aos(params).await
          })
      },
  )
  .await
  .map_err(|e| {
      // existing error handling
  })?;
  ```

**Configuration Rationale:**
- 3 attempts: Good balance between resilience and user wait time
- 200ms base: Reasonable for interactive operations
- 10s max: Won't hang indefinitely
- 1.5x backoff: Moderate growth rate
- Jitter enabled: Prevents client coordination

---

### 3. Module Export

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/lib.rs` (modified)

**Change:**
```rust
pub mod db_retry;  // Added to module list
```

---

### 4. Comprehensive Testing

#### Unit Tests (in `db_retry.rs`)
- `test_is_retryable_error()` - Error classification
- `test_retry_config_variants()` - Configuration presets
- 16 additional inline tests (lines 228-362)

#### Integration Tests
**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/tests/db_retry_tests.rs` (360+ lines)

**Test Coverage:**
1. Success scenarios (no retries needed)
2. Transient error scenarios (connection, lock, timeout, network)
3. Non-retryable error scenarios (validation, config)
4. Retry limit enforcement
5. Exponential backoff progression
6. Jitter application
7. Wrapper function testing
8. Multiple consecutive retries
9. Configuration preset validation

**Key Test Cases:**
- `test_no_retry_on_immediate_success()` - Verifies no unnecessary retries
- `test_retries_transient_error()` - Simulates connection failure then success
- `test_exhausts_max_attempts()` - Verifies max_attempts limit
- `test_exponential_backoff()` - Validates backoff formula
- `test_no_retry_on_validation_error()` - Prevents retry on permanent errors
- 13+ more comprehensive tests

---

### 5. Documentation

#### Implementation Guide
**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/DB_RETRY_IMPLEMENTATION.md`

**Sections:**
- Problem statement and solution architecture
- Error classification logic
- Exponential backoff with jitter explanation
- Configuration presets and recommendations
- Integration with aos_upload handler
- Logging strategy
- Testing overview
- Implementation details with flow diagrams
- Edge cases and limitations
- Files modified summary
- Future enhancement ideas
- Migration guide for existing code

#### Testing Guide
**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/TESTING_DB_RETRY.md`

**Sections:**
- Test execution instructions
- Detailed test category breakdown (8 categories, 18+ tests)
- Test data and scenarios with tables
- Manual testing guide
- Performance benchmarks
- Stress testing examples
- Troubleshooting failed tests
- CI/CD integration examples
- Known issues and limitations
- Next steps for production deployment

---

## Technical Details

### Error Classification

**Retryable Errors** (will trigger retries):
```
- Network errors (connection refused, reset, etc)
- Timeout errors
- IO errors: connection, timeout, deadlock, busy, locked
- SQLite: "database is locked", "disk i/o error", "out of memory"
- SQLx: timeout, connection, pool
```

**Non-Retryable Errors** (fail immediately):
```
- Validation errors (invalid input)
- Config errors (bad configuration)
- Policy violations
- Determinism violations
- Egress violations
- Isolation violations
```

### Exponential Backoff Formula

For each retry attempt N (starting at 0):
```
delay = min(base_delay × (backoff_factor ^ N), max_delay)
if jitter_enabled:
    delay = delay + random(0, delay × 0.1)
```

**Example** (base=100ms, factor=2.0, jitter enabled):
```
Attempt 1: 0ms (immediate)
Attempt 2: 100ms + jitter
Attempt 3: 200ms + jitter
Attempt 4: 400ms + jitter
Attempt 5: 800ms + jitter (capped at max_delay if applicable)
```

### Logging Levels

| Level | When | Fields |
|-------|------|--------|
| info! | Success after retry | operation, attempts, total_duration_ms |
| debug! | Retry attempt | operation, attempt, next_attempt_delay_ms |
| warn! | Transient error | operation, error, attempt, max_attempts, next_delay_ms |
| error! | Non-retryable error | operation, error, attempt |
| error! | Retries exhausted | operation, error, attempts, max_attempts, total_duration_ms |

---

## Code Quality

### Design Principles Applied

1. **Error Distinction** - Only retries transient errors, never permanent ones
2. **Configurable** - Multiple preset configs for different scenarios
3. **Observable** - Comprehensive logging for debugging
4. **Resilient** - Exponential backoff prevents overwhelming system
5. **Safe** - Jitter prevents thundering herd problem
6. **Tested** - 18+ test cases covering all scenarios

### Standards Compliance

✅ Follows CLAUDE.md conventions:
- Error handling with `Result<T>` (never `Option<T>`)
- Uses `tracing` macros (never `println!`)
- Rust naming conventions (PascalCase types, snake_case functions)
- Proper error context with `map_err()`

✅ Integrates with existing retry patterns:
- Compatible with `adapteros-core/src/retry_policy.rs`
- Uses same `AosError` types
- Follows similar backoff strategy

---

## Testing Strategy

### Unit Tests
- Test in isolation without async runtime
- Verify error classification logic
- Validate configuration values
- Test mathematical formulas (backoff, jitter)

### Integration Tests
- Use `#[tokio::test]` for async execution
- Simulate transient failures with counters
- Verify actual retry behavior
- Test with real error types

### Manual Testing
- Test against actual database with induced failures
- Monitor logs during operation
- Verify timing under load
- Stress test with concurrent operations

---

## Integration Points

### Existing Systems

**Respects:** `adapteros-core::AosError`
- Uses existing error types
- Compatible with error handling patterns
- Integrates with `Result<T>` pattern

**Works with:** `adapteros-db::Db`
- Calls existing database methods
- No database changes required
- Transparent to callers

**Coordinates with:** Existing logging
- Respects `tracing` infrastructure
- Logs to standard log collection
- Observable in existing dashboards

---

## Performance Impact

### Typical Latency

| Scenario | Overhead | Notes |
|----------|----------|-------|
| Success on attempt 1 | 0ms | No delay |
| Success on attempt 2 | ~200ms | 1 base_delay |
| Success on attempt 3 | ~500ms | 1 + 2 × base_delay |

### Resource Usage

| Resource | Impact | Notes |
|----------|--------|-------|
| Memory | Negligible | No queues, just stack allocation |
| CPU | Minimal | Just exponential calculation |
| Threads | None | Uses existing async runtime |
| Connections | None created | Retries existing operations |

### Failure Recovery

**Without Retry:**
- Transient 100ms network hiccup → Upload fails

**With Retry:**
- Transient 100ms network hiccup → Retried after 200ms → Success
- Users never see the brief outage

---

## Deployment Considerations

### No Breaking Changes
- Module is new, doesn't modify existing APIs
- Handler change is internal (transparent to callers)
- Fully backward compatible

### Configuration Options
- Retry behavior can be tuned via `DbRetryConfig`
- Presets provided for common scenarios
- Can be disabled by setting `max_attempts: 0` if needed

### Monitoring Recommendations
1. Track retry rate by operation type
2. Monitor success rate of retried operations
3. Alert if retry rate exceeds baseline
4. Log patterns to understand failure modes

---

## Files Summary

### Created Files
1. `/Users/star/Dev/aos/crates/adapteros-server-api/src/db_retry.rs` (362 lines)
   - Core retry logic implementation

2. `/Users/star/Dev/aos/crates/adapteros-server-api/tests/db_retry_tests.rs` (360+ lines)
   - Comprehensive integration tests

3. `/Users/star/Dev/aos/crates/adapteros-server-api/DB_RETRY_IMPLEMENTATION.md`
   - Implementation and architecture documentation

4. `/Users/star/Dev/aos/crates/adapteros-server-api/TESTING_DB_RETRY.md`
   - Complete testing guide and troubleshooting

### Modified Files
1. `/Users/star/Dev/aos/crates/adapteros-server-api/src/lib.rs`
   - Added: `pub mod db_retry;`

2. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/aos_upload.rs`
   - Added imports for retry module
   - Wrapped `register_adapter_with_aos()` with retry logic (~35 lines)

---

## Verification Checklist

- [x] Retry logic distinguishes transient from permanent errors
- [x] Exponential backoff implemented correctly (base × 2^N, capped)
- [x] Jitter applied to prevent thundering herd (±10% variance)
- [x] Max attempts respected (stops after configured attempts)
- [x] Logging at appropriate levels (info/debug/warn/error)
- [x] Configuration is flexible and preset-friendly (4 presets)
- [x] Integration with aos_upload handler (register_adapter_with_aos)
- [x] Comprehensive unit tests (18+ test cases)
- [x] Integration tests with simulated failures
- [x] Documentation complete (implementation + testing guides)
- [x] Code follows CLAUDE.md standards (errors, logging, naming)
- [x] No breaking changes to existing APIs
- [x] Backward compatible with existing code

---

## Future Enhancement Opportunities

1. **Metrics Export** - Prometheus metrics for retry operations
2. **Circuit Breaker** - Stop retrying if error rate exceeds threshold
3. **Adaptive Backoff** - Adjust delays based on observed patterns
4. **Dead Letter Queue** - Capture failed operations for analysis
5. **Per-Operation Config** - Allow handlers to customize retry behavior
6. **Distributed Tracing** - Correlate retries across service calls
7. **Dynamic Configuration** - Adjust retry settings without restart
8. **Retry Analytics** - Dashboard of retry patterns and success rates

---

## Related Documentation

- **CLAUDE.md** - Project standards and conventions
- **docs/DATABASE_REFERENCE.md** - Database schema and operations
- **crates/adapteros-core/src/retry_policy.rs** - Alternative retry implementation for other use cases
- **docs/ERROR_HANDLING.md** - Error handling standards (if exists)

---

## Implementation Notes

### Key Decisions

1. **Error Classification at Retry Level** - Rather than in error types themselves, keeps retry logic separate and testable

2. **Exponential Backoff Formula** - Uses proven 2^N pattern to balance quick recovery and eventual success

3. **Jitter ±10%** - Conservative variance to reduce thundering herd without excessive randomness

4. **Configurable Presets** - Provides `fast()`, `slow()`, `minimal()`, `default()` for flexibility without boilerplate

5. **Separate Testing Files** - Unit tests in module, integration tests separate for clear organization

### Lessons Applied

- From `adapteros-core/retry_policy.rs`: Structured config, circuit breaker patterns (reserved for future)
- From `adapteros-error-recovery/lib.rs`: Error classification and recovery strategies
- From PR-02 experience: Need for resilient database operations during development

---

## Sign-Off

**Implementation Date:** 2025-11-19
**Agent:** Agent 4 of 15 (PRD-2 Fixes)
**Status:** ✅ COMPLETE

All tasks completed:
1. ✅ Search for existing retry patterns
2. ✅ Implement exponential backoff retry logic
3. ✅ Add retry logic to register_adapter_with_aos()
4. ✅ Make retries configurable
5. ✅ Log retry attempts appropriately
6. ✅ Only retry transient errors
7. ✅ Add comprehensive tests

The adapter upload handler is now resilient to transient database failures, improving user experience by automatically recovering from temporary connection issues without forcing users to retry their uploads.

---

**Next Agent:** Agent 5 (PRD-2 continuation)
