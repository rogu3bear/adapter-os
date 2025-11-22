# Endpoint Verification Checklist

**Generated:** 2025-11-22
**Coverage:** 68% (60/88 endpoints)
**Missing:** 28 endpoints
**Status:** Analysis complete, ready for implementation

---

## Frontend Endpoints Called (88 Total)

### ✅ ALREADY IMPLEMENTED (60 endpoints)

#### Health & Auth (11)
- [x] GET `/healthz`
- [x] GET `/healthz/all`
- [x] GET `/readyz`
- [x] POST `/v1/auth/login`
- [x] POST `/v1/auth/logout`
- [x] GET `/v1/auth/me`
- [x] POST `/v1/auth/refresh`
- [x] GET `/v1/auth/sessions`
- [x] DELETE `/v1/auth/sessions/{jti}`
- [x] GET `/v1/auth/bootstrap`
- [x] POST `/v1/auth/bootstrap`

#### Core Adapters (13)
- [x] GET `/v1/adapters`
- [x] GET `/v1/adapters/{id}`
- [x] POST `/v1/adapters/register`
- [x] DELETE `/v1/adapters/{id}`
- [x] POST `/v1/adapters/{id}/load`
- [x] POST `/v1/adapters/{id}/unload`
- [x] GET `/v1/adapters/{id}/activations`
- [x] POST `/v1/adapters/{id}/lifecycle/promote`
- [x] POST `/v1/adapters/{id}/lifecycle/demote`
- [x] GET `/v1/adapters/{id}/lineage`
- [x] GET `/v1/adapters/{id}/detail`
- [x] GET `/v1/adapters/{id}/manifest`
- [x] POST `/v1/adapters/swap`

#### Training (3)
- [x] GET `/v1/training/jobs`
- [x] GET `/v1/training/jobs/{id}`
- [x] POST `/v1/training/start`
- [x] GET `/v1/training/templates`
- [x] POST `/v1/training/templates`

#### Tenants & Nodes (5)
- [x] GET `/v1/tenants`
- [x] POST `/v1/tenants`
- [x] GET `/v1/nodes`
- [x] POST `/v1/nodes/register`
- [x] GET `/v1/nodes/{id}/details`

#### Metrics (3)
- [x] GET `/v1/metrics/system`
- [x] GET `/v1/metrics/quality`
- [x] GET `/v1/metrics/adapters`

#### Inference (3)
- [x] POST `/v1/infer`
- [x] POST `/v1/infer/batch`
- [x] POST `/v1/infer/stream`

#### Routing & Debugging (2)
- [x] POST `/v1/routing/debug`
- [x] GET `/v1/routing/history`
- [x] POST `/v1/routing/decisions`

#### Policies (4)
- [x] GET `/v1/policies`
- [x] GET `/v1/policies/{id}`
- [x] POST `/v1/policies/validate`
- [x] POST `/v1/policies/apply`
- [x] POST `/v1/policies/compare`

#### Plans (4)
- [x] GET `/v1/plans`
- [x] POST `/v1/plans/build`
- [x] POST `/v1/plans/compare`
- [x] POST `/v1/plans/{id}/rebuild`

#### Promotion (3)
- [x] POST `/v1/cp/promote`
- [x] POST `/v1/cp/promote/dry-run`
- [x] POST `/v1/cp/rollback`
- [x] GET `/v1/cp/promotions`

#### Golden Runs (2)
- [x] GET `/v1/golden/runs`
- [x] POST `/v1/golden/compare`

#### Telemetry (2)
- [x] GET `/v1/telemetry/bundles`
- [x] POST `/v1/telemetry/bundles/purge`

#### Contacts & Streams (4)
- [x] GET `/v1/contacts`
- [x] POST `/v1/contacts`
- [x] GET `/v1/contacts/{id}`
- [x] GET `/v1/streams/training`
- [x] GET `/v1/streams/discovery`
- [x] GET `/v1/streams/contacts`

#### Code & Git (5)
- [x] POST `/v1/code/register-repo`
- [x] POST `/v1/code/scan`
- [x] POST `/v1/code/commit-delta`
- [x] GET `/v1/repositories`
- [x] GET `/v1/code/repositories`

#### Datasets (1)
- [x] POST `/v1/datasets`
- [x] GET `/v1/datasets`

#### Domain Adapters (1)
- [x] GET `/v1/domain-adapters`
- [x] POST `/v1/domain-adapters`

#### Adapter Stacks (2)
- [x] GET `/v1/adapter-stacks`
- [x] POST `/v1/adapter-stacks`
- [x] POST `/v1/adapter-stacks/deactivate` ✓ VERIFIED AT ROUTES.RS:751

#### Federation (3)
- [x] GET `/v1/federation/status`
- [x] GET `/v1/federation/quarantine`
- [x] POST `/v1/federation/release-quarantine`

#### Workers (2)
- [x] GET `/v1/workers`
- [x] POST `/v1/workers/spawn`

#### Audit (2)
- [x] GET `/v1/audit/logs`
- [x] GET `/v1/audit/compliance`

#### Monitoring (3)
- [x] POST `/v1/monitoring/rules`
- [x] GET `/v1/monitoring/anomalies`
- [x] GET `/v1/monitoring/dashboards`

#### Replay (1)
- [x] POST `/v1/replay/sessions`

#### Plugins (1)
- [x] GET `/v1/plugins`

#### System (1)
- [x] GET `/v1/system/memory`

#### Services (2)
- [x] POST `/v1/services/essential/start`
- [x] POST `/v1/services/essential/stop`

#### Meta (1)
- [x] GET `/v1/meta`

---

### ❌ NOT IMPLEMENTED (28 endpoints)

#### Models (5) - HIGH PRIORITY
- [ ] GET `/v1/models` - List all models
- [ ] GET `/v1/models/status/all` - Bulk status
- [ ] POST `/v1/models/{id}/download` - Download model
- [ ] GET `/v1/models/cursor-config` - Cursor IDE config
- [ ] GET `/v1/models/imports/{id}` - Import status tracking

**Status:** Not wired. Need 5 new handlers in `models.rs`

#### Training Sessions (4) - HIGH PRIORITY
- [ ] POST `/v1/training/sessions` → **MAP to `/v1/training/start`**
- [ ] GET `/v1/training/sessions` → **MAP to `/v1/training/jobs`**
- [ ] GET `/v1/training/sessions/{id}` → **MAP to `/v1/training/jobs/{id}`**
- [ ] POST `/v1/training/sessions/{id}/pause` - **IMPLEMENT**
- [ ] POST `/v1/training/sessions/{id}/resume` - **IMPLEMENT**

**Status:** 3 need mapping (frontend only), 2 need backend handlers

#### Memory (2) - HIGH PRIORITY
- [ ] GET `/v1/memory/usage` → **MAP to `/v1/system/memory`**
- [ ] POST `/v1/memory/adapters/{id}/evict` - **IMPLEMENT**

**Status:** 1 needs mapping, 1 needs handler

#### Auth Extensions (6) - MEDIUM PRIORITY
- [ ] POST `/v1/auth/logout-all` - **IMPLEMENT**
- [ ] GET `/v1/auth/token` - **IMPLEMENT**
- [ ] POST `/v1/auth/token/rotate` → **MAP to `/v1/auth/refresh`**
- [ ] GET `/v1/auth/profile` - **EXTEND `/v1/auth/me`**
- [ ] PUT `/v1/auth/config` - **IMPLEMENT**
- [ ] POST `/v1/auth/dev-bypass` - **CONDITIONAL (dev-only)**

**Status:** 4 need handlers, 1 needs extension, 1 needs mapping

#### Metrics (1) - MEDIUM PRIORITY
- [ ] GET `/v1/metrics/snapshot` - Composite metrics

**Status:** Not wired. Need 1 handler as convenience endpoint

#### Adapter Stacks (1) - MEDIUM PRIORITY
- [ ] POST `/v1/stacks/validate-name` - Validate stack name

**Status:** Not wired. Need 1 handler (similar to adapter validation)

#### Workspaces (1) - MEDIUM PRIORITY
- [ ] GET `/v1/workspaces/my` - Current user's workspaces

**Status:** Not wired. Need 1 handler

#### System Status (1) - LOW PRIORITY
- [ ] GET `/v1/status` - Overall system status

**Status:** Not wired. Can aggregate existing endpoints

#### Orchestration (3) - LOW PRIORITY (DEPRECATED)
- [ ] GET `/v1/orchestration/config` - **REMOVE from frontend**
- [ ] PUT `/v1/orchestration/config` - **REMOVE from frontend**
- [ ] POST `/v1/orchestration/analyze` - **REMOVE from frontend**
- [ ] GET `/v1/orchestration/metrics` - **REMOVE from frontend**

**Status:** Experimental feature. Frontend should not call these.

#### Admin (1) - LOW PRIORITY (DEPRECATED)
- [ ] POST `/v1/admin/users` - **REMOVE from frontend**

**Status:** Should be CLI-only. Remove from frontend.

#### Security (2) - LOW PRIORITY (DEPRECATED)
- [ ] POST `/v1/security/isolation/test` - **REMOVE from frontend**
- [ ] GET `/v1/security/anomaly/status` → **MAP to `/v1/monitoring/anomalies`**

**Status:** 1 should be removed, 1 should be mapped

#### Promotion (1) - VERIFY
- [ ] GET `/v1/cp/promotion-gates/{cpid}` - **VERIFY wiring**

**Status:** VERIFIED - Line 522-525 in routes.rs ✓

---

## Summary by Action Type

### 💾 IMPLEMENT Backend Handler (11)
1. `/v1/models` (list)
2. `/v1/models/status/all` (bulk)
3. `/v1/models/{id}/download` (async)
4. `/v1/models/cursor-config`
5. `/v1/models/imports/{id}` (track)
6. `/v1/training/jobs/{id}/pause`
7. `/v1/training/jobs/{id}/resume`
8. `/v1/memory/adapters/{id}/evict`
9. `/v1/auth/logout-all`
10. `/v1/auth/token`
11. `/v1/auth/config` (PUT)
12. `/v1/stacks/validate-name`
13. `/v1/metrics/snapshot`
14. `/v1/workspaces/my`
15. `/v1/status`

**Handler distribution:**
- `models.rs`: 5 handlers
- `training.rs`: 2 handlers
- `auth_enhanced.rs`: 3 handlers
- `adapters.rs`: 1 handler
- `adapter_stacks.rs`: 1 handler
- `workspaces.rs`: 1 handler
- Main handlers: 1 handler
- **Total: 14 handlers needed**

### 🔄 MAP/ALIAS Frontend Calls (3)
1. `/v1/training/sessions` → `/v1/training/start`
2. `/v1/training/sessions/{id}` → `/v1/training/jobs/{id}`
3. `/v1/memory/usage` → `/v1/system/memory`
4. `/v1/auth/token/rotate` → `/v1/auth/refresh`
5. `/v1/security/anomaly/status` → `/v1/monitoring/anomalies`

**Frontend modifications:**
- Update method signatures to call correct endpoints
- No backend changes needed
- Can be done incrementally

### 📝 EXTEND Existing Handler (1)
1. `/v1/auth/me` - Add profile data to response

**Modification:**
- Update response type to include additional fields
- Return from same endpoint

### ❌ REMOVE From Frontend (5)
1. `orchestrationGetConfig()`
2. `orchestrationUpdateConfig()`
3. `orchestrationAnalyze()`
4. `orchestrationGetMetrics()`
5. `createAdminUser()`
6. `testIsolation()`

**Action:** Delete methods from `ui/src/api/client.ts`

### ⚡ CONDITIONAL Implementation (1)
1. `/v1/auth/dev-bypass` - Dev-only route

**Implementation:**
- Guard with feature flag or dev mode check
- Return error in production

---

## Files to Modify Summary

### Backend
```
crates/adapteros-server-api/src/
├── routes.rs                    [Add 11 routes]
└── handlers/
    ├── models.rs               [Add 5 handlers]
    ├── training.rs             [Add 2 handlers]
    ├── auth_enhanced.rs        [Add 3 handlers]
    ├── adapters.rs             [Add 1 handler]
    ├── adapter_stacks.rs       [Add 1 handler]
    ├── workspaces.rs           [Add 1 handler]
    ├── auth.rs                 [Extend 1 handler]
    └── main                    [Add 1 handler]
```

### Frontend
```
ui/src/api/
├── client.ts                   [Add 15 methods, Remove 6, Update 5]
└── types.ts                    [May need response type additions]
```

---

## Pre-Implementation Checklist

- [ ] Review FRONTEND_BACKEND_ALIGNMENT.md (full details)
- [ ] Review FRONTEND_BACKEND_ALIGNMENT_SUMMARY.md (quick ref)
- [ ] Set up test environment
- [ ] Create feature branch for each phase
- [ ] Assign handlers to team members
- [ ] Prepare test cases
- [ ] Schedule code reviews

---

## Implementation Order (Recommended)

1. **Phase 1 (Critical Path)**
   - Model list endpoints (5 handlers)
   - Training session mappings (3 frontend updates)
   - Memory endpoint mappings (1 frontend update)
   - Auth token rotation mapping (1 frontend update)

2. **Phase 2 (Core Features)**
   - Training pause/resume (2 handlers)
   - Auth extensions (3 handlers)
   - Stack name validation (1 handler)

3. **Phase 3 (Polish)**
   - Memory eviction (1 handler)
   - Metrics snapshot (1 handler)
   - Status aggregate (1 handler)
   - Workspaces filter (1 handler)
   - Auth profile extension (1 handler)

4. **Phase 4 (Cleanup)**
   - Remove 6 deprecated frontend methods
   - Update documentation

---

## Testing Strategy

### Per-Endpoint Testing (x28)
For each missing endpoint:
- [ ] Handler exists and compiles
- [ ] Route is wired in routes.rs
- [ ] OpenAPI documentation generated
- [ ] Unit test passes
- [ ] Integration test passes (with auth)
- [ ] Error cases handled
- [ ] Frontend calls updated
- [ ] E2E test passes

### Regression Testing
- [ ] All 60 existing endpoints still work
- [ ] Auth tests pass
- [ ] Permission checks still enforced
- [ ] No breaking changes to response types

---

## Success Criteria

- ✅ All 28 missing endpoints addressed
- ✅ 11 handlers implemented
- ✅ 5 frontend methods mapped
- ✅ 1 handler extended
- ✅ 6 deprecated methods removed
- ✅ 1 conditional route added
- ✅ All tests passing
- ✅ Documentation updated
- ✅ Zero breaking changes

---

## Verification Commands (Post-Implementation)

```bash
# Verify backend routes
grep -c "\.route(" crates/adapteros-server-api/src/routes.rs
# Should be ~75 (was 64, +11 new)

# Verify frontend methods
grep -c "^\s*async\s" ui/src/api/client.ts
# Should match endpoint count

# Test all endpoints
cargo test --workspace

# Build docs
cargo doc --no-deps

# Check for unmapped calls
grep "/v1/" ui/src/api/client.ts | grep -v "/v1/metrics" | sort -u
```

---

**Status:** ✅ Analysis Complete - Ready for Implementation
**Document Version:** 1.0
**Last Updated:** 2025-11-22
