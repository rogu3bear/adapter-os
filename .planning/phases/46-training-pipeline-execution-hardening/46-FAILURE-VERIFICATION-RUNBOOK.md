# Phase 46 Deterministic Failure Verification Runbook

## Purpose

Provide deterministic, operator-safe steps to verify training failure-path behavior without broad test execution.

## Preconditions

1. Server running on `127.0.0.1:8080`.
2. Valid dataset and base model IDs available.
3. Dev mode only for fail-fast probe (`ADAPTEROS_DEV_TRAINING_FAILFAST`).

## Procedure

1. Verify readiness:
 - `curl -sS http://127.0.0.1:8080/readyz`
 - Expect: `ready=true`.

2. Verify active-model mismatch gate:
 - `POST /v1/training/start` with non-active `base_model_id`.
 - Expect: `BAD_REQUEST`, legacy code `ACTIVE_MODEL_MISMATCH`.

3. Verify worker-degraded gate:
 - Create marker: `var/run/training-worker.degraded`.
 - `POST /v1/training/start` with active model.
 - Expect: `SERVICE_UNAVAILABLE`, legacy code `TRAINING_WORKER_DEGRADED`.
 - Remove marker after check.

4. Verify deterministic fail-fast terminal reason:
 - Export `ADAPTEROS_DEV_TRAINING_FAILFAST=1`.
 - Optionally export `ADAPTEROS_DEV_TRAINING_FAILFAST_REASON="<reason>"`.
 - Create training job.
 - `GET /v1/training/jobs/{id}` until terminal.
 - Expect: `status=failed`, non-empty `error_message`, stable `error_code`.

## Cleanup

1. Unset fail-fast environment:
 - `unset ADAPTEROS_DEV_TRAINING_FAILFAST`
 - `unset ADAPTEROS_DEV_TRAINING_FAILFAST_REASON`
2. Ensure marker removed:
 - `rm -f /Users/star/Dev/adapter-os/var/run/training-worker.degraded`

## Citations

1. `/Users/star/Dev/adapter-os/crates/adapteros-orchestrator/src/training/execution.rs`
2. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
3. `/Users/star/Dev/adapter-os/crates/adapteros-db/src/training_jobs.rs`
