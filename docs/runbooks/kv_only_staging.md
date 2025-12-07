# KV-Only Staging Verification Runbook

Goal: prove KV-only stability in staging, exercise downgrade guard, and show zero drift/fallback/error counts while critical flows (auth/session, RAG, chat, training start) pass.

## Preconditions
- Staging access: API base URL, Prometheus/metrics endpoint, and an auth token or bootstrap credentials.
- CLI: `aosctl` available with network reachability to staging; `DATABASE_URL`/`AOS_DATABASE_URL` unset so explicit `--db-path` is used.
- Paths: `var/aos-cp.sqlite3` (SQL), `var/aos-kv.redb` (KV). Ensure writable workspace.

## Baseline capture (before flip)
1) Record KV metrics snapshot (Prometheus):
   - `kv_fallback_operations_total`
   - `kv_errors_total`
   - `kv_drift_detections_total`
   - `kv_degraded_events_total`
2) Check current mode and degradation flag:
   ```bash
   aosctl storage mode --db-path ./var/aos-cp.sqlite3 --json
   ```
3) Note `degradation_reason` if present.

## Flip to kv_only (guarded)
1) Apply schema (fresh DB is fine):
   ```bash
   aosctl db migrate --db-path ./var/aos-cp.sqlite3
   ```
2) Initialize KV (if absent) and switch:
   ```bash
   aosctl storage set-mode kv_only \
     --db-path ./var/aos-cp.sqlite3 \
     --kv-path ./var/aos-kv.redb \
     --init-kv
   ```
3) Guard behavior (code: `crates/adapteros-db/src/lib.rs::enforce_kv_only_guard`, test: `crates/adapteros-db/tests/kv_only_paths.rs`):
   - Blocks kv_only when coverage gaps exist; downgrades to kv_primary with a reason listing missing domains.
   - Even with full coverage, any KV fallback/error (`fallback_operations_total > 0` or `errors_total > 0`) triggers downgrade to kv_primary and sets `degraded_reason` with the counts.
4) Verify mode after set:
   ```bash
   aosctl storage mode --db-path ./var/aos-cp.sqlite3 --json
   ```

## Smoke flows (staging)
- Auth/session: login (or dev bootstrap), hit `/v1/auth/sessions/me`, ensure 200 and `storage_mode=kv_only` remains.
- RAG retrieval: run a known collection query; confirm documents returned and no downgrade.
- Chat: start a session and send a short prompt; streaming must succeed without fallback/downgrade.
- Training start: submit a small training job (noop/short epoch) and verify it transitions to queued/running.

## Load & drift check
1) Run each flow twice to exercise caches.
2) Re-snapshot metrics (same Prom queries). Expect:
   - `fallback_operations_total == 0`
   - `errors_total == 0`
   - `drift_detections_total == 0`
   - `degraded_events_total == 0`
3) Confirm mode stayed `kv_only`:
   ```bash
   aosctl storage mode --db-path ./var/aos-cp.sqlite3 --json
   ```

## Evidence to capture
- Prom snapshots (before/after) for the four counters above.
- `aosctl storage mode --json` output showing `mode=kv_only`, `degradation_reason=null`.
- If a downgrade occurs: capture `degradation_reason` and the KV metrics snapshot; include log lines mentioning “KV-only mode degraded” or “KV read fallback”.
- Smoke run notes: endpoints called, HTTP status, latency, and whether streaming/chat/train completed.

### Example log template
```
Timestamp: 2025-12-05T..
Mode before: kv_only, degraded_reason: null
Prom: fallback=0 errors=0 drift=0 degraded=0
Auth/session: PASS (token=..., latency=xxms)
RAG: PASS (k=5, latency=xxms)
Chat: PASS (stream ok, latency=xxms)
Training start: PASS (job_id=..., state=queued→running)
Mode after: kv_only, degraded_reason: null
Prom after: fallback=0 errors=0 drift=0 degraded=0
```

## Downgrade triage (if triggered)
1) Inspect `degradation_reason` (usually lists fallbacks/errors counts or missing domains).
2) Re-run `aosctl storage verify --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb --domains adapters,tenants,stacks,plans,auth_sessions,runtime_sessions,rag_artifacts --fail-on-drift --json` to surface domain gaps.
3) Check logs for “KV read fallback” / “KV write failed, recorded fallback” and backend errors; collect stack traces.
4) If domain gap: rerun migration for that domain; if backend errors: inspect KV storage health and retry after resolution.

## Exit criteria
- No downgrades during the run (mode remains kv_only).
- Prom counters for fallback/errors/drift/degraded stay at zero before and after load.
- All smoke flows pass; evidence captured with timestamps and metrics snapshots.

MLNavigator Inc 2025-12-05.

