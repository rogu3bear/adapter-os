---
phase: 51-adapter-inference-end-to-end-activation
plan: 01
status: complete
---

# Plan 51-01: Wire UDS Adapter Commands

## What Changed

Closed the central gap where adapter swaps updated DB state but never reached the GPU. Three changes wire the API's swap handler and streaming inference to the worker's HotSwapManager over UDS.

### Task 1: UdsClient::send_adapter_command_json
Added a new method to `UdsClient` that sends a JSON-serialized `AdapterCommand` to the worker's `POST /adapter/command` UDS route and parses the response as `AdapterCommandResult`. This bridges the API crate's UDS client to the worker's existing adapter command dispatch.

### Task 2: Swap handler UDS dispatch
Modified `swap_adapters` to send `AdapterCommand::Swap` over UDS after lifecycle manager updates succeed. Also replaced the silent DB-only fallback (when lifecycle_manager is absent) with a 503 error — fail closed, not silent degradation.

### Task 3: Preload-before-stream
Added `preload_adapters_for_inference()` to streaming_infer.rs. Called after effective_adapter_ids are resolved but before the SSE stream starts. If preload fails, returns HTTP error before stream begins (fail fast).

## Key Files

- `crates/adapteros-server-api/src/uds_client.rs` — added `send_adapter_command_json` method
- `crates/adapteros-server-api/src/handlers/adapters/swap.rs` — added UDS dispatch, fail-closed lifecycle_manager
- `crates/adapteros-server-api/src/handlers/streaming_infer.rs` — added `preload_adapters_for_inference` + call site

## Verification

- `cargo check -p adapteros-server-api` — clean compile
- No new dependencies introduced
- Import: `AdapterCommand` and `AdapterCommandResult` from `adapteros_lora_worker` (existing dependency)

## Deviations

- Used `InferenceCore::new(&state)` (borrow) instead of `InferenceCore::new(state.clone())` (clone) per the existing API convention
- `streaming_infer_with_progress` handler already handles adapter loading as part of its stream (emitting Loading events), so the explicit preload was only added to the main `streaming_infer` handler
