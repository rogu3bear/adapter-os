# Execution Plan

## Purpose

Stabilize CI-equivalent checks and resolve all open issues by batching fixes around baseline failures, then correctness/security issues, and finally docs/features, while maintaining determinism guarantees.

## Progress

- [x] 2026-01-05: Read repo rules (AGENTS/CLAUDE/CONTRIBUTING/README/SECURITY).
- [x] 2026-01-05: Created `maintenance/issue-sweep` branch and `.codex/` workspace.
- [x] 2026-01-05: Ran baseline checks and captured logs.
- [x] 2026-01-05: Fetched open GitHub issues.
- [x] 2026-01-05: Triage issues into tracker with priority/type/acceptance criteria.
- [x] 2026-01-05: Fix baseline failures (telemetry tests, missing anchor contract script, clippy errors).
- [x] 2026-01-05: Fix security regression suite failures (unsafe/panic detection, keychain unwraps) and rerun targeted tests.
- [ ] 2026-01-05: Full workspace tests without MLX_FORCE_STUB failed due to MLX link mismatch (see `.codex/LOGS/sweep_test_no_stub.txt`).
- [x] 2026-01-05: Verified real MLX builds succeed when `MLX_PATH` points to mlx-sys build output (see `.codex/LOGS/mlx_path_check.txt`).
- [ ] 2026-01-05: Execute issue batches (P0 → P3) with tests and commits.
- [ ] 2026-01-05: Full CI-equivalent rerun and finalize report/PR guidance.

## Surprises & Discoveries

- CI workflow references missing script `scripts/ci/check_anchor_contract.sh` (baseline failure).
- `cargo xtask check-all` fails due to outdated telemetry tests referencing removed types/variants.
- Clippy fails on unused variables and new clippy lints in `adapteros-core`.
- Local `main` is ahead of `origin/main` by 17 commits.
- Readiness `skip_worker_check` support already exists; issue #152 appears resolved in tree.
- Security regression suite flagged public-unsafe/crypto unwraps due to naive test heuristics and real keychain/io_utils patterns.
- Running tests without `MLX_FORCE_STUB=1` fails with MLX linker undefined symbols (local MLX version mismatch).
- `MLX_PATH` must be absolute; relative paths were ignored and build fell back to brew MLX.

## Decision Log

- Follow mission-required commit message format; note conflict with CONTRIBUTING in `.codex/CONFLICTS.md`.
- Keep `allow_silent_downgrade` field but reject `true` during deserialization to preserve compatibility while enforcing audit constraints.
- Keep security regression checks, but tighten test-block detection and move unsafe blocks into private helpers to avoid public API exposure.
- Prefer vendored MLX via `MLX_PATH` to mlx-sys build output to avoid brew/header mismatch.

## Batch Strategy

- Batch 0: Baseline stability (telemetry tests + clippy + missing script).
- Batch 1: P0 security/correctness issues (signature verification, unsafe transmute, NaN panic, hardcoded secrets).
- Batch 2: P1/P2 correctness and diagnostics/doc issues.
- Batch 3: Feature/PRD items; split into actionable children where large.

## Validation Commands

- `cargo fmt --all -- --check`
- `./scripts/check_env_paths.sh`
- `./scripts/ci/check_anchor_contract.sh`
- `cargo xtask check-all --verbose`
- `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings`
- `cargo test --workspace --all-targets --exclude adapteros-lora-mlx-ffi`
