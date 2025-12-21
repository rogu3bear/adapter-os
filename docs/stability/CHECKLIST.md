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

Gate: No clippy warnings allowed. The `clippy.toml` at the repo root configures test/example suppression.

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

Gate: All non-ignored tests must pass. Tests marked `#[ignore]` are excluded by default.

### 4. Determinism Verification

```bash
cargo test --test determinism_core_suite -- --test-threads=8 --test-timeout=45
cargo test -p adapteros-lora-router --test determinism
```

Gate: Deterministic inference output across runs. Seeded RNG and sorted collections required.

### 5. Inference Bypass Guard

```bash
./scripts/check_inference_bypass.sh
```

Gate: All inference must route through `InferenceCore`. Direct backend calls are forbidden (UDS client allowed).

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

Validates builds with default and all-features configurations to prevent feature drift.

```bash
make stability-ci
# Runs: ./scripts/ci/feature_matrix.sh
```

**What It Does:**

The feature matrix script runs two critical build profiles:

1. **Default Features (Production Profile)**
   - Tests the exact configuration that ships to production
   - Uses `Cargo.toml` workspace defaults: `["deterministic-only", "coreml-backend"]`
   - Command: `cargo test --workspace --no-run --locked`
   - This ensures production configuration always builds

2. **All Features (Drift Detection)**
   - Enables `--all-features` to catch feature interactions
   - Forces MLX stub mode (`MLX_FORCE_STUB=1`) by default
   - Command: `cargo test --workspace --no-run --locked --all-features`
   - This ensures optional features remain buildable and compatible

**Why Two Profiles?**

- **Default features** validates what actually ships to users
- **All features** catches drift where features become incompatible over time
- Together they ensure both production stability and feature maintenance

**Environment Variables:**

```bash
# Run with release optimizations
PROFILE=release make stability-ci

# Use real MLX backend (requires C++ library installation)
MLX_FORCE_STUB=0 make stability-ci
```

**Maintenance:**

If workspace default features in `Cargo.toml` change:
1. Update the script header in `scripts/ci/feature_matrix.sh`
2. Update this documentation section
3. Verify CI workflows reflect the new defaults

Current workspace defaults (from `Cargo.toml`):
```toml
[features]
default = ["deterministic-only", "coreml-backend"]
```

**Integration Points:**

- Script: `scripts/ci/feature_matrix.sh`
- Makefile target: `stability-ci` (line 175-176)
- CI workflow: `.github/workflows/stability.yml`

---

## Non-Blocking Suites (Review Required)

### Ignored Test Sweep

```bash
make test-ignored
# Runs:
#   cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
#   cargo test --workspace --features extended-tests --tests -- --ignored
```

Purpose: Surface failing ignored tests before release. These require infrastructure, hardware, or pending API work.

Customization:
```bash
# Exclude specific crates
IGNORED_EXCLUDE="adapteros-lora-mlx-ffi adapteros-memory" make test-ignored

# Disable extended-tests feature
IGNORED_FEATURES="" make test-ignored
```

### Hardware Tests

```bash
make test-hw
# Runs Metal/VRAM/residency tests on macOS with GPU
```

Purpose: Validate hardware-dependent behavior. Cannot run in CI (requires Metal device).

Suites executed:
- `lora_buffer_population_integration` (Metal kernel library)
- `kv_residency_quota_integration` (hardware-residency feature)
- `adapteros-lora-worker` residency/enforcement tests
- `adapteros-lora-kernel-coreml` integration tests
- `adapteros-memory` Metal heap tests

---

## CI Integration

### Stability Gate Job

Location: `.github/workflows/stability.yml`

```yaml
jobs:
  stability:
    name: Stability Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
        with:
          components: rustfmt, clippy, miri
      - run: scripts/ci/stability.sh
```

The `scripts/ci/stability.sh` script runs:
1. `./scripts/check_inference_bypass.sh`
2. `make stability-check`

### Ignored Test Sweep Job

```yaml
  ignored-tests:
    name: Ignored Test Sweep (Non-blocking)
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - run: make test-ignored
```

This job is non-blocking (`continue-on-error: true`). Review failures before release.

---

## Ignored Tests Registry

All ignored tests must include a tracking ID in the ignore reason:

```rust
#[ignore = "Reason here [tracking: STAB-IGN-XXXX]"]
```

The full registry is maintained at: **`docs/stability/IGNORED_TESTS.md`**

### Categories of Ignored Tests

| Category | Count | Notes |
| --- | --- | --- |
| Hardware-dependent | ~25 | Metal, VRAM, residency tests |
| Pending API refactoring | ~80 | Blocked by internal API changes |
| Merge conflicts | ~12 | Test files need cleanup |
| External dependencies | ~15 | KMS emulator, tokenizer models, database |
| Feature-gated | ~10 | Require `--features mlx`, `hardware-residency`, etc. |

### Before Release

1. Run `make ignored-tests-check` to verify registry is in sync
2. Review `docs/stability/IGNORED_TESTS.md` for stale entries
3. Run `make test-ignored` and triage new failures
4. Run `make test-hw` on a macOS machine with Metal GPU
5. Update tracking IDs for any newly ignored tests

---

## Release Profile

For release builds, set `PROFILE=release`:

```bash
PROFILE=release make stability-check
PROFILE=release make stability-ci
PROFILE=release make determinism-check
```

This validates release-mode compilation and runs determinism tests with optimizations enabled.

---

## Troubleshooting

### Clippy fails on test code

Check `clippy.toml` for test/example suppression rules. Some warnings are intentionally suppressed in test code.

### Determinism tests flaky

Ensure:
- All RNG is seeded via `deterministic-only` feature
- Collections are sorted before comparison
- No timestamp or random UUID in test assertions

### Ignored test count mismatch

Run the audit target:
```bash
make ignored-tests-audit
```

This shows:
- Count of ignored tests with tracking IDs in code
- Count of entries in the registry
- Any tests missing tracking IDs

### Feature matrix drift

If default features in `Cargo.toml` change:
1. Update this checklist (stability-ci section)
2. Update `scripts/ci/feature_matrix.sh` header documentation
3. Update CI workflow if feature names changed
4. Run `make stability-ci` to verify both profiles build

### MLX stub vs real backend issues

The feature matrix defaults to MLX stub mode (`MLX_FORCE_STUB=1`) to avoid requiring the MLX C++ library installation. If you need to test with the real MLX backend:

```bash
# Install MLX C++ library first (via homebrew or from source)
export MLX_INCLUDE_DIR=/opt/homebrew/include
export MLX_LIB_DIR=/opt/homebrew/lib

# Run with real MLX
MLX_FORCE_STUB=0 make stability-ci
```

---

## Stabilization Profile Summary

The stabilization profile consists of:

1. **Feature Matrix** (`make stability-ci`)
   - Default features build (production config)
   - All features build (drift detection)

2. **Core Test Suite** (`make test`)
   - Formatting, linting, unit tests, integration tests

3. **Determinism Verification** (`make determinism-check`)
   - Reproducible inference output validation

4. **Inference Bypass Guard** (`./scripts/check_inference_bypass.sh`)
   - Architectural constraint enforcement

All four must pass before release. The feature matrix is the newest addition and prevents the common problem where optional features break over time due to lack of regular testing.
