# UI Integration Backlog

This document tracks remaining UI integration tasks that were identified but not immediately addressable due to scope or architectural considerations.

## Critical Integration Issues ✓ COMPLETED

1. ✓ **SystemMetrics API Type Mismatches** - Fixed field name mismatches
2. ✓ **Policy Pack Count** - Updated from 22 to 20 per CLAUDE.md
3. ✓ **SSE Authentication** - Added token-based auth for EventSource
4. ✓ **Unused Component Imports** - Removed 12 unused imports
5. ✓ **SystemMetrics Type Definition** - Enhanced with proper documentation

## Outstanding Issues

### 1. Console Logging Violations ⚠ HIGH PRIORITY

**Status**: Deferred (requires architectural decision)
**Scope**: 32 files, 70+ instances
**Violation**: CLAUDE.md:207 requires `tracing` for logging (Rust), UI uses `console.*`

**Files Affected**:
```
App.tsx, Dashboard.tsx, Policies.tsx, Adapters.tsx, Nodes.tsx,
WorkersTab.tsx, TrainingWizard.tsx, Telemetry.tsx, and 24 others
```

**Recommendation**:
- **Short-term**: Remove debug `console.log` statements, keep `console.error` for critical errors
- **Long-term**: Implement structured logging framework compatible with backend telemetry
  - Options: `winston`, `pino`, or custom logger that sends to `/v1/telemetry/events`
  - Integrate with backend's canonical JSON event logging
  - Support log levels matching backend (debug, info, warn, error)

**Current State**:
- ✓ Using `toast` notifications for user-facing errors
- ✓ Using `console.error` for debugging
- ✗ No structured logging or telemetry integration

### 2. Incomplete API Implementations MEDIUM PRIORITY

**Status**: Documented as TODOs
**Scope**: 10+ components with placeholder functionality

**Components with TODO Comments**:

| Component | Line | Description |
|-----------|------|-------------|
| TrainingMonitor.tsx | 80 | Implement pause functionality |
| CodeIntelligence.tsx | 118 | Implement actual training API call |
| AdapterLifecycleManager.tsx | 195,207,219,230 | API calls for state/pin/evict/policy |
| CodeIntelligenceTraining.tsx | 118 | Replace with actual API calls |
| AdapterMemoryMonitor.tsx | 130,142,151 | API calls for memory/evict/pin |

**Recommendation**:
- Prioritize based on user workflows
- Connect to existing backend endpoints where available
- Work with backend team to implement missing endpoints
- Create feature flags to hide incomplete features

### 3. Hardcoded Mock Data MEDIUM PRIORITY

**Status**: Documented
**Location**: `Dashboard.tsx:200-209`

**Issue**: Recent Activity section uses static placeholder data

**Recommendation**:
- Create `/v1/telemetry/events/recent` or `/v1/audit/recent` endpoint
- Fetch real-time activity from telemetry bundles
- Filter by tenant_id and event types
- Implement SSE stream for live updates

**Mock Data Currently Shown**:
- Node recovery events
- Policy updates
- Build completions
- Adapter registrations
- Telemetry exports

### 4. Components Imported But Not Routed LOW PRIORITY

**Status**: Cleaned up unused imports
**Impact**: Components exist but are not accessible via UI navigation

**Unrouted Components** (may be intentionally hidden):
- `Tenants` - Available via Operations sub-section
- `Plans` - Build plan management
- `Promotion` - Control plane promotion
- `Telemetry` - Telemetry bundle management
- `CodeIntelligence` - Repository scanning
- `ContactsPage` - Contact discovery (CONTACTS_AND_STREAMS spec)
- `TrainingStreamPage` - Training stream SSE
- `DiscoveryStreamPage` - Discovery stream SSE
- `InferencePlayground` - Interactive inference
- `RouterConfigPage` - Router configuration
- `GitIntegrationPage` - Git integration

**Recommendation**:
- Review which features should be accessible
- Add to appropriate navigation categories (Operations, Settings, etc.)
- Consider feature flags for experimental features
- Update navigation routing in App.tsx

## Server-Side Requirements

### SSE Endpoint Token Authentication

**Status**: ⚠ **REQUIRES BACKEND CHANGES**

**Issue**: UI now passes token as query parameter for SSE endpoints:
```typescript
const url = token ? `${baseUrl}${endpoint}?token=${encodeURIComponent(token)}` : `${baseUrl}${endpoint}`;
```

**Required Backend Changes**:
All SSE streaming endpoints must:
1. Extract token from query parameter: `?token=xxx`
2. Validate JWT token
3. Reject unauthenticated connections

**Affected Endpoints**:
- `/v1/stream/metrics`
- `/v1/stream/telemetry`
- `/v1/stream/adapters`
- `/v1/streams/training`
- `/v1/streams/discovery`
- `/v1/streams/contacts`
- `/v1/streams/file-changes`

**Example Rust Handler**:
```rust
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct SseQuery {
    token: Option<String>,
}

async fn system_metrics_stream(
    Query(query): Query<SseQuery>,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Event>>, StatusCode> {
    // Validate token from query parameter
    let token = query.token.ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = verify_jwt(&token)?;

    // Continue with SSE stream...
}
```

## Testing Recommendations

### Integration Testing
- [ ] Test SystemMetrics fields match backend response
- [ ] Verify SSE authentication with valid/invalid tokens
- [ ] Test policy pack compliance dashboard shows 20 packs
- [ ] Verify all navigation routes work correctly

### E2E Testing
- [ ] User login → Dashboard metrics display
- [ ] Real-time metrics updates via SSE
- [ ] Policy management workflows
- [ ] Adapter lifecycle operations

## Priority Roadmap

1. **Phase 1** (Backend Critical):
   - Implement SSE token authentication on server
   - Test SSE endpoints with query parameter tokens

2. **Phase 2** (UI Enhancement):
   - Implement Recent Activity endpoint integration
   - Complete TODO API implementations
   - Remove debug console.log statements

3. **Phase 3** (Architecture):
   - Design structured logging framework
   - Integrate with backend telemetry
   - Add feature flags for incomplete features

4. **Phase 4** (UX Polish):
   - Route unintegrated components
   - Add comprehensive error handling
   - Implement loading states for all async operations

## Files Modified (This Session)

1. `ui/src/components/Dashboard.tsx` - Fixed SystemMetrics, added TODO comments
2. `ui/src/components/Policies.tsx` - Updated policy pack count to 20
3. `ui/src/hooks/useSSE.ts` - Added token authentication
4. `ui/src/App.tsx` - Removed unused imports
5. `ui/src/api/types.ts` - Enhanced SystemMetrics type definition

## Build Status

✓ TypeScript compilation successful
✓ No type errors
✓ 1758 modules transformed
✓ Built in 2.10s

---

**Last Updated**: 2025-01-14
**Reviewer**: Claude Code Analysis
