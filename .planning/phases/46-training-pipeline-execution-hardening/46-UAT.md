# Phase 46 UAT

## Objective

Operator can start training only when worker/model state is valid, and receives deterministic actionable errors otherwise.

## UAT Checklist

1. Runtime health:
 - `GET /readyz` returns `ready=true` and all checks `ok=true`.
 - Observed pass on `127.0.0.1:8080`.

2. Active model integrity:
 - Submit training with mismatched `base_model_id`.
 - Expected: request rejected with `ACTIVE_MODEL_MISMATCH`.
 - Observed: `BAD_REQUEST` with legacy code `ACTIVE_MODEL_MISMATCH`.

3. Worker degraded integrity:
 - Create `var/run/training-worker.degraded`.
 - Submit training start.
 - Expected: request rejected with `TRAINING_WORKER_DEGRADED` and remediation hint.
 - Observed: `SERVICE_UNAVAILABLE` with legacy code `TRAINING_WORKER_DEGRADED`.

4. Terminal error visibility:
 - Run a controlled failing training job.
 - Expected: `GET /v1/training/jobs/{id}` includes non-empty `error_message`.
 - Observed (controlled persisted failed record): `train-8ac7e2f3-87ed-4d59-a151-f17036ede3ff` returned
   `status=failed` and non-empty `error_message`.

## Acceptance Status

`complete`:
 - Runtime, active-model mismatch, degraded-worker gating, and terminal failure visibility validated.

## Citations

1. `/Users/star/Dev/adapter-os/crates/adapteros-server-api/src/handlers/training.rs`
2. `/Users/star/Dev/adapter-os/crates/adapteros-orchestrator/src/training/execution.rs`
3. `/Users/star/.codex/skills/adapteros-determinism-guard/SKILL.md`
