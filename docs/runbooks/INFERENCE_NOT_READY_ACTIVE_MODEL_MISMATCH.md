# Inference Not Ready: Active Model Mismatch

## Symptoms

- Web UI banner: `Inference not ready: Active model does not match the loaded runtime.`
- `/v1/system/status` returns `inference_ready=false` with `inference_blockers` containing `active_model_mismatch`.
- `/readyz` may include hint `active model mismatch (not loaded)` under the models check.

## What It Means (Canonical Definitions)

- **Active model**: `workspace_active_state.active_base_model_id` for the current workspace/tenant.
- **Loaded runtime**: a `base_model_status` row for the same `(tenant_id, model_id)` whose `status` parses as `ModelLoadStatus::Ready` (`"ready"` or legacy `"loaded"`).

The banner fires when an active base model is recorded but that model is not currently `ready` in `base_model_status`.

**Primary code path:** `crates/adapteros-server-api/src/handlers/system_status.rs` (`collect_inference_status`).

## Common Root Causes

- Worker restarted or unloaded the base model; `workspace_active_state.active_base_model_id` is sticky and remains set.
- A different model was loaded on the worker, but the active base model was not changed.
- Activating an adapter updates the workspace active state (including base model) to the adapter's `base_model_id`, but the base model was not loaded afterward.
- Local state was partially cleared under `./var/` (weights/caches) without clearing or reconciling DB state.

## Diagnosis (Deterministic, No Guessing)

### 1) Identify the Active Base Model

Via SQLite (local dev):

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select tenant_id, active_base_model_id, updated_at from workspace_active_state order by updated_at desc;"
```

Via API:

```bash
curl -sS http://localhost:${AOS_PORT:-8080}/v1/workspaces/<workspace_id>/active | jq .
```

### 2) Check Base Model Load Status for That Model

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select tenant_id, model_id, status, loaded_at, unloaded_at, updated_at, error_message from base_model_status order by updated_at desc;"
```

Mismatch criteria:
- `active_base_model_id` is non-null, AND
- its matching `base_model_status.status` is not `ready`/`loaded`, OR no status row exists for that `(tenant_id, model_id)`.

### 3) Confirm Worker Presence (Sanity)

```bash
sqlite3 -readonly var/aos-cp.sqlite3 \
  "select id, tenant_id, status, last_seen_at, uds_path from workers order by coalesce(last_seen_at, started_at) desc limit 10;"
```

If no healthy workers exist, you'll also see `worker_missing` as a blocker.

## Resolution

### Option A (Most Common): Load the Active Model

1. Determine `ACTIVE=<active_base_model_id>` from the diagnosis step.
2. Load it:

```bash
curl -sS -X POST http://localhost:${AOS_PORT:-8080}/v1/models/$ACTIVE/load | jq .
```

Or from the UI: `/models` -> select the active model -> click **Load**.

This should transition `base_model_status` for `(tenant_id, ACTIVE)` to `loading` then `loaded/ready`, clearing the mismatch.

### Option B: Active State Is Stale and You Want a Different Model Active

If you intentionally want a different base model to be active:

1. Clear the existing active base model:

```bash
curl -sS -X POST http://localhost:${AOS_PORT:-8080}/v1/models/$ACTIVE/unload | jq .
```

Note: this clears `workspace_active_state.active_base_model_id` if it matches, even if the model is not currently loaded.

2. Load the desired model (and let it become active when the active slot is empty):

```bash
DESIRED="<model_id>"
curl -sS -X POST http://localhost:${AOS_PORT:-8080}/v1/models/$DESIRED/load | jq .
```

Alternative (explicit): set workspace active state directly:

```bash
curl -sS -X POST http://localhost:${AOS_PORT:-8080}/v1/workspaces/<workspace_id>/active \
  -H 'content-type: application/json' \
  -d '{"active_base_model_id":"<model_id>","active_plan_id":null,"active_adapter_ids":[]}' | jq .
```

### Option C: Load Fails or Status Is Stuck (Clear `./var` Artifacts)

Only do this if model load repeatedly fails due to corrupted caches/weights.

1. Stop services:

```bash
./start down
```

2. Clear safe runtime artifacts (sockets/PIDs):

```bash
rm -f var/run/worker.sock var/run/worker.sock.stale var/run/backend.pid var/run/dev-worker.pid 2>/dev/null || true
```

3. Optional: clear model caches (may re-download weights; can be large):

```bash
rm -rf var/model-cache
```

4. Restart and load the active model again:

```bash
AOS_DEV_NO_AUTH=1 ./start
```

Then follow Option A.

## Deterministic Reproduction (CI/Test)

The mismatch condition is covered by a targeted test:

```bash
cargo test -p adapteros-server-api system_status_inference_flags_model_mismatch -- --test-threads=1
```

