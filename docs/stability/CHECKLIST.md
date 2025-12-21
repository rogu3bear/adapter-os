# Stability Checklist

Use this checklist before stabilization runs or release candidates.

## Quick Reference

```bash
# Full stability gate (must pass)
make stability-check

# Feature matrix validation
make stability-ci

# Ignored test sweep (non-blocking)
make test-ignored

# Hardware-dependent tests (local only)
make test-hw
```

---

## Blocking Gates (Must Pass)

### 1. Formatting

```bash
cargo fmt --all --check
```

Gate: All Rust code must be formatted. Run `cargo fmt --all` to fix.

### 2. Linting (Clippy)

```bash
cargo clippy --all-features -- -D warnings
```

Gate: No clippy warnings allowed.

### 3. Unit and Integration Tests (Rust + UI)

```bash
# Full suite (run by make test)
bash scripts/test/all.sh all
```

This runs:
- Database reset and migration check
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
- Rust unit tests (`cargo test --lib --bins --examples`)
- Rust integration tests (`cargo test --tests`)
- Miri checks on aos_worker
- UI lint and Vitest tests

Gate: All non-ignored tests must pass.

### 4. Determinism Verification

```bash
cargo test --test determinism_core_suite -- --test-threads=8 --test-timeout=45
cargo test -p adapteros-lora-router --test determinism
```

Gate: Deterministic inference output across runs.

### 5. Inference Bypass Guard

```bash
./scripts/check_inference_bypass.sh
```

Gate: All inference must route through `InferenceCore`. Direct backend calls forbidden.

---

## Combined Targets

### stability-check (Makefile)

Runs all blocking gates in sequence:

```bash
make stability-check
# Runs:
#   1. ./scripts/check_inference_bypass.sh
#   2. make test  (full suite: fmt, clippy, Rust tests, UI tests, Miri)
#   3. make determinism-check
```

### stability-ci (Feature Matrix)

```bash
make stability-ci
# Runs:
#   cargo test --workspace --no-run --locked (default features)
#   cargo test --workspace --no-run --locked --all-features (drift check)
```

---

## Non-Blocking Suites

### Ignored Test Sweep

```bash
make test-ignored
```

Purpose: Surface failing ignored tests before release.

### Hardware Tests

```bash
make test-hw
```

Purpose: Validate Metal/GPU-dependent behavior. Cannot run in CI.

---

## Ignored Tests Registry

All ignored tests must include a tracking ID:

```rust
#[ignore = "Reason [tracking: STAB-IGN-XXXX]"]
```

Registry: `docs/stability/IGNORED_TESTS.md`

### Before Release

1. Run `make ignored-tests-check` to verify registry is in sync
2. Review `docs/stability/IGNORED_TESTS.md` for stale entries
3. Run `make test-ignored` and triage new failures
4. Run `make test-hw` on a macOS machine with Metal GPU

---

## Troubleshooting

### Ignored test count mismatch

```bash
make ignored-tests-audit
```

### Feature matrix drift

If default features in `Cargo.toml` change:
1. Update this checklist
2. Update `scripts/ci/feature_matrix.sh`
