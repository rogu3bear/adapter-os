# PRD-RECT-001: Tenant Isolation — Adapter Lifecycle Queries

## Problem / Motivation

AdapterOS claims strong multi-tenant isolation, but we still have **tenant-unsafe adapter lifecycle access paths** called out in drift tracking (see `plan/drift-summary.md:10`). The codebase has tenant-scoped DB helpers (e.g. `get_adapter_for_tenant`), but some call sites still use tenant-unscoped reads and/or return cross-tenant errors that leak existence.

This PRD closes the remaining gaps for **adapter lifecycle** at the handler/service + DB call-site level, and adds regression tests.

## Goals

- Ensure **tenant-scoped endpoints never read adapter records cross-tenant**.
- Normalize semantics: on tenant mismatch, return **404** (not 403) for adapter resources unless the endpoint is explicitly “system/admin cross-tenant”.
- Add regression tests so future call sites can’t re-introduce leakage.

## Non-Goals

- Refactor the entire handler layout (e.g., splitting `handlers.rs`) beyond what’s required.
- Change database schema or add migrations (unless a critical vulnerability is discovered).
- Solve all tenant scoping for every entity type (datasets/repos/etc.) — this PRD is adapters lifecycle only.

## Requirements

### R1. Tenant-scoped adapter reads

For tenant-scoped operations, use **tenant-scoped DB methods**:

- Prefer: `Db::get_adapter_for_tenant(tenant_id, adapter_id)` over `Db::get_adapter(adapter_id)`.
- Prefer: `Db::list_adapters_for_tenant(tenant_id)` over `Db::list_all_adapters_system()` or deprecated `Db::list_adapters()`.

### R2. No existence leaks on mismatch

If a request refers to an adapter owned by another tenant, the response should be indistinguishable from “not found”:

- HTTP: `404`
- Error code: `NOT_FOUND` (or existing adapter-not-found code)
- No inclusion of foreign tenant ID in the response body.

### R3. Explicit system-level exceptions

If an endpoint is intentionally cross-tenant (admin/system tooling), it must:

- Require an explicit permission (e.g., `Permission::AdapterViewAll` if present, or existing admin/operator gating).
- Document in code comment that it is intentionally cross-tenant.
- Continue to use `validate_tenant_isolation` or equivalent admin allowlist checks.

## Implementation Notes (Concrete Targets)

Search targets (initial seed list):

- `rg -n "\\.get_adapter\\(" crates/adapteros-server-api/src`
- `rg -n "\\.find_expired_adapters\\(" crates`
- `rg -n "\\.list_adapters\\(" crates` (verify deprecated usage is not used in request paths)

High-risk/priority call sites:

- `crates/adapteros-server-api/src/inference_core.rs` (`validate_pinned_adapters_for_tenant` currently calls `get_adapter`).
- `crates/adapteros-server-api/src/services/adapter_service.rs` (uses `get_adapter`).
- `crates/adapteros-server-api/src/handlers/routing_decisions.rs` (uses `get_adapter`).
- `crates/adapteros-server-api/src/handlers/streaming.rs` (uses `get_adapter`).
- `crates/adapteros-server-api/src/handlers.rs` (large file; avoid editing unless necessary, but fix any tenant-scoped endpoints that still use unscoped adapter reads).

## Acceptance Criteria

- A tenant user cannot:
  - Load/read/delete/promote an adapter from another tenant via any tenant-scoped route.
  - Infer using pinned adapters from another tenant (should behave as adapter not found).
- All modified endpoints return `404` for cross-tenant adapter IDs unless explicitly “system/admin cross-tenant”.
- New regression tests cover:
  - Cross-tenant adapter ID returns `404` (not `403`) on at least one representative endpoint.
  - Inference pinned adapter mismatch returns a deterministic “not found” error.
- `cargo test -p adapteros-server-api` passes.

## Test Plan

- Add integration tests under `crates/adapteros-server-api/tests/tenant_isolation_adapters.rs`:
  - Create two tenants + two users/tokens.
  - Create an adapter in tenant A.
  - Attempt to access it using tenant B’s token across at least:
    - a “read” path
    - an “action” path (load/unload/delete)
  - Assert `404` and error code.
- Run:
  - `cargo test -p adapteros-server-api tenant_isolation_adapters -- --nocapture`

## Rollout / Risk

- Risk is primarily behavioral: some clients may currently rely on a `403 TENANT_ISOLATION_ERROR` for cross-tenant adapter IDs; this PR changes those to `404` for adapter resources.
- This is a security-hardening change; favor least-information behavior.
