# adapterOS Route Map

> **Developer Reference:** Maps API routes to handler implementations.
>
> Last updated: January 2026

This document helps developers navigate the API codebase by mapping routes to their implementations.

## Overview

| Metric | Count |
|--------|-------|
| **Total Routes** | ~189 |
| **Handler Files** | ~97 |
| **Handler Subdirectories** | 7 |
| **Middleware Modules** | 13 |

## Key Files

| File | Lines | Description |
|------|-------|-------------|
| `routes.rs` | ~2,548 | OpenAPI route registration (utoipa) |
| `handlers.rs` | ~4,098 | Handler module re-exports |
| `middleware/mod.rs` | ~1,282 | Middleware stack configuration |
| `state.rs` | ~1,575 | AppState dependency container |
| `auth.rs` | ~1,525 | JWT authentication |

## Handler Organization

### Handler Subdirectories

| Directory | Files | Purpose |
|-----------|-------|---------|
| `handlers/adapters/` | 21 | Adapter CRUD, versions, health |
| `handlers/auth_enhanced/` | 6 | Login, MFA, sessions, token refresh |
| `handlers/chat_sessions/` | 13 | Chat CRUD, messages, evidence, contacts |
| `handlers/datasets/` | 18 | Upload, chunked transfers, validation |
| `handlers/aliases/` | 3 | Backward compatibility route aliases |
| `handlers/monitoring/` | 1 | Monitoring endpoints |
| `handlers/streams/` | 1 | SSE streaming handlers |

### Major Handler Files

| File | Purpose |
|------|---------|
| `training.rs` | Training orchestration, checkpoints, metrics |
| `streaming_infer.rs` | SSE streaming inference with progress |
| `workspaces.rs` | Workspace CRUD, member management |
| `models.rs` | Model lifecycle, load/unload, validation |
| `replay.rs` | Determinism replay & inference history |
| `adapter_stacks.rs` | Stack composition, activation, history |
| `promotion.rs` | Promotion requests, approvals, rollback |
| `diagnostics.rs` | Determinism diagnostics, event diffing |

## Route Categories

### Authentication (`/v1/auth/*`)
- **Handler**: `handlers/auth.rs`, `handlers/auth_enhanced/`
- **Endpoints**: login, logout, refresh, MFA, sessions

### Inference (`/v1/infer/*`)
- **Handler**: `handlers/streaming_infer.rs`, `handlers/batch.rs`
- **Endpoints**: streaming inference, batch jobs

### Adapters (`/v1/adapters/*`)
- **Handler**: `handlers/adapters/`
- **Endpoints**: CRUD, versions, health, loading

### Training (`/v1/training/*`)
- **Handler**: `handlers/training.rs`
- **Endpoints**: jobs, datasets, checkpoints, metrics

### Chat (`/v1/chat/*`)
- **Handler**: `handlers/chat_sessions/`
- **Endpoints**: sessions, messages, evidence

### System (`/healthz`, `/readyz`, `/api/system/*`)
- **Handler**: `handlers/health.rs`, `handlers/system_status.rs`
- **Endpoints**: liveness, readiness, status

### Admin (`/v1/tenants/*`, `/v1/workspaces/*`)
- **Handler**: `handlers/tenants.rs`, `handlers/workspaces.rs`
- **Endpoints**: tenant CRUD, workspace management

## Change Dependencies

### Adding a New Route

1. **Create handler** in `handlers/*.rs`
   ```rust
   pub async fn my_handler(
       State(state): State<AppState>,
       user: User,
   ) -> Result<Json<Response>, ApiError> { ... }
   ```

2. **Export handler** in `handlers.rs`
   ```rust
   pub mod my_handler;
   ```

3. **Register route** in `routes.rs`
   ```rust
   #[openapi(
       paths(
           // ...existing paths...
           handlers::my_handler::my_handler,
       ),
   )]
   ```

4. **Add types** to `routes.rs` components
   ```rust
   components(
       schemas(
           // ...existing schemas...
           MyRequest,
           MyResponse,
       )
   )
   ```

5. **Regenerate OpenAPI spec**
   ```bash
   ./scripts/ci/check_openapi_drift.sh --fix
   ```

### Modifying Request/Response Types

1. Update types in `adapteros-api-types`
2. Run `./scripts/ci/check_openapi_drift.sh --fix`
3. Rebuild WASM UI: `cd crates/adapteros-ui && trunk build --release`

### Changing Auth Requirements

1. Update middleware in route registration
2. Update `smoke_e2e.sh` test expectations
3. Update API documentation

## Test Coverage

| Route Category | Test Location |
|----------------|---------------|
| Auth | `smoke_e2e.sh:162-167` |
| Inference | `e2e_inference_harness.rs` |
| Adapters | `e2e_adapter_lifecycle.rs` |
| Training | `e2e_training_workflow.rs` |
| System | `smoke_e2e.sh:149-155` |

## Middleware Stack

Routes pass through these middleware layers (in order):

1. CORS
2. Compression
3. Tracing
4. Request ID injection
5. Request size limits
6. Security headers
7. Rate limiting
8. Request tracking
9. Client IP extraction
10. Auth (required or optional)
11. Idempotency
12. Context injection
13. Audit logging
14. Policy enforcement
15. Error code enforcement

---

*For full API reference, see `docs/API_REFERENCE.md`*
*For OpenAPI spec, see `docs/api/openapi.json`*
