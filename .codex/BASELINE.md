# Baseline Health Check

- Date: 2026-01-05T12:44:46Z
- Branch: maintenance/issue-sweep

## Commands Run

- `cargo fmt --all -- --check`
- `./scripts/check_env_paths.sh`
- `./scripts/ci/check_anchor_contract.sh`
- `cargo xtask check-all --verbose`
- `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings`
- `cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi`

## Results

- `cargo fmt --all -- --check` passed.
- `./scripts/check_env_paths.sh` passed.
- `./scripts/ci/check_anchor_contract.sh` failed: script missing (No such file or directory).
- `cargo xtask check-all --verbose` failed across feature sets (default/full/metal-backend/no-metal) due to telemetry test compile errors:
  - `UnifiedTelemetryEvent` type missing in `adapteros_telemetry::unified_events`.
  - `IdentityEnvelope::system()` missing.
  - `EventType::Audit` variant missing.
- `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings` failed with unused variables in `adapteros-core` tests and clippy lints (`io_other_error`, `assertions_on_constants`, `clone_on_copy`).
- `cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi` failed with the same telemetry test compile errors.

## Logs

- `.codex/LOGS/baseline_fmt.txt`
- `.codex/LOGS/baseline_env_paths.txt`
- `.codex/LOGS/baseline_anchor_contract.txt`
- `.codex/LOGS/baseline_xtask_check_all.txt`
- `.codex/LOGS/baseline_clippy.txt`
- `.codex/LOGS/baseline_test.txt`
test
