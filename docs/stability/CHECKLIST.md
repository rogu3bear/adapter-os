# Stability Checklist

Use this checklist before stabilization runs or release candidates.

## Quick Reference

```bash
# Full stability gate (must pass)
bash scripts/ci/stability.sh

# Feature matrix validation
./scripts/ci/feature_matrix.sh

# Ignored test sweep (non-blocking)
cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
cargo test --workspace --features extended-tests --tests -- --ignored

# Hardware-dependent tests (local only)
# See the Hardware Tests section below for exact commands.
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

### 3. Unit and Integration Tests (Rust + Leptos UI)

```bash
# Full suite
bash scripts/test/all.sh all
```

This runs:
- Database reset and migration check
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
- Rust unit tests (`cargo test --lib --bins --examples`)
- Rust integration tests (`cargo test --tests`)
- Miri checks on aos_worker
- Leptos UI unit tests

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

## Combined Sequence

### Stability Gate

Runs all blocking gates in sequence:

```bash
bash scripts/ci/stability.sh
# Runs:
#   1. ./scripts/check_inference_bypass.sh
#   2. bash scripts/test/all.sh all  (fmt, clippy, Rust tests, Leptos UI tests, Miri)
#   3. determinism checks (see below)
```

### stability-ci (Feature Matrix)

Validates builds with default and all-features configurations to prevent feature drift.

```bash
./scripts/ci/feature_matrix.sh
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
PROFILE=release ./scripts/ci/feature_matrix.sh

# Use real MLX backend (requires C++ library installation)
MLX_FORCE_STUB=0 PROFILE=release ./scripts/ci/feature_matrix.sh
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
- CI workflow: `.github/workflows/stability.yml`

---

## Non-Blocking Suites (Review Required)

### Ignored Test Sweep

```bash
cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
cargo test --workspace --features extended-tests --tests -- --ignored
# Runs:
#   cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
#   cargo test --workspace --features extended-tests --tests -- --ignored
```

Purpose: Surface failing ignored tests before release. These require infrastructure, hardware, or pending API work.

Customization:
```bash
# Exclude specific crates
cargo test --workspace --exclude adapteros-lora-mlx-ffi --exclude adapteros-memory \
  --features extended-tests --lib --bins --examples -- --ignored
cargo test --workspace --exclude adapteros-lora-mlx-ffi --exclude adapteros-memory \
  --features extended-tests --tests -- --ignored

# Disable extended-tests feature
cargo test --workspace --lib --bins --examples -- --ignored
cargo test --workspace --tests -- --ignored
```

### Hardware Tests

```bash
cargo test --test lora_buffer_population_integration --features extended-tests --profile release -- --ignored --nocapture
cargo test --test kv_residency_quota_integration --features hardware-residency
cargo test -p adapteros-lora-worker --features hardware-residency,ci-residency --test worker_enforcement_tests
cargo test -p adapteros-lora-worker --features hardware-residency,ci-residency --test residency_probe
cargo test -p adapteros-lora-kernel-coreml --test integration_tests -- --ignored
cargo test -p adapteros-memory --test metal_heap_tests --profile release -- --ignored
cargo test -p adapteros-memory --lib --profile release -- --ignored
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
2. `bash scripts/test/all.sh all`
3. `cargo test --test determinism_core_suite -- --test-threads=8`
4. `cargo test -p adapteros-lora-router --test determinism`
5. `bash scripts/check_fast_math_flags.sh`

### Ignored Test Sweep Job

```yaml
  ignored-tests:
    name: Ignored Test Sweep (Non-blocking)
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
      - run: cargo test --workspace --features extended-tests --lib --bins --examples -- --ignored
      - run: cargo test --workspace --features extended-tests --tests -- --ignored
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

1. Run `grep -rn '#\\[ignore *= *"' --include='*.rs' crates tests | grep -v 'tracking: STAB-IGN'` to verify registry is in sync
2. Review `docs/stability/IGNORED_TESTS.md` for stale entries
3. Run the ignored-test sweep commands and triage new failures
4. Run the hardware test commands on a macOS machine with Metal GPU
5. Update tracking IDs for any newly ignored tests

---

## Release Profile

For release builds, set `PROFILE=release`:

```bash
PROFILE=release bash scripts/ci/stability.sh
PROFILE=release ./scripts/ci/feature_matrix.sh
cargo test --release --test determinism_core_suite -- --test-threads=8
cargo test --release -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh
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
grep -rn '#\\[ignore *= *"' --include='*.rs' crates tests 2>/dev/null | grep -c 'tracking: STAB-IGN'
grep -rn '#\\[ignore *= *"' --include='*.rs' crates tests 2>/dev/null | grep -v 'tracking: STAB-IGN'
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
4. Run `./scripts/ci/feature_matrix.sh` to verify both profiles build

### MLX stub vs real backend issues

The feature matrix defaults to MLX stub mode (`MLX_FORCE_STUB=1`) to avoid requiring the MLX C++ library installation. If you need to test with the real MLX backend:

```bash
# Install MLX C++ library first (via homebrew or from source)
export MLX_INCLUDE_DIR=/opt/homebrew/include
export MLX_LIB_DIR=/opt/homebrew/lib

# Run with real MLX
MLX_FORCE_STUB=0 ./scripts/ci/feature_matrix.sh
```

---

## Stabilization Profile Summary

The stabilization profile consists of:

1. **Feature Matrix** (`./scripts/ci/feature_matrix.sh`)
   - Default features build (production config)
   - All features build (drift detection)

2. **Core Test Suite** (`bash scripts/test/all.sh all`)
   - Formatting, linting, unit tests, integration tests

3. **Determinism Verification** (`cargo test --test determinism_core_suite -- --test-threads=8`, `cargo test -p adapteros-lora-router --test determinism`, `bash scripts/check_fast_math_flags.sh`)
   - Reproducible inference output validation

4. **Inference Bypass Guard** (`./scripts/check_inference_bypass.sh`)
   - Architectural constraint enforcement

All four must pass before release. The feature matrix is the newest addition and prevents the common problem where optional features break over time due to lack of regular testing.
