# Inference Not Ready: Active Model Mismatch

## Symptoms

- UI banner: `Inference not ready: Active model is not loaded on any worker.`
- `/v1/system/status` returns `inference_ready=false` with `inference_blockers` containing `active_model_mismatch`.
- `/readyz` may include the hint `active model mismatch (not loaded)` under the models readiness check.

## What It Means (Canonical Definitions)

- **Active model**: `workspace_active_state.active_base_model_id` for the workspace/tenant.
- **Loaded runtime**: a `base_model_status` row for the same `(tenant_id, model_id)` whose status parses as `ModelLoadStatus::Ready` (`"ready"` or legacy `"loaded"`).

The banner appears when an active base model is recorded, but that exact model is not currently `ready` in `base_model_status`.

**Primary computation:** `crates/adapteros-server-api/src/handlers/system_status.rs` (`collect_inference_status`).

## Common Root Causes

- Worker restarted or unloaded the base model, but `workspace_active_state.active_base_model_id` remained set.
- A different model was loaded/ensured, but the active base model was never updated.
- Activating an adapter updates the workspace active state (including base model) to the adapter's `base_model_id`, but the base model was not loaded afterward.
- Local runtime artifacts under `./var/` were partially cleared (weights/caches) without reconciling DB state.

## Diagnosis (Deterministic)

### 1) Inspect Active Base Model

SQLite (local dev):

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select tenant_id, active_base_model_id, updated_at from workspace_active_state order by updated_at desc;"
```

API (requires access to the workspace):

```bash
curl -sS "http://localhost:${AOS_SERVER_PORT:-8080}/v1/workspaces/<workspace_id>/active" | jq .
```

### 2) Inspect Base Model Status for That Model

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select tenant_id, model_id, status, loaded_at, unloaded_at, updated_at, error_message from base_model_status order by updated_at desc;"
```

Mismatch criteria:
- `active_base_model_id` is non-null, AND
- its matching `base_model_status.status` is not `ready/loaded`, OR no status row exists for that `(tenant_id, model_id)`.

### 3) Confirm Worker Presence (Sanity)

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select id, tenant_id, status, last_seen_at, uds_path from workers order by coalesce(last_seen_at, started_at) desc limit 10;"
```

If there are no healthy workers, you will also see `worker_missing` as an inference blocker.

## Resolution

### A) Load the Active Model (Most Common Fix)

1. Determine `ACTIVE=<active_base_model_id>` from Diagnosis step (1).
2. Load it:

```bash
curl -sS -X POST "http://localhost:${AOS_SERVER_PORT:-8080}/v1/models/$ACTIVE/load" | jq .
```

Or in UI: `/models` → select the active model → click **Load**.

This should move `base_model_status` for `(tenant_id, ACTIVE)` to `loading` then `ready/loaded`, clearing the mismatch.

### B) Active State Is Stale (You Want a Different Model Active)

If you intentionally want a different base model to be active:

1. Clear the recorded active base model:

```bash
curl -sS -X POST "http://localhost:${AOS_SERVER_PORT:-8080}/v1/models/$ACTIVE/unload" | jq .
```

Note: this clears `workspace_active_state.active_base_model_id` if it matches, even if the model is not currently loaded.

2. Load the desired model (it becomes active if no active model is set):

```bash
DESIRED="<model_id>"
curl -sS -X POST "http://localhost:${AOS_SERVER_PORT:-8080}/v1/models/$DESIRED/load" | jq .
```

### C) If Loads Fail Repeatedly: Clear Runtime Artifacts (Dev Only)

Only do this in development when model load fails due to corrupted local caches or stale sockets.

1. Stop services:

```bash
./start down
```

2. Clear safe runtime artifacts (sockets/PIDs):

```bash
rm -f var/run/worker.sock var/run/worker.sock.stale 2>/dev/null || true
rm -f var/backend.pid var/node.pid 2>/dev/null || true
```

3. Optional (may trigger re-downloads): clear model cache:

```bash
rm -rf var/model-cache
```

4. Restart and load the active model again:

```bash
AOS_DEV_NO_AUTH=1 ./start
curl -sS -X POST "http://localhost:${AOS_SERVER_PORT:-8080}/v1/models/$ACTIVE/load" | jq .
```

## Deterministic Reproduction (Test)

The mismatch condition is covered by a targeted test:

```bash
cargo test -p adapteros-server-api system_status_inference_flags_model_mismatch -- --test-threads=1
```

