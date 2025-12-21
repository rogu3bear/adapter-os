# Rectification PRDs (Conflict-Free Split)

This folder contains PRDs intended to be handed out as **separate PRs** that can be implemented in parallel with minimal merge-conflict risk.

## How to Use

- Assign **one PRD per PR**.
- Follow the **file boundaries** in this README strictly.
- Do not edit shared “hot” files unless explicitly allowed (notably `crates/adapteros-server-api/src/handlers.rs`).
- Prefer adding **new test files** rather than editing existing ones to avoid conflicts.

## PRD Index

| PRD | Focus | Allowed Files (high level) |
|---|---|---|
| PRD-RECT-001 | Tenant isolation: adapters lifecycle | `crates/adapteros-server-api/` (selected files), `crates/adapteros-db/src/adapters.rs`, new tests |
| PRD-RECT-002 | Worker lifecycle: tenant scoping + transitions | `crates/adapteros-server-api/src/handlers/workers.rs`, `crates/adapteros-db/src/workers.rs`, new tests |
| PRD-RECT-003 | Backend cache: eviction predictability + observability | `crates/adapteros-lora-worker/` (model cache + metrics), new tests |
| PRD-RECT-004 | DB triggers: tenant isolation revalidation | `crates/adapteros-db/tests/*`, optional `migrations/*` (exclusive to this PRD) |
| PRD-RECT-005 | Model loading: integrity + graceful error handling | `crates/adapteros-lora-worker/src/backend_factory.rs`, `model_handle_cache.rs`, `aos_worker.rs` |

## File Boundaries (No-Conflict Contract)

### PRD-RECT-001 (Tenant Isolation — Adapter Lifecycle)

**Allowed**
- `crates/adapteros-db/src/adapters.rs`
- `crates/adapteros-server-api/src/inference_core.rs`
- `crates/adapteros-server-api/src/services/adapter_service.rs`
- `crates/adapteros-server-api/src/handlers/routing_decisions.rs`
- `crates/adapteros-server-api/src/handlers/streaming.rs`
- `crates/adapteros-server-api/src/handlers.rs` (only if required; do not do unrelated cleanup)
- New test file(s):
  - `crates/adapteros-server-api/tests/tenant_isolation_adapters.rs`

**Not allowed**
- Any `migrations/*` (reserved for PRD-RECT-004)
- Any other `crates/adapteros-db/src/*` besides `adapters.rs`
- Any UI files

### PRD-RECT-002 (Worker Lifecycle — Tenant Scoping + Status Transitions)

**Allowed**
- `crates/adapteros-db/src/workers.rs`
- `crates/adapteros-server-api/src/handlers/workers.rs`
- `crates/adapteros-core/src/worker_status.rs` (only if needed to expose helpers; avoid behavior changes)
- New test file(s):
  - `crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs`

**Not allowed**
- `crates/adapteros-server-api/src/handlers.rs`
- Any `migrations/*` (reserved for PRD-RECT-004)
- Any UI files

### PRD-RECT-003 (Backend Cache — Eviction + Observability)

**Allowed**
- `crates/adapteros-lora-worker/src/model_handle_cache.rs`
- `crates/adapteros-lora-worker/src/model_key.rs`
- `crates/adapteros-lora-worker/src/metrics.rs` (or wherever worker metrics are defined)
- `crates/adapteros-lora-worker/src/backend_factory/**` (only if needed for cache key clarity)
- New test file(s):
  - `crates/adapteros-lora-worker/tests/model_handle_cache_eviction.rs`

**Not allowed**
- Any server API handlers
- Any DB code or migrations

### PRD-RECT-004 (Tenant DB Triggers — Revalidation)

This PRD is the **only one** allowed to touch migrations.

**Allowed**
- New test file(s):
  - `crates/adapteros-db/tests/tenant_trigger_isolation.rs`
- Optional (only if required):
  - `migrations/*.sql`
  - `migrations/signatures.json` (if your workflow requires updating it)

**Not allowed**
- Server API code
- Worker code
- Any existing migrations unless strictly necessary (prefer additive new migration)

### PRD-RECT-005 (Model Loading — Integrity + Graceful Error Handling)

**Allowed**
- `crates/adapteros-lora-worker/src/backend_factory.rs`
- `crates/adapteros-lora-worker/src/model_handle_cache.rs`
- `crates/adapteros-lora-worker/src/bin/aos_worker.rs`
- New test file(s):
  - `crates/adapteros-lora-worker/tests/model_handle_cache_eviction.rs` (already exists from PRD-003)

**Not allowed**
- Server API handlers
- DB code or migrations
- Control plane code
