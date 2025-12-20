# PRD-RECT-004: Tenant Isolation — DB Trigger Revalidation (Migration 0131)

## Problem / Motivation

We rely on database-level protections (composite FKs + triggers; referenced in docs as “migration 0131”), but drift tracking calls out that **cross-tenant leak protections in triggers were not revalidated** (`plan/drift-findings.json` `tenant-01`).

This PRD adds DB-level regression tests and (only if necessary) fixes the triggers with an additive migration.

## Goals

- Prove DB-level tenant isolation invariants with tests.
- If isolation is missing, fix it in SQLite with additive triggers/FKs and back it with tests.

## Non-Goals

- Full schema redesign.
- Large-scale migration renumbering or rewrite.

## Requirements

### R1. Add DB-level tests

Add tests that demonstrate SQLite rejects cross-tenant references for critical relationships (examples; choose the real tables):

- Adapter version in tenant A cannot reference repo/dataset/model in tenant B.
- Stack in tenant A cannot include adapter IDs from tenant B (if represented at DB layer).
- Any join tables that relate entities across tables enforce tenant_id equality.

### R2. If a gap exists, fix with additive migration

If tests fail, add a new migration (additive only) that:

- Introduces missing triggers/FKs/indexes needed to enforce tenant_id consistency.
- Does not break existing seeded/demo fixtures.

## Acceptance Criteria

- `cargo test -p adapteros-db tenant_trigger_isolation` passes.
- Tests fail in the presence of tenant isolation regressions (i.e., they’re not vacuous).
- If a new migration is added, it is additive and documented in the migration message.

## Test Plan

- Create a fresh in-memory (or temp file) SQLite DB using the normal migration runner.
- Insert minimal fixtures for two tenants.
- Attempt cross-tenant inserts/updates that should violate constraints and assert failure.

## Rollout / Risk

- Schema-level enforcement is high impact. Keep migrations narrowly scoped and additive.
- If production already contains inconsistent rows, consider adding “detect + alert” before “hard fail”; document any required cleanup steps.
