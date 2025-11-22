# Testing Coverage Gaps and Limitations

## Overview

This document identifies corners cut and gaps in the stress testing and failure scenario test implementation.

## Critical Gaps

### 1. **No HTTP Handler Integration Testing**

**Issue**: Tests call `AdapterLoader` directly, bypassing the actual HTTP handlers.

**Missing Coverage**:
- No testing through `POST /v1/adapters/{id}/load` endpoint
- Missing authentication/authorization checks
- Missing request validation
- Missing HTTP error responses
- Missing telemetry/operation progress events

**Impact**: Race conditions and state checks in handlers are not tested.

**Why Cut**: Requires full server setup with auth, which is complex for unit tests.

**Should Fix**: Add integration tests using `axum_test` or similar.

### 2. **Missing State Check Before Load**

**Issue**: The actual `load_adapter` handler (line 6435-6439) does NOT check if adapter is already in "loading" state before starting.

**Missing Code**:
```rust
// ACTUAL CODE (has race condition):
state.db.update_adapter_state(&adapter_id, "loading", "user_request").await?;
// ... then loads

// SHOULD BE (like model handler):
match current_state.as_str() {
    "loading" => return Err(CONFLICT),
    "warm" => return Err(CONFLICT),
    // ...
}
```

**Impact**: Multiple concurrent loads can race and both proceed.

**Why Cut**: This is actually a **BUG IN PRODUCTION CODE** that should be fixed first.

**Should Fix**: Add state check in handler, then test it.

### 3. **No OperationTracker for Adapters**

**Issue**: Model handlers use `OperationTracker` to prevent concurrent operations, but adapter handlers don't.

**Missing Coverage**:
- No conflict detection for concurrent operations
- No operation deduplication
- No timeout handling at handler level

**Impact**: Unlike models, adapters can have concurrent operations.

**Why Cut**: Adapter handlers don't implement this, so can't test what doesn't exist.

**Should Fix**: Add OperationTracker to adapter handlers (like models have).

### 4. **No Database Transaction Testing**

**Issue**: Tests use individual DB calls, not transactions.

**Missing Coverage**:
- Atomic state updates
- Transaction rollback on failure
- Concurrent transaction handling

**Impact**: Real handler may use transactions, but we don't test that path.

**Why Cut**: Handler code doesn't clearly show transaction usage for adapters (unlike models).

**Should Fix**: Verify if handlers use transactions, add if missing, then test.

### 5. **Simplified MockAdapterLoader**

**Issue**: `MockAdapterLoader` doesn't fully match `AdapterLoader` interface.

**Missing**:
- No `resolve_path` security validation
- No memory-mapped loading path
- No hot-swap support
- Simplified error handling

**Impact**: Mock may not catch real-world failure scenarios.

**Why Cut**: Full mock would be complex and error-prone.

**Should Fix**: Use dependency injection or trait-based mocking.

### 6. **No LifecycleManager Integration**

**Issue**: Handler uses `LifecycleManager`, but tests don't properly test with it.

**Missing Coverage**:
- LifecycleManager lock contention
- State synchronization between LifecycleManager and DB
- LifecycleManager error paths

**Impact**: Real production path uses LifecycleManager, but we test loader directly.

**Why Cut**: LifecycleManager adds complexity and we wanted fast unit tests.

**Should Fix**: Add integration tests with full LifecycleManager setup.

### 7. **No Actual File I/O Failure Testing**

**Issue**: Tests use dummy files, don't test real I/O failures.

**Missing Coverage**:
- Disk full scenarios
- Permission denied
- File corruption
- Network filesystem failures

**Impact**: May miss real production failures.

**Why Cut**: Requires mocking filesystem or using real failures (flaky).

**Should Fix**: Use `mockall` or similar for filesystem mocking.

### 8. **No Progress Event Testing**

**Issue**: Handler emits `OperationProgressEvent` (line 6493), but tests don't verify.

**Missing Coverage**:
- Progress event emission
- Progress event ordering
- Progress event on failure

**Impact**: Telemetry may be broken and we wouldn't know.

**Why Cut**: Requires setting up event channels in tests.

**Should Fix**: Add event verification to tests.

## What We Did Right

1. **Database State Consistency**: Tests verify no adapters stuck in intermediate states
2. **Concurrent Operations**: Stress tests verify behavior under load
3. **Failure Scenarios**: Comprehensive partial failure testing
4. **Memory Tracking**: Validates memory accounting under stress
5. **State Transitions**: Tests all state transition paths

## Recommendations

### High Priority Fixes

1. **Fix Handler Race Condition**: Add state check before loading (like models)
2. **Add OperationTracker**: Prevent concurrent adapter operations
3. **Add HTTP Integration Tests**: Test through actual endpoints

### Medium Priority

4. **Add Transaction Testing**: Verify atomic operations
5. **Improve MockAdapterLoader**: Better match real interface
6. **Add LifecycleManager Tests**: Test full integration path

### Low Priority

7. **Add I/O Failure Tests**: Test filesystem edge cases
8. **Add Progress Event Tests**: Verify telemetry

## Test Quality Assessment

**Current Coverage**: ~60%
- ✅ Unit-level concurrency: Good
- ✅ State consistency: Good  
- ✅ Failure scenarios: Good
- ❌ Handler-level: Missing
- ❌ Integration: Missing
- ❌ Production paths: Partial

**Recommendation**: Add handler-level and integration tests before considering production-ready.

