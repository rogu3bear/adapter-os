# Database Optimization Coordination Framework

This document defines the coordination mechanism and operational rules for database optimizations in adapterOS.

## Scope

This applies to **any change whose primary intent is performance** (or query-plan stability), including:

- SQLite schema changes intended to optimize reads/writes (indexes, partial indexes, triggers used for performance, materialized-like tables)
- Query rewrites (SQL text changes, new code paths)
- SQLite PRAGMA changes (cache sizing, WAL/checkpoint tuning)
- Background jobs that affect planner behavior (e.g., periodic `ANALYZE`)

## R1. Coordination Framework

### 1) Ownership

Each optimization MUST have:

- A unique optimization ID (`id`)
- A single **accountable owner team** (`owner_team`)
- At least one human **owner contact** (`owner_contacts`)

The canonical registry is [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1).

### 2) Conflict detection and resolution

Optimizations declare a conflict surface via `touches`.

`touches` is intentionally simple and **string-based** so it can be validated automatically in CI.

Recommended formats:

- Index: `table_name:index_name` (example: `adapters:idx_adapters_tenant_active_tier_created`)
- Query path: `crate::module::function` (example: `adapteros_db::adapters::list_adapters_for_tenant`)
- Planner behavior: `pragma:cache_size`, `pragma:wal_autocheckpoint`

**Rule**: No two non-archived optimizations may declare the same `touches` entry.

If overlap is unavoidable:

- One optimization must be marked `archived`, OR
- The overlap must be removed by refining `touches` to a smaller surface, OR
- The work must be re-scoped into a single optimization entry (single owner), OR
- The conflict must be resolved before implementation (explicitly sequenced via `depends_on`).

### 3) Dependency tracking

Each optimization can specify:

- `depends_on`: optimization IDs that must land/rollout first
- `conflicts_with`: optimization IDs that must not be rolled out together

CI validates that all referenced IDs exist.

### 4) Rollout coordination procedures

Every optimization must include:

- `canary`: a short, actionable canary procedure
- `rollback`: a short, actionable rollback procedure
- `impact_assessment`: required pre-rollout checks

The operational runbook is [`docs/runbooks/DB_OPTIMIZATION_ROLLOUT.md`](docs/runbooks/DB_OPTIMIZATION_ROLLOUT.md:1).

## R3. Change Management Integration

### PR requirements

Any PR that includes DB optimization work must:

1. Add/update an entry in [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1)
2. Include rollback artifacts (rollback script or explicit backup restoration procedure)
3. Include an impact assessment (baseline + verification)

The PR checklist is enforced via [` .github/pull_request_template.md`](.github/pull_request_template.md:1).

## Training / operational expectations

- All engineers who touch migrations should be able to:
  - Identify whether their change is an optimization
  - Add a registry entry
  - Produce a rollback script (when applicable)
  - Run `EXPLAIN QUERY PLAN` validation and record baseline/after

