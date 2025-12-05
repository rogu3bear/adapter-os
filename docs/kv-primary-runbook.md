KV Primary / KV-Only Runbook
============================

Modes
- sql_only: SQLite only
- dual_write: SQL primary, KV mirror
- kv_primary: KV read/write, SQL fallback allowed
- kv_only: KV only (blocks if coverage incomplete; falls back to kv_primary with degradation note)

Config (env or cp.toml)
- AOS_STORAGE_BACKEND=kv_primary|kv_only|dual_write|sql
- AOS_KV_PATH=var/aos-kv.redb
- AOS_TANTIVY_PATH=var/aos-search (optional)
- DATABASE_URL / AOS_DATABASE_URL for SQL modes

Bootstrap (KV-only or KV-primary)
- Start control plane: STORAGE_BACKEND=kv_only AOS_KV_PATH=... make dev
- Guard may downgrade to kv_primary if unsupported domains remain; check degradation_reason.
- System bootstrap: ensures KV backend opened; in kv_only no SQLite migrations run.
- Auth bootstrap: use dev bootstrap or manual user creation via KV (users/auth sessions KV repos).

Dual-write drift checks
- Enable dual_write; run diff tool (todo) comparing KV vs SQL for tenants/users/adapters/stacks/plans.
- Monitor kv_metrics fallback counters and degradation_reason for downgrades.

Deterministic ordering
- KV list operations sort by created_at DESC then id ASC (tenants, stacks, plans, auth sessions).
- Per-tenant namespaces enforced via key prefixes tenant/{tenant_id}/...

Rollback
- Set STORAGE_BACKEND=sql or dual_write to re-enable SQL reads.
- KV-only guard will fall back to kv_primary with degraded flag if coverage is incomplete.

Known gaps
- Policy bindings remain SQL-backed; KV-only skips policy seed.
- Additional domains (documents, messages, rag, replay, telemetry) remain SQL-only.

Operational checks
- db.is_degraded() -> reason
- global_kv_metrics().snapshot() -> fallback and error counters
- kv_health_check() for KV connectivity/latency

MLNavigator Inc 2025-12-05.

