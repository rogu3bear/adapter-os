# TROUBLESHOOTING

---

## Blank UI

```bash
./scripts/build-ui.sh
```

UI assets must exist in `crates/adapteros-server/static/`. Backend serves from there.

---

## Port in use

```bash
./scripts/fresh-build.sh
```

Stops services, frees ports.

---

## Health fails

```bash
./aosctl doctor
./aosctl preflight
```

For full diagnostics (system info, ports, env, DB, bundle creation), use:

```bash
./aosctl diag run --full
```

Check `var/logs/` for errors. Readiness checks: DB, worker, models. See `handlers::ready`, `ReadyzQuery`.

---

## Worker readiness mismatch (`registered` != ready)

If startup/status signals conflict (for example, control-plane state is `registered` but inference is unavailable), use first-principles readiness checks:

- Worker process is alive
- Worker UDS socket exists (`var/run/worker.sock`)
- Socket has a live listener

```bash
scripts/service-manager.sh status
if [ -f var/worker.pid ]; then cat var/worker.pid; fi
ls -la var/run/worker.sock
lsof -t var/run/worker.sock
tail -n 200 var/logs/service-manager.log
tail -n 200 var/logs/start.log
```

`registered` is transitional and not sufficient for readiness.

For full method and incident template, use:
- [FIRST_PRINCIPLES_DEBUG](runbooks/FIRST_PRINCIPLES_DEBUG.md)
- [WORKER_CRASH](runbooks/WORKER_CRASH.md)

---

## Migration issues

```bash
./aosctl db migrate
# Verify: migrations/signatures.json
```

---

## Chat unavailable: no base model active

**Symptom**: Chat UI shows "Conversation unavailable" with the reason "No base model is active" and a link to the Base Model Registry. The `/v1/system/status` endpoint returns `inference_ready: "false"` with `inference_blockers: ["no_model_loaded"]`.

**Root cause**: The inference pipeline requires three things to be true simultaneously:

1. A model row exists in the `models` table (seeded via `aosctl models seed`)
2. A worker process is running and registered (the `base_model_status` table has a row)
3. That model's status is `"ready"` in `base_model_status` (set when the model is loaded on a worker via `POST /v1/models/{id}/load`)

The `./start` script automates all three steps, but **only when `AOS_MODEL_PATH` is set**. Without it, the boot sequence starts the backend and worker but skips model seeding and loading, leaving the system in a state where inference is blocked.

Additionally, the server itself will **refuse to start** (FATAL exit) if neither `AOS_MODEL_CACHE_DIR`/`AOS_BASE_MODEL_ID` nor `AOS_MODEL_PATH` resolve to an existing model directory on disk.

**Fix (recommended)**:

1. Download a model if you don't have one:

```bash
./scripts/download-model.sh
```

This downloads `Qwen3.5-27B` into `var/models/` and updates `.env`.

2. Set the required environment variables (if not already in `.env`):

```bash
# Option A: Canonical env vars (preferred)
export AOS_MODEL_CACHE_DIR=var/models
export AOS_BASE_MODEL_ID=Qwen3.5-27B

# Option B: Legacy single-path var (deprecated but still works)
export AOS_MODEL_PATH=var/models/Qwen3.5-27B
```

3. Start with auth bypass for local dev:

```bash
AOS_DEV_NO_AUTH=1 ./start
```

The `./start` script will: start the backend, seed the model into the database, start the worker, load the model on the worker, and verify `/readyz` returns 200.

**Fix (manual, if `./start` fails at model load)**:

If the worker starts but model loading fails, you can manually seed and load:

```bash
# Seed model into database
./aosctl models seed --model-path var/models/Qwen3.5-27B

# Load model on worker (requires backend + worker running)
curl -X POST http://localhost:18080/v1/models/<model-id>/load \
  -H "Content-Type: application/json" -d '{}'
```

Get `<model-id>` from `aosctl models list` or from the `models` table in `var/aos-cp.sqlite3`.

**Verify**:

```bash
# Check system status (requires auth or AOS_DEV_NO_AUTH=1)
curl -s http://localhost:18080/v1/system/status \
  -H "Authorization: Bearer <token>" | jq '{
    inference_ready,
    inference_blockers,
    readiness: .readiness.overall
  }'
```

**Expected** (when inference is ready):

```json
{
  "inference_ready": "true",
  "inference_blockers": [],
  "readiness": "ready"
}
```

**Expected** (when blocked -- the symptom this section addresses):

```json
{
  "inference_ready": "false",
  "inference_blockers": ["no_model_loaded"],
  "readiness": "not_ready"
}
```

Other possible blockers visible in the same field:
- `worker_missing` -- worker process not running or not registered
- `active_model_mismatch` -- workspace has an active model ID that doesn't match any loaded model
- `system_booting` -- boot sequence not yet complete
- `database_unavailable` -- SQLite unreachable
- `boot_failed` -- server startup failed a critical phase

**Playwright / PW_DEV_BYPASS=1 path**:

When running Playwright tests with `PW_DEV_BYPASS=1`, the test config (`playwright.ui.config.ts`) starts the server with `AOS_DEV_NO_AUTH=1` and sets `AOS_BASE_MODEL_ID=mistral-7b-instruct-v0.3-4bit` pointing to a stub directory in `var/playwright/models/`. The `/readyz` endpoint returns 200 in `DevBypass` mode regardless of actual model readiness. This means Playwright UI tests do NOT require a real model download or inference engine -- they test the UI shell, not inference.

However, if a Playwright test tries to exercise actual inference (e.g., sending a chat message and expecting a response), it will fail unless a real model is loaded. The `PW_DEV_BYPASS` path is intentionally designed for UI-only testing.

**Required config change (if using `./start` without `.env`)**:

The `./start` script reads `AOS_MODEL_PATH` to decide whether to seed and load a model. If you rely on `AOS_MODEL_CACHE_DIR`/`AOS_BASE_MODEL_ID` instead, the server will start and resolve the model path, but `./start` will skip the seed-and-load step because it only checks `AOS_MODEL_PATH`.

Proposed fix for `start` (document only -- do not apply without team lead confirmation):

```diff
# In the start script, line ~1273:
# Current:
- if [ $backend_ok -eq 1 ] && [ -n "${AOS_MODEL_PATH:-}" ]; then
# Proposed:
+ AOS_EFFECTIVE_MODEL_PATH="${AOS_MODEL_PATH:-}"
+ if [ -z "$AOS_EFFECTIVE_MODEL_PATH" ] && [ -n "${AOS_MODEL_CACHE_DIR:-}" ] && [ -n "${AOS_BASE_MODEL_ID:-}" ]; then
+     AOS_EFFECTIVE_MODEL_PATH="${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}"
+ fi
+ if [ $backend_ok -eq 1 ] && [ -n "$AOS_EFFECTIVE_MODEL_PATH" ]; then
```

The same pattern applies at line ~1388 where `load_seeded_model_on_worker` is called.

**Notes**:
- Does NOT require changing production defaults
- The `dev_bypass = false` setting in `configs/cp.toml` is correct for production; set `AOS_DEV_NO_AUTH=1` in your shell or `.env` for local dev
- The server reads `dev_bypass` from `[security]` in `cp.toml` and exports `AOS_DEV_NO_AUTH=1` if it's `true` (see `start` lines 84-92)
- Model cache root defaults to `var/models/` and base model ID defaults to `Qwen3.5-27B` if neither env vars nor config overrides are set

---

## Runbooks

[runbooks/](runbooks/) for incident procedures.

---

## Action log diagnostics

On **March 3, 2026**, legacy HTTP log endpoints were retired:

- `/v1/services/{service_id}/logs`
- `/v1/training/jobs/{job_id}/logs`

Use local action logs and `jobs.logs_path` instead:

```bash
# Job/training/service action logs (JSONL)
tail -n 200 var/logs/actions/jobs/<job_id>.log
tail -n 200 var/logs/actions/training/<job_id>.log
tail -n 200 var/logs/actions/services/<service_id>.log

# UDS tail request (bounded)
printf '%s\n' '{"path":"actions/jobs/<job_id>.log","lines":100}' \
  | socat - UNIX-CONNECT:var/run/action-logs.sock
```

For retention knobs and failure modes, see [ACTION_LOGS](runbooks/ACTION_LOGS.md).
