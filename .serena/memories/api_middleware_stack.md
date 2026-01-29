# API Middleware Stack

## Core Module: `crates/adapteros-server-api/`

### Architecture
- Axum-based HTTP server
- Layered middleware system
- Route tiers with different auth requirements

---

## Route Tiers

### 1. Health Routes (No Middleware)
```
/healthz           → handlers::health
/readyz            → handlers::ready
/version           → handlers::infrastructure::get_version
```
**Purpose**: Kubernetes probes, must be cheap and fast.

### 2. Public Routes (Policy Only)
```
/v1/auth/login     → auth_enhanced::login_handler
/v1/auth/bootstrap → auth_enhanced::bootstrap_admin_handler
/v1/auth/config    → auth_enhanced::get_auth_config_handler
/v1/auth/register  → auth_enhanced::register_handler
/v1/meta           → handlers::meta
/v1/search         → handlers::search::global_search
```
**Middleware**: `policy_enforcement_middleware` only

### 3. Metrics Route (Custom Auth)
```
/metrics           → handlers::metrics_handler
/v1/metrics        → handlers::metrics_handler
```
**Purpose**: Prometheus scraping, bearer token auth (not JWT)

### 4. Optional Auth Routes
```
/v1/models/status  → infrastructure::get_base_model_status
/v1/topology       → topology::get_topology
```
**Middleware Stack**:
1. `optional_auth_middleware`
2. `context_middleware`
3. `audit_middleware`
4. `policy_enforcement_middleware`

### 5. Internal Routes (Worker-to-CP)
```
/v1/workers/fatal    → handlers::receive_worker_fatal
/v1/workers/register → workers::register_worker
/v1/workers/status   → workers::notify_worker_status
```
**Auth**: UDS peer credentials or manifest binding (no user JWT)

### 6. Protected Routes (Full Auth)
All other `/v1/*` routes.

---

## Middleware Stack (Protected Routes)

### Execution Order (outermost → innermost)
```rust
// routes/mod.rs:2228-2244
.layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
.layer(middleware::from_fn(tenant_route_guard_middleware))
.layer(middleware::from_fn(csrf_middleware))
.layer(middleware::from_fn(context_middleware))
.layer(middleware::from_fn_with_state(state.clone(), policy_enforcement_middleware))
.layer(middleware::from_fn_with_state(state.clone(), audit_middleware))
```

| Layer | File | Purpose |
|-------|------|---------|
| `auth_middleware` | `middleware/mod.rs` | JWT/session validation |
| `tenant_route_guard_middleware` | `middleware/mod.rs` | Enforce `/tenants/{id}` isolation |
| `csrf_middleware` | `middleware/mod.rs` | Double-submit cookie validation |
| `context_middleware` | `middleware/context.rs` | Build `RequestContext` |
| `policy_enforcement_middleware` | `middleware/policy_enforcement.rs` | Check policy constraints |
| `audit_middleware` | `middleware/audit.rs` | Log audit trail |

---

## Global Middleware Stack

Applied to all routes (innermost → outermost):

```rust
// routes/mod.rs:2256-2281
.layer(TraceLayer::new_for_http())
.layer(ErrorCodeEnforcementLayer)
.layer(idempotency_middleware)
.layer(cors_layer())
.layer(rate_limiting_middleware)
.layer(request_size_limit_middleware)
.layer(security_headers_middleware)
.layer(caching::caching_middleware)
.layer(versioning::versioning_middleware)
.layer(trace_context_middleware)
.layer(request_id::request_id_middleware)
.layer(seed_isolation_middleware)
.layer(client_ip_middleware)
.layer(request_tracking_middleware)
.layer(lifecycle_gate)
.layer(drain_middleware)
.layer(observability_middleware)
.layer(CompressionLayer::new())
.layer(request_id::request_id_middleware)
```

| Layer | Module | Purpose |
|-------|--------|---------|
| `TraceLayer` | tower-http | Request tracing |
| `ErrorCodeEnforcementLayer` | `middleware/error_code_enforcement.rs` | Machine-readable error codes |
| `idempotency_middleware` | `idempotency.rs` | Idempotency keys |
| `cors_layer` | - | CORS headers |
| `rate_limiting_middleware` | `rate_limit.rs` | Rate limiting |
| `request_size_limit_middleware` | - | Body size limits |
| `security_headers_middleware` | `middleware_security.rs` | Security headers |
| `caching_middleware` | `middleware/caching.rs` | HTTP caching |
| `versioning_middleware` | `middleware/versioning.rs` | API versioning |
| `trace_context_middleware` | `middleware/trace_context.rs` | W3C Trace Context |
| `request_id_middleware` | `middleware/request_id.rs` | X-Request-Id |
| `seed_isolation_middleware` | `middleware/seed_isolation.rs` | Thread-local seed |
| `client_ip_middleware` | `middleware/mod.rs` | Extract client IP |
| `request_tracking_middleware` | `request_tracker.rs` | In-flight tracking |
| `lifecycle_gate` | `middleware/mod.rs` | Reject during drain |
| `drain_middleware` | - | Graceful shutdown |
| `observability_middleware` | `middleware/observability.rs` | Logging |
| `CompressionLayer` | tower-http | gzip/br/deflate |

---

## Auth Middleware Details

### `auth_middleware` (`middleware/mod.rs`)

#### Token Sources (`JwtSource` enum)
1. `Authorization: Bearer <token>` header
2. `access_token` cookie
3. `X-API-Key` header (API keys)

#### Flow
1. Extract token from header or cookie
2. Validate JWT signature (Ed25519 or HMAC)
3. Check session existence in DB
4. Build `Principal` from claims
5. Inject into request extensions

#### Dev Bypass
```rust
fn dev_no_auth_enabled() -> bool  // Check AOS_DEV_NO_AUTH env var
fn inject_dev_bypass_claims()     // Inject mock admin claims
```

### `optional_auth_middleware`
Same as `auth_middleware` but doesn't reject unauthenticated requests.

### `dual_auth_middleware`
Supports both JWT and API key authentication.

---

## CSRF Protection

### `csrf_middleware` (`middleware/mod.rs`)

Double-submit cookie pattern:
1. Server sets `csrf_token` cookie
2. Client sends `X-CSRF-Token` header
3. Middleware validates header matches cookie

#### Skip Conditions
- Non-mutating methods (GET, HEAD, OPTIONS)
- Bearer token auth (stateless)
- API key auth

---

## Tenant Isolation

### `tenant_route_guard_middleware`

For routes like `/tenants/{tenant_id}/*`:
1. Extract `tenant_id` from path
2. Compare with `Principal.tenant_id`
3. Reject if mismatch (unless admin)

---

## Policy Enforcement

### `policy_enforcement_middleware` (`middleware/policy_enforcement.rs`)

1. Load active policy pack for tenant
2. Check request against policy rules
3. Reject if policy violated
4. Log policy decision

---

## Middleware Chain Builder (`middleware/chain_builder.rs`)

Type-safe builder ensuring correct middleware ordering:

```rust
pub fn protected_chain(state: AppState) -> ProtectedMiddlewareChain<NeedsAuth>
pub fn optional_auth_chain(state: AppState) -> ProtectedMiddlewareChain<NeedsAuth>
pub fn api_key_chain(state: AppState) -> ProtectedMiddlewareChain<NeedsAuth>
pub fn internal_chain(state: AppState) -> ProtectedMiddlewareChain<NeedsAuth>
```

### State Machine Pattern
```rust
NeedsAuth -> NeedsTenantGuard -> NeedsCsrf -> NeedsContext -> NeedsPolicy -> NeedsAudit -> Complete
```

Each step enforces the next state, making incorrect ordering a compile error.

---

## AppState (`state.rs`)

Central state shared across handlers.

### Key Fields
| Field | Type | Purpose |
|-------|------|---------|
| `db` | `SqlitePool` | Database pool |
| `config` | `ApiConfig` | Server configuration |
| `clock` | `Arc<dyn Clock>` | Deterministic time |
| `lifecycle_manager` | `Arc<LifecycleManager>` | Graceful shutdown |
| `crypto` | `CryptoState` | Signing keys |
| `policy_manager` | `Arc<PolicyManager>` | Policy enforcement |
| `registry` | `Arc<AdapterRegistry>` | Adapter management |
| `sse_manager` | `Arc<SseManager>` | SSE connections |
| `idempotency_store` | `Arc<IdempotencyStore>` | Idempotency tracking |
| `rate_limiter` | `Arc<RateLimiter>` | Rate limiting |
| `tick_ledger` | `Arc<TickLedger>` | Determinism ledger |
| `boot_attestation` | `BootAttestation` | Boot integrity |

### Builder Pattern
```rust
AppState::new(db, config)
    .with_lifecycle(lifecycle_manager)
    .with_worker(worker_handle)
    .with_registry(registry)
    .with_policy_manager(policy_manager)
    .with_sse_manager(sse_manager)
    // ... etc
```

---

## Security Configuration (`ApiConfig.security`)

```rust
pub struct SecurityConfigApi {
    pub jwt_mode: String,           // "ed25519" or "hmac"
    pub token_ttl_seconds: u64,
    pub access_token_ttl_seconds: u64,
    pub session_ttl_seconds: u64,
    pub require_mfa: bool,
    pub require_pf_deny: bool,      // Require PF egress check
    pub dev_bypass: bool,           // Allow dev bypass
    pub cookie_same_site: String,   // "lax", "strict", "none"
    pub cookie_secure: bool,
    pub clock_skew_seconds: i64,
}
```

---

## Common Patterns

### 1. Adding a Protected Route
```rust
// In routes/mod.rs protected_routes section
.route("/v1/my-endpoint", get(handlers::my_handler))
```

### 2. Adding an Internal Route
```rust
// In routes/mod.rs internal_routes section
.route("/v1/internal/my-endpoint", post(handlers::internal::my_handler))
```

### 3. Creating a Custom Middleware
```rust
pub async fn my_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Pre-processing
    let response = next.run(request).await;
    // Post-processing
    response
}
```

### 4. Extracting Auth Info
```rust
pub async fn my_handler(
    Extension(principal): Extension<Principal>,
    Extension(ctx): Extension<RequestContext>,
) -> Result<Json<Response>, ApiError> {
    // principal.user_id, principal.tenant_id, etc.
}
```

---

## Route Files

| File | Purpose |
|------|---------|
| `routes/mod.rs` | Main router with `build()` function |
| `routes/auth_routes.rs` | Auth-related protected routes |
| `routes/chat_routes.rs` | Chat session routes |
| `routes/tenant_routes.rs` | Tenant-scoped routes |
| `routes/training_routes.rs` | Training job routes |
| `routes/adapters.rs` | Adapter management routes |
