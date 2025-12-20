# PRD-RECT-002: Worker Lifecycle — Tenant Scoping + Status Transitions

## Problem / Motivation

We document worker lifecycle ordering (`created → registered → healthy → draining → stopped/error`), but drift tracking notes the **DB/telemetry mapping and tenant scoping were not validated** (`plan/drift-findings.json` `lifecycle-01`).

This PRD makes worker lifecycle behavior **provably tenant-scoped** at API boundaries and ensures persisted status transitions respect the `WorkerStatus` transition contract.

## Goals

- Ensure any worker lifecycle operation that is tenant-scoped cannot be performed cross-tenant.
- Ensure persisted worker status changes are validated against `WorkerStatus` allowed transitions.
- Add targeted tests that pin the behavior.

## Non-Goals

- Re-architect worker orchestration or introduce multi-node scheduling.
- Add migrations unless strictly required to fix a correctness/security issue.

## Requirements

### R1. Tenant-scoped worker lookup

Any handler that accepts `worker_id` and is tenant-scoped must use a tenant-scoped lookup path:

- Add `Db::get_worker_for_tenant(tenant_id, worker_id)` (or equivalent) and use it where appropriate.
- If a worker exists but belongs to another tenant, respond **404** (avoid existence leak).

### R2. Enforced status transitions

When persisting status changes:

- Validate transitions using `adapteros_core::WorkerStatus` (e.g. `WorkerStatus::transition_to`).
- Reject invalid transitions with a clear error code (and do not mutate DB state).

### R3. Telemetry carries tenant context

Worker lifecycle telemetry events emitted by the control plane must include the correct `tenant_id` in the `IdentityEnvelope` (no `system` unless truly system-level).

## Acceptance Criteria

- Cross-tenant access to a worker by ID returns `404` for tenant-scoped worker endpoints.
- Invalid status transitions (e.g., `healthy → created`) are rejected and do not update the DB.
- `cargo test -p adapteros-server-api worker_lifecycle_tenant_scoping` passes.

## Test Plan

In `crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs`:

- Create two tenants.
- Insert a worker for tenant A into the DB.
- Call a tenant-scoped worker endpoint as tenant B and assert `404`.
- Attempt an invalid transition via the handler and assert:
  - response indicates invalid transition
  - DB status unchanged

## Rollout / Risk

- Low risk: changes are scoped to lifecycle endpoints and validations.
- Watch for any internal/admin tooling that intentionally uses cross-tenant worker operations; those should use explicit “system” routes, not tenant-scoped ones.
