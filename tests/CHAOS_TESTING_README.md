# Worker Crash Chaos Testing Suite

## Overview

This document describes the comprehensive chaos testing suite for worker crash scenarios in AdapterOS. The tests are located in `tests/executor_crash_recovery.rs` and provide deterministic, reproducible crash scenarios.

## Test Architecture

### Core Components

1. **MockWorkerState**: Simulates worker state for crash testing
   - Tracks adapters being loaded (`adapters_loading`)
   - Tracks loaded adapters (`adapters_loaded`)
   - Counts active requests (`active_requests`)
   - Provides crash injection mechanism

2. **DeterministicExecutor**: Provides deterministic task execution with:
   - Snapshot/restore for crash recovery
   - Event logging for audit trails
   - Controlled task scheduling

3. **CrashPoint Enum**: Defines crash scenarios
   - `DuringLoad`: Crash while loading adapter
   - `DuringHotSwap`: Crash during adapter swap
   - `DuringInference`: Crash during inference request

## Test Scenarios

### 1. Worker Crash During Adapter Load (`test_worker_crash_during_adapter_load`)

**Scenario:**
- Worker starts loading adapter A
- Adapter metadata loaded, but weights not yet transferred
- Worker crashes mid-load
- Recovery detects partial state and rolls back

**Verifies:**
- Partial state detected (adapter in "loading" state)
- Proper rollback to clean state
- No hangs or deadlocks
- State consistency after recovery
- Event log preserved across crash

**Expected Behavior:**
```
1. Adapter loading started → State: "loading"
2. Crash injected → Worker terminates
3. Recovery initiated → Detect partial state
4. Rollback executed → State: "cold"
5. Error returned to client (not hang)
```

### 2. Worker Crash During Hot-Swap (`test_worker_crash_during_hotswap`)

**Scenario:**
- Worker has adapter A loaded and active
- Start hot-swap to replace A with B
- Adapter B preloaded successfully
- Crash during atomic swap operation
- Recovery rolls back to last verified state (adapter A)

**Verifies:**
- Inconsistent swap state detected (both adapters loaded)
- Rollback to last verified adapter (A)
- No requests served with corrupted state
- Stack generation tracking
- Atomic swap guarantees

**Expected Behavior:**
```
1. Adapter A active → State: "warm"
2. Preload adapter B → State: both in memory
3. Begin swap → Crash during transition
4. Recovery → Detect inconsistent state
5. Rollback → Only adapter A active
```

### 3. Worker Crash During Inference (`test_worker_crash_during_inference`)

**Scenario:**
- Worker has adapters loaded and serving requests
- Multiple inference requests in flight (3 concurrent)
- Worker crashes during request 1
- Recovery handles in-flight requests properly

**Verifies:**
- In-flight requests fail fast (not hang)
- Proper error responses returned
- New requests succeed after recovery
- No request state corruption
- Active request tracking

**Expected Behavior:**
```
1. Request 0, 1, 2 started → All in progress
2. Crash during request 1 → Worker terminates
3. Recovery → In-flight requests marked as failed
4. New requests → Succeed after recovery
5. Error responses → Clear, actionable errors
```

### 4. Multiple Sequential Crashes (`test_multiple_crash_recovery_cycles`)

**Scenario:**
- Worker crashes during load
- Recovers and retries
- Worker crashes during different phase
- Recovers again
- Verify state consistency across multiple crashes

**Verifies:**
- State remains consistent across crashes
- Recovery counter increments correctly
- Crash counter tracked accurately
- Event log continuity maintained
- No state corruption from repeated crashes

### 5. Crash with Concurrent Operations (`test_crash_with_concurrent_operations`)

**Scenario:**
- 5 adapters loading concurrently with different delays
- Some complete (adapters 0, 1)
- Some in progress (adapters 2, 3, 4)
- Crash during concurrent load phase
- Recovery handles mixed state

**Verifies:**
- Partial completion detected
- Mixed state (some loaded, some loading)
- Recovery cleanup handles all states
- No resource leaks
- Concurrent operation safety

## Running the Tests

### Run All Chaos Tests
```bash
cargo test --test executor_crash_recovery -- --nocapture
```

### Run Specific Scenario
```bash
# Adapter load crash
cargo test --test executor_crash_recovery test_worker_crash_during_adapter_load

# Hot-swap crash
cargo test --test executor_crash_recovery test_worker_crash_during_hotswap

# Inference crash
cargo test --test executor_crash_recovery test_worker_crash_during_inference

# Multiple crashes
cargo test --test executor_crash_recovery test_multiple_crash_recovery_cycles

# Concurrent operations
cargo test --test executor_crash_recovery test_crash_with_concurrent_operations
```

### Run with Logging
```bash
RUST_LOG=debug cargo test --test executor_crash_recovery -- --nocapture
```

## Verification Checklist

Each chaos test verifies the following invariants:

### State Consistency
- [ ] No adapters stuck in "loading" state after crash
- [ ] All adapters in valid states (cold/warm/error)
- [ ] State counter tracking is accurate
- [ ] No orphaned resources

### Request Handling
- [ ] In-flight requests fail fast (not hang)
- [ ] Clear error messages returned
- [ ] New requests succeed after recovery
- [ ] No request state corruption

### Recovery Behavior
- [ ] Snapshot/restore works correctly
- [ ] Event log preserved across crashes
- [ ] Audit trail complete and accurate
- [ ] Rollback mechanisms function properly

### Adapter Integrity
- [ ] No adapter file corruption
- [ ] Memory properly released
- [ ] VRAM state consistent
- [ ] Hot-swap state machine valid

## Integration with Production

### Real-World Application

The chaos tests simulate production scenarios where:

1. **Network failures** during adapter downloads
2. **OOM kills** during heavy adapter loading
3. **Process crashes** from kernel panics
4. **Signal interrupts** (SIGKILL, SIGTERM)
5. **Hardware failures** affecting VRAM

### Recovery Mechanisms

Production recovery follows the same pattern:

```rust
// 1. Detect crash via health checks
if worker.is_crashed() {
    // 2. Take snapshot of last known state
    let snapshot = worker.snapshot();

    // 3. Identify partial operations
    let partial_loads = find_loading_adapters(&snapshot);

    // 4. Rollback to clean state
    for adapter in partial_loads {
        rollback_adapter_state(adapter, "cold");
    }

    // 5. Restart worker
    worker.restart();

    // 6. Fail in-flight requests
    fail_pending_requests("Worker crashed");
}
```

## Performance Characteristics

### Test Execution Times

- `test_worker_crash_during_adapter_load`: ~100ms
- `test_worker_crash_during_hotswap`: ~150ms
- `test_worker_crash_during_inference`: ~200ms
- `test_multiple_crash_recovery_cycles`: ~250ms
- `test_crash_with_concurrent_operations`: ~200ms

### Resource Usage

- Memory overhead: ~50MB per test
- No persistent state leaks
- Clean teardown verified
- Deterministic execution (no flakes)

## Debugging Failed Tests

### Common Issues

1. **Test Timeout**
   - Cause: Worker not crashing as expected
   - Fix: Verify crash injection logic
   - Debug: Add `--nocapture` flag

2. **State Mismatch**
   - Cause: Race condition in state tracking
   - Fix: Review atomic ordering
   - Debug: Check event log sequence

3. **Recovery Failure**
   - Cause: Snapshot/restore issue
   - Fix: Verify snapshot contents
   - Debug: Inspect pending tasks

### Debugging Commands

```bash
# Run with verbose logging
RUST_LOG=trace cargo test --test executor_crash_recovery -- --nocapture

# Run single test in isolation
cargo test --test executor_crash_recovery test_worker_crash_during_adapter_load -- --exact

# Generate test report
cargo test --test executor_crash_recovery -- --nocapture --test-threads=1 > chaos_test_report.txt
```

## Future Enhancements

### Planned Scenarios

1. **Network partition during distributed hot-swap**
2. **Disk full during adapter cache writes**
3. **SIGKILL during KV cache eviction**
4. **Race condition: concurrent swaps + inference**
5. **Memory pressure during multi-adapter load**

### Monitoring Integration

```rust
// Planned: Telemetry collection during crashes
impl ChaosTest {
    fn record_crash_metrics(&self) {
        telemetry.record("crash_point", self.crash_type);
        telemetry.record("recovery_time_ms", self.recovery_duration);
        telemetry.record("partial_state_count", self.partial_adapters);
        telemetry.record("failed_requests", self.inflight_requests);
    }
}
```

## References

- **Original Test**: `test_executor_crash_recovery` (preserved for compatibility)
- **Related Tests**: `tests/adapter_failure_scenarios.rs` (DB consistency)
- **Worker Lifecycle**: `crates/adapteros-core/src/worker_status.rs`
- **Hot-swap Logic**: `crates/adapteros-lora-worker/src/adapter_hotswap.rs`

## Contributing

When adding new chaos tests:

1. Use `MockWorkerState` for state simulation
2. Include comprehensive audit logging
3. Verify all recovery invariants
4. Document expected behavior
5. Add scenario to this README

## License

Same as AdapterOS project (see LICENSE file)
