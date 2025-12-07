KV Primary / KV-Only Runbook
============================

Modes
- sql_only: SQLite only
- dual_write: SQL primary, KV mirror
- kv_primary: KV read/write, SQL fallback allowed
- kv_only: KV only (downgrades to kv_primary when KV fallbacks/errors occur)

Config (env or cp.toml)
- AOS_STORAGE_BACKEND=kv_primary|kv_only|dual_write|sql
- AOS_KV_PATH=var/aos-kv.redb
- AOS_TANTIVY_PATH=var/aos-search (optional)
- DATABASE_URL / AOS_DATABASE_URL for SQL modes
- KV-only guard: `kv_coverage_summary().unsupported_domains` is currently empty, so coverage alone no longer blocks kv_only; guard still downgrades to kv_primary when KV fallbacks/errors are non-zero.
- `AOS_ATOMIC_DUAL_WRITE_STRICT` is auto-enforced in `kv_primary` and `kv_only`; dual_write remains best-effort elsewhere.

Bootstrap (KV-only or KV-primary)
- Start control plane: STORAGE_BACKEND=kv_only AOS_KV_PATH=... make dev
- Guard may downgrade to kv_primary if KV fallbacks/errors counters are non-zero; check degradation_reason.
- System bootstrap: `Db::ensure_system_tenant()` creates the `system` tenant and seeds core policies (egress/determinism/isolation/evidence) via KV; no SQLite required.
- Auth bootstrap: dev bootstrap endpoint works with KV-only (users/auth sessions/policy bindings in KV); grants are skipped if SQL pool is absent.

Dual-write drift checks
- Enable dual_write; run diff tool comparing KV vs SQL for tenants/users/adapters/stacks/plans/policy bindings/users.
- CLI: `aosctl storage verify --kv-path <path>` (flags: `--adapters-only`, `--tenants-only`, `--stacks-only`).
- Monitor kv_metrics fallback counters and degradation_reason for downgrades.
- KV alerting: `kv_alert_rules()` + `evaluate_global_kv_alerts()` wire drift/fallback/error/degraded counters into the alert engine (defaults emit Warning on fallbacks/drift, Critical on backend errors/degraded events).
- KV-only guard downgrades to kv_primary when coverage gaps exist or KV fallback/error counters are non-zero; degradation_reason includes the counts.

CI drift check
- Add to CI: `make kv-verify` (runs `aosctl storage verify --json --domains adapters,tenants,stacks,plans,auth_sessions,runtime_sessions,rag_artifacts,policy_audit,training_jobs,chat_sessions --fail-on-drift`), fails on drift without repairing. Override paths via `KV_VERIFY_DB` / `KV_VERIFY_KV` / `KV_VERIFY_DOMAINS`; KV is pre-seeded from SQL via `storage migrate` to keep ordering deterministic.
- JSON contract (`--json`): `drift` (issue count), `drift_events` (kv_metrics.drift_detections_total), `fallback` (kv_metrics.fallback_operations_total), `errors` (kv_metrics.errors_total), `degraded` (bool), `degradation_reason` (string/nullable), `kv_metrics` (full snapshot), `issues` (diff entries). CI gate: fail if `drift > 0` or `fallback > 0` or `errors > 0` or `degraded == true`.
- Sample pass:
```
{
  "drift": 0,
  "drift_events": 0,
  "fallback": 0,
  "errors": 0,
  "degraded": false,
  "degradation_reason": null,
  "kv_metrics": { "...": "..." },
  "issues": []
}
```
- Sample fail:
```
{
  "drift": 2,
  "drift_events": 1,
  "fallback": 3,
  "errors": 1,
  "degraded": true,
  "degradation_reason": "fallbacks=3 errors=1",
  "kv_metrics": { "...": "..." },
  "issues": [
    { "domain": "tenants", "id": "default", "field": "name", "sql_value": "default", "kv_value": "diff" }
  ]
}
```
- For migration runs, use `aosctl storage migrate --domains ... --batch-size 200 --resume --checkpoint-path ./var/aos-migrate.checkpoint.json --dry-run` to validate without writes; drop `--dry-run` to execute.
- Resume semantics: checkpoint file is only written when not in `--dry-run`; re-run with `--resume` to skip processed rows deterministically.

Deterministic ordering
- KV list operations sort by created_at DESC then id ASC (tenants, adapters, stacks, plans, auth sessions, documents, collections, chat sessions; training jobs by started_at DESC).
- Per-tenant namespaces enforced via key prefixes tenant/{tenant_id}/...

Rollback
- Set STORAGE_BACKEND=sql or dual_write to re-enable SQL reads.
- KV-only guard will fall back to kv_primary with degraded flag if coverage is incomplete.

Known gaps
- Coverage snapshot is now complete; `unsupported_domains` is empty. KV-only guard downgrades only when KV fallbacks/errors counters are non-zero.
- CI: `KV Primary Mode` job runs a gated KV-only smoke (`kv_only_paths`) when coverage is empty; it reports degradation reasons (fallbacks/errors) if kv_only is downgraded.

Operational checks
- db.is_degraded() -> reason
- global_kv_metrics().snapshot() -> fallback and error counters
- kv_health_check() for KV connectivity/latency

Backup / restore (KV + Tantivy)
- Quiesce writes (or stop control plane) before backup.
- KV (redb): copy `AOS_KV_PATH` to safe storage (e.g., `cp var/aos-kv.redb /backups/aos-kv-<date>.redb`); optional: checksum with `b3sum`.
- Tantivy (if enabled): `rsync -a --delete var/aos-search/ /backups/aos-search-<date>/`.
- Restore: stop services; replace `AOS_KV_PATH` and (if used) `AOS_TANTIVY_PATH` with the backup copies; start in `kv_primary` and run `aosctl storage verify --kv-path <path>` to confirm counts; switch to `kv_only` only after fallbacks/errors stay at 0.

Compaction cadence
- redb is append-only; periodic backup+restore to a fresh file serves as compaction. Schedule at low-traffic windows (e.g., weekly), using the backup/restore steps above.

Downgrade-to-kv_primary playbook
- Trigger: kv_only guard downgrades when `fallback_operations_total > 0` or `errors_total > 0`.
- Action: leave system in kv_primary, investigate KV backend, clear counters via restart or `global_kv_metrics().reset()` in maintenance, re-test with `aosctl storage verify --json --fail-on-drift`, then re-enable kv_only only after counters remain at 0.

Alert thresholds and channels
- Background monitor: control plane evaluates `kv_alert_rules()` every 5s, logs any alert, and records `kv.fallbacks_total`, `kv.errors_total`, `kv.drift_detections_total`, `kv.degraded_events_total` into the metrics registry/dashboard.
- Default rules (`kv_alert_rules()`): Warning on any fallback or drift (>0); Critical on any backend error or degraded event (>0). Channel default: `log:kv-alerts` (forward to Slack/PagerDuty as needed).
- Sample alert payloads:
  - Warning (fallback): `{ "rule_name": "kv_fallbacks_detected", "metric": "kv.fallbacks_total", "value": 1, "severity": "Warning" }`
  - Warning (drift): `{ "rule_name": "kv_drift_detected", "metric": "kv.drift_detections_total", "value": 1, "severity": "Warning" }`
  - Critical (backend error): `{ "rule_name": "kv_backend_errors", "metric": "kv.errors_total", "value": 1, "severity": "Critical" }`
  - Critical (degraded): `{ "rule_name": "kv_degraded_events", "metric": "kv.degraded_events_total", "value": 1, "severity": "Critical" }`
- Runbook (alerts/degraded):
  1) Open Dashboard → KV health card; confirm counters.
  2) If degraded > 0, expect kv_only downgraded to kv_primary automatically; keep it there.
  3) Inspect KV logs and `kv_health_check`; run `aosctl storage verify --json --fail-on-drift`.
  4) If fallbacks/errors persist, restart worker/control plane to clear counters only after fixes; rerun verify to confirm 0s before re-enabling kv_only.

MLNavigator Inc 2025-12-05.

