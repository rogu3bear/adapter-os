# KV Cutover, Observability, and Rollback Runbook

## Config switches
- `db.storage_mode`: `sql_only` (default), `dual_write`, `kv_primary`, `kv_only` (guarded, will downgrade to `kv_primary` if unsupported domains remain).
- `db.kv_path`: redb path (default `var/aos-kv.redb`).
- `db.kv_tantivy_path`: search index path (optional).
- Environment aliases: `AOS_STORAGE_BACKEND`/`AOS_STORAGE_MODE`, `AOS_KV_PATH`, `AOS_KV_TANTIVY_PATH`.

## Cutover path (SQL → KV)
1) **Prep**: ensure migrations applied; set `db.storage_mode=dual_write`; confirm KV reachable (`kv_health_check` and `global_kv_metrics().snapshot()`).
2) **Dual-write bake**: run normal workload; watch `fallback_*` and `errors_*` in KV metrics; resolve any degraded reason.
3) **Migration**: backfill adapters (and other KV-backed domains) using existing migration utilities; confirm KV counts match SQL.
4) **Switch to kv_primary**: update config/env; restart; verify effective mode log (falls back if guard blocks kv_only).
5) **Observe**: monitor `drift_detections_total`, `fallback_*`, `errors_*`, degraded flag, and CI `KV Primary Mode` job; zero or near-zero fallbacks required before proceeding.
6) **(Optional) kv_only**: only after fallbacks == 0 and guard reports no unsupported domains; guard will downgrade to `kv_primary` and mark degraded with reason otherwise.

## Rollback
- **Return to dual_write or sql_only**: set `db.storage_mode` accordingly and restart; SQL remains authoritative.
- **KV issues mid-flight**: guard will mark degraded; track reason via `degradation_reason()` and metrics `degraded_events_total`.
- **Purge KV (if needed)**: stop services, remove `db.kv_path` (and `kv_tantivy_path`), restart in `sql_only`, then re-enable dual_write after health check.
- **Alerts**: trigger on non-zero `fallback_*`, rising `errors_backend`, `drift_detections_total > 0`, or degraded state toggles.

## Verification / CI
- `ci.yml` now includes `KV Primary Mode` job (`AOS_STORAGE_BACKEND=kv_primary`, `AOS_KV_PATH=/tmp/aos-kv/aos-kv.redb`) running db and server-api suites.
- Local smoke: `cargo test --package adapteros-db --tests kv_smoke` (uses KV-primary with in-memory SQL + KV, asserts no fallbacks).

MLNavigator Inc 2025-12-05.

