# Baseline Health Check

**Date:** 2026-01-05
**Branch:** maintenance/fix-all-issues
**Base:** main (80fca458b)

## Repository Info

- **Remote:** https://github.com/rogu3bear/adapter-os.git
- **Owner/Repo:** rogu3bear/adapter-os
- **Default Branch:** main

## Stashed Changes

Pre-existing changes were stashed:
```
Pre-maintenance-run stash
```

## CI-Equivalent Commands

### Format Check
```bash
cargo fmt --all -- --check
```
**Result:** PASS (no output = no formatting issues)

### Clippy Check
```bash
cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings
```
**Result:** FAIL

#### Errors in `crates/adapteros-config/tests/config_validation_tests.rs`:
- Lines 158-171: 16 errors for `bool_assert_comparison` - using `assert_eq!` with literal bool instead of `assert!`/`assert!(!...)`

#### Errors in `crates/adapteros-config/src/model.rs`:
- Lines 766-804: 6 errors for `field_reassign_with_default` - field assignment outside initializer for Default::default() instances

### Test Suite
```bash
cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi
```
**Result:** FAIL (compilation error)

#### Compilation Error in `crates/adapteros-api/tests/streaming_tests.rs`:
- Line 135: Non-exhaustive match - `StreamEvent::Paused { .. }` not covered

### Warnings (non-blocking)
- `adapteros-orchestrator`: unused variable `i`, unused import `DatasetVersionSelection`, unused mut
- `adapteros-server-api`: unused import `super::*` in preflight_adapter.rs

## Priority 0 Issues (CI Blockers)

1. **LOCAL-001**: Fix clippy `bool_assert_comparison` errors in config_validation_tests.rs
2. **LOCAL-002**: Fix clippy `field_reassign_with_default` errors in model.rs
3. **LOCAL-003**: Fix non-exhaustive match in streaming_tests.rs

These must be fixed before proceeding with GitHub issues to ensure CI stays green.
