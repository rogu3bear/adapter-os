---
phase: 49-training-worker-spawn-fix
plan: 01
subsystem: server/boot
tags: [training-worker, binary-resolution, preflight, boot-gate]
dependency_graph:
  requires: []
  provides: [training-worker-binary-resolution, preflight-boot-gate]
  affects: [adapteros-config, adapteros-server, adapteros-server-api]
tech_stack:
  added: []
  patterns: [config-driven-binary-resolution, preflight-validation]
key_files:
  created: []
  modified:
    - crates/adapteros-config/src/types.rs
    - crates/adapteros-server/src/boot/background_tasks.rs
    - configs/cp.toml
    - crates/adapteros-server/src/boot/api_config.rs
    - crates/adapteros-server/src/boot/database.rs
    - crates/adapteros-server/src/boot/runtime.rs
    - crates/adapteros-server-api/src/state.rs
    - crates/adapteros-server-api/src/settings_loader.rs
    - crates/adapteros-server-api/src/handlers/infrastructure.rs
    - crates/adapteros-server-api/src/handlers/streaming_infer.rs
    - crates/adapteros-server-api/src/inference_core/tests/policy_tests.rs
    - crates/adapteros-server-api/src/inference_core/tests/adapter_tests.rs
    - crates/adapteros-server-api/tests/manifest_fetch_tests.rs
    - crates/adapteros-server-api/tests/common/mod.rs
    - crates/adapteros-server-api/tests/support/e2e_harness.rs
    - crates/adapteros-server-api/tests/policy_knobs_e2e_test.rs
decisions:
  - Config-driven binary path via PathsConfig.training_worker_bin with Option<String>
  - 5-tier resolution: env > config > sibling > workspace target > Err (never bare PATH)
  - Preflight validation before supervisor spawn blocks boot on missing binary
metrics:
  duration: 22m
  completed: 2026-03-05T04:23:00Z
  tasks_completed: 2
  tasks_total: 2
---

# Phase 49 Plan 01: Binary Resolution Fix and Preflight Boot Gate Summary

Config-driven training worker binary resolution with 5-tier priority and preflight boot gate that fails with actionable error when binary is missing.

## Changes

### PathsConfig Extension
Added `training_worker_bin: Option<String>` with `#[serde(default)]` to `PathsConfig` in `adapteros-config`. Updated all 16 struct initializers across server, server-api, and test crates to include the new field.

### Deterministic Binary Resolution
Replaced `resolve_training_worker_bin() -> String` with `resolve_training_worker_bin(config: &PathsConfig) -> Result<String>`. Resolution priority:

1. `AOS_TRAINING_WORKER_BIN` env var
2. `config.training_worker_bin` from cp.toml (validates existence, warns + falls through if missing)
3. Sibling to `current_exe()`
4. Workspace `target/debug` and `target/release` relative to `AOS_VAR_DIR` parent or cwd
5. `Err` with build instructions (never bare `"aos-training-worker"` PATH lookup)

Each candidate logged at `debug!`, final resolved path at `info!`.

### Preflight Boot Gate
Before spawning the training worker supervisor task, the boot sequence now:
1. Reads `PathsConfig` from the config lock
2. Calls `resolve_training_worker_bin()` (returns `Err` if not found anywhere)
3. Validates the resolved path exists and is a file
4. Passes the validated path into the supervisor closure for all restart attempts

If the binary is missing, boot fails with:
```
Training worker binary not found at <path>. Build it: cargo build -p adapteros-training-worker
```

### spawn_training_worker Refactor
Changed `spawn_training_worker(env)` to `spawn_training_worker(env, bin_path)`, separating binary resolution (preflight) from spawn (runtime). The supervisor reuses the pre-resolved path for all restart attempts.

### Fallback Error Handling Cleanup
Removed the `is_training_worker_fallback_error` branch in the supervisor loop that would write a degraded marker and signal readiness on binary-not-found. This code path is no longer reachable because the preflight gate catches missing binaries before the supervisor starts.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed missing training_worker_bin field in 15 PathsConfig struct initializers**
- **Found during:** Task 1 (cargo check)
- **Issue:** Adding a new field to PathsConfig broke all explicit struct initializers across 7 crate files
- **Fix:** Added `training_worker_bin: None` to all 15 initializers (13 test/default, 2 config-propagating)
- **Files modified:** api_config.rs, database.rs, runtime.rs, state.rs, settings_loader.rs, plus 6 test files
- **Commit:** 1f563297e

**2. [Rule 1 - Bug] Removed unused mut on spawn_disabled_due_to_fallback_error**
- **Found during:** Task 2 (cargo check warning)
- **Issue:** Removing the fallback block left `spawn_disabled_due_to_fallback_error` never mutated
- **Fix:** Changed `let mut` to `let` (subsequently replaced entirely in plan 49-02)
- **Commit:** 1f563297e

## Verification

```
cargo check -p adapteros-config -p adapteros-server --lib  # clean (0 warnings, 0 errors)
grep -n '"aos-training-worker"' background_tasks.rs        # only in join() candidate, not a bare fallback
grep 'fn resolve_training_worker_bin' background_tasks.rs  # returns Result<String>
```
