# Error Recovery and Fault Tolerance Test Suite

**Location:** `/Users/star/Dev/aos/tests/error_recovery_integration.rs`

## Overview

Comprehensive chaos testing and fault tolerance validation for critical AdapterOS components. This test suite ensures proper error handling, recovery mechanisms, and state consistency across all failure scenarios.

## Test Modules

### Module 1: Circuit Breaker Error Recovery
Tests the circuit breaker pattern for service failure isolation:
- Initial state verification (closed/healthy)
- Automatic opening after failure threshold (3 failures)
- Half-open transition after timeout
- Circuit closes after successful recovery
- Metrics tracking (requests, successes, failures)
- Concurrent access patterns

**Key Tests:**
- `test_circuit_opens_on_failures` - Verifies circuit opens at threshold
- `test_half_open_transition` - Tests recovery probe mechanism
- `test_concurrent_circuit_breaker_access` - 20 concurrent tasks stress test

### Module 2: Hash Verification Failures
Tests BLAKE3 hash verification and corrupted data detection:
- Hash determinism (same data = same hash)
- Multi-part hash equivalence
- Hex encoding/decoding roundtrips
- Invalid hex string handling
- Single-bit corruption detection
- File hashing capabilities

**Key Tests:**
- `test_corrupted_data_detection` - Single byte flip detection
- `test_adapter_hash_mismatch_error` - Proper error construction
- `test_hash_file` - File-based hash verification

### Module 3: Error Type Construction
Validates all AosError variants are properly constructible and displayable:
- 19+ error variant tests
- Timeout errors with duration tracking
- Circuit breaker state errors
- Policy hash mismatches
- Feature disabled errors with alternatives
- RNG errors with seed tracking

**Key Tests:**
- `test_error_variants` - All major error types
- `test_circuit_breaker_errors` - Open/HalfOpen states
- `test_rng_error` - Determinism violation tracking

### Module 4: Error Context Chaining
Tests error context attachment and propagation:
- Single context attachment
- Multi-level context chaining
- Display formatting of chained errors
- Context preservation through layers

**Key Tests:**
- `test_context_chaining` - Nested context verification
- `test_context_display_formatting` - Error message formatting

### Module 5: Memory and Resource Errors
Tests memory pressure and resource exhaustion handling:
- Memory pressure detection (headroom < 15%)
- Resource exhaustion errors
- Unavailable service errors

**Key Tests:**
- `test_memory_pressure_error` - Headroom threshold violations
- `test_resource_exhaustion_error` - GPU memory exhaustion

### Module 6: Invalid Manifest Handling
Tests adapter manifest validation and rejection:
- Empty manifest bytes
- Missing required fields
- Invalid field types
- .aos format validation (header structure)

**Key Tests:**
- `test_aos_format_validation` - Binary format validation
- `test_manifest_missing_fields` - Required field detection

### Module 7: Concurrent Error Scenarios
Tests error handling under concurrent load:
- Error accumulation (100 concurrent tasks)
- Rapid error generation performance (10,000 errors/sec)
- Barrier-synchronized error scenarios

**Key Tests:**
- `test_error_accumulation_under_load` - 33% failure rate stress test
- `test_rapid_error_generation` - Performance validation (<1s)
- `test_barrier_synchronized_errors` - Race condition testing

### Module 8: Policy and Security Errors
Tests security-critical error paths:
- Quarantine triggers (3 consecutive violations)
- Determinism violations (thread_rng detection)
- Egress violations (production mode blocking)
- Isolation violations (tenant boundary checks)

**Key Tests:**
- `test_quarantine_error` - System quarantine on policy violations
- `test_determinism_violation` - Non-deterministic RNG detection
- `test_isolation_violation` - Tenant boundary enforcement

### Module 9: Database and Crypto Errors
Tests database and cryptographic error handling:
- Database connection failures
- Crypto signature validation
- Encryption/decryption failures
- Sealed data corruption

**Key Tests:**
- `test_encryption_errors` - Key derivation failures
- `test_sealed_data_error` - Corrupted envelope detection

### Module 10: Chaos Integration
End-to-end chaos testing scenarios:
- Cascading failure isolation (service A failure doesn't affect service B)
- Error preservation across async boundaries
- Error type conversions (std::io, serde_json)

**Key Tests:**
- `test_cascading_failure_isolation` - Multi-service failure independence
- `test_error_preservation_across_async` - Tokio task error handling

### Module 11: Metal GPU Recovery Tests (macOS only)
Tests Metal GPU error recovery mechanisms:
- Command buffer failure detection
- Panic catching and device degradation
- Recovery with buffer cleanup
- Multiple failure/recovery cycles
- Health check enforcement

**Key Tests:**
- `test_metal_command_buffer_failure_recovery` - Full recovery cycle
- `test_metal_multiple_failure_recovery_cycles` - 3 consecutive failures

**Recovery Steps Validated:**
1. Panic caught → device marked degraded
2. Health check fails while degraded
3. Buffer cleanup callback invoked
4. New command queue created
5. Test dispatch verifies functionality
6. Device unmarked as degraded
7. Subsequent operations succeed

### Module 12: Hot-Swap Quarantine Tests
Tests adapter hot-swap failure handling and quarantine logic:
- 3-failure quarantine threshold
- RCU retry count enforcement
- State consistency after rollback

**Key Tests:**
- `test_hotswap_quarantine_after_three_failures` - Quarantine trigger
- `test_hotswap_rcu_retry_enforcement` - Max retry validation

**Quarantine Workflow:**
1. Preload adapter metadata
2. Attempt swap (kernel load)
3. On failure: rollback
4. After 3 failures: quarantine (no retry)

### Module 13: Deterministic Executor Crash Recovery Tests
Tests snapshot-based crash recovery for deterministic execution:
- Snapshot creation with event log
- Seed validation on restore
- Prevent restore while running
- State restoration verification

**Key Tests:**
- `test_executor_crash_recovery_via_snapshot` - Full snapshot/restore cycle
- `test_executor_snapshot_seed_validation` - Seed mismatch rejection
- `test_executor_running_restore_prevention` - Safety validation

**Recovery Guarantees:**
- Tick counter preserved
- Event log restored
- Global seed validated
- Task queue reconstructed
- Execution continues deterministically

### Module 14: Resource Leak Detection Tests
Tests for memory and resource leaks during failures:
- Load/unload cycle leak detection
- State consistency after partial failures
- Memory accounting verification

**Key Tests:**
- `test_no_memory_leaks_load_unload_cycles` - 10 load/unload cycles
- `test_state_consistency_after_partial_failure` - Partial eviction consistency

**Leak Detection:**
- Total VRAM accounting
- Active adapter count
- Memory-mapped file cleanup
- GPU buffer deallocation

## Test Coverage Summary

| Category | Tests | Coverage |
|----------|-------|----------|
| Circuit Breakers | 6 | State transitions, metrics, concurrency |
| Hash Verification | 8 | Corruption detection, file hashing |
| Error Construction | 8 | All AosError variants |
| Error Chaining | 3 | Multi-level context |
| Memory Errors | 4 | Pressure, exhaustion, unavailability |
| Manifest Validation | 5 | Format, fields, types |
| Concurrent Errors | 3 | Load, performance, synchronization |
| Security Errors | 4 | Quarantine, determinism, isolation |
| Database/Crypto | 4 | Connection, encryption, sealing |
| Chaos Integration | 4 | Cascading failures, conversions |
| Metal GPU Recovery | 2 | Command buffer failures, recovery |
| Hot-Swap Quarantine | 2 | Failure threshold, RCU retry |
| Executor Recovery | 3 | Snapshot, validation, safety |
| Resource Leaks | 2 | Memory accounting, consistency |

**Total: 58 tests**

## Running the Tests

### All Tests
```bash
cargo test --test error_recovery_integration
```

### Specific Module
```bash
# Circuit breaker tests
cargo test --test error_recovery_integration circuit_breaker

# Metal GPU tests (macOS only)
cargo test --test error_recovery_integration metal_gpu_recovery

# Extended tests (requires feature flag)
cargo test --test error_recovery_integration --features extended-tests
```

### Chaos Mode (All Extended Tests)
```bash
cargo test --test error_recovery_integration --features extended-tests -- --nocapture
```

## Key Patterns Tested

### 1. Fault Isolation
- Circuit breakers prevent cascading failures
- Service A failure doesn't propagate to Service B
- Quarantine prevents repeated bad adapter loads

### 2. Graceful Degradation
- Metal GPU panic → device degraded → recovery
- Hot-swap failure → rollback → retry
- Executor crash → snapshot → restore

### 3. State Consistency
- Atomic swap operations
- Rollback on partial failure
- Memory accounting accuracy

### 4. Error Propagation
- Context chaining preserves full error path
- Error types preserved across async boundaries
- Actionable error messages (capitalized, specific)

### 5. Resource Safety
- No leaks during load/unload cycles
- Proper cleanup after failures
- Memory pressure triggers eviction

## Compliance

All tests ensure:
- ✅ Proper error propagation (typed errors, no panics)
- ✅ State consistency after recovery
- ✅ No resource leaks during failures
- ✅ Audit logging of failures (via AosError types)
- ✅ Deterministic recovery (executor snapshot/restore)
- ✅ Policy enforcement (quarantine, isolation)

## Future Enhancements

Potential additions for comprehensive coverage:
1. Network partition simulation (split-brain scenarios)
2. Disk full error recovery
3. OOM killer simulation
4. Power loss recovery (persistence validation)
5. Byzantine failure detection
6. Clock skew handling
7. Thermal throttling adaptation
8. GPU hang detection and recovery
