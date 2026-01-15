# Runbook: DB Optimization Rollout

This runbook standardizes rollout for database optimizations to prevent conflicts and ensure safe deployment.

## Preconditions (before merge)

- Optimization registered in [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1)
- Impact assessment completed and attached to PR (baseline + expected deltas)
- Rollback procedure exists:
  - Prefer a rollback script under [`migrations/rollbacks/`](migrations/rollbacks/0072_tenant_snapshots_rollback.sql:1), or
  - Documented backup restore steps if rollback scripts are not safe/meaningful

## R2. Deployment Safety Procedures

### Canary deployment

**Goal**: Validate behavior under real workload patterns before broad exposure.

For adapterOS deployments that support multiple environments/nodes:

1. Apply the migration/optimization in the canary environment/node only.
2. Verify:
   - Target queries use the intended indexes (`EXPLAIN QUERY PLAN`)
   - p95/p99 latency does not regress
   - SQLite WAL growth remains stable
3. Promote to the next ring (staging → prod, or ring0 → ring1 → ring2).

For single-node deployments:

1. Take a pre-migration backup of the DB file.
2. Apply optimization.
3. Run validation queries + monitor metrics for a defined window.

### Automated rollback procedure (operational)

Rollback is defined as one of:

- **Backup restore** (primary for schema migrations in SQLite)
- **Rollback scripts** (drop indexes / revert tables) when safe and lossless enough

Operationally, rollback should be executed by the on-call owner listed in the registry entry.

### Impact assessment procedure

Minimum required checks:

- `EXPLAIN QUERY PLAN` before/after for the hot queries
- Representative read/write load test (or reuse existing perf tests)
- Confirm no tenant isolation regressions (tenant_id filters still present)

## Communication

- Announce rollout start/stop in the designated ops channel.
- Include optimization ID(s) and rollback plan.

## Post-rollout

- Update registry status to `rolled_out`.
- Record observed metrics and any follow-ups.

