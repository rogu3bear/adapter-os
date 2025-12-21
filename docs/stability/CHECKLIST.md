# Stability Checklist

Use this checklist before stabilization runs or release candidates.

## Build/Test Feature Matrix

- [ ] Run `./scripts/ci/feature_matrix.sh` (or `make stability-ci`).
- [ ] Run `./scripts/check_inference_bypass.sh` to ensure inference routes through InferenceCore (UDS client whitelisted).
- [ ] Confirm the default-feature run uses `Cargo.toml` defaults (currently `deterministic-only` + `coreml-backend`).
- [ ] Confirm the `--all-features` run succeeds to catch drift.
- [ ] If `Cargo.toml` defaults change, update this checklist and the script.

## Blocking Suites (Stability Gate)

- [ ] `make stability-check` (runs `make check` + `make determinism-check`).
- [ ] `make check` includes formatter, clippy, Rust unit/integration tests, and UI tests.
- [ ] Rust integration tests are blocking unless marked `#[ignore]` (or `cfg_attr(..., ignore = "...")`).

## Ignored/Hardware Suites (Non-blocking)

- [ ] `make test-ignored` to run all ignored Rust tests (unit + integration).
- [ ] `make test-hw` to run hardware-dependent suites (Metal/VRAM/residency).
- [ ] Ensure ignored tests include a tracking tag in the ignore reason, e.g. `[tracking: STAB-IGN-001]`.

## Notes

- The stabilization profile compiles tests (`cargo test --no-run`) for the workspace.
- Set `PROFILE=release` to validate release-mode builds.
