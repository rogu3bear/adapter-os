# KV Cutover, Observability, and Rollback Runbook

## Config switches
- `db.storage_mode`: `sql_only` (default), `dual_write`, `kv_primary`, `kv_only` (guarded, will downgrade to `kv_primary` when KV fallbacks/errors are non-zero).
- `db.kv_path`: redb path (default `var/aos-kv.redb`).
- `db.kv_tantivy_path`: search index path (optional).
- Environment aliases: `AOS_STORAGE_BACKEND`/`AOS_STORAGE_MODE`, `AOS_KV_PATH`, `AOS_KV_TANTIVY_PATH`.
- `AOS_ATOMIC_DUAL_WRITE_STRICT` is enforced automatically for kv_primary/kv_only to fail/rollback on KV errors during cutover.

## Cutover path (SQL → KV)
1) **Prep**: ensure migrations applied; set `db.storage_mode=dual_write`; confirm KV reachable (`kv_health_check` and `global_kv_metrics().snapshot()`).
2) **Dual-write bake**: run normal workload; watch `fallback_*` and `errors_*` in KV metrics; resolve any degraded reason.
3) **Migration**: backfill adapters (and other KV-backed domains) using existing migration utilities; confirm KV counts match SQL.
4) **Switch to kv_primary**: update config/env; restart; verify effective mode log (falls back if guard blocks kv_only).
5) **Observe**: monitor `drift_detections_total`, `fallback_*`, `errors_*`, degraded flag, and CI `KV Primary Mode` job; zero or near-zero fallbacks required before proceeding.
6) **(Optional) kv_only**: only after fallbacks == 0; guard will downgrade to `kv_primary` and mark degraded with reason otherwise.
- Coverage gate: `kv_coverage_summary().unsupported_domains` is empty; coverage no longer blocks kv_only, but health counters still do.

## Rollback
- **Return to dual_write or sql_only**: set `db.storage_mode` accordingly and restart; SQL remains authoritative.
- **KV issues mid-flight**: guard will mark degraded; track reason via `degradation_reason()` and metrics `degraded_events_total`.
- **Purge KV (if needed)**: stop services, remove `db.kv_path` (and `kv_tantivy_path`), restart in `sql_only`, then re-enable dual_write after health check.
- **Alerts**: trigger on non-zero `fallback_*`, rising `errors_backend`, `drift_detections_total > 0`, or degraded state toggles.
- Alert wiring: use `kv_alert_rules()` + `evaluate_global_kv_alerts()` to bind fallbacks/errors/drift/degraded counters into PagerDuty/Slack channels via the alerting engine.
- Downgrade reasons for kv_only → kv_primary include coverage gaps and KV fallback/error metrics; degradation_reason string includes the counters.

## Verification / CI
- `ci.yml` now includes `KV Primary Mode` job (`AOS_STORAGE_BACKEND=kv_primary`, `AOS_KV_PATH=/tmp/aos-kv/aos-kv.redb`) running db and server-api suites.
- The same job runs a gated KV-only smoke (`kv_only_paths`) when coverage is empty; if coverage is incomplete, it logs the blocking domains and skips the kv_only run.
- Local smoke: `cargo test --package adapteros-db --tests kv_smoke` (uses KV-primary with in-memory SQL + KV, asserts no fallbacks).

Backup / restore (KV + Tantivy)
- Quiesce writes or stop control plane before backup.
- KV (redb): copy `AOS_KV_PATH` (e.g., `cp var/aos-kv.redb /backups/aos-kv-<date>.redb`); optionally checksum with `b3sum`.
- Tantivy (if enabled): `rsync -a --delete var/aos-search/ /backups/aos-search-<date>/`.
- Restore: stop services; replace KV/Tantivy paths with the backups; start in `kv_primary`, run `aosctl storage verify --kv-path <path>`; return to `kv_only` only after fallbacks/errors remain at 0.

Compaction cadence
- redb is append-only; perform a periodic backup+restore to a fresh file to reclaim space (weekly/low traffic).

Downgrade-to-kv_primary playbook
- Trigger: kv_only guard detects `fallback_operations_total > 0` or `errors_total > 0`.
- Action: accept kv_primary, investigate KV backend, reset counters (restart or `global_kv_metrics().reset()` in maintenance), verify with `aosctl storage verify --json --fail-on-drift`, then re-enable kv_only only after counters remain 0.

Alert thresholds / payloads
- Rules: Warning on fallbacks/drift (>0); Critical on backend errors/degraded (>0) via `kv_alert_rules()`/`evaluate_global_kv_alerts()`.
- Channel: default `log:kv-alerts` (fan out to Slack/PagerDuty via alerting sink).
- Sample Warning payload: `{ "rule_name": "kv_fallbacks_detected", "metric": "kv.fallbacks_total", "value": 1, "severity": "Warning" }`
- Sample Critical payload: `{ "rule_name": "kv_backend_errors", "metric": "kv.errors_total", "value": 1, "severity": "Critical" }`

MLNavigator Inc 2025-12-05.

