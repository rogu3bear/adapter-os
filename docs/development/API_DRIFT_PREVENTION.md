# API Drift Prevention Guide

This guide explains how AdapterOS prevents OpenAPI specification and TypeScript type drift between the backend and UI.

## Quick Reference

### When You Modify Backend API

If you change files in:
- `crates/adapteros-server-api/src/handlers/`
- `crates/adapteros-server-api/src/routes.rs`
- `crates/adapteros-api-types/src/`

Run before committing:
```bash
make fix-api-drift
```

### Pre-commit Hook Failures

**"OpenAPI spec is out of sync"**
```bash
make fix-api-drift
git add docs/api/openapi.json ui/src/api/generated.ts
```

**"TypeScript types are out of sync"**
```bash
make gen-types
git add ui/src/api/generated.ts
```

## Validation Commands

| Command | Purpose |
|---------|---------|
| `make validate-api` | Check if types are in sync (CI-style check) |
| `make fix-api-drift` | Regenerate and stage all API files |
| `make check-types-drift` | Check drift without fixing |
| `make gen-types` | Regenerate TypeScript types only |
| `cargo xtask openapi-coverage` | Check all routes have utoipa annotations |
| `make pre-commit-check` | Run all pre-commit checks manually |

## Adding New Endpoints

1. **Add handler function** with `#[utoipa::path(...)]` annotation:
   ```rust
   #[utoipa::path(
       tag = "my-feature",
       post,
       path = "/v1/my-endpoint",
       request_body = MyRequest,
       responses(
           (status = 200, description = "Success", body = MyResponse),
           (status = 400, description = "Bad request", body = ErrorResponse)
       )
   )]
   pub async fn my_handler(...) -> impl IntoResponse { ... }
   ```

2. **Register route** in `routes.rs`:
   ```rust
   .route("/v1/my-endpoint", post(handlers::my_handler))
   ```

3. **Add handler to ApiDoc** paths macro in `routes.rs`:
   ```rust
   #[openapi(
       paths(
           // ... existing paths ...
           handlers::my_handler,  // Add here
       ),
       // ...
   )]
   ```

4. **Add types to components** if you have new request/response types:
   ```rust
   components(schemas(
       // ... existing schemas ...
       crate::types::MyRequest,
       crate::types::MyResponse,
   ))
   ```

5. **Regenerate types**:
   ```bash
   make fix-api-drift
   ```

## Type Flow

```
┌─────────────────────────────────────┐
│  Rust handlers + utoipa annotations │
│  crates/adapteros-server-api/       │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│       export-openapi binary         │
│  cargo run -p adapteros-server-api  │
│       --bin export-openapi          │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│     docs/api/openapi.json           │
│     (OpenAPI 3.1 spec)              │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│       openapi-typescript            │
│     (generates TS types)            │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│    ui/src/api/generated.ts          │
│    (TypeScript types)               │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│     UI services import types        │
│     ui/src/api/services/*.ts        │
└─────────────────────────────────────┘
```

## Pre-commit Checks

The pre-commit hook (`.githooks/pre-commit`) runs these checks:

1. **[1/6] cargo check** - Compilation check
2. **[2/6] clippy** - Linting
3. **[3/6] cargo fmt** - Code formatting
4. **[4/6] SQLx cache** - Database query cache validation
5. **[5/6] OpenAPI spec drift** - Detects if spec needs regeneration
6. **[6/6] TypeScript types drift** - Detects if TS types need regeneration

The OpenAPI and TypeScript checks only run when relevant files are staged.

## CI Validation

The `api-types-drift` job in `.github/workflows/ci.yml` performs:

1. Generates fresh OpenAPI spec from Rust backend
2. Compares against committed `docs/api/openapi.json`
3. Generates fresh TypeScript types
4. Compares against committed `ui/src/api/generated.ts`
5. Runs TypeScript type check
6. Runs transformer tests
7. Validates OpenAPI spec quality (has paths and schemas)
8. Checks OpenAPI route coverage (all routes have utoipa annotations)

## Troubleshooting

### "Failed to generate OpenAPI spec"
The Rust backend has compilation errors. Fix them first:
```bash
cargo check -p adapteros-server-api
```

### "openapi-typescript not found"
Install UI dependencies:
```bash
cd ui && pnpm install
```

### "Route missing from OpenAPI documentation"
Your handler is registered in routes.rs but missing the utoipa annotation.
Add `#[utoipa::path(...)]` to the handler and include it in the ApiDoc paths() macro.

### "pnpm not found"
Install pnpm globally:
```bash
npm install -g pnpm
```
