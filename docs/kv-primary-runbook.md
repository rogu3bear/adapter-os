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
- KV-only stays blocked until `kv_coverage_summary().unsupported_domains` is empty; once empty, KV-only is allowed even without an SQL pool.
- `AOS_ATOMIC_DUAL_WRITE_STRICT` is auto-enforced in `kv_primary` and `kv_only`; dual_write remains best-effort elsewhere.

Bootstrap (KV-only or KV-primary)
- Start control plane: STORAGE_BACKEND=kv_only AOS_KV_PATH=... make dev
- Guard may downgrade to kv_primary if unsupported domains remain; check degradation_reason.
- System bootstrap: `Db::ensure_system_tenant()` creates the `system` tenant and seeds core policies (egress/determinism/isolation/evidence) via KV; no SQLite required.
- Auth bootstrap: dev bootstrap endpoint works with KV-only (users/auth sessions/policy bindings in KV); grants are skipped if SQL pool is absent.

Dual-write drift checks
- Enable dual_write; run diff tool comparing KV vs SQL for tenants/users/adapters/stacks/plans/policy bindings/users.
- CLI: `aosctl storage verify --kv-path <path>` (flags: `--adapters-only`, `--tenants-only`, `--stacks-only`).
- Monitor kv_metrics fallback counters and degradation_reason for downgrades.
- KV alerting: `kv_alert_rules()` + `evaluate_global_kv_alerts()` wire drift/fallback/error/degraded counters into the alert engine (defaults emit Warning on fallbacks/drift, Critical on backend errors/degraded events).
- KV-only guard downgrades to kv_primary when coverage gaps exist or KV fallback/error counters are non-zero; degradation_reason includes the counts.

CI drift check
- Add to CI: `make kv-verify` (runs `aosctl storage verify --json --domains adapters,tenants,stacks,plans,auth_sessions,runtime_sessions,rag_artifacts --fail-on-drift`), fails on drift without repairing.
- For migration runs, use `aosctl storage migrate --domains ... --batch-size 200 --resume --checkpoint-path ./var/aos-migrate.checkpoint.json --dry-run` to validate without writes; drop `--dry-run` to execute.
- Resume semantics: checkpoint file is only written when not in `--dry-run`; re-run with `--resume` to skip processed rows deterministically.

Deterministic ordering
- KV list operations sort by created_at DESC then id ASC (tenants, adapters, stacks, plans, auth sessions, documents, collections, chat sessions; training jobs by started_at DESC).
- Per-tenant namespaces enforced via key prefixes tenant/{tenant_id}/...

Rollback
- Set STORAGE_BACKEND=sql or dual_write to re-enable SQL reads.
- KV-only guard will fall back to kv_primary with degraded flag if coverage is incomplete.

Known gaps
- Remaining SQL-only domains (documents, collections, policy_audit, training_jobs, chat_sessions) block kv_only; guard downgrades to kv_primary with a reason.
- CI: `KV Primary Mode` job now also runs a gated KV-only smoke (`kv_only_paths`) when coverage is empty; otherwise it reports the blocking domains.

Operational checks
- db.is_degraded() -> reason
- global_kv_metrics().snapshot() -> fallback and error counters
- kv_health_check() for KV connectivity/latency

MLNavigator Inc 2025-12-05.

