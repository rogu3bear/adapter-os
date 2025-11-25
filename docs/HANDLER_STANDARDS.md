# Handler Standards for AdapterOS API

**Purpose:** Establish consistent patterns for Axum handler functions to prevent compilation errors and ensure maintainability.

**Last Updated:** 2025-01-20

---

## Return Type Standard

### Required Pattern

All handlers MUST return:
```rust
Result<Json<T>, (StatusCode, Json<ErrorResponse>)>
```

### Forbidden Pattern

DO NOT return:
```rust
Result<(StatusCode, Json<T>), ...>
```

**Reason:** Axum's `Handler` trait inference fails with tuple return types in the `Ok` variant, causing compilation errors when registering routes.

### Examples

**Correct:**
```rust
pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    // ... handler logic ...
    Ok(Json(TenantResponse { ... }))
}
```

**Incorrect:**
```rust
pub async fn create_stack(
    // ...
) -> Result<(StatusCode, Json<StackResponse>), (StatusCode, Json<ErrorResponse>)> {
    // This causes Handler trait inference failure
    Ok((StatusCode::CREATED, Json(StackResponse { ... })))
}
```

---

## Error Handling Pattern

### Error Conversion

Use `map_err` to convert errors to the standard error tuple:

```rust
let result = state.db.insert_stack(&db_req).await.map_err(|e| {
    if e.to_string().contains("UNIQUE constraint failed") {
        (
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("Stack name already exists")
                .with_code("CONFLICT")),
        )
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to create stack: {}", e))
                .with_code("DATABASE_ERROR")),
        )
    }
})?;
```

### Permission Checks

Use `require_permission` which already returns the correct error type:

```rust
require_permission(&claims, Permission::AdapterRegister)?;
```

This automatically propagates `(StatusCode::FORBIDDEN, Json<ErrorResponse>)` on failure.

---

## Status Code Handling

### StatusCode::CREATED (201)

**Important:** OpenAPI annotations (`#[utoipa::path]`) are documentation only - they do NOT set HTTP status codes at runtime. Axum requires explicit status code handling.

To return 201 CREATED while avoiding Handler trait inference issues, use `Response` return type with `IntoResponse`:

```rust
use axum::response::{Response, IntoResponse};

pub async fn create_stack(...) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // ... handler logic ...
    
    let json_response = Json(StackResponse { ... });
    
    // Convert to Response with 201 status code
    Ok((StatusCode::CREATED, json_response).into_response())
}
```

**Alternative (simpler):** Return `Json<T>` and accept 200 OK (matches `create_tenant` pattern):

```rust
pub async fn create_stack(...) -> Result<Json<StackResponse>, ...> {
    // Returns 200 OK by default
    Ok(Json(StackResponse { ... }))
}
```

**Note:** `create_tenant` and other stable handlers return 200 OK. For REST best practices, 201 CREATED is preferred for creation endpoints, but 200 OK is acceptable for consistency.

---

## Tenant ID Requirement

All service calls that accept `tenant_id` MUST include it:

```rust
state.training_service.start_training(
    adapter_name,
    config,
    template_id,
    repo_id,
    dataset_id,
    Some(claims.tenant_id.clone()), // Always include tenant_id
).await?;
```

**Reason:** Ensures proper tenant isolation and audit trail.

---

## Reference Examples

### Stable Handlers (Follow These Patterns)

1. **`create_tenant`** - `crates/adapteros-server-api/src/handlers/tenants.rs:47-105`
   - Returns `Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)>`
   - Uses `require_role` for permission checks
   - Converts database errors with `map_err`

2. **`create_workspace`** - `crates/adapteros-server-api/src/handlers/workspaces.rs`
   - Returns `Result<Json<WorkspaceResponse>, (StatusCode, Json<ErrorResponse>)>`
   - Includes audit logging

3. **`create_message`** - `crates/adapteros-server-api/src/handlers/messages.rs:34-125`
   - Returns `Result<Json<MessageResponse>, (StatusCode, Json<ErrorResponse>)>`
   - Includes workspace access checks

### Training Service Pattern

**Correct `start_training` call:**
```rust
state.training_service.start_training(
    req.adapter_name.clone(),
    config,
    req.template_id.clone(),
    req.repo_id.clone(),
    req.dataset_id.clone(),
    Some(claims.tenant_id.clone()), // 6th parameter - required
).await?;
```

**Reference:** `crates/adapteros-server-api/src/handlers/training.rs:202-209`

---

## Common Mistakes

### Mistake 1: Tuple Return Type
```rust
// WRONG - causes Handler trait inference failure
Ok((StatusCode::CREATED, Json(response)))
```

**Fix:** Remove StatusCode from Ok variant, rely on OpenAPI annotation.

### Mistake 2: Missing Tenant ID
```rust
// WRONG - missing 6th parameter
.start_training(name, config, template_id, repo_id, dataset_id)
```

**Fix:** Add `Some(claims.tenant_id.clone())` as 6th parameter.

### Mistake 3: Inconsistent Error Types
```rust
// WRONG - mixing error types
.map_err(|e| AosError::Database(e.to_string()))?;
```

**Fix:** Convert to `(StatusCode, Json<ErrorResponse>)` tuple.

---

## Linting and Enforcement

### Code Comments

All handler modules should include a comment block documenting these standards:

```rust
//! Handler Return Type Standards
//!
//! All handlers MUST return: Result<Json<T>, (StatusCode, Json<ErrorResponse>)>
//! DO NOT return: Result<(StatusCode, Json<T>), ...> (causes Handler trait inference issues)
```

### Manual Review Checklist

Before submitting PRs with new handlers:

- [ ] Return type is `Result<Json<T>, (StatusCode, Json<ErrorResponse>)>`
- [ ] No `StatusCode` in `Ok` variant
- [ ] All service calls include `tenant_id` parameter
- [ ] Errors converted with `map_err` to standard tuple format
- [ ] Permission checks use `require_permission` or `require_role`

---

## Related Documentation

- [CLAUDE.md](../CLAUDE.md) - General development standards
- [RBAC.md](RBAC.md) - Permission and role definitions
- [API Reference](../crates/adapteros-server-api/src/routes.rs) - Route registration patterns

---

**Maintained by:** AdapterOS Development Team  
**Last Reviewed:** 2025-01-20

