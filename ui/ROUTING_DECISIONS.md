# Component Routing Decisions

**Date**: 2025-01-15  
**Purpose**: Document routing decisions for unrouted components  
**Source**: [ui/UI_INTEGRATION_BACKLOG.md L80-L103], [ui/AUDIT_BASELINE.md#12]

---

## Status Verification

### Already Routed / Embedded

| Component | Status | Route | Notes |
|-----------|--------|-------|-------|
| `Tenants` | ✅ Routed | `/tenants` | Admin role required |
| `Promotion` | ✅ Routed | `/promotion` | ML Pipeline group |
| `Telemetry` | ✅ Routed | `/telemetry` | Operations group |
| `InferencePlayground` | ✅ Embedded | `/inference` | Embedded in `InferencePage.tsx` |

**Verification**:
- `InferencePlayground` is imported and used in `InferencePage.tsx` (line 3, 12)
- Already accessible via `/inference` route

---

## Components Requiring Decisions

### 1. Plans (Build Plan Management)

**Component**: `src/components/Plans.tsx`  
**Type**: Build plan management  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Add to ML Pipeline nav group (`/plans`)
  - ✅ Build plans are core workflow feature
  - ✅ Fits naturally in ML Pipeline group
  - ✅ User-facing feature (not experimental)
  
- **Option B**: Feature flag for experimental
  - ❌ Plans appear to be core feature, not experimental

**Recommendation**: **Option A** - Add to ML Pipeline group

**Implementation**:
- Add route to `routes.ts`:
  ```typescript
  {
    path: '/plans',
    component: PlansPage, // Create wrapper page
    requiresAuth: true,
    navGroup: 'ML Pipeline',
    navTitle: 'Build Plans',
    navIcon: FileCode, // or appropriate icon
    navOrder: 7, // After Adapters
  }
  ```
- Create `src/pages/PlansPage.tsx` wrapper

**Decision**: **APPROVED** - Route to `/plans` in ML Pipeline group  
**Owner**: UI team  
**Status**: 🟢 Ready for implementation

---

### 2. CodeIntelligence (Repository Scanning)

**Component**: `src/components/CodeIntelligence.tsx`  
**Type**: Repository scanning / code intelligence  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Add to Operations group (`/code-intelligence`)
  - ✅ User-facing feature
  - ⚠️ May be experimental
  
- **Option B**: Feature flag for experimental
  - ✅ Allows gradual rollout
  - ✅ Can enable for specific users/tenants

**Recommendation**: **Option B** - Feature flag initially

**Rationale**: Code intelligence is a complex feature that may need refinement. Feature flag allows controlled rollout and easy rollback.

**Implementation**:
- Create feature flag: `FEATURE_CODE_INTELLIGENCE`
- Add route behind feature flag:
  ```typescript
  {
    path: '/code-intelligence',
    component: CodeIntelligencePage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Code Intelligence',
    navIcon: Code2, // or appropriate icon
    navOrder: 4, // After Replay
    featureFlag: 'FEATURE_CODE_INTELLIGENCE',
  }
  ```

**Decision**: **AWAITING APPROVAL** - Feature flag approach  
**Owner**: Product owner + UI team lead  
**Status**: 🟡 Needs decision

---

### 3. ContactsPage (Contact Discovery)

**Component**: `src/components/ContactsPage.tsx`  
**Type**: Contact discovery (CONTACTS_AND_STREAMS spec)  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Add to Operations group (`/contacts`)
  - ⚠️ CONTACTS_AND_STREAMS spec may be incomplete
  
- **Option B**: Feature flag
  - ✅ Spec pending, safe to flag

**Recommendation**: **Option B** - Feature flag until spec complete

**Rationale**: CONTACTS_AND_STREAMS spec may be in flux. Feature flag prevents exposing incomplete feature.

**Implementation**:
- Create feature flag: `FEATURE_CONTACTS_DISCOVERY`
- Add route behind feature flag:
  ```typescript
  {
    path: '/contacts',
    component: ContactsPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Contacts',
    navIcon: Users, // or appropriate icon
    navOrder: 5,
    featureFlag: 'FEATURE_CONTACTS_DISCOVERY',
  }
  ```

**Decision**: **AWAITING APPROVAL** - Feature flag approach  
**Owner**: Product owner + UI team lead  
**Status**: 🟡 Needs decision

---

### 4. TrainingStreamPage (Training Stream SSE)

**Component**: `src/components/TrainingStreamPage.tsx`  
**Type**: Training stream SSE visualization  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Integrate into existing `/training` page
  - ✅ Natural fit (training-related)
  - ✅ Reduces navigation complexity
  - ✅ Single source for training info
  
- **Option B**: Separate route (`/training/stream`)
  - ⚠️ May fragment training workflow
  - ✅ Clear separation of concerns

**Recommendation**: **Option A** - Integrate into `/training` page

**Rationale**: Training stream is a view of training data, not a separate workflow. Better UX to have it embedded.

**Implementation**:
- Modify `src/pages/TrainingPage.tsx` to include stream view
- Add tab/toggle to switch between list view and stream view
- Or add stream section below training jobs list

**Decision**: **APPROVED** - Integrate into `/training` page  
**Owner**: UI team  
**Status**: 🟢 Ready for implementation

---

### 5. DiscoveryStreamPage (Discovery Stream SSE)

**Component**: `src/components/DiscoveryStreamPage.tsx`  
**Type**: Discovery stream SSE visualization  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Feature flag for experimental
  - ✅ Discovery is likely experimental
  - ✅ Allows controlled rollout
  
- **Option B**: Add to Operations group
  - ⚠️ May be too experimental for main nav

**Recommendation**: **Option A** - Feature flag

**Rationale**: Discovery features are typically experimental. Feature flag allows gradual rollout.

**Implementation**:
- Create feature flag: `FEATURE_DISCOVERY_STREAM`
- Add route behind feature flag:
  ```typescript
  {
    path: '/discovery/stream',
    component: DiscoveryStreamPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Discovery Stream',
    navIcon: Compass, // or appropriate icon
    navOrder: 6,
    featureFlag: 'FEATURE_DISCOVERY_STREAM',
  }
  ```

**Decision**: **AWAITING APPROVAL** - Feature flag approach  
**Owner**: Product owner + UI team lead  
**Status**: 🟡 Needs decision

---

### 6. RouterConfigPage (Router Configuration)

**Component**: `src/components/RouterConfigPage.tsx`  
**Type**: Router configuration UI  
**Current Status**: Component exists, not routed

**Verification**:
- `RoutingPage.tsx` uses `RoutingInspector`, not `RouterConfigPage`
- RouterConfigPage is separate component

**Options**:
- **Option A**: Integrate into `/routing` page
  - ✅ Router config is related to routing
  - ✅ Single source for routing-related features
  
- **Option B**: Add to Settings/Operations group
  - ⚠️ May fragment routing workflow

**Recommendation**: **Option A** - Integrate into `/routing` page

**Rationale**: Router configuration is inherently routing-related. Better UX to have it in one place.

**Implementation**:
- Modify `src/pages/RoutingPage.tsx` to include config view
- Add tab/toggle: "Inspector" | "Configuration"
- Or add config section alongside inspector

**Decision**: **APPROVED** - Integrate into `/routing` page  
**Owner**: UI team  
**Status**: 🟢 Ready for implementation

---

### 7. GitIntegrationPage (Git Integration)

**Component**: `src/components/GitIntegrationPage.tsx`  
**Type**: Git repository integration  
**Current Status**: Component exists, not routed

**Options**:
- **Option A**: Feature flag for experimental
  - ✅ Git integration may be experimental
  - ✅ Allows controlled rollout
  
- **Option B**: Add to Operations group
  - ⚠️ May be too experimental for main nav

**Recommendation**: **Option A** - Feature flag initially

**Rationale**: Git integration is a complex feature that may need refinement. Feature flag allows safe rollout.

**Implementation**:
- Create feature flag: `FEATURE_GIT_INTEGRATION`
- Add route behind feature flag:
  ```typescript
  {
    path: '/git/integration',
    component: GitIntegrationPage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'Git Integration',
    navIcon: GitBranch, // or appropriate icon
    navOrder: 7,
    featureFlag: 'FEATURE_GIT_INTEGRATION',
  }
  ```

**Decision**: **AWAITING APPROVAL** - Feature flag approach  
**Owner**: Product owner + UI team lead  
**Status**: 🟡 Needs decision

---

## Summary

### Approved for Implementation (🟢)

| Component | Decision | Route/Integration |
|-----------|----------|------------------|
| Plans | Route to `/plans` | ML Pipeline group |
| TrainingStreamPage | Integrate into `/training` | Embedded in TrainingPage |
| RouterConfigPage | Integrate into `/routing` | Embedded in RoutingPage |

### Awaiting Decision (🟡)

| Component | Recommendation | Decision Needed |
|-----------|----------------|----------------|
| CodeIntelligence | Feature flag | Product owner approval |
| ContactsPage | Feature flag | Product owner approval |
| DiscoveryStreamPage | Feature flag | Product owner approval |
| GitIntegrationPage | Feature flag | Product owner approval |

### Already Routed (✅)

| Component | Route |
|-----------|-------|
| Tenants | `/tenants` |
| Promotion | `/promotion` |
| Telemetry | `/telemetry` |
| InferencePlayground | `/inference` (embedded) |

---

## Feature Flag System Requirements

If feature flags are approved, implement:

1. **Feature Flag Configuration**
   - Environment variable: `VITE_FEATURE_FLAGS` (comma-separated)
   - Example: `VITE_FEATURE_FLAGS=FEATURE_CODE_INTELLIGENCE,FEATURE_CONTACTS_DISCOVERY`
   - Or tenant-specific flags via API

2. **Route Guard**
   - Check feature flag in `RouteGuard` component
   - Hide nav items if flag disabled
   - Redirect to 404 or dashboard if route accessed without flag

3. **Documentation**
   - Document available feature flags
   - Document how to enable/disable
   - Document flag lifecycle (experimental → beta → stable)

---

## Implementation Order

1. **Phase 1** (Immediate):
   - Integrate TrainingStreamPage into TrainingPage
   - Integrate RouterConfigPage into RoutingPage
   - Add Plans route

2. **Phase 2** (After decision):
   - Implement feature flag system (if approved)
   - Add feature-flagged routes
   - Update navigation components

---

## Next Steps

1. **Product Owner Review**: Review and approve feature flag recommendations
2. **Feature Flag System**: Implement if approved
3. **Route Updates**: Update `routes.ts` with approved routes
4. **Navigation Updates**: Update nav components to show/hide routes
5. **Documentation**: Document feature flags and routing decisions

---

**Last Updated**: 2025-01-15  
**Next Review**: After product owner decisions

