# AdapterOS REST API Design Assessment

**Date:** 2025-11-22  
**Codebase:** /Users/star/Dev/aos  
**Analysis Scope:** REST API endpoints, naming conventions, RESTful principles, request/response structures, versioning

---

## Executive Summary

AdapterOS implements a comprehensive REST API with **~189 registered routes** across 10 major resource categories. The API demonstrates strong adherence to RESTful principles with consistent HTTP method usage, standardized response structures, and clear authentication/authorization patterns. The design uses **OpenAPI/Swagger** for documentation with **Utoipa** integration. However, several areas warrant improvement for production-grade consistency.

---

## 1. ENDPOINT NAMING CONVENTIONS

### Current Conventions

**Strengths:**
- **Consistent versioning:** All routes use `/v1/` prefix (v1 namespace)
- **Resource-oriented paths:** RESTful hierarchy followed consistently
  - `/v1/adapters` - collection
  - `/v1/adapters/{adapter_id}` - singular resource
  - `/v1/adapters/{adapter_id}/lifecycle/promote` - action on resource
- **Clear action naming:** Verbs used for non-idempotent operations
  - `/load`, `/unload`, `/promote`, `/demote` (POST actions)
  - `/validate`, `/test` (POST actions)
- **Nested resources:** Properly hierarchical
  - `/v1/tenants/{tenant_id}/usage`
  - `/v1/adapters/{adapter_id}/activations`
  - `/v1/workspaces/{workspace_id}/members/{member_id}`

### Conventions Found in Codebase

**Route Pattern Distribution:**
| Pattern Type | Count | Examples |
|---|---|---|
| Collection operations | 35+ | `/v1/adapters`, `/v1/tenants`, `/v1/datasets` |
| Singular resource ops | 40+ | `/v1/adapters/{id}`, `/v1/nodes/{id}` |
| Nested sub-resources | 45+ | `/v1/adapters/{id}/lifecycle/promote` |
| State transitions | 30+ | `/pause`, `/archive`, `/load`, `/unload` |
| Query/search ops | 15+ | `/search`, `/query`, `/list` |

### Issues Identified

1. **Inconsistent plural/singular usage:**
   - `/v1/adapters` (plural) vs `/v1/nodes` (plural) ✓ Correct
   - `/v1/promotion` vs `/v1/promotions` (inconsistency in line 1004)
   - Some streams use both `/v1/streams/training` and `/v1/stream/metrics` (inconsistent)

2. **Action endpoint naming variations:**
   - `/v1/adapters/{id}/load` (verb directly on resource)
   - `/v1/adapters/{id}/lifecycle/promote` (verb under namespace)
   - `/v1/adapters/{id}/state/promote` (different state namespace)
   - `/v1/adapters/{id}/pin` (multi-method on same endpoint: GET/POST/DELETE)

3. **Validation endpoints scattered:**
   - `/v1/adapters/validate-name` (collection-level)
   - `/v1/datasets/{id}/validate` (resource-level)
   - `/v1/policies/validate` (collection-level)

---

## 2. RESTful PRINCIPLES COMPLIANCE

### HTTP Method Usage Analysis

**Distribution of HTTP Methods:**

```
GET    - 146 routes (queries, retrieval)
POST   - 195 routes (create, actions, state changes)
PUT    - 8 routes (updates)
DELETE - 16 routes (removal)
```

**RESTful Compliance Assessment:**

| Principle | Status | Evidence |
|-----------|--------|----------|
| **GET = Safe & Idempotent** | ✓ Good | All GET routes are read-only queries |
| **POST = Create/Action** | ✓ Good | Used for creation and state transitions |
| **PUT = Full Update** | ⚠️ Limited | Only 8 PUT routes; inconsistent usage |
| **DELETE = Removal** | ✓ Good | Used for deletions consistently |
| **PATCH = Partial Update** | ✗ Missing | No PATCH routes; PUT used minimally |
| **HEAD/OPTIONS** | ✗ Missing | Not implemented |

**Issues:**

1. **Overuse of POST for state transitions:**
   - `/v1/adapters/{id}/load` (POST - creates resource in memory)
   - `/v1/adapters/{id}/unload` (POST - destroys resource state)
   - `/v1/adapters/{id}/lifecycle/promote` (POST - state transition)
   - These could be PATCH requests or proper state resource design

2. **Mixed usage patterns:**
   - Same endpoint with GET/POST/DELETE:
     ```
     /v1/adapters/{id}/pin - GET (status), POST (pin), DELETE (unpin)
     /v1/adapter-stacks/{id} - GET (retrieve), DELETE (remove)
     /v1/workspaces/{id} - GET, PUT, DELETE (mixed patterns)
     ```

3. **Inconsistent CRUD patterns:**
   - Some resources support GET + POST on collection:
     ```
     /v1/tenants - GET (list), POST (create) ✓
     /v1/adapters - GET (list), POST (not available - use /register) ✗
     /v1/adapter-stacks - GET, POST ✓
     ```

---

## 3. REQUEST/RESPONSE STRUCTURES

### Request Types Consistency

**Total request types identified:** 60+ custom request types

**Common patterns:**

```rust
// Standard Create Request
pub struct CreateTenantRequest {
    pub name: String,
    pub metadata: Option<HashMap<String, String>>,
}

// Standard Update Request
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

// Standard List Query (optional pagination)
pub struct QueryParams {
    pub page: u32,
    pub limit: u32,
    pub filter: Option<String>,
}

// Standard Batch Request
pub struct BatchInferRequest {
    pub requests: Vec<BatchInferItemRequest>,
}

// Standard State Transition
pub struct PromoteRequest {
    pub reason: Option<String>,
    pub metadata: Option<Value>,
}
```

### Response Types Consistency

**Base Response Structure:**

All responses include:
```json
{
  "schema_version": "1.0",
  "data": { /* resource */ },
  // OR
  "error": "message",
  "code": "ERROR_CODE",
  "details": { /* optional */ }
}
```

**Response Types:** 77 response struct types

**Common patterns:**
```rust
// Single Resource Response
pub struct AdapterResponse {
    pub id: String,
    pub hash: String,
    pub tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

// List Response (wrapper)
pub struct AdaptersResponse {
    pub adapters: Vec<AdapterResponse>,
    pub total: u32,
}

// Paginated Response (generic)
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub limit: u32,
}

// Status Response
pub struct HealthResponse {
    pub schema_version: String,
    pub status: String,
    pub version: String,
}
```

### Issues with Request/Response Structures

1. **Inconsistent pagination:**
   - Some endpoints use `/v1/adapters?page=1&limit=50` (standard)
   - Others have custom query params
   - No standard `PaginationParams` applied uniformly

2. **Optional field handling:**
   - Heavy use of `Option<T>` with `skip_serializing_if`
   - Good practice but inconsistently applied:
     ```rust
     // Some responses include optional fields
     #[serde(skip_serializing_if = "Option::is_none")]
     pub metadata: Option<Value>,
     
     // Others include all fields
     pub details: Option<String>,  // Without skip_serializing_if
     ```

3. **List response structure inconsistency:**
   - Some use wrapper struct: `{ "adapters": [...] }`
   - Others use generic: `{ "items": [...] }`
   - No standardized wrapper naming

4. **Error response consistency:**
   - All errors use standard `ErrorResponse`:
     ```rust
     pub struct ErrorResponse {
       pub schema_version: String,
       pub error: String,
       pub code: String,
       pub details: Option<Value>,
     }
     ```
   - User-friendly error mapping implemented (good practice)
   - Status codes mapped consistently (155 `INTERNAL_SERVER_ERROR`, 37 `NOT_FOUND`, etc.)

5. **Batch request/response pattern:**
   - Well-defined for batch inference:
     ```json
     POST /v1/infer/batch
     {
       "requests": [
         { "id": "1", "prompt": "...", ... },
         { "id": "2", "prompt": "...", ... }
       ]
     }
     
     Response:
     {
       "responses": [
         { "id": "1", "response": {...} },
         { "id": "2", "error": {...} }
       ]
     }
     ```
   - Pattern not generalized to other endpoints

---

## 4. API VERSIONING STRATEGY

### Current Versioning Implementation

**Schema Version:** `1.0` (defined in `adapteros-api-types`)

```rust
// crates/adapteros-api-types/src/lib.rs
pub const API_SCHEMA_VERSION: &str = "1.0";

// Included in all responses
pub struct HealthResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub status: String,
    pub version: String,
}
```

**URL Versioning:** All routes prefixed with `/v1/`
- No v2 routes planned
- Public routes (`/healthz`, `/readyz`) unversioned
- OpenAPI at `/api-docs/openapi.json` (unversioned)

### Versioning Characteristics

| Aspect | Implementation | Assessment |
|--------|---|---|
| **URL Path Strategy** | `/v1/` prefix | Standard practice |
| **Response Header** | `schema_version` in body | Good for clients |
| **Backward Compatibility** | Same response struct for all | Tied to deployment |
| **Deprecation Path** | No v2 routes yet | Design ready for v2 |
| **Breaking Changes** | Would require v2 routes | No clear migration strategy |
| **Feature Flags** | Not observed | Could enable gradual rollout |

### Issues & Gaps

1. **No explicit versioning in headers:**
   - No `API-Version` header
   - No `Accept: application/vnd.adapteros.v1+json` header support
   - `schema_version` in response body is late (client must parse)

2. **No documented breaking change policy:**
   - How will v2 be introduced?
   - Migration path unclear
   - No deprecation notices in responses

3. **Inconsistent versioning scope:**
   - `/v1/` covers main API
   - Some endpoints unversioned:
     - `/healthz` (makes sense for probes)
     - `/swagger-ui` (docs)
     - `/api-docs/openapi.json` (docs)

4. **No minor version strategy:**
   - If `1.0` → `1.1` changes, how are clients notified?
   - No support for multiple versions concurrently

---

## 5. AUTHENTICATION & AUTHORIZATION

### Authentication Patterns

**JWT-Based Auth (EdDSA):**
- Route: `POST /v1/auth/login`
- Token format: JWT with Ed25519 signature
- TTL: 8 hours (from CLAUDE.md)
- Header: `Authorization: Bearer <token>`
- Fallback: Query param `?token=<token>` (allowed in streaming)

**Special Auth Cases:**
- `/v1/auth/bootstrap` (admin bootstrap, one-time)
- `/v1/auth/dev-bypass` (debug builds only)
- `/v1/metrics` (custom bearer token auth, not JWT)

**Middleware Chain:**
```
1. client_ip_middleware - Extract client IP
2. auth_middleware - Validate JWT token
3. Rate limiting - Per-user rate limits
4. Request size limit - Prevent oversized requests
5. Security headers - Add HSTS, CSP, etc.
```

### Authorization Patterns

**RBAC System (5 roles):**
- **Admin** (full permissions)
- **Operator** (runtime operations)
- **SRE** (infrastructure debugging)
- **Compliance** (audit-only)
- **Viewer** (read-only)

**Permission checks:**
```rust
require_permission(&claims, Permission::AdapterRegister)?;
```

**Audit logging:**
```rust
log_success(&db, &claims, actions::ADAPTER_REGISTER, resources::ADAPTER, Some(&id)).await;
```

---

## 6. OPENAPI/SWAGGER INTEGRATION

**Status:** ✓ Fully integrated

**Implementation:**
- Tool: `Utoipa` (OpenAPI macro-based)
- Endpoint: `/swagger-ui` (UI) + `/api-docs/openapi.json` (spec)
- Documentation: 189 paths documented with Utoipa macros
- Schema: 100+ type schemas with `ToSchema` derives

**Quality:**
- All handlers have `#[utoipa::path(...)]` decorators
- Request/response types have `ToSchema` derives
- Tags organized by domain (adapters, training, inference, etc.)
- Examples minimal (could be enhanced)

---

## 7. MIDDLEWARE & SECURITY

### Security Middleware Stack

1. **CORS Layer**
   - Properly configured via `cors_layer()`
   - Applied to all routes

2. **Rate Limiting**
   - Per-user rate limiting implemented
   - Fallback to IP-based limiting
   - Returns 429 on limit exceeded

3. **Request Size Limiting**
   - Prevents oversized payloads
   - Middleware: `request_size_limit_middleware`

4. **Security Headers**
   - HSTS, CSP, X-Content-Type-Options, etc.
   - Middleware: `security_headers_middleware`

5. **Client IP Extraction**
   - Handles proxied clients (X-Forwarded-For, etc.)
   - Outermost middleware for audit logging

### Issues

1. **Missing PATCH method support:**
   - Only GET, POST, PUT, DELETE implemented
   - No PATCH handlers

2. **No explicit 404 handling:**
   - Axum's default 404 responses used
   - Could return custom `ErrorResponse`

3. **Missing request validation middleware:**
   - Validation done per-handler
   - No centralized schema validation

---

## 8. STREAMING & REAL-TIME APIs

**SSE Streaming Endpoints:** 7 implemented

```
GET /v1/streams/training        - Training events
GET /v1/streams/discovery       - Discovery events
GET /v1/streams/contacts        - Contact events
GET /v1/streams/file-changes    - Git file changes
GET /v1/stream/metrics          - System metrics
GET /v1/stream/telemetry        - Telemetry events
GET /v1/stream/adapters         - Adapter state
```

**Streaming Response Format:**
- Standard SSE: `data: {...}\n\n`
- JSON-encoded events in SSE data field
- No explicit heartbeat/keep-alive visible in code

**Issues:**
- Inconsistent path naming: `/v1/stream/` vs `/v1/streams/`
- No chunked upload streaming (separate endpoint pattern used)

---

## 9. CONSISTENCY ISSUES SUMMARY

### High Priority

1. **Inconsistent collection endpoints:**
   ```
   /v1/adapters (use /v1/adapters/register to create)
   /v1/adapter-stacks (use POST on collection to create)
   ```

2. **Mixed state transition patterns:**
   ```
   /v1/adapters/{id}/lifecycle/promote      (POST)
   /v1/adapters/{id}/state/promote          (POST - different from lifecycle)
   /v1/adapters/{id}/load                   (POST)
   /v1/tenants/{id}/pause                   (POST)
   ```

3. **Response wrapper inconsistency:**
   ```json
   // Some use resource name
   { "adapters": [...], "total": 10 }
   
   // Others use generic
   { "items": [...], "total": 10 }
   
   // Some use no wrapper
   [{ "id": "1" }, ...]
   ```

### Medium Priority

1. **Plural/singular path inconsistency:**
   ```
   /v1/stream/metrics vs /v1/streams/training
   /v1/promotion vs /v1/promotions
   ```

2. **Missing batch operation standardization:**
   - Only `/v1/infer/batch` implemented
   - Could generalize to other resources

3. **No unified search/filter pattern:**
   - Some endpoints use `/query`
   - Others use `/search`
   - No consistent filter syntax

---

## 10. RECOMMENDATIONS

### Tier 1: Critical Fixes

1. **Standardize collection POST behavior:**
   ```
   CHANGE: /v1/adapters/register
   TO:     POST /v1/adapters (standard REST)
   ```

2. **Unify state transition endpoints:**
   ```
   /v1/adapters/{id}/state/promote  - tier-based
   /v1/adapters/{id}/lifecycle/promote - lifecycle-based
   
   RENAME to:
   /v1/adapters/{id}/promote-tier
   /v1/adapters/{id}/promote-lifecycle
   ```

3. **Add versioning headers:**
   ```
   Response headers:
   API-Version: 1.0
   API-Deprecated: false
   ```

### Tier 2: Improvements

1. **Standardize response wrapper structure:**
   ```rust
   // For lists, always use consistent wrapper
   pub struct ListResponse<T> {
     pub items: Vec<T>,
     pub pagination: Pagination,
   }
   ```

2. **Implement PATCH support:**
   - For partial updates
   - Use JSON Merge Patch or JSON Patch

3. **Add request ID tracking:**
   - X-Request-ID header (in, out)
   - Improves debugging and tracing

4. **Document API contracts:**
   - OpenAPI examples (enhance existing)
   - Request/response examples for each endpoint
   - Error codes catalog

### Tier 3: Enhancements

1. **Add content negotiation:**
   ```
   Accept: application/vnd.adapteros.v1+json
   Accept: application/json
   ```

2. **Implement conditional requests:**
   - ETag support
   - If-Modified-Since headers

3. **Add deprecation warnings:**
   ```
   Deprecation: true
   Sunset: "2025-12-31"
   Link: </v2/adapters>; rel="successor-version"
   ```

4. **Standardize pagination:**
   ```
   // Consistent query params across all list endpoints
   ?page=1&limit=50&sort=name&filter=active
   ```

---

## 11. SCORING SUMMARY

| Category | Score | Status |
|----------|-------|--------|
| **Endpoint Naming** | 8/10 | Mostly consistent with minor variations |
| **RESTful Compliance** | 7/10 | Good but overuses POST, limited PUT/PATCH |
| **Request/Response Structure** | 8/10 | Well-structured with minor inconsistencies |
| **API Versioning** | 6/10 | Basic v1 strategy, no forward compatibility headers |
| **Documentation** | 8/10 | Good OpenAPI integration, could add examples |
| **Security** | 9/10 | Strong authentication, RBAC, audit logging |
| **Consistency** | 7/10 | Good patterns but some deviations |
| **Overall** | 7.6/10 | Production-ready but needs refinement |

---

## Conclusion

AdapterOS demonstrates a well-designed REST API foundation with strong security and clear architecture. The API follows RESTful principles with 189 endpoints, comprehensive OpenAPI documentation, and proper authentication/authorization. However, the design would benefit from:

1. **Standardization** of endpoint naming patterns and response structures
2. **Explicit versioning** via headers for forward compatibility
3. **Consistency** in HTTP method usage (reduce POST overload)
4. **Documentation** enhancements with request/response examples

The API is suitable for production use with these refinements addressed in upcoming releases (v1.1+).

