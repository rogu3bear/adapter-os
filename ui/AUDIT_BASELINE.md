# UI Integration Audit Baseline

**Date**: 2025-01-15  
**Scope**: UI integration debt, API gaps, and compliance standards alignment  
**Sources**: [ui/UI_INTEGRATION_BACKLOG.md L52-L108], [ui/API_IMPLEMENTATION_PLAN.md L30-L200], [CONTRIBUTING.md L110-L170]

---

## Executive Summary

This audit baseline consolidates integration debt items, API gaps, and compliance requirements to ensure all future UI work aligns with documented scope and repository standards.

**Status**: 3 MEDIUM/HIGH priority items requiring backend coordination; 11 unrouted components needing routing decisions; 1 API endpoint gap.

---

## 1. Integration Debt Items

### 1.1 Hardcoded Mock Data - Dashboard Recent Activity (MEDIUM PRIORITY)

**Location**: `Dashboard.tsx:200-209`  
**Source**: [ui/UI_INTEGRATION_BACKLOG.md L60-L79]

**Current State**:
- Recent Activity section uses static placeholder data
- Shows mock events: node recovery, policy updates, build completions, adapter registrations, telemetry exports

**Required Changes**:
1. **Backend Endpoint**: Create `/v1/telemetry/events/recent` or `/v1/audit/recent`
   - Accept `tenant_id` query parameter (required)
   - Accept `event_types[]` query parameter (optional filter)
   - Accept `limit` query parameter (default: 50, max: 200)
   - Return events sorted by timestamp (descending)
   
2. **SSE Stream**: Implement `/v1/telemetry/events/recent/stream` for live updates
   - Stream new events as they occur
   - Filter by tenant_id (from JWT claims)
   - Filter by event_types (optional)
   - Use SSE keep-alive (30s interval)

3. **UI Integration**: Replace mock data in `Dashboard.tsx`
   - Fetch initial data from REST endpoint on mount
   - Connect to SSE stream for live updates
   - Implement graceful degradation if SSE unavailable
   - Show loading states during fetch

**Owner**: Backend team (endpoint), UI team (integration)  
**Tracking**: See [ISSUES.md](./ISSUES.md#issue-1)

---

### 1.2 Components Imported But Not Routed (LOW PRIORITY)

**Source**: [ui/UI_INTEGRATION_BACKLOG.md L80-L103]

**Unrouted Components Inventory**:

| Component | Current Status | Recommended Action |
|-----------|---------------|-------------------|
| `Tenants` | Available via Operations sub-section | ✅ **ROUTED** - Already in `/tenants` route (Admin role) |
| `Plans` | Build plan management | ⚠️ **NEEDS DECISION** - Add to ML Pipeline group or feature flag |
| `Promotion` | Control plane promotion | ✅ **ROUTED** - Already in `/promotion` route |
| `Telemetry` | Telemetry bundle management | ✅ **ROUTED** - Already in `/telemetry` route |
| `CodeIntelligence` | Repository scanning | ⚠️ **NEEDS DECISION** - Feature flag for experimental? |
| `ContactsPage` | Contact discovery (CONTACTS_AND_STREAMS spec) | ⚠️ **NEEDS DECISION** - Add route or feature flag |
| `TrainingStreamPage` | Training stream SSE | ⚠️ **NEEDS DECISION** - Integrate into TrainingPage or separate route? |
| `DiscoveryStreamPage` | Discovery stream SSE | ⚠️ **NEEDS DECISION** - Feature flag for experimental |
| `InferencePlayground` | Interactive inference | ⚠️ **NEEDS DECISION** - Merge into InferencePage or separate route? |
| `RouterConfigPage` | Router configuration | ⚠️ **NEEDS DECISION** - Add to Operations group or Settings |
| `GitIntegrationPage` | Git integration | ⚠️ **NEEDS DECISION** - Feature flag for experimental |

**Routing Status Check** ([source: ui/src/config/routes.ts]):
- ✅ `/tenants` - Routed (Admin role)
- ✅ `/promotion` - Routed (ML Pipeline group)
- ✅ `/telemetry` - Routed as `/telemetry`
- ❌ Plans - No route found
- ❌ CodeIntelligence - No route found
- ❌ ContactsPage - No route found
- ❌ TrainingStreamPage - No route found
- ❌ DiscoveryStreamPage - No route found
- ❌ InferencePlayground - Note: `/inference` exists, verify if it includes playground
- ❌ RouterConfigPage - Note: `/routing` exists, verify if it includes config
- ❌ GitIntegrationPage - No route found

**Action Items**:
1. Verify if `InferencePlayground` is embedded in `/inference` page
2. Verify if `RouterConfigPage` is embedded in `/routing` page
3. Decide routing strategy for remaining 7 components
4. Create feature flags for experimental features
5. Update `routes.ts` with new routes or flag configurations

**Owner**: UI team lead + Product owner  
**Tracking**: See [ISSUES.md](./ISSUES.md#issue-2)

---

### 1.3 SSE Endpoint Token Authentication (HIGH PRIORITY - BACKEND REQUIRED)

**Source**: [ui/UI_INTEGRATION_BACKLOG.md L104-L150]

**Current State**:
- UI passes token as query parameter: `?token=${encodeURIComponent(token)}`
- Backend SSE endpoints currently use `Extension(claims)` middleware (header-based auth)
- Query parameter auth not implemented on server

**Affected SSE Endpoints**:
- `/v1/stream/metrics`
- `/v1/stream/telemetry`
- `/v1/stream/adapters`
- `/v1/streams/training`
- `/v1/streams/discovery`
- `/v1/streams/contacts`
- `/v1/streams/file-changes`

**Required Backend Changes**:
1. Extract token from query parameter `?token=xxx`
2. Validate JWT token
3. Reject unauthenticated connections (401)
4. Maintain compatibility with header-based auth (fallback)

**Implementation Pattern** (from backlog example):
```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct SseQuery {
    token: Option<String>,
}

async fn sse_handler(
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Event>>, StatusCode> {
    let token = query.token
        .or_else(|| extract_token_from_header(&headers).ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = verify_jwt(&token)?;
    // Continue with SSE stream...
}
```

**Owner**: Backend team  
**Tracking**: See [ISSUES.md](./ISSUES.md#issue-3)

---

## 2. API Integration Status & Gaps

### 2.1 Adapter Lifecycle Management

**Source**: [ui/API_IMPLEMENTATION_PLAN.md L34-L80]

**Status**: ✅ Mostly complete

**Completed**:
- ✅ `POST /v1/adapters/:id/promote` - Exists
- ✅ `POST /v1/adapters/:id/pin` - Exists
- ✅ `POST /v1/memory/adapters/:id/evict` - Exists

**Gap**: ❌ `PUT /v1/adapters/:id/policy` - **NEEDS BACKEND IMPLEMENTATION**

**Component**: `AdapterLifecycleManager.tsx`  
**TODO Location**: Line 230  
**Owner**: Backend team  
**Tracking**: See [ISSUES.md](./ISSUES.md#issue-4)

---

### 2.2 Adapter Memory Monitor

**Source**: [ui/API_IMPLEMENTATION_PLAN.md L88-L133]

**Status**: ✅ Complete

**Endpoints**:
- ✅ `GET /v1/memory/usage` - Exists
- ✅ `POST /v1/memory/adapters/:id/evict` - Exists
- ✅ `POST /v1/memory/adapters/:id/pin` - Exists

**Enhancement Opportunity**: Consider SSE stream for auto-refresh instead of polling (5s interval suggested in plan).

---

### 2.3 Training Monitor

**Source**: [ui/API_IMPLEMENTATION_PLAN.md L136-L179]

**Status**: ✅ Complete

**Endpoints**:
- ✅ `POST /v1/training/sessions/:id/pause` - Implemented (idempotent; Operator role)
- ✅ `POST /v1/training/sessions/:id/resume` - Implemented (strict validation; Operator role)
- ✅ `GET /v1/training/sessions/:id` - Exists
- ✅ `GET /v1/training/sessions` - Exists

**Enhancement Opportunity**: Backend optimization for GPU resource release during pause (tracked separately).

---

## 3. Compliance Baseline

**Source**: [CONTRIBUTING.md L110-L170]

### 3.1 Code Standards

**Formatting & Linting**:
- ✅ Use `cargo fmt --all` for Rust formatting
- ✅ Use `cargo clippy --workspace -- -D warnings` for linting
- ✅ Follow Rust naming conventions (PascalCase for types, snake_case for functions)
- ✅ Prefer `Result<T>` over `Option<T>` for error handling

**Logging**:
- ✅ Use `tracing` for all logging (never `println!`)
- ✅ Use appropriate log levels: `trace`, `debug`, `info`, `warn`, `error`
- ✅ Include structured fields for querying

**Documentation**:
- ✅ Document all public APIs
- ✅ Include examples for complex functions
- ✅ Update README.md for user-facing changes

**Testing**:
- ✅ Add tests for new functionality
- ✅ Run `cargo test --workspace` before committing

### 3.2 Policy Compliance

- ✅ All changes must comply with 20 canonical policy packs
- ✅ Security-sensitive code requires review
- ✅ Performance changes need benchmarks
- ✅ Breaking changes need migration guides

### 3.3 Commit Guidelines

**Format**:
```
type(scope): brief description

Detailed description of changes, including:
- What was changed
- Why it was changed
- Any breaking changes or migration notes

Fixes #123
```

**Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

**See**: [COMPLIANCE_CHECKLIST.md](./COMPLIANCE_CHECKLIST.md) for PR gating checklist

---

## 4. Definition of Done (Baseline)

### Integration Debt
- [ ] No mock UI sections remain where APIs exist
- [ ] Mock-only areas are feature-flagged and documented
- [ ] Recent Activity endpoint integrated (REST + SSE)
- [ ] SSE token authentication working on all endpoints
- [ ] Unrouted component decisions recorded and implemented

### API Gaps
- [ ] `PUT /v1/adapters/:id/policy` endpoint implemented on backend
- [ ] UI wired to policy update endpoint
- [ ] All TODO items in `AdapterLifecycleManager.tsx` resolved

### Compliance
- [ ] All changes pass `cargo fmt --all`
- [ ] All changes pass `cargo clippy --workspace -- -D warnings`
- [ ] No `println!` statements (only `tracing`)
- [ ] Tests added for new functionality
- [ ] Commit messages follow conventional format

---

## 5. Priority Roadmap

1. **Phase 1** (Backend Critical - Week 1):
   - Implement SSE token authentication on server (Issue #3)
   - Implement `PUT /v1/adapters/:id/policy` endpoint (Issue #4)

2. **Phase 2** (UI Enhancement - Week 2):
   - Integrate Recent Activity endpoint (REST + SSE) (Issue #1)
   - Complete unrouted component decisions and routing (Issue #2)

3. **Phase 3** (Architecture - Week 3+):
   - Feature flags system for experimental features
   - Structured logging framework integration

---

## 6. Tracking Documents

- **Issues**: [ISSUES.md](./ISSUES.md) - Detailed issue tracking with owners and status
- **Compliance**: [COMPLIANCE_CHECKLIST.md](./COMPLIANCE_CHECKLIST.md) - PR gating checklist

---

**Last Updated**: 2025-01-15  
**Next Review**: After Phase 1 completion

