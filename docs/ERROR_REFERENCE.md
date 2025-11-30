# Compilation Error Reference
## Quick Fix Guide for AdapterOS Build Failures

Generated: 2025-11-22
Status: 59 Errors - Blocking Compilation

---

## Critical Error #1: Missing Database Methods (8 errors)

### Problem
The following database methods are called but not implemented:
- `log_model_operation()` - 2 occurrences
- `update_model_operation()` - 2 occurrences
- `list_user_tutorial_statuses()` - 1 occurrence
- `mark_tutorial_completed()` - 1 occurrence
- `unmark_tutorial_completed()` - 1 occurrence
- `mark_tutorial_dismissed()` - 1 occurrence
- `unmark_tutorial_dismissed()` - 1 occurrence

### Error Messages
```
error[E0599]: no method named `log_model_operation` found for struct `Db`
   --> crates/adapteros-server-api/src/handlers/models.rs:185:10

error[E0599]: no method named `update_model_operation` found for struct `Db`
   --> crates/adapteros-server-api/src/handlers/models.rs:384:18

error[E0599]: no method named `list_user_tutorial_statuses` found for struct `Db`
   --> crates/adapteros-server-api/src/handlers/tutorials.rs:117:10
```

### Root Cause
New handler code was added to the API layer without corresponding persistence layer implementation. The database trait `Db` does not define these methods.

### Solution

#### Step 1: Add Methods to Database Trait
**File:** `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs`

Find the `impl Db` block and add these method signatures:

```rust
pub async fn log_model_operation(
    &self,
    tenant_id: &str,
    model_id: &str,
    operation_type: &str,
    user_id: &str,
    status: &str,
    error_message: Option<String>,
    timestamp: &str,
    started_at: Option<String>,
    completed_at: Option<String>,
) -> Result<String> {
    // Returns operation_id
    todo!("Implement model operation logging")
}

pub async fn update_model_operation(
    &self,
    operation_id: &str,
    status: &str,
    error_message: Option<String>,
    completed_at: Option<String>,
) -> Result<()> {
    todo!("Implement model operation update")
}

pub async fn list_user_tutorial_statuses(
    &self,
    user_id: &str,
) -> Result<Vec<TutorialStatus>> {
    todo!("Implement tutorial status listing")
}

pub async fn mark_tutorial_completed(
    &self,
    user_id: &str,
    tutorial_id: &str,
) -> Result<()> {
    todo!("Implement mark tutorial completed")
}

pub async fn unmark_tutorial_completed(
    &self,
    user_id: &str,
    tutorial_id: &str,
) -> Result<()> {
    todo!("Implement unmark tutorial completed")
}

pub async fn mark_tutorial_dismissed(
    &self,
    user_id: &str,
    tutorial_id: &str,
) -> Result<()> {
    todo!("Implement mark tutorial dismissed")
}

pub async fn unmark_tutorial_dismissed(
    &self,
    user_id: &str,
    tutorial_id: &str,
) -> Result<()> {
    todo!("Implement unmark tutorial dismissed")
}
```

#### Step 2: Create Migration Files

**File:** `/Users/star/Dev/aos/migrations/0081_model_operations.sql`

```sql
-- Model operation tracking table
-- Tracks all model loading/unloading operations for audit trail

CREATE TABLE model_operations (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    operation_type TEXT NOT NULL,  -- 'load', 'unload', 'update', etc
    user_id TEXT NOT NULL,
    status TEXT NOT NULL,          -- 'in_progress', 'completed', 'failed'
    error_message TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id),
    UNIQUE(tenant_id, model_id, id)
);

CREATE INDEX idx_model_ops_tenant ON model_operations(tenant_id);
CREATE INDEX idx_model_ops_model ON model_operations(model_id);
CREATE INDEX idx_model_ops_status ON model_operations(status);
```

**File:** `/Users/star/Dev/aos/migrations/0082_tutorial_statuses.sql`

```sql
-- Tutorial progress tracking
-- Tracks which tutorials users have completed, dismissed, or viewed

CREATE TABLE tutorial_statuses (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tutorial_id TEXT NOT NULL,
    completed BOOLEAN DEFAULT FALSE,
    dismissed BOOLEAN DEFAULT FALSE,
    viewed BOOLEAN DEFAULT FALSE,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    dismissed_at TEXT,
    UNIQUE(user_id, tutorial_id)
);

CREATE INDEX idx_tutorial_status_user ON tutorial_statuses(user_id);
CREATE INDEX idx_tutorial_status_tutorial ON tutorial_statuses(tutorial_id);
CREATE INDEX idx_tutorial_status_completed ON tutorial_statuses(completed);
```

#### Step 3: Run Migration
```bash
./target/release/aosctl db migrate
```

---

## Critical Error #2: ErrorResponse Schema Mismatch (6 errors)

### Problem
`ErrorResponse` struct was modified to include a required `schema_version` field, but call sites weren't updated.

### Error Messages
```
error[E0063]: missing field `schema_version` in initializer of `adapteros_api_types::ErrorResponse`
   --> crates/adapteros-server-api/src/handlers/services.rs:68:39

error[E0599]: `(reqwest::StatusCode, axum::Json<adapteros_api_types::ErrorResponse>)` doesn't implement `std::fmt::Display`
   --> crates/adapteros-server-api/src/handlers/services.rs:68:39
    |
68  |             Json(ErrorResponse::new(e.to_string()).with_code("FORBIDDEN")),
```

### Root Cause
1. `ErrorResponse::new()` now requires `schema_version` field
2. Error handling code tries to call `.to_string()` on tuple `(StatusCode, Json<ErrorResponse>)` which doesn't implement Display

### Solution

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/services.rs`

Fix all 6 occurrences (lines 68, 122, 176, 225, 270, 322):

**Before:**
```rust
require_permission(&claims, Permission::NodeManage).map_err(|e| {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(e.to_string()).with_code("FORBIDDEN")),
    )
})?;
```

**After:**
```rust
require_permission(&claims, Permission::NodeManage).map_err(|_e| {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            code: "FORBIDDEN".to_string(),
            message: "Permission denied".to_string(),
            schema_version: "1.0".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }),
    )
})?;
```

Or if you have a helper function:
```rust
fn forbidden_response() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            code: "FORBIDDEN".to_string(),
            message: "Permission denied".to_string(),
            schema_version: "1.0".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }),
    )
}
```

Then use:
```rust
require_permission(&claims, Permission::NodeManage).map_err(|_e| forbidden_response())?;
```

---

## Critical Error #3: Missing Type Definition

### Problem
`OperationProgressEvent` type is imported but not defined.

### Error Message
```
error[E0432]: unresolved import `crate::types::OperationProgressEvent`
   --> crates/adapteros-server-api/src/handlers/models.rs:XXX:XX
```

### Solution

**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/types.rs`

Add this type definition:

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OperationProgressEvent {
    pub operation_id: String,
    pub status: String,  // 'in_progress', 'completed', 'failed'
    pub progress_percentage: f32,
    pub error_message: Option<String>,
    pub timestamp: String,
}
```

---

## Medium Error #4: Missing Dependencies

### Problem
`once_cell` and `tokio-util` crates are used but not declared in Cargo.toml.

### Error Messages
```
error[E0433]: failed to resolve: use of unresolved module or crate `once_cell`
error[E0433]: failed to resolve: use of unresolved module or crate `tokio_util`
```

### Solution

**File:** `/Users/star/Dev/aos/Cargo.toml`

Add to dependencies:

```toml
[dependencies]
# ... existing dependencies ...
once_cell = "1.19"
tokio-util = "0.7"
```

Then run:
```bash
cargo update
```

---

## Medium Error #5: Module Not Exported

### Problem
The `tutorials` module exists in adapteros-db but is not exported.

### Error Message
```
error[E0433]: failed to resolve: use of unresolved module `tutorials` in `adapteros_db`
```

### Solution

**File:** `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs`

Find the module declarations and add:

```rust
pub mod tutorials;  // Add this line with other module declarations
```

---

## High Error #6: Missing OpenAPI Paths

### Problem
utoipa macros reference handler functions that don't exist or aren't properly annotated.

### Error Messages
```
error[E0412]: cannot find type `__path_auth_logout` in module `handlers`
error[E0412]: cannot find type `__path_auth_me` in module `handlers`
```

### Solution

Option A: Implement the missing handlers
**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/auth.rs`

```rust
/// User logout
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    responses(
        (status = 200, description = "Logged out successfully", body = AuthLogoutResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn auth_logout(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<AuthLogoutResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(AuthLogoutResponse {
        message: "Logged out successfully".to_string(),
    }))
}

/// Get current user info
#[utoipa::path(
    get,
    path = "/v1/auth/me",
    responses(
        (status = 200, description = "User info retrieved", body = Claims),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn auth_me(
    Extension(claims): Extension<Claims>,
) -> Result<Json<Claims>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(claims))
}
```

Option B: Remove references from routes.rs
**File:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/routes.rs`

Comment out or remove:
```rust
// #[path = "/api-docs/openapi.json"]
// pub fn ...
//     __path_auth_logout,
//     __path_auth_me,
// ...
```

---

## Warning Issues (Non-blocking)

### Missing Documentation (53 warnings in adapteros-types)
Add doc comments to enum variants:
```rust
/// Variant description
VariantName,
```

### Unnecessary Unsafe Blocks (14 warnings in adapteros-ingest-docs)
Remove `unsafe` keyword where not actually needed.

### Unused Imports (15+ warnings)
Use `cargo fix --allow-dirty` to auto-remove, or manually delete.

### Deprecated Method Calls (3 warnings in lora-router)
```rust
// Before
router.route(...)

// After
router.route_with_adapter_info(...)
```

---

## Build Verification Steps

After applying all fixes:

```bash
# Step 1: Clean build artifacts
cargo clean

# Step 2: Full release build
cargo build --release -p adapteros-server-api

# Step 3: Run tests
cargo test --no-run -p adapteros-server-api

# Step 4: Check for warnings
cargo clippy --workspace -- -D warnings

# Step 5: Format code
cargo fmt --all
```

### Success Criteria
- All 59 errors resolved
- Code compiles without errors
- Warning count reduced significantly
- All tests pass

---

## File Reference by Error Type

### Must Fix for Compilation
1. `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs` - Add 8 methods
2. `/Users/star/Dev/aos/migrations/0081_model_operations.sql` - New
3. `/Users/star/Dev/aos/migrations/0082_tutorial_statuses.sql` - New
4. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/services.rs` - Fix error handling
5. `/Users/star/Dev/aos/crates/adapteros-server-api/src/types.rs` - Add OperationProgressEvent
6. `/Users/star/Dev/aos/Cargo.toml` - Add dependencies

### Should Fix for API Functionality
7. `/Users/star/Dev/aos/crates/adapteros-server-api/src/routes.rs` - Fix OpenAPI references
8. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/auth.rs` - Implement handlers

### Can Fix Later (Warnings)
9. `/Users/star/Dev/aos/crates/adapteros-types/src/lib.rs` - Documentation
10. `/Users/star/Dev/aos/crates/adapteros-ingest-docs/src/lib.rs` - Unsafe blocks
11. `/Users/star/Dev/aos/crates/adapteros-lora-router/src/lib.rs` - Deprecated calls

---

## Estimated Resolution Time

- Database methods implementation: 30-45 min
- Migration files: 15-20 min
- Type/schema fixes: 20-30 min
- Dependency updates: 5-10 min
- Testing and verification: 30-45 min

**Total: 2-3 hours**

---

See `/Users/star/Dev/aos/COMPILATION_RESULTS.md` for detailed analysis and additional context.
