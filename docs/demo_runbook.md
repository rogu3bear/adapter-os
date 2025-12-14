# Pilot Demo Runbook (local dev) — brutally step-by-step

**Goal**: Run a clean local demo (UI + API), show the key screens, and have a 60-second fallback if live inference/worker is unhappy.

## Copy/paste commands (run in repo root)

**reset**
```bash
./aosctl db reset --force
```

**seed**
```bash
./aosctl db seed-fixtures --skip-reset
```

**dev-up**
```bash
# IMPORTANT: keep the seeded DB (ui/dev-server.mjs wipes it unless you set AOS_E2E_RESET_DB=0)
AOS_E2E_RESET_DB=0 VITE_ENABLE_DEV_BYPASS=true pnpm --dir ui dev
```

**smoke-demo**
```bash
./aosctl check startup --server-url http://127.0.0.1:8080/api --timeout 10
```

## Step-by-step (exactly what to do)

### 0) One-time (only if needed): build the CLI
If `./aosctl` doesn’t exist:
```bash
make cli
```

### 1) Reset + seed the demo DB (Terminal B)
1. Stop any running dev server first (Terminal A: hit `Ctrl+C`).
2. Reset the DB:
   ```bash
   ./aosctl db reset --force
   ```
3. Seed deterministic fixtures:
   ```bash
   ./aosctl db seed-fixtures --skip-reset
   ```
   Expected output includes these IDs (you’ll use them in the UI):
   - tenant: `tenant-test`
   - user: `test@example.com` / `password`
   - adapter: `adapter-test` (“Test Adapter”)
   - stack: `stack-test` (“stack.test”)

### 2) Start the dev stack (Terminal A)
1. Start UI + backend together:
   ```bash
   AOS_E2E_RESET_DB=0 VITE_ENABLE_DEV_BYPASS=true pnpm --dir ui dev
   ```
2. Wait until both are up:
   - UI should open (or be reachable) at `http://localhost:3200/`
   - API should be reachable at `http://127.0.0.1:8080/api/`

### 3) Smoke check (Terminal B)
Run:
```bash
./aosctl check startup --server-url http://127.0.0.1:8080/api --timeout 10
```
Expected: a table of checks with passes (health, meta, auth responding).

## Browser walkthrough (exact URLs + what you should see)

### A) API sanity (optional, but good when demoing)
- `http://127.0.0.1:8080/api/readyz`
  - Expect HTTP `200` and JSON with `"status":"ready"` or `"status":"fully-ready"`.
- `http://127.0.0.1:8080/api/swagger-ui`
  - Expect Swagger UI to load (interactive API docs).

### B) UI login (only if you see the login screen)
- `http://localhost:3200/login`
  - Expect a login form.
  - Use `test@example.com` / `password`.
  - If you see a dev-bypass option, it’s safe to use for demos.

### C) Confirm seeded org + stack
- `http://localhost:3200/admin/tenants`
  - Expect an “Organizations” table with `tenant-test` / “Test Tenant”.
- `http://localhost:3200/admin/stacks`
  - Expect an “Adapter Stacks” table with `stack-test` / “stack.test”.

### D) Confirm seeded adapter
- `http://localhost:3200/adapters`
  - Expect an adapter list containing `adapter-test` / “Test Adapter”.

### E) Live inference demo (primary path)
- `http://localhost:3200/inference`
  1. Confirm you’re on the “Inference” page (prompt box + Run button).
  2. Select:
     - Stack: `stack-test` (if there’s a stack selector)
     - Adapter: `adapter-test` (or leave adapter as “none/auto” if you want routing to pick)
  3. Prompt (copy/paste):
     ```text
     In one sentence: what is AdapterOS?
     ```
  4. Click **Run**.
  5. Expect:
     - Output text appears.
     - A “Run Receipt” / trace panel appears below or to the side (IDs + digests).
  6. Click the in-page link “View telemetry for this session…” (goes to Telemetry Viewer).

## 60-second fallback (if worker/inference fails)

This path avoids the worker entirely and still lets you demo traces + evidence in the UI.

**Precondition**: you started dev-up with `VITE_ENABLE_DEV_BYPASS=true` (it enables `/api/testkit/*`).

1. Create a small deterministic trace fixture (Terminal B):
   ```bash
   curl -fsS -X POST http://127.0.0.1:8080/api/testkit/create_trace_fixture \
     -H 'Content-Type: application/json' \
     -d '{"tenant_id":"tenant-test","token_count":3,"adapter_ids":["adapter-test"]}'
   ```
2. Create a matching evidence fixture (Terminal B):
   ```bash
   curl -fsS -X POST http://127.0.0.1:8080/api/testkit/create_evidence_fixture \
     -H 'Content-Type: application/json' \
     -d '{"tenant_id":"tenant-test","inference_id":"trace-fixture"}'
   ```
3. Open these pages (Browser):
   - `http://localhost:3200/telemetry/viewer/trace-fixture`
     - Expect a trace viewer that loads and shows token decisions (3 tokens).
   - `http://localhost:3200/security/evidence`
     - Expect an evidence table with a “Fixture Document” entry tied to `trace-fixture`.

If you need a “model output” line to say out loud, use the stub output (Terminal B):
```bash
curl -fsS -X POST http://127.0.0.1:8080/api/testkit/inference_stub \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"demo"}'
```
Expected: JSON with `"text":"Echo: demo"` and a `run_receipt` block.

## “If X happens, do Y” (common failures)

1) **UI keeps redirecting to login / looks unauthorized**
- Do: re-run seed, then hard refresh the browser.
- If still stuck: go to `http://localhost:3200/login` and log in with `test@example.com` / `password`.

2) **DB looks empty / seeded tenant not present**
- Most common cause: you started dev-up without `AOS_E2E_RESET_DB=0` and the DB was wiped.
- Do: stop dev-up (`Ctrl+C`), then run `reset` + `seed` again, then restart dev-up with `AOS_E2E_RESET_DB=0`.

3) **Port 8080 or 3200 already in use**
- Do:
  ```bash
  lsof -ti:8080 | xargs kill -9 2>/dev/null || true
  lsof -ti:3200 | xargs kill -9 2>/dev/null || true
  ```
  Then rerun dev-up.

4) **`/api/readyz` returns 503 (“booting…”, “maintenance”, or “draining”)**
- Do: wait 10–30 seconds and refresh `http://127.0.0.1:8080/api/readyz`.
- If it stays 503: check the running server logs in the dev-up terminal.

5) **Inference errors/timeouts**
- Do: immediately switch to the fallback path (trace + evidence fixtures).
- Optional sanity check: `http://localhost:3200/system/workers` should show whether any workers are connected.
