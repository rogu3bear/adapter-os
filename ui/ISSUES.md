# UI Integration Issues Tracking

**Date**: 2025-01-15  
**Purpose**: Track API gaps, backend requirements, and routing decisions with owners and status

---

## Issue #1: Dashboard Recent Activity API Integration

**Priority**: MEDIUM  
**Type**: Backend + UI Integration  
**Source**: [ui/UI_INTEGRATION_BACKLOG.md L60-L79], [ui/AUDIT_BASELINE.md#11]

**Description**:
Dashboard Recent Activity section currently uses hardcoded mock data. Requires REST endpoint and SSE stream for real-time updates.

**Requirements**:

1. **REST Endpoint**: `GET /v1/telemetry/events/recent`
   - Query params: `tenant_id` (required, from JWT), `event_types[]` (optional), `limit` (default: 50, max: 200)
   - Response: Array of recent events sorted by timestamp (descending)
   - Event types: `node_recovery`, `policy_update`, `build_completion`, `adapter_registration`, `telemetry_export`

2. **SSE Stream**: `GET /v1/telemetry/events/recent/stream`
   - Query params: `tenant_id` (from JWT), `event_types[]` (optional)
   - Stream new events as they occur
   - SSE keep-alive: 30s interval
   - Filter by tenant_id and event_types

3. **UI Integration**:
   - Replace mock data in `Dashboard.tsx:200-209`
   - Fetch initial data from REST endpoint on mount
   - Connect to SSE stream for live updates
   - Graceful degradation if SSE unavailable

**Owners**:
- Backend: Backend team (endpoint implementation)
- UI: UI team (integration)

**Status**: 🔴 **OPEN**  
**Labels**: `backend-required`, `ui-integration`, `medium-priority`

**Acceptance Criteria**:
- [ ] REST endpoint returns recent events filtered by tenant
- [ ] SSE stream pushes new events in real-time
- [ ] UI displays real data (no mock)
- [ ] UI handles SSE disconnection gracefully
- [ ] Tests cover both REST and SSE endpoints

---

## Issue #2: Unrouted Components Decision & Implementation

**Priority**: LOW  
**Type**: Routing Decision + Implementation  
**Source**: [ui/UI_INTEGRATION_BACKLOG.md L80-L103], [ui/AUDIT_BASELINE.md#12]

**Description**:
Multiple components exist but are not accessible via UI navigation. Requires routing decisions and potential feature flag implementation.

**Components Requiring Decision**:

1. **Plans** (Build plan management)
   - Option A: Add to ML Pipeline nav group (`/plans`)
   - Option B: Feature flag for experimental
   - **Recommendation**: ML Pipeline group (build plans are core workflow)

2. **CodeIntelligence** (Repository scanning)
   - Option A: Add to Operations group (`/code-intelligence`)
   - Option B: Feature flag for experimental
   - **Recommendation**: Feature flag initially (experimental feature)

3. **ContactsPage** (Contact discovery)
   - Option A: Add to Operations group (`/contacts`)
   - Option B: Feature flag
   - **Recommendation**: Feature flag (CONTACTS_AND_STREAMS spec pending)

4. **TrainingStreamPage** (Training stream SSE)
   - Option A: Integrate into existing `/training` page
   - Option B: Separate route (`/training/stream`)
   - **Recommendation**: Integrate into `/training` page

5. **DiscoveryStreamPage** (Discovery stream SSE)
   - Option A: Feature flag for experimental
   - Option B: Integrate into existing page
   - **Recommendation**: Feature flag (experimental)

6. **InferencePlayground** (Interactive inference)
   - Option A: Verify if embedded in `/inference` page
   - Option B: Separate route if not embedded
   - **Recommendation**: Verify current state first

7. **RouterConfigPage** (Router configuration)
   - Option A: Verify if embedded in `/routing` page
   - Option B: Add to Operations group if not embedded
   - **Recommendation**: Verify current state first

8. **GitIntegrationPage** (Git integration)
   - Option A: Feature flag for experimental
   - Option B: Add to Operations group
   - **Recommendation**: Feature flag initially

**Action Items**:
1. Verify current state of `InferencePlayground` and `RouterConfigPage`
2. Product owner review routing decisions
3. Implement feature flag system if not exists
4. Update `routes.ts` with new routes or flag configurations
5. Update navigation components

**Owners**:
- Decision: Product owner + UI team lead
- Implementation: UI team

**Status**: 🟡 **AWAITING DECISION**  
**Labels**: `routing`, `low-priority`, `needs-decision`

**Acceptance Criteria**:
- [ ] Routing decisions documented and approved
- [ ] Feature flag system implemented (if needed)
- [ ] All decided routes added to `routes.ts`
- [ ] Navigation updated to show new routes
- [ ] Experimental features behind flags documented

---

## Issue #3: SSE Endpoint Token Authentication

**Priority**: HIGH  
**Type**: Backend Required  
**Source**: [ui/UI_INTEGRATION_BACKLOG.md L104-L150], [ui/AUDIT_BASELINE.md#13]

**Description**:
UI passes JWT token as query parameter for SSE endpoints (`?token=xxx`), but backend SSE endpoints currently only support header-based authentication via `Extension(claims)` middleware.

**Affected Endpoints**:
- `/v1/stream/metrics`
- `/v1/stream/telemetry`
- `/v1/stream/adapters`
- `/v1/streams/training`
- `/v1/streams/discovery`
- `/v1/streams/contacts`
- `/v1/streams/file-changes`

**Required Changes**:

1. Extract token from query parameter `?token=xxx`
2. Validate JWT token
3. Reject unauthenticated connections (401)
4. Maintain compatibility with header-based auth (fallback)

**Implementation Pattern**:
```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct SseQuery {
    token: Option<String>,
}

async fn sse_handler(
    Query(query): Query<SseQuery>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Event>>, StatusCode> {
    // Try query param first, fallback to header
    let token = query.token
        .or_else(|| extract_token_from_header(&headers).ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let claims = verify_jwt(&token)?;
    
    // Continue with SSE stream...
}
```

**Testing Requirements**:
- Test with query parameter token
- Test with header token (backward compatibility)
- Test with invalid/missing token (401)
- Test with expired token (401)

**Owner**: Backend team
**Status**: ✅ **RESOLVED** (2025-11-15)
**Labels**: `backend-required`, `high-priority`, `security`

**Resolution**:
Fixed at the middleware level in `crates/adapteros-server-api/src/middleware.rs`. Both `auth_middleware` and `dual_auth_middleware` now properly extract and validate tokens from query parameters using `url::form_urlencoded::parse()`.

**Implementation Details**:
- Modified `auth_middleware` (L71-76) to parse `?token=xxx` from query string
- Modified `dual_auth_middleware` (L175-180) to parse `?token=xxx` from query string
- Token precedence: Bearer header → Cookie → Query parameter
- All SSE endpoints automatically benefit from this fix via middleware
- Backward compatibility maintained with header-based and cookie-based auth

**Acceptance Criteria**:
- [x] All 7 SSE endpoints accept token from query parameter (via middleware)
- [x] Header-based auth still works (backward compatibility)
- [x] Cookie-based auth still works (backward compatibility)
- [x] Invalid/missing tokens return 401 (existing validation)
- [ ] UI SSE connections work without errors (requires testing)

---

## Issue #4: Adapter Policy Update Endpoint

**Priority**: MEDIUM  
**Type**: Backend Required  
**Source**: [ui/API_IMPLEMENTATION_PLAN.md L34-L80], [ui/AUDIT_BASELINE.md#21]

**Description**:
`AdapterLifecycleManager.tsx` has a TODO at line 230 to call API for policy updates, but the endpoint `PUT /v1/adapters/:id/policy` does not exist on backend.

**Component**: `AdapterLifecycleManager.tsx`  
**TODO Location**: Line 230

**Required Endpoint**: `PUT /v1/adapters/:id/policy`

**Request Body**:
```json
{
  "policy_changes": {
    "memory_limit_mb": 512,
    "eviction_priority": "low",
    "allowed_tenants": ["tenant-1", "tenant-2"]
  }
}
```

**Response**: Updated adapter with new policy

**Security**:
- Require JWT authentication
- Require Admin or Operator role
- Validate policy changes against policy packs

**Owner**: Backend team  
**Status**: 🔴 **OPEN**  
**Labels**: `backend-required`, `medium-priority`, `api-gap`

**Acceptance Criteria**:
- [ ] Endpoint `PUT /v1/adapters/:id/policy` implemented
- [ ] Endpoint validates policy changes against 20 canonical policy packs
- [ ] Endpoint requires Admin/Operator role
- [ ] UI TODO in `AdapterLifecycleManager.tsx:230` resolved
- [ ] Tests cover policy update scenarios
- [ ] Policy updates reflected in adapter behavior

---

## Issue Summary

| Issue | Priority | Type | Owner | Status |
|-------|----------|------|-------|--------|
| #1: Recent Activity API | MEDIUM | Backend + UI | Backend + UI | 🔴 OPEN |
| #2: Unrouted Components | LOW | Routing | Product + UI | 🟡 AWAITING DECISION |
| #3: SSE Token Auth | HIGH | Backend | Backend | 🔴 OPEN |
| #4: Policy Update Endpoint | MEDIUM | Backend | Backend | 🔴 OPEN |

**Legend**:
- 🔴 OPEN - Work not started
- 🟡 AWAITING DECISION - Blocked on decision
- 🟢 IN PROGRESS - Work in progress
- ✅ CLOSED - Completed

---

**Last Updated**: 2025-01-15

