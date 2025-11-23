# AdapterOS Routes Improvements

**Copyright:** 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22

This document outlines recommended improvements identified during the route mapping and documentation process.

---

## Summary of Issues Found

| Category | Count | Priority |
|----------|-------|----------|
| Unwired Handlers | 3 | Medium |
| Deprecated Routes | 3 | Low |
| Missing API Methods | 5 | Medium |
| Inconsistencies | 4 | Low |
| Documentation Gaps | 2 | Low |

---

## 1. Unwired Handlers

### Issue: Handler modules exist but are not wired to routes

**Location:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/`

| Module | File | Purpose | Recommendation |
|--------|------|---------|----------------|
| `messages.rs` | handlers/messages.rs | Workspace messaging | Wire to `/v1/workspaces/{id}/messages` |
| `journeys.rs` | handlers/journeys.rs | Journey tracking | Wire to `/v1/journeys/{type}/{id}` |
| `git_repository.rs` | handlers/git_repository.rs | Git repo management | Merge with `code.rs` or add routes |

### Recommended Actions

**1. Wire messages.rs:**

```rust
// In routes.rs protected_routes:
.route(
    "/v1/workspaces/{workspace_id}/messages",
    get(handlers::messages::list_messages)
        .post(handlers::messages::create_message),
)
.route(
    "/v1/workspaces/{workspace_id}/messages/{message_id}",
    get(handlers::messages::get_message)
        .put(handlers::messages::update_message)
        .delete(handlers::messages::delete_message),
)
```

**2. Wire journeys.rs:**

```rust
// In routes.rs protected_routes:
.route(
    "/v1/journeys/{journey_type}/{id}",
    get(handlers::journeys::get_journey),
)
```

**3. Evaluate git_repository.rs:**

Review whether functionality overlaps with `code.rs`. If unique:

```rust
.route(
    "/v1/git/repositories",
    get(handlers::git_repository::list_repositories)
        .post(handlers::git_repository::register_repository),
)
```

---

## 2. Deprecated Routes

### Issue: Routes marked deprecated but still active

| Route | Replacement | Status |
|-------|-------------|--------|
| `GET /v1/repositories` | `GET /v1/code/repositories` | Active, should redirect |
| `/alerts` (frontend) | `/metrics` | Redirect implemented |
| `/journeys` (frontend) | `/audit` | Redirect implemented |

### Recommended Actions

**1. Add deprecation warning to /v1/repositories:**

```rust
pub async fn list_repositories(/* ... */) -> /* ... */ {
    // Add deprecation header
    tracing::warn!("Deprecated endpoint /v1/repositories called - use /v1/code/repositories");
    // ... existing logic
}
```

**2. Consider removal timeline:**

- Mark deprecated endpoints in OpenAPI with `deprecated: true`
- Add `Deprecation` header to responses
- Plan removal for next major version

---

## 3. Missing API Client Methods

### Issue: Frontend API client missing methods for some backend endpoints

| Backend Endpoint | Status | Action |
|------------------|--------|--------|
| `PUT /v1/tenants/{tenant_id}` | Missing in client.ts | Add `updateTenant()` |
| `POST /v1/tenants/{tenant_id}/pause` | Missing in client.ts | Add `pauseTenant()` |
| `POST /v1/tenants/{tenant_id}/archive` | Missing in client.ts | Add `archiveTenant()` |
| `GET /v1/nodes/{node_id}/details` | Missing in client.ts | Add `getNodeDetails()` |
| Tutorial endpoints | Missing in client.ts | Add tutorial methods |

### Recommended Client Methods

```typescript
// Add to ui/src/api/client.ts

async updateTenant(tenantId: string, data: types.UpdateTenantRequest): Promise<types.Tenant> {
  return this.request<types.Tenant>(`/v1/tenants/${tenantId}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
}

async pauseTenant(tenantId: string): Promise<void> {
  return this.request<void>(`/v1/tenants/${tenantId}/pause`, {
    method: 'POST',
  });
}

async archiveTenant(tenantId: string): Promise<void> {
  return this.request<void>(`/v1/tenants/${tenantId}/archive`, {
    method: 'POST',
  });
}

// Tutorial methods
async listTutorials(): Promise<types.Tutorial[]> {
  return this.request<types.Tutorial[]>('/v1/tutorials');
}

async markTutorialCompleted(tutorialId: string): Promise<void> {
  return this.request<void>(`/v1/tutorials/${tutorialId}/complete`, {
    method: 'POST',
  });
}
```

---

## 4. Route Inconsistencies

### Issue: Naming and structure inconsistencies

| Issue | Location | Current | Recommended |
|-------|----------|---------|-------------|
| Plural/singular mismatch | Adapter stacks | `/v1/adapter-stacks` | Consistent with other routes |
| Path parameter naming | Various | `{adapter_id}`, `{id}` | Standardize to `{adapter_id}` |
| Duplicate functionality | Repos | `/v1/repositories` and `/v1/code/repositories` | Consolidate |
| SSE path inconsistency | Streams | `/v1/streams/*` and `/v1/stream/*` | Standardize to `/v1/streams/*` |

### Recommended Actions

**1. Standardize SSE paths:**

```rust
// Change:
.route("/v1/stream/metrics", get(handlers::system_metrics_stream))
.route("/v1/stream/telemetry", get(handlers::telemetry_events_stream))
.route("/v1/stream/adapters", get(handlers::adapter_state_stream))

// To:
.route("/v1/streams/metrics", get(handlers::system_metrics_stream))
.route("/v1/streams/telemetry", get(handlers::telemetry_events_stream))
.route("/v1/streams/adapters", get(handlers::adapter_state_stream))
```

**2. Standardize path parameters:**

Use consistent naming across all routes:
- `{adapter_id}` for adapters
- `{tenant_id}` for tenants
- `{job_id}` for jobs
- `{dataset_id}` for datasets

---

## 5. Documentation Gaps

### Issue: Some endpoints lack proper OpenAPI documentation

| Endpoint Group | Issue | Action |
|----------------|-------|--------|
| SSE Streams | Missing in OpenAPI spec | Add OpenAPI annotations |
| Monitoring endpoints | Incomplete descriptions | Add descriptions |
| Tutorial endpoints | Not in paths list | Add to ApiDoc |

### Recommended Actions

**1. Add SSE documentation:**

```rust
/// System metrics stream (SSE)
#[utoipa::path(
    tag = "streams",
    get,
    path = "/v1/streams/metrics",
    responses(
        (status = 200, description = "SSE stream of system metrics")
    )
)]
pub async fn system_metrics_stream(/* ... */) -> /* ... */ {
    // ...
}
```

**2. Add to ApiDoc paths:**

```rust
#[openapi(
    paths(
        // ... existing
        handlers::system_metrics_stream,
        handlers::telemetry_events_stream,
        handlers::adapter_state_stream,
        handlers::tutorials::list_tutorials,
        handlers::tutorials::mark_tutorial_completed,
        // ...
    ),
)]
```

---

## 6. Security Improvements

### Issue: Some endpoints may need additional security measures

| Endpoint | Current | Recommended |
|----------|---------|-------------|
| `POST /v1/auth/dev-bypass` | Active | Ensure disabled in production |
| `DELETE /v1/adapters/{id}` | Admin only | Add confirmation requirement |
| `POST /v1/cp/rollback` | Admin only | Add audit logging |

### Recommended Actions

**1. Environment-based endpoint availability:**

```rust
// In routes.rs
#[cfg(debug_assertions)]
.route("/v1/auth/dev-bypass", post(handlers::auth_enhanced::dev_bypass_handler))
```

**2. Add confirmation for destructive operations:**

```rust
#[derive(Deserialize)]
pub struct DeleteRequest {
    confirm: bool,
    #[serde(default)]
    reason: Option<String>,
}

pub async fn delete_adapter(
    // ...
    Json(req): Json<DeleteRequest>,
) -> /* ... */ {
    if !req.confirm {
        return Err((StatusCode::BAD_REQUEST, "Confirmation required"));
    }
    // ...
}
```

---

## 7. Performance Improvements

### Issue: Some routes could benefit from optimization

| Issue | Location | Recommendation |
|-------|----------|----------------|
| N+1 queries | Adapter list with stats | Use JOIN or batch loading |
| Missing pagination | Some list endpoints | Add `page` and `page_size` params |
| No caching | Static config endpoints | Add cache headers |

### Recommended Actions

**1. Add pagination to all list endpoints:**

```rust
#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 50 }
```

**2. Add cache headers for config endpoints:**

```rust
pub async fn get_meta() -> impl IntoResponse {
    (
        [("Cache-Control", "max-age=300")],
        Json(meta_response)
    )
}
```

---

## 8. Testing Gaps

### Issue: Some routes lack integration tests

| Route Category | Test Coverage | Action |
|----------------|---------------|--------|
| SSE Streams | Low | Add SSE integration tests |
| Chunked upload | Medium | Add edge case tests |
| Golden run promotion | Low | Add workflow tests |

### Recommended Test Structure

```
tests/
  integration/
    auth_test.rs
    adapters_test.rs
    training_test.rs
    inference_test.rs
    streams_test.rs  # NEW
    promotion_test.rs  # NEW
```

---

## 9. Frontend Route Improvements

### Issue: Some frontend routes could be enhanced

| Issue | Current | Recommendation |
|-------|---------|----------------|
| No error boundary per route | Global only | Add route-level error boundaries |
| Missing loading states | Some pages | Add consistent skeleton loading |
| No prefetching | None | Add route prefetching for nav |

### Recommended Actions

**1. Add route-level error handling:**

```tsx
// In RouteGuard.tsx
<ErrorBoundary fallback={<RouteErrorFallback />}>
  <Suspense fallback={<RouteSkeleton variant={route.skeletonVariant} />}>
    <Component />
  </Suspense>
</ErrorBoundary>
```

**2. Add route prefetching:**

```tsx
// On nav hover
const prefetchRoute = (path: string) => {
  const route = getRouteByPath(path);
  if (route) {
    route.component.preload?.();
  }
};
```

---

## 10. API Versioning Strategy

### Issue: No clear API versioning strategy documented

### Recommended Strategy

1. **URL Versioning:** Continue using `/v1/` prefix
2. **Breaking Changes:** Require new version (`/v2/`)
3. **Deprecation Period:** Minimum 6 months notice
4. **Headers:** Add `API-Version` response header

```rust
// Add to all responses
.layer(axum::middleware::from_fn(|req, next| async {
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "API-Version",
        "1.0.0".parse().unwrap()
    );
    response
}))
```

---

## Implementation Priority

### High Priority (Next Sprint)

1. Wire `messages.rs` handler
2. Add missing API client methods
3. Standardize SSE paths

### Medium Priority (Next Quarter)

4. Add pagination to all list endpoints
5. Improve OpenAPI documentation
6. Add route-level error boundaries

### Low Priority (Backlog)

7. Wire `journeys.rs` handler
8. Remove deprecated routes
9. Add route prefetching
10. Comprehensive integration tests

---

## Checklist for New Routes

When adding new routes, ensure:

- [ ] Handler function created with proper error handling
- [ ] Route added to `routes.rs` in correct section (public/protected)
- [ ] OpenAPI annotation added with all responses
- [ ] Schema types added to `ApiDoc` components
- [ ] Permission/role checks added if needed
- [ ] Audit logging added for sensitive operations
- [ ] Frontend route config added if UI needed
- [ ] API client method added
- [ ] TypeScript types added
- [ ] Integration test created
- [ ] Documentation updated

---

## References

- [ROUTES_REFERENCE.md](./ROUTES_REFERENCE.md) - Complete route documentation
- [ROUTE_MAP_DIAGRAM.md](./ROUTE_MAP_DIAGRAM.md) - Visual diagrams
- [RBAC.md](./RBAC.md) - Permission matrix
- [API Specification](./api-docs/openapi.json) - OpenAPI spec
