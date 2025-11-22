# PM Demo Readiness Report

**Generated:** 2025-11-21
**Status:** ⚠️ PARTIALLY READY - Critical blockers identified

---

## Executive Summary

The AdapterOS system has been thoroughly investigated using 25+ parallel agents. The UI and E2E tests are structurally complete, but **several critical integration gaps** will affect the PM demo.

### Quick Status

| Component | Status | Blocker Level |
|-----------|--------|---------------|
| Backend Compilation | ✅ Passes | None |
| Database Migrations | ✅ Complete (78 files, 0001-0080) | None |
| UI Build | ✅ Passes (zero TS errors) | None |
| Authentication | 🔴 BROKEN | **CRITICAL** |
| SSE Real-time Updates | ⚠️ Path mismatch | HIGH |
| Demo Scripts | 🔴 Missing CLI commands | **CRITICAL** |
| Streaming Inference | ⚠️ UI stub only | MEDIUM |
| Promotion Workflow | ⚠️ Mock data | MEDIUM |
| Multi-adapter Routing | 🔴 Only uses first adapter | **HIGH** |

---

## Critical Blockers (Must Fix Before Demo)

### 1. 🔴 Authentication is BROKEN

**Location:** `crates/adapteros-server-api/src/handlers/auth.rs:24-34`

**Problem:** The `/v1/auth/login` endpoint is a stub that **ALWAYS returns 401 Unauthorized**.

```rust
pub async fn auth_login(...) -> Result<Json<LoginResponse>, ...> {
    Err((StatusCode::UNAUTHORIZED,
         Json(ErrorResponse::new("Invalid credentials".to_string()))))
}
```

**Impact:** No user can log in. The entire UI is inaccessible.

**Solution:** A complete `login_handler()` exists in `auth_enhanced.rs:168` but is NOT wired to routes. Wire this handler instead of the stub.

**Additional Type Mismatches:**
- Backend expects `username` field; UI doesn't send it
- Backend returns `expires_in` (u64); UI expects `expires_at` (string)

---

### 2. 🔴 Demo Scripts Have Missing CLI Commands

**Location:** `demo_mvp.sh`, `scripts/demo_inference.sh`

**Missing Commands:**
| Command | Status |
|---------|--------|
| `adapteros-cli infer` | NOT IMPLEMENTED |
| `adapteros-cli telemetry-list` | NOT IMPLEMENTED |
| `adapteros-cli load-adapter` | Unknown/Different name |

**Impact:** Cannot run the main demo scripts without these commands.

**Solution:** Either implement the missing CLI commands or update scripts to use existing commands.

---

### 3. 🔴 Multi-Adapter Routing Broken

**Location:** Per CLAUDE.md update

**Problem:** Router only uses the first adapter in a stack. K-sparse routing is not functional.

**Impact:** Adapter stack demonstrations will not show correct routing behavior.

---

## High Priority Issues (Should Fix)

### 4. ⚠️ SSE Endpoint Path Mismatch

**Problem:** UI client expects `/stream/metrics` but backend provides `/v1/stream/metrics`

**Location:** `ui/src/api/client.ts:1903`

**Fix:** Add `/v1` prefix to metrics URL in UI client.

**Additional Missing SSE Endpoints:**
- `/v1/stream/notifications` - Backend handler NOT implemented
- `/v1/stream/messages/{workspaceId}` - Backend handler NOT implemented

---

### 5. ⚠️ Adapter Pinning Endpoints Missing

**UI Expects:**
- `POST /v1/adapters/{id}/pin`
- `POST /v1/adapters/{id}/unpin`

**Backend Status:** No handlers implemented (database schema exists via migration 0060)

**Impact:** Pin/unpin buttons in UI will fail with 404.

---

### 6. ⚠️ Training Artifacts Endpoint Missing

**UI Expects:** `GET /v1/training/jobs/{id}/artifacts`

**Backend Status:** No handler implemented

**Impact:** Cannot view training artifacts after job completion.

---

## Medium Priority Issues (Demo Workarounds Available)

### 7. Streaming Inference UI is Stub

**Location:** `ui/src/components/InferencePlayground.tsx`

**Problem:** UI has "Streaming" mode button but it calls the same non-streaming `infer()` method.

**Backend:** Full SSE streaming implementation exists at `/v1/chat/completions` on UDS socket.

**Workaround:** Use "Standard" mode for demo.

---

### 8. Promotion Workflow Uses Mock Data

**Location:** `ui/src/components/golden/PromotionWorkflow.tsx:462-502`

**Problem:** Component uses hardcoded mock stages instead of real API calls.

**Backend:** Full promotion API exists at `/v1/golden/{run_id}/promote`, `/v1/golden/{run_id}/promotion`, etc.

**Missing in UI Client:** 6 promotion API methods not implemented.

**Workaround:** Demo golden comparison (works) but skip promotion workflow.

---

### 9. Policy Validation is Mock

**Location:** `crates/adapteros-server-api/src/handlers/promotion.rs:927`

**Problem:** Policy gate validation always returns `{ policies_checked: 23, policies_passed: 23 }`.

**Impact:** Promotion will appear to always pass policy checks.

---

### 10. Backend Implementations are Placeholders

Per CLAUDE.md update:
- **CoreML:** Adapter loading is placeholder implementation
- **MLX:** Stub - compiles but not fully functional
- **Worker tests:** 29 test failures

---

## API Coverage Summary

**Total UI Client Methods:** 120+
**Properly Implemented:** 87 (72%)
**Missing Backend Handlers:** 34 (28%)

### Key Missing Endpoints

| Category | Missing Count | Examples |
|----------|---------------|----------|
| Auth Enhancement | 7 | Sessions, token rotation, config |
| Adapter Management | 8 | Pin, swap, category policies |
| Model Management | 6 | Load/unload, validation |
| Training | 1 | Artifacts |
| Streaming | 2 | Notifications, messages |

---

## PRD Compliance Status

| PRD | Feature | Status |
|-----|---------|--------|
| PRD-1 | Circuit Breaker | ✅ Implemented |
| PRD-2 | Hot-Swap Recovery | ⚠️ 62% complete |
| PRD-3 | Adapter Health State Machine | ✅ Implemented |
| PRD-4 | Memory Pressure Prediction | ✅ Implemented |
| PRD-5 | API Response Schema Validation | ✅ Implemented |

---

## E2E Test Coverage

**Total Test Files:** 315+ Rust, 16 UI
**MVP Flow Coverage:** ~75%

### Passing Test Areas
- Dataset to inference pipeline
- Adapter lifecycle management
- Training workflow
- Policy enforcement
- Determinism verification
- Auth integration (unit tests)

### Known Test Failures
- `adapteros-lora-worker`: 29 test errors

---

## Recommended Demo Path

Given the blockers, here's a safe demo flow:

### ✅ Safe to Demo
1. **Dashboard** - Shows KPIs, resources (uses polling fallback)
2. **Adapter List** - View, filter adapters
3. **Training Page** - Start training, monitor progress
4. **Inference Playground** - Standard mode only
5. **Golden Comparison** - Compare baselines
6. **Monitoring** - System metrics, alerts
7. **Audit Logs** - View activity

### ⚠️ Skip or Demo Carefully
- **Login** - Auth broken (use dev bypass if available)
- **Streaming Mode** - Not functional
- **Promotion Workflow** - Mock data only
- **Pin/Unpin** - Missing endpoints
- **Multi-adapter Stacks** - Routing broken

---

## Quick Fixes for Demo

### Fix 1: Wire Real Auth Handler
```diff
// In routes.rs, change:
- .route("/v1/auth/login", post(handlers::auth_login))
+ .route("/v1/auth/login", post(handlers::auth_enhanced::login_handler))
```

### Fix 2: SSE Path Correction
```diff
// In ui/src/api/client.ts:1903
- const eventSource = new EventSource(`/stream/metrics`);
+ const eventSource = new EventSource(`/v1/stream/metrics`);
```

### Fix 3: Add Promote Route
```rust
// In routes.rs ~line 485
.route(
    "/v1/adapters/:adapter_id/promote",
    post(handlers::promote_adapter_state),
)
```

---

## Files Modified for This Report

This investigation read but did not modify the following key files:
- `crates/adapteros-server-api/src/handlers/*.rs`
- `crates/adapteros-server-api/src/routes.rs`
- `ui/src/api/client.ts`
- `ui/src/components/**/*.tsx`
- `tests/**/*.rs`

---

## Next Steps

1. **Immediate:** Fix auth stub to use real handler
2. **Short-term:** Implement missing pin/unpin endpoints
3. **Medium-term:** Connect promotion UI to real API
4. **Long-term:** Fix multi-adapter routing

---

*Report generated by 25-agent parallel investigation of UI integration, E2E tests, and PRD compliance.*
