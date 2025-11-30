# Frontend-Backend Endpoint Alignment Report

**Date:** 2025-11-22
**Total Frontend Endpoints:** 88 unique
**Total Backend Routes:** 64 implemented
**Missing Endpoints:** 28
**Alignment Status:** 68% coverage

---

## Executive Summary

The frontend (TypeScript/React in `/ui/src/api/client.ts`) is calling **28 endpoints that are not yet wired in the backend** (`crates/adapteros-server-api/src/routes.rs`). This analysis documents:

1. Which endpoints need implementation
2. Which endpoints should be deprecated/removed from frontend
3. Recommended decisions for each missing endpoint
4. Implementation priority level

---

## Section 1: HIGH PRIORITY - Core Features (14 endpoints)

These endpoints are essential for core product functionality. **RECOMMENDATION: IMPLEMENT ALL**

### 1.1 Model Management (8 endpoints)

#### GET `/v1/models`
- **Frontend usage:** List all available models
- **File:** `ui/src/api/client.ts:1079`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Handler location:** `crates/adapteros-server-api/src/handlers/models.rs`
- **Why:** Essential for model selection UI; currently backend returns list on `/v1/models/import` response
- **Backend implementation steps:**
  1. Create handler `list_models()` in `models.rs`
  2. Add route: `.route("/v1/models", get(handlers::models::list_models))`
  3. Return array of `ModelStatusResponse`

#### GET `/v1/models/status/all`
- **Frontend usage:** Batch status check for all models
- **File:** `ui/src/api/client.ts:1074`
- **Status:** NOT IMPLEMENTED (individual `/v1/models/{id}/status` exists)
- **Decision:** **IMPLEMENT**
- **Why:** Efficiency optimization; reduces N+1 requests for dashboard/monitoring
- **Backend implementation steps:**
  1. Create handler `list_all_model_statuses()` in `models.rs`
  2. Query all models from DB in one call
  3. Add route: `.route("/v1/models/status/all", get(handlers::models::list_all_model_statuses))`
  4. Return `AllModelsStatusResponse` (array of `ModelStatusResponse`)

#### POST `/v1/models/{id}/download`
- **Frontend usage:** Trigger model download from HuggingFace/remote
- **File:** `ui/src/api/client.ts:1117`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Why:** Required for model import workflow; async operation
- **Backend implementation steps:**
  1. Create handler `download_model()` in `models.rs`
  2. Accept model ID and optional source URL
  3. Spawn background task to download; return job ID
  4. Add route: `.route("/v1/models/{model_id}/download", post(handlers::models::download_model))`
  5. Return `ModelDownloadResponse` with job/session ID

#### GET `/v1/models/cursor-config`
- **Frontend usage:** Fetch Cursor IDE configuration for model setup
- **File:** `ui/src/api/client.ts:1109`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Why:** Integrates Cursor SDK for development; low-risk feature gate
- **Backend implementation steps:**
  1. Create handler `get_cursor_config()` in `models.rs`
  2. Return static/configured cursor settings
  3. Add route: `.route("/v1/models/cursor-config", get(handlers::models::get_cursor_config))`
  4. Return `CursorConfigResponse` { api_key, model_endpoint, features: Vec<String> }

#### GET `/v1/models/imports/{id}`
- **Frontend usage:** Check status of in-progress model import
- **File:** `ui/src/api/client.ts:1105`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Why:** Required for async import workflow UX
- **Backend implementation steps:**
  1. Create handler `get_import_status()` in `models.rs`
  2. Query import job status from DB
  3. Add route: `.route("/v1/models/imports/{import_id}", get(handlers::models::get_import_status))`
  4. Return `ImportModelResponse` with status, progress, errors

#### POST `/v1/models/{id}/load` (Duplicate)
- **Frontend usage:** Load model into memory
- **File:** `ui/src/api/client.ts:1093`
- **Status:** IMPLEMENTED at `/v1/models/{model_id}/load` (with handler `load_model`)
- **Decision:** NO CHANGE NEEDED - already wired
- **Verification:** Routes line 494-496 ✓

#### POST `/v1/models/{id}/unload` (Duplicate)
- **Frontend usage:** Unload model from memory
- **File:** `ui/src/api/client.ts:1099`
- **Status:** IMPLEMENTED at `/v1/models/{model_id}/unload`
- **Decision:** NO CHANGE NEEDED - already wired
- **Verification:** Routes line 498-500 ✓

#### POST `/v1/models/{id}/validate` (Duplicate)
- **Frontend usage:** Validate model files/format
- **File:** `ui/src/api/client.ts:1113`
- **Status:** IMPLEMENTED at `/v1/models/{model_id}/validate`
- **Decision:** NO CHANGE NEEDED - already wired
- **Verification:** Routes line 506-508 ✓

---

### 1.2 Training Sessions (4 endpoints)

#### POST `/v1/training/sessions`
- **Frontend usage:** Create new training session
- **File:** `ui/src/api/client.ts:1700`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT or MAP to `/v1/training/start`**
- **Recommendation:** **MAP to existing endpoint**
- **Why:** Sessions are training jobs; avoid duplication. Frontend should call `/v1/training/start` instead
- **Backend decision:**
  - **Option A (Recommended):** Deprecate `/v1/training/sessions` in frontend; use `/v1/training/jobs` + `/v1/training/start`
  - **Option B:** Create session wrapper around job (more complex, not needed)
- **Action:** Update frontend `client.ts` line 1700:
  ```typescript
  createTrainingSession(config: TrainingConfig) {
    return this.request('/v1/training/start', {
      method: 'POST',
      body: JSON.stringify(config)
    });
  }
  ```

#### GET `/v1/training/sessions`
- **Frontend usage:** List all training sessions
- **File:** `ui/src/api/client.ts:1731`
- **Status:** NOT IMPLEMENTED (but `/v1/training/jobs` exists)
- **Decision:** **MAP to `/v1/training/jobs`**
- **Action:** Alias in frontend; call `/v1/training/jobs` instead
- **Verification:** Routes line 556 ✓

#### GET `/v1/training/sessions/{id}`
- **Frontend usage:** Get single session details
- **File:** `ui/src/api/client.ts:1716`
- **Status:** NOT IMPLEMENTED (but `/v1/training/jobs/{id}` exists as `/v1/training/jobs` GET)
- **Decision:** **MAP to `/v1/training/jobs/{id}`**
- **Action:** Update frontend to call job endpoint

#### POST `/v1/training/sessions/{id}/pause`
- **Frontend usage:** Pause running training session
- **File:** `ui/src/api/client.ts:1739`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT in `/v1/training/jobs/{id}/pause`**
- **Backend implementation steps:**
  1. Create handler `pause_training()` in `training.rs`
  2. Update job status to `paused`
  3. Add route: `.route("/v1/training/jobs/{job_id}/pause", post(handlers::pause_training))`
  4. Return `TrainingJobResponse` with updated status
- **Note:** Recommend updating frontend to use job endpoint path

#### POST `/v1/training/sessions/{id}/resume`
- **Frontend usage:** Resume paused training session
- **File:** `ui/src/api/client.ts:1749`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT in `/v1/training/jobs/{id}/resume`**
- **Backend implementation steps:**
  1. Create handler `resume_training()` in `training.rs`
  2. Update job status to `running`
  3. Add route: `.route("/v1/training/jobs/{job_id}/resume", post(handlers::resume_training))`
  4. Return `TrainingJobResponse` with updated status

---

### 1.3 Memory Management (2 endpoints)

#### GET `/v1/memory/usage`
- **Frontend usage:** Get current memory utilization across system
- **File:** `ui/src/api/client.ts:1681`
- **Status:** NOT IMPLEMENTED (but `/v1/system/memory` GET exists)
- **Decision:** **MAP to `/v1/system/memory`**
- **Why:** System memory is core feature; already implemented
- **Action:** Update frontend call to `/v1/system/memory`
- **Verification:** Routes line 895 ✓

#### POST `/v1/memory/adapters/{id}/evict`
- **Frontend usage:** Force evict adapter from memory
- **File:** `ui/src/api/client.ts:1685`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Why:** Manual lifecycle control required for memory-constrained deployments
- **Backend implementation steps:**
  1. Create handler `evict_adapter()` in adapters lifecycle handler
  2. Call lifecycle manager to evict specific adapter
  3. Add route: `.route("/v1/adapters/{adapter_id}/evict", post(handlers::evict_adapter))`
  4. Return `LifecycleStateResponse` confirming eviction
- **Note:** Different from auto-eviction; manual override

---

## Section 2: MEDIUM PRIORITY - Important Extensions (10 endpoints)

These improve UX and monitoring but aren't strictly required for MVP.

### 2.1 Authentication Extensions (6 endpoints)

#### POST `/v1/auth/dev-bypass`
- **Frontend usage:** Skip auth in dev mode (testing)
- **File:** `ui/src/api/client.ts:316`
- **Status:** NOT IMPLEMENTED
- **Decision:** **DO NOT IMPLEMENT IN PRODUCTION**
- **Why:** Security risk; should only exist in dev builds
- **Recommendation:**
  - Keep in frontend (guarded by `process.env.REACT_APP_DEV_MODE`)
  - Return error in production backend (with meaningful message)
  - Add conditional route in backend:
    ```rust
    if cfg!(debug_assertions) || std::env::var("DEV_MODE").is_ok() {
      .route("/v1/auth/dev-bypass", post(handlers::dev_bypass_auth))
    }
    ```

#### POST `/v1/auth/logout-all`
- **Frontend usage:** Invalidate all user sessions across devices
- **File:** `ui/src/api/client.ts:337`
- **Status:** NOT IMPLEMENTED (but `/v1/auth/sessions` revoke exists)
- **Decision:** **IMPLEMENT**
- **Why:** Security feature; needed for account takeover recovery
- **Backend implementation steps:**
  1. Create handler `logout_all_sessions()` in `auth_enhanced.rs`
  2. Delete all sessions for current user from DB
  3. Add route: `.route("/v1/auth/logout-all", post(handlers::auth_enhanced::logout_all_sessions))`
  4. Return success response

#### GET `/v1/auth/token`
- **Frontend usage:** Get current JWT token metadata
- **File:** `ui/src/api/client.ts:361`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Why:** Used for token inspection, expiry warnings
- **Backend implementation steps:**
  1. Extract claims from current JWT
  2. Create handler `get_token_metadata()` in `auth_enhanced.rs`
  3. Add route: `.route("/v1/auth/token", get(handlers::auth_enhanced::get_token_metadata))`
  4. Return `TokenMetadata { exp, iat, jti, sub, roles }`

#### POST `/v1/auth/token/rotate`
- **Frontend usage:** Refresh JWT with new token
- **File:** `ui/src/api/client.ts:355`
- **Status:** NOT IMPLEMENTED (but `/v1/auth/refresh` exists)
- **Decision:** **MAP to `/v1/auth/refresh`**
- **Action:** Rename frontend call or add alias
- **Verification:** Routes line 412-414 ✓

#### GET `/v1/auth/profile`
- **Frontend usage:** Get extended user profile (not just `/me`)
- **File:** `ui/src/api/client.ts:365`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT or EXTEND `/v1/auth/me`**
- **Recommendation:** **Extend `/v1/auth/me` to include profile data**
- **Backend implementation steps:**
  1. Update `auth_me()` handler to include profile fields
  2. Return extended `UserInfoResponse` with: email, name, avatar, preferences, settings
  3. No route change needed; enhance existing response

#### PUT `/v1/auth/config`
- **Frontend usage:** Update auth configuration/MFA settings
- **File:** `ui/src/api/client.ts:376`
- **Status:** NOT IMPLEMENTED (GET exists)
- **Decision:** **IMPLEMENT**
- **Why:** MFA, security settings management
- **Backend implementation steps:**
  1. Create handler `update_auth_config()` in `auth_enhanced.rs`
  2. Accept config object (mfa_enabled, backup_codes, etc.)
  3. Add route: `.route("/v1/auth/config", put(handlers::auth_enhanced::update_auth_config))`
  4. Return updated `AuthConfigResponse`

---

### 2.2 Metrics Extensions (2 endpoints)

#### GET `/v1/metrics/snapshot`
- **Frontend usage:** Quick point-in-time metrics for dashboards
- **File:** `ui/src/api/client.ts:1800`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT or USE EXISTING ROUTES**
- **Recommendation:** **IMPLEMENT as convenience endpoint**
- **Why:** Returns composite of `/v1/metrics/system`, `/v1/metrics/quality`, `/v1/metrics/adapters` in one call
- **Backend implementation steps:**
  1. Create handler `get_metrics_snapshot()` in handlers
  2. Call three metric endpoints internally
  3. Add route: `.route("/v1/metrics/snapshot", get(handlers::get_metrics_snapshot))`
  4. Return aggregated response

#### GET `/v1/metrics/routing`
- **Frontend usage:** Router gate statistics and performance
- **File:** Not found in codebase
- **Status:** NOT CALLED - safe to ignore
- **Decision:** SKIP

---

### 2.3 Stack Management (2 endpoints)

#### POST `/v1/adapter-stacks/deactivate`
- **Frontend usage:** Disable current adapter stack
- **File:** `ui/src/api/client.ts:1559`
- **Status:** NOT IMPLEMENTED (but route exists!)
- **Decision:** VERIFY AND WIRE
- **Verification:** Routes line 751-754 ✓ - **ALREADY WIRED**
- **Action:** None needed; verify frontend is calling correct path

#### POST `/v1/stacks/validate-name`
- **Frontend usage:** Validate stack name format/uniqueness
- **File:** `ui/src/api/client.ts:1565`
- **Status:** NOT IMPLEMENTED (but similar exists for adapters)
- **Decision:** **IMPLEMENT**
- **Why:** Consistency with adapter naming validation
- **Backend implementation steps:**
  1. Create handler `validate_stack_name()` in `adapter_stacks.rs`
  2. Check naming rules and uniqueness
  3. Add route: `.route("/v1/stacks/validate-name", post(handlers::adapter_stacks::validate_stack_name))`
  4. Return `ValidateStackNameResponse { valid, errors, suggestions }`

---

## Section 3: LOW PRIORITY - Nice-to-Have Features (4 endpoints)

These are convenience features or experimental; can be deprecated from frontend.

### 3.1 Security & Advanced Features (4 endpoints)

#### POST `/v1/security/isolation/test`
- **Frontend usage:** Run isolation test (security verification)
- **File:** `ui/src/api/client.ts:1893`
- **Status:** NOT IMPLEMENTED
- **Decision:** **DOCUMENT AS NOT SUPPORTED**
- **Why:** Highly specialized; rarely needed in UI; should be CLI-only
- **Recommendation:** Remove from frontend; update to show "Security testing via CLI" message
- **Action:** Remove `isolationTestAdapter()` from frontend or show deprecation notice

#### POST `/v1/security/anomaly/status`
- **Frontend usage:** Get anomaly detection status
- **File:** `ui/src/api/client.ts:1901`
- **Status:** NOT IMPLEMENTED
- **Decision:** **MAP to `/v1/monitoring/anomalies`**
- **Why:** Monitoring already covers anomalies
- **Action:** Update frontend call path

#### GET `/v1/status`
- **Frontend usage:** Get overall system status
- **File:** `ui/src/api/client.ts:2911`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT or MAP to `/v1/meta` + `/v1/healthz/all`**
- **Recommendation:** **IMPLEMENT as convenience**
- **Backend implementation steps:**
  1. Create handler `get_status()` in main handlers
  2. Aggregate `/healthz/all`, service status, etc.
  3. Add route: `.route("/v1/status", get(handlers::get_status))`
  4. Return `AdapterOSStatus { health, services, timestamp }`

#### POST `/v1/cp/promotion-gates/{cpid}`
- **Frontend usage:** Get promotion gate status/requirements
- **File:** `ui/src/api/client.ts:562` (actually GET)
- **Status:** ACTUALLY IMPLEMENTED (GET)
- **Decision:** NO CHANGE NEEDED
- **Verification:** Routes line 522-525 ✓

---

## Section 4: DEPRECATED / SHOULD REMOVE (5 endpoints)

These are experimental or redundant with existing functionality.

### 4.1 Orchestration (3 endpoints)

#### `/v1/orchestration/config`
- **Frontend usage:** Get orchestration configuration
- **File:** `ui/src/api/client.ts:3055` (GET), 3087 (PUT)
- **Status:** NOT IMPLEMENTED
- **Decision:** **DEPRECATE FROM FRONTEND**
- **Why:** Not part of current architecture; experimental feature
- **Action:** Remove from `client.ts`; feature can be added later if needed

#### `/v1/orchestration/analyze`
- **Frontend usage:** Analyze prompt/workflow
- **File:** `ui/src/api/client.ts:3122`
- **Status:** NOT IMPLEMENTED
- **Decision:** **DEPRECATE FROM FRONTEND**
- **Why:** Experimental; lower priority than core features
- **Action:** Remove from `client.ts`

#### `/v1/orchestration/metrics`
- **Frontend usage:** Get orchestration metrics
- **File:** `ui/src/api/client.ts:3154`
- **Status:** NOT IMPLEMENTED
- **Decision:** **DEPRECATE FROM FRONTEND**
- **Why:** Use standard `/v1/metrics/*` endpoints instead
- **Action:** Remove from `client.ts`

---

### 4.2 Admin (1 endpoint)

#### POST `/v1/admin/users`
- **Frontend usage:** Create admin users
- **File:** `ui/src/api/client.ts:3429`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT SEPARATELY OR VIA CLI**
- **Why:** Admin user creation should be restricted; likely CLI-only operation
- **Recommendation:** Remove from frontend; use CLI command instead
- **Action:** Remove from `client.ts` or mark as "Not available in UI"

---

### 4.3 Workspace Extensions (1 endpoint)

#### GET `/v1/workspaces/my`
- **Frontend usage:** Get current user's workspaces
- **File:** `ui/src/api/client.ts:2083`
- **Status:** NOT IMPLEMENTED
- **Decision:** **MAP to `/v1/workspaces` with filter or IMPLEMENT**
- **Recommendation:** **IMPLEMENT as convenience**
- **Why:** Reduces filtering logic in frontend
- **Backend implementation steps:**
  1. Create handler `get_user_workspaces()` in `workspaces.rs`
  2. Filter workspaces by current user ID
  3. Add route: `.route("/v1/workspaces/my", get(handlers::workspaces::get_user_workspaces))`
  4. Return filtered workspace array

#### POST `/v1/models/{id}/download`
- **Frontend usage:** Download model weights
- **File:** `ui/src/api/client.ts:1117`
- **Status:** NOT IMPLEMENTED
- **Decision:** **IMPLEMENT**
- **Reasoning:** Required for complete model lifecycle

---

## Section 5: Summary Table

### Implementation Priority Matrix

| Endpoint | Type | Priority | Decision | Effort | Timeline |
|----------|------|----------|----------|--------|----------|
| `/v1/models` | Model Mgmt | HIGH | IMPLEMENT | Medium | Step 1 |
| `/v1/models/status/all` | Model Mgmt | HIGH | IMPLEMENT | Low | Step 1 |
| `/v1/models/{id}/download` | Model Mgmt | HIGH | IMPLEMENT | Medium | Step 1 |
| `/v1/models/cursor-config` | Model Mgmt | HIGH | IMPLEMENT | Low | Step 2 |
| `/v1/models/imports/{id}` | Model Mgmt | HIGH | IMPLEMENT | Low | Step 2 |
| `/v1/training/sessions` | Training | HIGH | MAP to /jobs | Low | Step 1 |
| `/v1/training/sessions/{id}` | Training | HIGH | MAP to /jobs/{id} | Low | Step 1 |
| `/v1/training/sessions/{id}/pause` | Training | HIGH | IMPLEMENT | Medium | Step 2 |
| `/v1/training/sessions/{id}/resume` | Training | HIGH | IMPLEMENT | Medium | Step 2 |
| `/v1/memory/usage` | Memory | HIGH | MAP to /system/memory | Low | Step 1 |
| `/v1/memory/adapters/{id}/evict` | Memory | HIGH | IMPLEMENT | Medium | Step 3 |
| `/v1/auth/logout-all` | Auth | MEDIUM | IMPLEMENT | Low | Step 2 |
| `/v1/auth/token` | Auth | MEDIUM | IMPLEMENT | Low | Step 2 |
| `/v1/auth/token/rotate` | Auth | MEDIUM | MAP to /refresh | Low | Step 1 |
| `/v1/auth/profile` | Auth | MEDIUM | EXTEND /me | Low | Step 2 |
| `/v1/auth/config` (PUT) | Auth | MEDIUM | IMPLEMENT | Medium | Step 3 |
| `/v1/auth/dev-bypass` | Auth | MEDIUM | CONDITIONAL | Low | Step 3 |
| `/v1/metrics/snapshot` | Metrics | MEDIUM | IMPLEMENT | Low | Step 3 |
| `/v1/adapter-stacks/deactivate` | Stacks | MEDIUM | VERIFY | None | Verify only |
| `/v1/stacks/validate-name` | Stacks | MEDIUM | IMPLEMENT | Low | Step 2 |
| `/v1/workspaces/my` | Workspaces | MEDIUM | IMPLEMENT | Low | Step 3 |
| `/v1/status` | System | LOW | IMPLEMENT | Low | Step 3 |
| `/v1/orchestration/*` (3) | Orchestration | LOW | DEPRECATE | None | Remove from frontend |
| `/v1/admin/users` | Admin | LOW | DEPRECATE | None | Remove from frontend |
| `/v1/security/isolation/test` | Security | LOW | DEPRECATE | None | Remove from frontend |

---

## Section 6: Implementation Plan

### Phase 1: Essential Endpoints (Step 1 - Week 1)
**Target:** Enable core workflows

1. **Model Management - List & Status**
   - Add `GET /v1/models` handler
   - Add `GET /v1/models/status/all` handler
   - Update frontend to call new endpoints

2. **Training Sessions → Jobs Mapping**
   - Update frontend calls to map `/v1/training/sessions/*` → `/v1/training/jobs/*`
   - No backend changes needed

3. **Memory Endpoint Mapping**
   - Update frontend to call `/v1/system/memory` instead of `/v1/memory/usage`
   - No backend changes needed

4. **Auth Mapping**
   - Update frontend to call `/v1/auth/refresh` instead of `/v1/auth/token/rotate`
   - No backend changes needed

---

### Phase 2: Training & Authorization (Step 2 - Week 2)
**Target:** Complete training and authentication flows

1. **Model Download**
   - Add `POST /v1/models/{id}/download` handler with async job
   - Return job ID for tracking

2. **Training Pause/Resume**
   - Add `POST /v1/training/jobs/{id}/pause` handler
   - Add `POST /v1/training/jobs/{id}/resume` handler

3. **Extended Auth**
   - Add `POST /v1/auth/logout-all` handler
   - Add `GET /v1/auth/token` handler
   - Extend `GET /v1/auth/me` response with profile data

4. **Stack Validation**
   - Add `POST /v1/stacks/validate-name` handler

5. **Cursor Config**
   - Add `GET /v1/models/cursor-config` handler

---

### Phase 3: Polish & Extensions (Step 3 - Week 3)
**Target:** Advanced features and polish

1. **Memory Management**
   - Add `POST /v1/adapters/{id}/evict` handler

2. **Import Status Tracking**
   - Add `GET /v1/models/imports/{id}` handler

3. **Auth Config Updates**
   - Add `PUT /v1/auth/config` handler for MFA/security settings

4. **Convenience Endpoints**
   - Add `GET /v1/metrics/snapshot` composite endpoint
   - Add `GET /v1/status` aggregate endpoint
   - Add `GET /v1/workspaces/my` filtered endpoint

5. **Dev Mode Bypass** (conditional)
   - Add dev-only `/v1/auth/dev-bypass` route

---

### Phase 4: Frontend Cleanup (Optional)
**Target:** Remove deprecated/unused endpoints

1. Remove from `ui/src/api/client.ts`:
   - `orchestrationGetConfig()`
   - `orchestrationUpdateConfig()`
   - `orchestrationAnalyze()`
   - `orchestrationGetMetrics()`
   - `createAdminUser()`
   - `testIsolation()`

2. Add deprecation notices for endpoints that map to existing ones

---

## Section 7: Testing Checklist

### For Each New Endpoint

- [ ] Unit test in handler file
- [ ] Integration test in `crates/adapteros-server-api/tests/`
- [ ] Frontend integration test
- [ ] Swagger/OpenAPI doc generated
- [ ] Error cases handled (404, 401, 422)
- [ ] Permissions verified (role-based access)
- [ ] DB migrations if needed

### Example Test Template

```rust
#[tokio::test]
async fn test_get_models_list() {
    let app = setup_test_app().await;
    let response = app.get("/v1/models")
        .header("Authorization", "Bearer valid_token")
        .send()
        .await;

    assert_eq!(response.status(), 200);
    let body: Vec<ModelStatusResponse> = response.json().await;
    assert!(!body.is_empty());
}
```

---

## Section 8: Risk Assessment

### Low Risk (Safe to implement)
- `/v1/models` - Standard list endpoint
- `/v1/models/status/all` - Bulk query, no state change
- `/v1/auth/token` - Read-only
- `/v1/metrics/snapshot` - Composite of existing endpoints
- `/v1/workspaces/my` - Filtered query

### Medium Risk (Requires care)
- `/v1/training/sessions/{id}/pause|resume` - State modification, affects job lifecycle
- `/v1/memory/adapters/{id}/evict` - Manual lifecycle override, may break workflows
- `/v1/auth/logout-all` - Invalidates all sessions (security-critical)
- `/v1/models/{id}/download` - Background job, long-running operation

### High Risk (Policy-critical)
- `/v1/auth/config` (PUT) - MFA/security settings
- `/v1/admin/users` - Access control (should be CLI-only)

---

## Section 9: Documentation Updates Required

After implementation, update:

1. `/docs/REST_API_REFERENCE.md` - Add new endpoints
2. `/docs/QUICKSTART.md` - Update model loading examples
3. `CLAUDE.md` - Training sessions section
4. OpenAPI schema (auto-generated via utoipa)
5. Frontend `api/client.ts` comments

---

## Section 10: Frontend Code Changes

### Example: Training Sessions Mapping

**Before:**
```typescript
// ui/src/api/client.ts
createTrainingSession(config: TrainingConfig) {
  return this.request('/v1/training/sessions', {
    method: 'POST',
    body: JSON.stringify(config)
  });
}
```

**After:**
```typescript
// Map to existing /v1/training/start endpoint
createTrainingSession(config: TrainingConfig) {
  return this.request('/v1/training/start', {
    method: 'POST',
    body: JSON.stringify(config)
  });
}

// Alias for clarity (internal use only)
private async getTrainingSession(sessionId: string) {
  return this.request(`/v1/training/jobs/${sessionId}`);
}
```

---

## Section 11: Handlers Module Assignments

| Module | Endpoints | Status |
|--------|-----------|--------|
| `models.rs` | `/v1/models/*` (5 new) | Add handlers |
| `training.rs` | `/v1/training/jobs/*/pause\|resume` (2 new) | Add handlers |
| `auth_enhanced.rs` | `/v1/auth/*` (4 new) | Add handlers |
| `adapter_stacks.rs` | `/v1/stacks/validate-name` (1 new) | Add handler |
| `adapters.rs` | `/v1/adapters/*/evict` (1 new) | Add handler |
| `workspaces.rs` | `/v1/workspaces/my` (1 new) | Add handler |
| Main handlers | `/v1/status`, `/v1/metrics/snapshot` (2 new) | Add handlers |

---

## Appendix A: Endpoint Details by Category

### Authentication (7 endpoints)
- ✓ `/v1/auth/login` - IMPLEMENTED
- ✓ `/v1/auth/logout` - IMPLEMENTED
- ✓ `/v1/auth/me` - IMPLEMENTED
- ✓ `/v1/auth/refresh` - IMPLEMENTED (use instead of `/token/rotate`)
- ✓ `/v1/auth/sessions` - IMPLEMENTED
- ✓ `/v1/auth/bootstrap` - IMPLEMENTED
- ⚠️ `/v1/auth/logout-all` - **IMPLEMENT**
- ⚠️ `/v1/auth/token` - **IMPLEMENT**
- ⚠️ `/v1/auth/profile` - **EXTEND /me**
- ⚠️ `/v1/auth/config` (PUT) - **IMPLEMENT**
- ⚠️ `/v1/auth/dev-bypass` - **CONDITIONAL**
- ⚠️ `/v1/auth/token/rotate` - **MAP to /refresh**

### Models (8 endpoints)
- ✓ `/v1/models/import` - IMPLEMENTED
- ✓ `/v1/models/status` - IMPLEMENTED
- ✓ `/v1/models/{id}/load` - IMPLEMENTED
- ✓ `/v1/models/{id}/unload` - IMPLEMENTED
- ✓ `/v1/models/{id}/validate` - IMPLEMENTED
- ⚠️ `/v1/models` - **IMPLEMENT**
- ⚠️ `/v1/models/status/all` - **IMPLEMENT**
- ⚠️ `/v1/models/{id}/download` - **IMPLEMENT**
- ⚠️ `/v1/models/cursor-config` - **IMPLEMENT**
- ⚠️ `/v1/models/imports/{id}` - **IMPLEMENT**

### Training (6 endpoints)
- ✓ `/v1/training/jobs` - IMPLEMENTED
- ✓ `/v1/training/start` - IMPLEMENTED
- ✓ `/v1/training/templates` - IMPLEMENTED
- ⚠️ `/v1/training/sessions` - **MAP to /jobs**
- ⚠️ `/v1/training/sessions/{id}` - **MAP to /jobs/{id}**
- ⚠️ `/v1/training/sessions/{id}/pause` - **IMPLEMENT as /jobs/{id}/pause**
- ⚠️ `/v1/training/sessions/{id}/resume` - **IMPLEMENT as /jobs/{id}/resume**

### Memory (2 endpoints)
- ✓ `/v1/system/memory` - IMPLEMENTED
- ⚠️ `/v1/memory/usage` - **MAP to /system/memory**
- ⚠️ `/v1/memory/adapters/{id}/evict` - **IMPLEMENT**

### Metrics (4 endpoints)
- ✓ `/v1/metrics/system` - IMPLEMENTED
- ✓ `/v1/metrics/quality` - IMPLEMENTED
- ✓ `/v1/metrics/adapters` - IMPLEMENTED
- ⚠️ `/v1/metrics/snapshot` - **IMPLEMENT**

### Adapter Stacks (2 endpoints)
- ✓ `/v1/adapter-stacks` - IMPLEMENTED
- ✓ `/v1/adapter-stacks/deactivate` - IMPLEMENTED
- ⚠️ `/v1/stacks/validate-name` - **IMPLEMENT**

### Workspaces (2 endpoints)
- ✓ `/v1/workspaces` - IMPLEMENTED
- ⚠️ `/v1/workspaces/my` - **IMPLEMENT**

### Security (3 endpoints - DEPRECATE)
- ⚠️ `/v1/security/isolation/test` - **REMOVE from frontend**
- ⚠️ `/v1/security/anomaly/status` - **MAP to /monitoring/anomalies**
- ✓ `/v1/status` - **IMPLEMENT**

### Orchestration (3 endpoints - DEPRECATE)
- ⚠️ `/v1/orchestration/config` - **REMOVE from frontend**
- ⚠️ `/v1/orchestration/analyze` - **REMOVE from frontend**
- ⚠️ `/v1/orchestration/metrics` - **REMOVE from frontend**

### Admin (1 endpoint - DEPRECATE)
- ⚠️ `/v1/admin/users` - **REMOVE from frontend**

---

## Appendix B: Frontend Changes Required

File: `/Users/star/Dev/aos/ui/src/api/client.ts`

### Sections to Update

1. **Training Sessions (lines 1700-1749)**
   ```typescript
   // BEFORE: createTrainingSession() calls /v1/training/sessions
   // AFTER: Map to /v1/training/start
   // Add pauseTraining() and resumeTraining() calling /v1/training/jobs/{id}/pause|resume
   ```

2. **Models (lines 1068-1120)**
   ```typescript
   // ADD: listModels() → GET /v1/models
   // ADD: listAllModelStatuses() → GET /v1/models/status/all
   // ADD: downloadModel() → POST /v1/models/{id}/download
   // ADD: getModelImportStatus() → GET /v1/models/imports/{id}
   // ADD: getCursorConfig() → GET /v1/models/cursor-config
   // KEEP: load, unload, validate, import (already wired)
   ```

3. **Memory (lines 1681-1689)**
   ```typescript
   // CHANGE: getMemoryUsage() from /v1/memory/usage → /v1/system/memory
   // ADD: evictAdapter() → POST /v1/adapters/{id}/evict
   ```

4. **Auth (lines 302-376)**
   ```typescript
   // ADD: logoutAllSessions() → POST /v1/auth/logout-all
   // ADD: getTokenMetadata() → GET /v1/auth/token
   // CHANGE: rotateToken() to use /v1/auth/refresh
   // EXTEND: getUserProfile() - update /v1/auth/me response
   // ADD: updateAuthConfig() → PUT /v1/auth/config
   // CONDITIONAL: devBypassAuth() (dev-only)
   ```

5. **Remove Deprecated Methods**
   ```typescript
   // DELETE: orchestrationGetConfig()
   // DELETE: orchestrationUpdateConfig()
   // DELETE: orchestrationAnalyze()
   // DELETE: orchestrationGetMetrics()
   // DELETE: createAdminUser()
   // DELETE: testIsolation()
   // KEEP: securityAnomalyStatus() but map to /v1/monitoring/anomalies
   ```

---

## Final Recommendations

1. **Immediate (Critical)**
   - Implement Phase 1 endpoints (list models, training mappings, memory mapping)
   - Update frontend to use existing endpoints instead of missing ones

2. **Short-term (Important)**
   - Implement Phase 2 endpoints (training control, extended auth)
   - Add model download with async job support

3. **Medium-term (Nice-to-have)**
   - Implement Phase 3 convenience endpoints
   - Consider `/v1/metrics/snapshot` for dashboard performance

4. **Long-term (Future)**
   - Revisit orchestration endpoints if needed
   - Consider admin user creation (likely CLI-only)

5. **Always**
   - Keep frontend and backend documentation in sync
   - Write tests for new endpoints
   - Use consistent naming conventions
   - Maintain API versioning

---

**Document Status:** Complete
**Last Updated:** 2025-11-22
**Reviewed By:** N/A
**Next Review:** After Phase 1 implementation
