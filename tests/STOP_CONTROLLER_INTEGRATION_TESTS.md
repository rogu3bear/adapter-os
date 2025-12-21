# Stop Controller Inference Integration Tests

## Overview

The file `tests/stop_controller_inference_integration.rs` contains comprehensive integration tests for the stop controller during actual inference operations.

## Test Coverage

### 1. Stop Reason Persistence Tests

These tests verify that stop decisions are correctly persisted to inference receipts:

- **test_stop_controller_budget_max_persisted_to_receipt**: Verifies BUDGET_MAX stop reason is persisted when token budget is exceeded
- **test_stop_controller_completion_confident_persisted**: Verifies COMPLETION_CONFIDENT stop reason is persisted when EOS probability exceeds threshold
- **test_stop_controller_repetition_guard_persisted**: Verifies REPETITION_GUARD stop reason is persisted when n-gram repetition is detected
- **test_stop_controller_length_eos_persisted**: Verifies LENGTH stop reason is persisted when EOS token is encountered

Each test:
- Creates a specific stop policy
- Simulates token generation until stop condition
- Persists the trace to an in-memory database
- Verifies stop_reason_code, stop_reason_token_index, and stop_policy_digest_b3 fields in the receipt

### 2. Determinism Tests

These tests verify that stop controller behavior is deterministic:

- **test_determinism_same_policy_same_receipt_digest**: Verifies that identical policies and token sequences produce identical receipt digests
- **test_different_policies_different_digests**: Verifies that different policies produce different policy digests
- **test_stop_policy_digest_committed_to_merkle_bundle**: Verifies that the stop policy digest is included in the receipt digest (Merkle bundle commitment)

### 3. Policy Override Tests

- **test_stop_policy_override_from_request**: Verifies that custom stop policies from requests override default policies
- **test_stop_decision_token_index_accuracy**: Verifies that the token index in stop decisions is accurate

### 4. Comprehensive Trigger Tests

- **test_all_stop_reasons_trigger_correctly_in_integration**: Verifies all four stop reasons (BUDGET_MAX, COMPLETION_CONFIDENT, REPETITION_GUARD, LENGTH) can be triggered correctly

## Test Infrastructure

### InferenceSimulation Struct

A helper struct that simulates token generation with stop controller integration:

```rust
struct InferenceSimulation {
    controller: StopController,
    eos_token_id: u32,
    vocab_size: usize,
}
```

Methods:
- `new()`: Creates a new simulation with a given stop policy
- `generate_until_stop()`: Generates tokens until a stop condition is met
- `policy_digest()`: Returns the BLAKE3 digest of the stop policy

### Database Setup

The `init_test_db()` function creates an in-memory SQLite database with the necessary schema:
- `inference_traces`: Main trace table
- `inference_trace_tokens`: Token-level routing decisions
- `inference_trace_receipts`: Final receipts with stop fields

### Helper Functions

- `make_token_input()`: Creates TraceTokenInput for token recording

## Stop Fields in Receipts

The tests verify that three stop-related fields are correctly persisted:

1. **stop_reason_code**: TEXT field containing the stop reason (e.g., "BUDGET_MAX", "LENGTH")
2. **stop_reason_token_index**: INTEGER field containing the token index where stop occurred
3. **stop_policy_digest_b3**: BLOB field containing the BLAKE3 digest of the stop policy

These fields are:
- Included in the receipt digest calculation (Merkle commitment)
- Deterministically serialized
- Nullable (None is represented as empty string, 0xFFFFFFFF, or zero bytes)

## Running the Tests

```bash
# Run all stop controller integration tests
cargo test --test stop_controller_inference_integration

# Run a specific test
cargo test --test stop_controller_inference_integration test_stop_controller_budget_max_persisted_to_receipt

# Run with output
cargo test --test stop_controller_inference_integration -- --nocapture
```

## Design Principles

1. **Determinism**: All stop decisions must be deterministic given the same inputs
2. **Auditability**: Stop policy digest is committed to Merkle bundle
3. **Completeness**: All four stop reasons are tested
4. **Integration**: Tests use actual TraceSink/TraceFinalization infrastructure
5. **Repeatability**: Same policy + same tokens = same receipt digest

## Related Files

- `crates/adapteros-lora-worker/src/stop_controller.rs`: StopController implementation
- `crates/adapteros-lora-worker/tests/stop_controller_tests.rs`: Unit tests for StopController
- `crates/adapteros-db/src/inference_trace.rs`: Trace persistence with stop fields
- `crates/adapteros-api-types/src/inference.rs`: StopPolicySpec and StopReasonCode types
- `tests/determinism_smoke.rs`: Similar determinism testing patterns

## Future Enhancements

Potential areas for expansion:
- End-to-end tests with actual worker processes
- Tests for stop policy serialization in replay metadata
- Tests for stop policy enforcement in streaming inference
- Tests for stop policy validation and error handling
