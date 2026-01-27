# Ignored Tests Audit

This document tracks all `#[ignore]` tests in the codebase, their status, and remediation plans.

## Summary

| Category | Count | Status |
|----------|-------|--------|
| CLI Help (commands not wired) | 0 | Resolved |
| FFI/API Updates | 4 | Blocked - requires API changes |
| Hardware Dependent | 12 | Expected - run with `--ignored` |
| Timing Tests | 1 | Intentional - local investigation only |
| **Quick-wins** | 2 | Ready to enable |

## Quick-Win Tests (P1)

These tests can be enabled with minimal effort:

### 1. Enable MLX E2E with Feature Flag
**File**: `crates/adapteros-lora-mlx-ffi/tests/e2e_workflow_tests.rs`
**Tracking**: STAB-IGN-0038
**Status**: Ready - just needs `--features mlx` flag
**Action**: Run with `cargo test --features mlx -- --ignored`

### 2. Quantization Accuracy Tests
**File**: `crates/adapteros-lora-mlx-ffi/tests/quantization_accuracy_tests.rs`
**Status**: May be working now - needs verification
**Action**: Re-test with current codebase

## CLI Help Tests (Resolved)

All CLI help coverage tests are now enabled in `crates/adapteros-cli/tests/cli_help.rs`.

## FFI/API Update Tests (Blocked)

| Test | File | Tracking | Issue |
|------|------|----------|-------|
| Memory management | mlx-ffi | STAB-IGN-0040 | memory module functions missing |
| Generate method | model_loading_tests | STAB-IGN-0041 | MockMLXFFIModel incomplete |
| Embedding model | model_loading_tests | STAB-IGN-0042 | Requires model files |
| Embedding model 2 | model_loading_tests | STAB-IGN-0043 | Requires model files |

## Hardware Dependent Tests (Intentional)

These tests require actual hardware/models and should remain ignored in CI:

| Category | Count | Run Command |
|----------|-------|-------------|
| Memory pool (GPU) | 3 | `cargo test --features mlx -- --ignored` |
| Resilience (model) | 3 | `cargo test --features mlx -- --ignored` |
| Model loading | 2 | `cargo test --features mlx -- --ignored` |
| Performance regression | 1 | Local benchmarks only |
| Metal heap | 3 | Hardware tests |

## Timing Tests (Intentional)

| Test | File | Purpose |
|------|------|---------|
| `timing_probe_local_only` | auth_security_fixes_test.rs | Local timing investigation |

**Resolution**: Keep ignored - designed for local investigation only

## Remediation Tracking

### Phase 1: Quick-Wins (This PR)
- [x] Document all ignored tests
- [x] Categorize by remediation effort
- [x] Create tracking codes for untracked tests

### Phase 2: CLI Wiring (Future PR)
- [x] Wire `telemetry verify` command
- [x] Wire `manual` command
- [x] Add Examples section to `infer` help

### Phase 3: FFI Exports (Future PR)
- [x] Export TensorBridgeType
- [x] Export attention module
- [ ] Export memory module functions

## Test Commands

```bash
# Run all ignored tests (hardware required)
cargo test --workspace -- --ignored

# Run MLX-dependent ignored tests
cargo test --features mlx -- --ignored

# Run extended test suite
cargo test --workspace --features extended-tests
```
