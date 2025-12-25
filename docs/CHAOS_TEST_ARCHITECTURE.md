# Worker Crash Chaos Test Architecture

## System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                  Chaos Test Framework                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────────┐    ┌──────────────────┐              │
│  │ MockWorkerState  │    │ Deterministic    │              │
│  │                  │    │ Executor         │              │
│  │ - adapters_loaded│    │ - snapshot()     │              │
│  │ - adapters_loading│   │ - restore()      │              │
│  │ - active_requests│    │ - event_log()    │              │
│  │ - crash_injected │    │ - delay()        │              │
│  └──────────────────┘    └──────────────────┘              │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Crash Scenario Testing Matrix

| Crash Point        | Partial State | In-Flight Ops | Recovery Action      | Test Coverage |
|-------------------|---------------|---------------|---------------------|---------------|
| During Load       | ✓ Yes         | ✗ No          | Rollback to cold    | ✓ Covered     |
| During Hot-Swap   | ✓ Yes         | ✓ Maybe       | Rollback to prev    | ✓ Covered     |
| During Inference  | ✗ No          | ✓ Yes         | Fail inflight reqs  | ✓ Covered     |
| Concurrent Ops    | ✓ Yes         | ✓ Yes         | Mixed recovery      | ✓ Covered     |
| Sequential Crashes| ✓ Varies      | ✓ Varies      | Multi-cycle recovery| ✓ Covered     |

## Test Flow Diagrams

### 1. Adapter Load Crash

```
Start Loading Adapter
         ↓
   ┌─────────────┐
   │ State: COLD │
   └─────────────┘
         ↓
   ┌─────────────┐
   │State:LOADING│
   └─────────────┘
         ↓
    [Load Metadata]
         ↓
    [Transfer Weights] ← ⚠️ CRASH HERE
         ↓
   ┌─────────────┐
   │ CRASHED     │
   └─────────────┘
         ↓
   [Recovery Detects Partial State]
         ↓
   ┌─────────────┐
   │ Rollback    │
   │ to COLD     │
   └─────────────┘
         ↓
   ✓ Clean State
```

### 2. Hot-Swap Crash

```
Adapter A Active
         ↓
   ┌─────────────┐
   │  A: WARM    │
   └─────────────┘
         ↓
   [Preload B]
         ↓
   ┌─────────────┐
   │ A:WARM      │
   │ B:STAGED    │
   └─────────────┘
         ↓
   [Begin Swap]
         ↓
   [Atomic Pointer Flip] ← ⚠️ CRASH HERE
         ↓
   ┌─────────────┐
   │ CRASHED     │
   │ A,B:LOADED  │ (Inconsistent!)
   └─────────────┘
         ↓
   [Recovery Detects]
         ↓
   ┌─────────────┐
   │ Rollback    │
   │ to A only   │
   └─────────────┘
         ↓
   ✓ A Active, B Unloaded
```

### 3. Inference Crash

```
Requests In-Flight
         ↓
   ┌─────────────┐
   │ Req 0: RUN  │
   │ Req 1: RUN  │ ← ⚠️ CRASH DURING REQ 1
   │ Req 2: RUN  │
   └─────────────┘
         ↓
   [Worker Terminates]
         ↓
   ┌─────────────┐
   │ Req 0: ???  │
   │ Req 1: ???  │
   │ Req 2: ???  │
   └─────────────┘
         ↓
   [Recovery]
         ↓
   ┌─────────────┐
   │ Req 0: FAIL │ (Error: Worker crashed)
   │ Req 1: FAIL │
   │ Req 2: FAIL │
   └─────────────┘
         ↓
   [New Requests]
         ↓
   ✓ New Req 3: SUCCESS
```

## State Machine

```
         ┌──────────┐
         │  COLD    │ ◄──────────────┐
         └──────────┘                 │
              │                       │
              │ start_load            │ crash
              ↓                       │ recovery
         ┌──────────┐                 │
    ┌──► │ LOADING  │ ────────────────┘
    │    └──────────┘
    │         │
    │         │ finish_load
    │         ↓
    │    ┌──────────┐
    │    │  WARM    │
    │    └──────────┘
    │         │
    │         │ start_swap
    │         ↓
    │    ┌──────────┐        crash
    │    │ SWAPPING │ ──────────────┐
    │    └──────────┘               │
    │         │                     │
    │         │ complete_swap       │
    │         ↓                     ↓
    │    ┌──────────┐          ┌──────────┐
    └────│  WARM    │          │ CRASHED  │
         │(new stack)│          └──────────┘
         └──────────┘               │
                                    │ recovery
                                    ↓
                              ┌──────────┐
                              │ ROLLBACK │
                              └──────────┘
```

## Recovery Guarantees

### Invariants Maintained

1. **State Consistency**
   - No adapters stuck in transitional states
   - All adapters in valid lifecycle states
   - State transitions follow allowed paths

2. **Request Safety**
   - In-flight requests fail fast (< 5 seconds)
   - Clear error messages returned
   - No request hangs or timeouts

3. **Data Integrity**
   - No adapter file corruption
   - Memory properly released
   - VRAM state consistent

4. **Audit Trail**
   - Event log preserved across crashes
   - State transitions logged
   - Recovery actions recorded

### Recovery Algorithm

```rust
fn recover_from_crash(snapshot: Snapshot) -> Result<()> {
    // 1. Detect crash point
    let crash_point = identify_crash_point(&snapshot);

    // 2. Find partial operations
    let partial_ops = find_partial_operations(&snapshot);

    // 3. Rollback based on crash point
    match crash_point {
        CrashPoint::DuringLoad => {
            rollback_loading_adapters(&partial_ops);
        }
        CrashPoint::DuringHotSwap => {
            rollback_to_last_verified_stack(&partial_ops);
        }
        CrashPoint::DuringInference => {
            fail_inflight_requests(&partial_ops);
        }
    }

    // 4. Verify state consistency
    verify_state_consistency()?;

    // 5. Resume normal operations
    resume_operations()?;

    Ok(())
}
```

## Test Execution Model

```
┌─────────────────────────────────────────┐
│  Test Phase 1: Normal Operation         │
├─────────────────────────────────────────┤
│  - Start operation (load/swap/inference)│
│  - Execute partial work                 │
│  - Inject crash signal                  │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│  Test Phase 2: Crash Simulation         │
├─────────────────────────────────────────┤
│  - Abort executor task                  │
│  - Take state snapshot                  │
│  - Record event log                     │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│  Test Phase 3: Verification             │
├─────────────────────────────────────────┤
│  - Verify partial state detected        │
│  - Verify crash recorded in logs        │
│  - Verify audit trail complete          │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│  Test Phase 4: Recovery                 │
├─────────────────────────────────────────┤
│  - Create new executor                  │
│  - Restore from snapshot                │
│  - Execute recovery logic               │
│  - Spawn recovery tasks                 │
└─────────────────────────────────────────┘
              ↓
┌─────────────────────────────────────────┐
│  Test Phase 5: Post-Recovery Validation │
├─────────────────────────────────────────┤
│  - Verify state consistency             │
│  - Verify rollback successful           │
│  - Verify new operations work           │
│  - Verify event log continuity          │
└─────────────────────────────────────────┘
```

## Metrics and Observability

### Test Metrics Collected

- **Crash Detection Time**: Time to detect crash < 100ms
- **Recovery Time**: Time to complete recovery < 1s
- **State Consistency**: 100% of tests maintain consistency
- **Request Failure Time**: In-flight requests fail < 5s
- **Event Log Completeness**: 100% of events preserved

### Telemetry Integration

```rust
// Example telemetry during chaos test
telemetry.record_crash_event(CrashEvent {
    crash_point: "during_adapter_load",
    timestamp: Instant::now(),
    partial_state: vec!["adapter-123"],
    active_requests: 0,
    recovery_action: "rollback_to_cold",
});
```

## Integration Testing

### E2E Crash Scenarios

Beyond unit-level chaos tests, E2E scenarios include:

1. **Network partition during distributed swap**
2. **OOM kill during multi-adapter load**
3. **SIGTERM during KV cache operations**
4. **Disk full during adapter cache write**
5. **Clock skew during deterministic routing**

### Production Readiness

The chaos tests ensure production readiness by:

- ✓ Simulating real-world crash conditions
- ✓ Verifying recovery under various states
- ✓ Testing error propagation to clients
- ✓ Validating state consistency guarantees
- ✓ Ensuring no resource leaks

## Running Tests

### Quick Start

```bash
# Run all chaos tests
./scripts/run_chaos_tests.sh

# Run specific scenario
cargo test --test executor_crash_recovery test_worker_crash_during_adapter_load

# Run with verbose output
RUST_LOG=debug cargo test --test executor_crash_recovery -- --nocapture
```

### CI/CD Integration

```yaml
# Example .github/workflows/chaos-tests.yml
name: Chaos Tests
on: [push, pull_request]
jobs:
  chaos:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run chaos tests
        run: ./scripts/run_chaos_tests.sh
      - name: Upload report
        if: always()
        uses: actions/upload-artifact@v2
        with:
          name: chaos-test-report
          path: var/chaos_test_reports/
```

## Future Enhancements

### Planned Features

1. **Fault injection framework** for arbitrary crash points
2. **Distributed chaos** testing across multiple workers
3. **Performance chaos** (high memory pressure, slow I/O)
4. **Byzantine failures** (corrupted messages, time skew)
5. **Automated chaos in production** (controlled rollout)

### Research Areas

- Deterministic replay of production crashes
- ML-based crash prediction
- Automated recovery strategy synthesis
- Chaos engineering metrics dashboard

## References

- Test Implementation: `tests/executor_crash_recovery.rs`
- Worker State Machine: `crates/adapteros-core/src/worker_status.rs`
- Hot-Swap Logic: `crates/adapteros-lora-worker/src/adapter_hotswap.rs`
- Supervisor Recovery: `crates/adapteros-orchestrator/src/supervisor.rs`
