# Phase 46 Verification

## Scope

Validate deterministic training preflight hardening, active-model consistency enforcement, and runtime operability on `Qwen3.5-27B`.

## Evidence

1. Compile checks passed:
 - `cargo check -p adapteros-server-api -q`
 - `cargo check -p adapteros-orchestrator -q`

2. Runtime ready on expected endpoint:
 - `GET http://127.0.0.1:8080/readyz`
 - Result: `ready=true`; checks `db.ok=true`, `worker.ok=true`, `models_seeded.ok=true`.

3. Primary model env default corrected:
 - `/Users/star/Dev/adapter-os/.env` now sets:
   - `AOS_BASE_MODEL_ID=Qwen3.5-27B`
   - `AOS_MODEL_PATH=./var/models/Qwen3.5-27B`
 - `/Users/star/Dev/adapter-os/.env.production` now sets:
   - `AOS_MODEL_PATH=./var/models/Qwen3.5-27B`

4. Training preflight gates present:
 - `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
   - worker degraded marker fail-closed (`TRAINING_WORKER_DEGRADED`)
   - stale heartbeat / capability fail-closed
   - active base-model mismatch fail-closed (`ACTIVE_MODEL_MISMATCH`)

5. Failure reason fallback mapping present:
 - `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
   - falls back to metadata error fields when progress payload omits reason.

6. Controlled persisted fail-fast probe record surfaced terminal reason via API:
 - Probe job id: `train-8ac7e2f3-87ed-4d59-a151-f17036ede3ff`
 - API: `GET /v1/training/jobs/train-8ac7e2f3-87ed-4d59-a151-f17036ede3ff`
 - Result:
   - `status=failed`
   - `error_message=\"Deterministic fail-fast probe triggered (phase46 verification)\"`
   - `error_code=\"TRAINING_EXECUTION_FAILED\"`

7. Active model mismatch preflight verified:
 - API: `POST /v1/training/start`
 - Request `base_model_id=mdl-019c87824e8e7b93aed872deb58a1bda`
 - Active model: `mdl-019ca2b66df871f0bb0134e4c760ba07`
 - Result: `BAD_REQUEST` with legacy code `ACTIVE_MODEL_MISMATCH`.

8. Worker degraded preflight verified:
 - Created marker: `var/run/training-worker.degraded`
 - API: `POST /v1/training/start`
 - Result: `SERVICE_UNAVAILABLE` with legacy code `TRAINING_WORKER_DEGRADED`.

9. Targeted regression checks for incomplete-feature closure:
 - `cargo test -p adapteros-db --test training_job_status_authority_tests -q`
   - Result: `2 passed; 0 failed`.
 - `cargo test -p adapteros-server-api resolve_terminal_error_fields_ --lib -q`
   - Result: `3 passed; 0 failed`.

10. Deterministic operator runbook added:
 - `/Users/star/Dev/adapter-os/.planning/phases/46-training-pipeline-execution-hardening/46-FAILURE-VERIFICATION-RUNBOOK.md`

11. Single-ready model invariant and tenant-scoped status read paths hardened:
 - `crates/adapteros-db/src/models.rs` (`update_base_model_status`, `list_base_model_statuses_for_tenant`)
 - `crates/adapteros-db/src/worker_model_state.rs` (`recompute_base_model_status_projection`)
 - `migrations/20260228153000_single_ready_base_model_per_tenant.sql`

12. Duplicate active-model setter logic removed in favor of DB primitive reuse:
 - `crates/adapteros-db/src/workspace_active_state.rs` (`set_active_base_model`)
 - `crates/adapteros-server-api/src/handlers/models.rs`
 - `crates/adapteros-server-api-models/src/handlers.rs`

13. Workspace reconciliation worker probing hardened to use tenant-scoped candidate sockets:
 - `crates/adapteros-server-api/src/handlers/workspaces.rs`

14. Targeted validation run (pass):
 - `cargo check -p adapteros-db -q`
 - `cargo check -p adapteros-server-api-models -q`
 - `cargo check -p adapteros-server-api -q`
 - `cargo test -p adapteros-server-api --test model_status_contract ready_status_enforces_single_loaded_model_per_tenant -q`
 - `cargo test -p adapteros-server-api --test tenant_isolation_models test_get_base_model_status_cross_tenant_denied -q`
 - `cargo test -p adapteros-server-api --test model_handlers_integration model_status_respects_tenant_isolation -q`
 - `cargo test -p adapteros-server-api --test workspace_active_state_tests -q`
 - `cargo test -p adapteros-server-api --test system_status_tests system_status_inference_flags_model_mismatch -q`

15. Follow-up effective-read alignment + hot-swap/reconciliation hardening validation (pass):
 - `cargo check -p adapteros-db -q`
 - `cargo check -p adapteros-server-api -q`
 - `cargo test -p adapteros-server-api --test model_status_contract ready_status_enforces_single_loaded_model_per_tenant -q`
 - `cargo test -p adapteros-server-api --test workspace_active_state_tests -q`
 - `cargo test -p adapteros-server-api --test system_status_tests system_status_inference_flags_model_mismatch -q`
 - `cargo test -p adapteros-server-api --test model_handlers_integration model_status_respects_tenant_isolation -q`

## Remaining Verification Gap

1. None for Phase 46 plan scope.

## Citations

1. `/Users/star/Dev/adapter-os/crates/adapteros-orchestrator/src/training/execution.rs`
2. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
3. `/Users/star/.codex/skills/gsd-full-suite/SKILL.md`
4. `/Users/star/.codex/skills/adapteros-test-selector/SKILL.md`
5. `/Users/star/.codex/skills/adapteros-determinism-guard/SKILL.md`
6. `/Users/star/Dev/adapter-os/crates/adapteros-db/src/workspace_active_state.rs`
7. `/Users/star/Dev/adapter-os/crates/adapteros-db/src/models.rs`
8. `/Users/star/Dev/adapter-os/crates/adapteros-db/src/worker_model_state.rs`
9. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/models.rs`
10. `/Users/star/Dev/adapter-os/crates/adapteros-server-api-models/src/handlers.rs`
11. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/workspaces.rs`
12. `/Users/star/Dev/adapter-os/migrations/20260228153000_single_ready_base_model_per_tenant.sql`
13. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/infrastructure.rs`
14. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/run_evidence.rs`
15. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/inference_core/core.rs`
