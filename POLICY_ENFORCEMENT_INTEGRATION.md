# Policy Enforcement Middleware Integration Guide

## Quick Start: Wire into routes.rs

### Step 1: Add Import

In `crates/adapteros-server-api/src/routes.rs`, add to the imports section (around line 5):

```rust
use crate::middleware::{
    auth_middleware,
    client_ip_middleware,
    policy_enforcement_middleware,  // ADD THIS LINE
};
```

### Step 2: Add Middleware Layer (Recommended Approach)

Find the `protected_routes` definition (around line 555) and add the policy enforcement layer **AFTER** the auth middleware layer:

```rust
// Protected routes (require auth)
let protected_routes = Router::new()
    .route("/v1/auth/logout", post(auth::auth_logout))
    .route("/v1/auth/me", get(auth::auth_me))
    // ... all other protected routes ...

    // FIND THIS BLOCK (around line 1552):
    .layer(middleware::from_fn_with_state(
        state.clone(),
        auth_middleware,
    ))

    // ADD THIS BLOCK IMMEDIATELY AFTER:
    .layer(middleware::from_fn_with_state(
        state.clone(),
        policy_enforcement_middleware,
    ));
```

**Complete example:**

```rust
let protected_routes = Router::new()
    .route("/v1/auth/logout", post(auth::auth_logout))
    .route("/v1/auth/me", get(auth::auth_me))
    .route("/v1/tenants", get(handlers::list_tenants).post(handlers::create_tenant))
    // ... hundreds of other routes ...
    .route("/v1/dashboard/config", get(handlers::dashboard::get_dashboard_config))
    .route("/v1/dashboard/config", post(handlers::dashboard::update_dashboard_config))
    .route("/v1/dashboard/config/reset", post(handlers::dashboard::reset_dashboard_config))
    // Auth middleware (validates JWT, sets Claims)
    .layer(middleware::from_fn_with_state(
        state.clone(),
        auth_middleware,
    ))
    // Policy enforcement middleware (validates against policy packs)
    .layer(middleware::from_fn_with_state(
        state.clone(),
        policy_enforcement_middleware,
    ));
```

### Why This Order?

Axum applies middleware in **reverse order**:

```
Request Flow:
1. Request enters protected_routes
2. policy_enforcement_middleware runs FIRST
3. auth_middleware runs SECOND
4. Handler runs LAST

Response Flow (reverse):
1. Handler returns response
2. auth_middleware processes response
3. policy_enforcement_middleware processes response
4. Response sent to client
```

This ensures:
- Policy enforcement has access to `Claims` (set by auth_middleware)
- Policy violations are caught BEFORE the handler executes
- Blocked requests never reach the handler

## Alternative: Global Application

If you want to apply policy enforcement to **all routes** (including public routes), add it to the main router stack:

```rust
// In routes.rs, around line 1559
Router::new()
    .merge(public_routes)
    .merge(metrics_route)
    .merge(protected_routes)
    .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
    // Apply layers (innermost to outermost):
    .layer(TraceLayer::new_for_http())
    .layer(CompressionLayer::new())
    .layer(cors_layer())
    .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limiting_middleware))
    .layer(axum::middleware::from_fn(request_size_limit_middleware))
    .layer(axum::middleware::from_fn(security_headers_middleware))
    .layer(axum::middleware::from_fn(caching::caching_middleware))
    .layer(axum::middleware::from_fn(versioning::versioning_middleware))
    .layer(axum::middleware::from_fn(request_id::request_id_middleware))
    .layer(axum::middleware::from_fn(client_ip_middleware))
    // ADD THIS: Policy enforcement (processes after request_id and client_ip)
    .layer(axum::middleware::from_fn_with_state(
        state.clone(),
        policy_enforcement_middleware,
    ))
    .layer(axum::middleware::from_fn_with_state(state.clone(), request_tracking_middleware))
    .layer(axum::middleware::from_fn_with_state(state.clone(), drain_middleware))
    .layer(axum::middleware::from_fn(adapteros_telemetry::middleware::api_logger_middleware))
    .with_state(state)
```

**Note:** This is **not recommended** because:
- Public routes (health checks, login) don't need policy validation
- Adds unnecessary overhead for unauthenticated requests
- Policy enforcement may not have Claims for public routes

## Testing the Integration

### 1. Build the Server

```bash
cd /Users/mln-dev/Dev/adapter-os
cargo build --release -p adapteros-server
```

### 2. Start the Server

```bash
./target/release/adapteros-server
```

### 3. Test with Curl

**Test 1: Login (should work - public route)**
```bash
curl -X POST http://localhost:8080/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@adapteros.local",
    "password": "your-password"
  }'
```

**Test 2: List Adapters (should work if no policy violations)**
```bash
TOKEN="your-jwt-token-here"

curl -X GET http://localhost:8080/v1/adapters \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json"
```

**Test 3: Inference (may be blocked by Egress policy)**
```bash
curl -X POST http://localhost:8080/v1/infer \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Hello, world!",
    "max_tokens": 100
  }'
```

### 4. Check Logs

Look for policy enforcement logs in the server output:

```
INFO  policy_enforcement: Policy validation completed
  request_id="550e8400-e29b-41d4-a716-446655440000"
  operation="GET /v1/adapters"
  violations=0
  warnings=0
  duration_ms=2

WARN  policy_enforcement: Request blocked by policy violations
  request_id="660e9500-f39c-52e5-b827-557766551111"
  operation="POST /v1/infer"
  violations=1

ERROR policy_enforcement: Policy violation (Error)
  request_id="660e9500-f39c-52e5-b827-557766551111"
  operation="POST /v1/infer"
  policy_pack="Egress Ruleset"
  violation_id="egress-001"
  message="Network access forbidden during inference"
```

## Configuration Options

### Option 1: Environment Variable Control

Add environment variable support to enable/disable policy enforcement:

```bash
# Disable policy enforcement for development
export AOS_POLICY_ENFORCEMENT=false

# Enable policy enforcement for production (default)
export AOS_POLICY_ENFORCEMENT=true
```

Modify the middleware to check this variable:

```rust
// In crates/adapteros-server-api/src/middleware/policy_enforcement.rs

pub async fn policy_enforcement_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Check if policy enforcement is enabled
    if !is_policy_enforcement_enabled() {
        return Ok(next.run(req).await);
    }

    // ... rest of middleware implementation ...
}

fn is_policy_enforcement_enabled() -> bool {
    std::env::var("AOS_POLICY_ENFORCEMENT")
        .map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no" | "off"))
        .unwrap_or(true) // Default: enabled
}
```

### Option 2: Add PolicyPackManager to AppState

For better performance, cache the PolicyPackManager in AppState instead of creating it per request.

**Step 1: Update AppState**

```rust
// In crates/adapteros-server-api/src/state.rs

use adapteros_policy::PolicyPackManager;
use std::sync::Arc;

pub struct AppState {
    // ... existing fields ...

    /// Policy pack manager for runtime enforcement
    pub policy_manager: Arc<PolicyPackManager>,
}
```

**Step 2: Initialize in Server Startup**

```rust
// In crates/adapteros-server/src/main.rs or lib.rs

use adapteros_policy::PolicyPackManager;

let policy_manager = Arc::new(PolicyPackManager::new());

let state = AppState {
    // ... existing fields ...
    policy_manager,
};
```

**Step 3: Use in Middleware**

```rust
// In crates/adapteros-server-api/src/middleware/policy_enforcement.rs

// Change from:
let policy_manager = PolicyPackManager::new();

// To:
let policy_manager = Arc::clone(&state.policy_manager);
```

## Troubleshooting

### Issue 1: Compilation Errors

**Error:** `cannot find value 'policy_enforcement_middleware' in this scope`

**Solution:** Make sure you added the import at the top of routes.rs:
```rust
use crate::middleware::{auth_middleware, client_ip_middleware, policy_enforcement_middleware};
```

### Issue 2: All Requests Blocked

**Error:** Every request returns 403 Forbidden

**Possible Causes:**
1. Policy packs are too restrictive
2. PolicyPackManager not initialized correctly
3. Environment variable blocking all operations

**Solution:**
- Check policy pack configurations in `PolicyPackManager::get_default_config()`
- Temporarily disable specific policy packs
- Add debug logging to see which policies are failing

### Issue 3: Performance Issues

**Error:** Server response time significantly increased

**Solution:**
- Move PolicyPackManager to AppState (see Option 2 above)
- Use Arc for thread safety
- Consider disabling policy enforcement for low-risk operations

### Issue 4: Missing Claims

**Error:** Policy enforcement can't access tenant_id or user_id

**Solution:**
- Ensure policy enforcement middleware comes AFTER auth_middleware
- Verify auth_middleware is setting Claims correctly
- Check that the route is protected (not public)

## Verification Checklist

After integration, verify:

- [ ] Server compiles without errors
- [ ] Server starts successfully
- [ ] Health check endpoint works (`/healthz`)
- [ ] Login works (public route, no policy check)
- [ ] Protected endpoints return 403 for policy violations
- [ ] Policy violations are logged with full context
- [ ] Non-violating requests work normally
- [ ] Performance is acceptable (< 5ms overhead)

## Complete Diff

Here's the complete set of changes needed:

**File: `crates/adapteros-server-api/src/middleware/mod.rs`**
```diff
 pub mod caching;
 pub mod compression;
+pub mod policy_enforcement;
 pub mod request_id;
 pub mod versioning;

 pub use caching::{caching_middleware, CacheControl};
 pub use compression::compression_middleware;
+pub use policy_enforcement::policy_enforcement_middleware;
 pub use request_id::request_id_middleware;
 pub use versioning::{versioning_middleware, ApiVersion, DeprecationInfo};
```

**File: `crates/adapteros-server-api/src/routes.rs`**
```diff
-use crate::middleware::{auth_middleware, client_ip_middleware};
+use crate::middleware::{auth_middleware, client_ip_middleware, policy_enforcement_middleware};
```

```diff
     .layer(middleware::from_fn_with_state(
         state.clone(),
         auth_middleware,
     ))
+    .layer(middleware::from_fn_with_state(
+        state.clone(),
+        policy_enforcement_middleware,
+    ));
```

**New File: `crates/adapteros-server-api/src/middleware/policy_enforcement.rs`**
- Complete implementation provided in previous response
- 377 lines of middleware code
- Includes unit tests

## Summary

The policy enforcement middleware is now ready to integrate. To activate it:

1. Add import to routes.rs
2. Add middleware layer after auth_middleware
3. Build and test
4. Monitor logs for policy violations
5. Tune policy configurations as needed

**Recommended Next Steps:**
1. Start with Option 1 (protected routes only)
2. Test thoroughly with various operations
3. Monitor performance impact
4. Add PolicyPackManager to AppState for production
5. Implement environment variable controls
6. Add metrics collection for policy violations
