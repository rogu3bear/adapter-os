---
phase: 51-adapter-inference-end-to-end-activation
plan: 02
status: complete
---

# Plan 51-02: Adapter Command Integration Tests

## What Changed

Added integration tests verifying serialization compatibility between the API client's `AdapterCommand` construction and the worker's UDS server parser.

### Worker tests (hotswap_uds_integration.rs)
6 tests covering all `AdapterCommand` variants (Swap, Preload, Rollback, VerifyStack) plus `AdapterCommandResult` deserialization. Verifies tagged JSON format matches `#[serde(tag = "type", rename_all = "snake_case")]`.

### Server-API tests (adapter_swap_uds_test.rs)
5 tests covering the exact command formats the swap handler and preload function construct. Also tests edge cases: multi-adapter swap, minimal worker response, reject_reason field.

## Key Files

- `crates/adapteros-lora-worker/tests/hotswap_uds_integration.rs` — 6 serialization round-trip tests
- `crates/adapteros-server-api/tests/adapter_swap_uds_test.rs` — 5 command format compatibility tests

## Verification

- `cargo test -p adapteros-lora-worker --test hotswap_uds_integration` — 6/6 pass
- `cargo test -p adapteros-server-api --test adapter_swap_uds_test` — 5/5 pass
