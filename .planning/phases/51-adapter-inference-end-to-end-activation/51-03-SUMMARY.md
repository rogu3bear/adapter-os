---
phase: 51-adapter-inference-end-to-end-activation
plan: 03
status: complete
---

# Plan 51-03: Training Round-Trip Test and Requirements Registration

## What Changed

### Task 1: Round-trip integration test
Created `e2e_adapter_round_trip.rs` with 3 tests:

1. **test_packager_produces_valid_aos** — trains on CPU, quantizes, packages, verifies .aos file exists with correct manifest metadata
2. **test_full_round_trip_train_package_swap** — full lifecycle: train → quantize → package → preload into AdapterTable → swap → verify stack hash changes → verify adapter in active set
3. **test_swap_changes_stack_hash** — verifies stack hash is deterministic and changes predictably with add/remove operations

### Task 2: Requirements registration
Added INF-51-01, INF-51-02, TRN-51-01, TRN-51-02 to REQUIREMENTS.md under "Adapter Inference Activation" section. Updated traceability table and coverage count (7 → 11).

## Key Files

- `crates/adapteros-lora-worker/tests/e2e_adapter_round_trip.rs` — 3 round-trip tests
- `.planning/REQUIREMENTS.md` — 4 new requirements + traceability

## Verification

- `cargo test -p adapteros-lora-worker --test e2e_adapter_round_trip` — 3/3 pass
- All 4 requirement IDs present in REQUIREMENTS.md traceability table
