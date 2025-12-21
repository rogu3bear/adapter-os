# Stability Checklist

Use this checklist before stabilization runs or release candidates.

## Build/Test Feature Matrix

- [ ] Run `./scripts/ci/feature_matrix.sh` (or `make stability-ci`).
- [ ] Run `./scripts/check_inference_bypass.sh` to ensure inference routes through InferenceCore (UDS client whitelisted).
- [ ] Confirm the default-feature run uses `Cargo.toml` defaults (currently `deterministic-only` + `coreml-backend`).
- [ ] Confirm the `--all-features` run succeeds to catch drift.
- [ ] If `Cargo.toml` defaults change, update this checklist and the script.

## Notes

- The stabilization profile compiles tests (`cargo test --no-run`) for the workspace.
- Set `PROFILE=release` to validate release-mode builds.
