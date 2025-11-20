# Database Retry Logic Implementation (PRD-2 Agent 4)

## Overview

This document describes the database operation retry logic implemented to handle transient database failures, improving system resilience and reducing false failures during temporary issues.

## Problem Statement

Database operations in `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/aos_upload.rs` had no retry mechanism for transient failures such as:
- Connection timeouts
- Database locks ("database is locked" errors)
- Temporary network issues
- Short-term resource exhaustion

Without retries, brief connectivity issues or temporary locks would cause adapter upload operations to fail permanently, forcing users to retry the entire upload.

## Solution Architecture

### 1. Retry Module (`db_retry.rs`)

Created a new module at `/Users/star/Dev/aos/crates/adapteros-server-api/src/db_retry.rs` providing:

**Core Components:**

- `DbRetryConfig` - Configuration for retry behavior
  - `max_attempts`: Total retry attempts (default: 3)
  - `base_delay`: Initial delay between retries (default: 100ms)
  - `max_delay`: Maximum delay cap (default: 10s)
  - `backoff_factor`: Exponential growth rate (default: 2.0x)
  - `enable_jitter`: Randomize delays to prevent thundering herd (default: true)

- `RetryStats` - Metrics from retry operations
  - `attempts`: Number of attempts made
  - `total_duration`: Time spent retrying
  - `succeeded`: Whether operation succeeded
  - `final_error`: Error if failed

- `retry_db_operation()` - Main retry implementation
  - Executes async database operations with exponential backoff
  - Distinguishes transient vs permanent errors
  - Returns both result and retry statistics
  - Logs retry attempts at appropriate levels

- `retry_db_simple()` - Convenience wrapper
  - Returns just the operation result
  - No retry statistics collection

### 2. Error Classification

The module implements intelligent error classification to only retry transient errors:

**Retryable Errors** (will trigger retries):
- `AosError::Network("...")` - Network connectivity issues
- `AosError::Timeout { ... }` - Operation timeouts
- `AosError::Io(...)` containing:
  - "connection" - Connection failures
  - "timeout" - Timeout errors
  - "deadlock" - Database deadlock
  - "busy" - Resource busy
  - "locked" - Database locked
- `AosError::Sqlite(...)` containing:
  - "database is locked" - SQLite lock contention
  - "disk i/o error" - Temporary IO issues
  - "out of memory" - Temporary memory pressure

**Non-Retryable Errors** (will NOT retry):
- `AosError::Validation(...)` - Invalid input validation
- `AosError::Config(...)` - Configuration errors
- `AosError::PolicyViolation(...)` - Policy violations
- `AosError::DeterminismViolation(...)` - Determinism violations
- `AosError::EgressViolation(...)` - Egress policy violations
- `AosError::IsolationViolation(...)` - Isolation violations

### 3. Exponential Backoff with Jitter

The retry logic uses exponential backoff to avoid overwhelming the system:

```
Attempt 1: 0ms (immediate)
Attempt 2: 100ms + jitter (0-10ms)
Attempt 3: 200ms + jitter (0-20ms) = (100 * 2.0) + jitter
Attempt 4: 400ms + jitter (0-40ms) = (200 * 2.0) + jitter
...capped at max_delay (10s)
```

**Jitter** (10% random variance):
- Prevents "thundering herd" problem where multiple clients retry simultaneously
- Spreads retry load over time
- Can be disabled for deterministic testing

### 4. Configurable Presets

The module provides pre-configured retry policies for different scenarios:

```rust
// Fast retries: 3 attempts, 50-500ms delays (for quick operations)
DbRetryConfig::fast()

// Slow retries: 5 attempts, 500ms-30s delays (for heavy operations)
DbRetryConfig::slow()

// Minimal: 1 retry, no jitter (for testing)
DbRetryConfig::minimal()

// Default: 3 attempts, 100ms-10s delays, with jitter
DbRetryConfig::default()
```

## Integration with aos_upload Handler

The retry logic is integrated into the adapter upload handler:

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/aos_upload.rs`

**Modified Section:** Database registration call (~line 524-558)

**Before:**
```rust
let id = state.db.register_adapter_with_aos(params).await.map_err(|e| {
    error!("Database error during adapter registration: {}", e);
    // ... cleanup ...
    (StatusCode::INTERNAL_SERVER_ERROR, "Unable to register adapter".to_string())
})?;
```

**After:**
```rust
let retry_config = DbRetryConfig {
    max_attempts: 3,
    base_delay: Duration::from_millis(200),
    max_delay: Duration::from_secs(10),
    backoff_factor: 1.5,
    enable_jitter: true,
};

let db = state.db.clone();
let params_clone = params.clone();

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
    error!("Database error during adapter registration after retries: {}", e);
    // ... cleanup ...
    (StatusCode::INTERNAL_SERVER_ERROR, "Unable to register adapter".to_string())
})?;
```

**Configuration:**
- Max 3 attempts for adapter registration
- 200ms base delay (balanced for interactive upload)
- 10s max delay (reasonable timeout)
- 1.5x backoff factor (moderate growth)
- Jitter enabled (smoother retry spreading)

## Logging Strategy

Retry operations log at appropriate levels:

| Level | When | Example |
|-------|------|---------|
| `info!()` | Successful retry | "Database operation succeeded after retries" |
| `debug!()` | Retry attempt | "Retrying database operation" |
| `warn!()` | Transient error | "Transient database error, retrying..." |
| `error!()` | Non-retryable error | "Non-retryable database error encountered" |
| `error!()` | Retries exhausted | "Database operation failed after exhausting retries" |

**Logged Context:**
- Operation name
- Error message
- Current attempt number
- Max attempts configured
- Total duration of retries
- Next delay (if retrying)

## Testing

### Unit Tests (`db_retry.rs`)

The module includes comprehensive inline tests:

1. **`test_is_retryable_error`** - Error classification logic
2. **`test_retry_config_variants`** - Config presets
3. **`test_successful_operation_no_retry`** - No retry on success
4. **`test_retries_transient_error`** - Retries connection errors
5. **`test_retries_on_database_locked`** - Retries SQLite locks
6. **`test_retries_on_timeout`** - Retries timeout errors
7. **`test_no_retry_on_validation_error`** - No retry on validation
8. **`test_no_retry_on_config_error`** - No retry on config
9. **`test_exhausts_max_attempts`** - Respects max_attempts limit
10. **`test_exponential_backoff`** - Backoff progression
11. **`test_jitter_application`** - Jitter variance
12. **`test_retry_db_simple_wrapper`** - Simple wrapper function
13. **`test_multiple_transient_failures_then_success`** - Multiple retries
14. **`test_network_error_is_retryable`** - Network error handling
15. **`test_fast_config_preset`** - Fast config validation
16. **`test_slow_config_preset`** - Slow config validation
17. **`test_minimal_config_preset`** - Minimal config validation
18. **`test_default_config`** - Default config validation

### Integration Tests (`db_retry_tests.rs`)

Standalone integration tests in `/Users/star/Dev/aos/crates/adapteros-server-api/tests/db_retry_tests.rs` that simulate:
- Transient failures followed by success
- Exhausting retry attempts
- Non-retryable error handling
- Configuration variants

## Configuration Recommendations

### For Interactive Operations (like adapter uploads)

```rust
DbRetryConfig {
    max_attempts: 3,
    base_delay: Duration::from_millis(200),
    max_delay: Duration::from_secs(10),
    backoff_factor: 1.5,
    enable_jitter: true,
}
```

**Rationale:**
- 3 attempts = good balance between resilience and speed
- 200ms base delay = reasonable for user perception
- 10s max = won't hang indefinitely
- 1.5x backoff = avoids excessive delays
- Jitter = prevents client coordination issues

### For Background Operations (like batch processing)

```rust
DbRetryConfig::slow()  // 5 attempts, 500ms-30s, 1.5x backoff
```

**Rationale:**
- More resilient since user isn't waiting
- Longer delays acceptable for background jobs
- Higher max_attempts for better success rate

### For Fast Operations (like metadata queries)

```rust
DbRetryConfig::fast()  // 3 attempts, 50-500ms, 2.0x backoff
```

**Rationale:**
- Quick failures for user-facing queries
- Faster backoff progression
- Lower max delay for responsiveness

## Implementation Details

### Retry Loop Flow

```
Initial attempt
├── If success → Return (attempts = 1, no duration)
└── If error:
    ├── Check if retryable
    ├── If not retryable → Return error immediately
    └── If retryable:
        ├── Check if attempts exhausted
        ├── If exhausted → Return error
        └── If more attempts available:
            ├── Calculate delay (base × backoff^N, capped at max)
            ├── Apply jitter (±10%)
            ├── Sleep for calculated duration
            └── Retry operation
```

### Error Message Format

When all retries are exhausted:

```
Database operation 'register_adapter_with_aos' failed after 3 attempts:
database is locked
```

### Statistics Tracking

Each retry operation returns:
- `attempts`: Count of how many times operation was tried
- `total_duration`: Time spent in backoff delays
- `succeeded`: Boolean success indicator
- `final_error`: The last error that occurred (if failed)

## Monitoring & Observability

### Log Parsing

Operators can find retry operations by searching logs:

```bash
# Find all retry operations
grep "Retrying database operation" logs/

# Find exhausted retries
grep "after exhausting retries" logs/

# Find specific operation retries
grep "register_adapter_with_aos" logs/
```

### Metrics to Track

Recommended metrics for monitoring retry health:
1. **Retry rate** - How often retries are triggered per operation type
2. **Retry success rate** - What % of retries succeed vs fail
3. **Average retry attempts** - Mean attempts before success
4. **Total retry time** - Aggregate time spent retrying
5. **Transient error rate** - Frequency of different error types

## Edge Cases & Limitations

### Edge Case 1: Cascading Timeouts

**Issue:** If a retry's sleep completes but the operation still times out, we retry again.

**Mitigation:** Set `max_delay` appropriately. Default 10s is safe for database operations.

### Edge Case 2: Thundering Herd

**Issue:** Multiple clients retry simultaneously at the same interval.

**Mitigation:** Jitter enabled by default spreads requests over time.

### Edge Case 3: Slow Database Recovery

**Issue:** Database still locked after max_attempts.

**Mitigation:** Adjust `max_attempts` and `max_delay` based on expected recovery time.

### Edge Case 4: Permanent vs Transient Distinction

**Issue:** Some errors could be either transient or permanent.

**Mitigation:** Conservative approach - only retry well-known transient errors. When in doubt, don't retry.

## Files Modified

### Core Implementation
- **Created:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/db_retry.rs` (362 lines)
  - Retry logic with exponential backoff
  - Error classification
  - Configuration presets
  - Inline unit tests

- **Modified:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/lib.rs`
  - Added `pub mod db_retry;` export

- **Modified:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/aos_upload.rs`
  - Added import: `use crate::db_retry::{DbRetryConfig, retry_db_simple};`
  - Added `use std::time::Duration;`
  - Wrapped `register_adapter_with_aos()` call with retry logic (~35 lines)

### Testing
- **Created:** `/Users/star/Dev/aos/crates/adapteros-server-api/tests/db_retry_tests.rs` (360+ lines)
  - 18+ comprehensive test cases
  - Integration tests with simulated transient failures
  - Config preset validation
  - Error classification verification

## Future Enhancements

1. **Metrics Export** - Add Prometheus metrics for retry operations
2. **Circuit Breaker Integration** - Stop retrying if error rate exceeds threshold
3. **Adaptive Backoff** - Adjust delays based on observed error patterns
4. **Dead Letter Queue** - Capture failed operations for later analysis
5. **Per-Operation Configuration** - Allow handlers to customize retry behavior
6. **Distributed Tracing** - Correlate retries across service calls

## Migration Notes

For existing code using direct database calls:

**Before:**
```rust
let result = db.register_adapter_with_aos(params).await?;
```

**After:**
```rust
let retry_config = DbRetryConfig::default();
let db = state.db.clone();
let params_clone = params.clone();

let result = retry_db_simple(
    &retry_config,
    "operation_name",
    || {
        let db = db.clone();
        let params = params_clone.clone();
        Box::pin(async move { db.register_adapter_with_aos(params).await })
    },
)
.await?;
```

## Verification Checklist

- [x] Retry logic distinguishes transient from permanent errors
- [x] Exponential backoff implemented correctly
- [x] Jitter applied to prevent thundering herd
- [x] Max attempts respected
- [x] Logging at appropriate levels (info/debug/warn/error)
- [x] Configuration is flexible and preset-friendly
- [x] Integration with aos_upload handler
- [x] Comprehensive unit tests
- [x] Integration tests with simulated failures
- [x] Documentation complete

## Related Documentation

- [Database Reference](docs/DATABASE_REFERENCE.md)
- [Error Handling Standards](CLAUDE.md#error-handling)
- [Logging Standards](CLAUDE.md#logging-use-tracing-never-println)

---

**Implementation Date:** 2025-11-19
**Agent:** Agent 4 of 15 (PRD-2 fixes)
**Status:** Complete
