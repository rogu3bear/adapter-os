# Backend-to-Frontend Readiness Map

This document maps UI operations to backend readiness flows so each button matches a "ready" or loaded state rather than causing errors.

## Backend Readiness Sources

### 1. System Status (`/v1/system/status`)

| Field | Meaning | Used By |
|-------|---------|---------|
| `readiness.overall` | `Ready` \| `NotReady` \| `Unknown` — DB, migrations, workers, models | Dashboard, System, Welcome, Status Center, Models, Workers, Documents |
| `inference_ready` | `True` \| `False` \| `Unknown` — inference can run | Chat, ChatDock, InferenceBanner, LogicalRail, Topbar, Stacks |
| `inference_blockers` | Why inference is blocked | InferenceBanner, guidance |

**Inference is ready when all of:**
- Boot complete (not booting, not failed)
- DB available
- At least one worker active
- At least one model loaded and ready
- No ActiveModelMismatch
- No TelemetryDegraded

### 2. Training Backend Readiness (`/v1/training/backend/readiness`)

- CoreML/Metal/MLX capability
- Base model status
- Used by: Training page `BackendReadinessPanel` (informational; does not block job creation)

### 3. Health Endpoints

- `/healthz` — liveness
- `/readyz` — readiness
- `/system/ready` — system gate status
- Used by: System page, Monitoring, SystemTray

---

## Page-by-Page Operations and Readiness Gates

### Chat & Inference

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Chat | Send message | `POST /v1/infer/stream` | Queues when `!inference_ready`; sends when ready | ✅ OK |
| Chat | New conversation | `POST /v1/chat/sessions` | Uses `inference_ready` | ✅ OK |
| Chat | Attach data | Various | `attach_busy` | ✅ OK |
| ChatDock | Send | Same as Chat | Queues/sends based on `inference_ready` | ✅ OK |

**Chat flow:** Messages are queued when inference is not ready; when it becomes ready, queued messages are processed. Send button is enabled when there is text; no error from clicking before ready.

---

### Models

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Models | Load | `POST /v1/models/{id}/load` | `system_not_ready` (`readiness.overall`), `can_manage_models`, `lifecycle_in_progress`, `request_in_flight` | ✅ OK |
| Models | Unload | `POST /v1/models/{id}/unload` | Same | ✅ OK |
| Models | Validate | `GET /v1/models/{id}/validate` | Same | ✅ OK |
| Models | Seed/Import | `POST /v1/models/import` | `system_not_ready`, `loading`, `is_valid` | ✅ OK |

**Implementation:** Load/Unload/Validate/Import are disabled when `readiness.overall != Ready`. "Backend not ready" tooltip shown when disabled.

---

### Workers

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Workers | Spawn | `POST /v1/workers/spawn` | `system_not_ready` (`readiness.overall`), `action_loading`, `can_submit` (nodes/plans loaded) | ✅ OK |
| Workers | Drain/Stop/Restart/Remove | Various | `action_loading`, data loaded | ✅ OK |

**Implementation:** Spawn (quick and advanced) is disabled when `readiness.overall != Ready`.

---

### Training

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Training | Create job | `POST /v1/training/jobs` | `train_disabled` (dataset, base_model, is_training) | ⚠️ No training backend readiness |
| Training | Cancel job | `POST /v1/training/jobs/{id}/cancel` | `is_cancelling` | ✅ OK |
| Training | Retry job | `POST /v1/training/jobs/{id}/retry` | Data loaded | ✅ OK |

**Note:** `BackendReadinessPanel` shows training backend status but does not block Create. Backend uses automatic backend selection if readiness is unknown. This is intentional (graceful degradation).

---

### Stacks

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Stacks | Activate | `POST /v1/adapter-stacks/{id}/activate` | `inference_not_ready` (`inference_ready`), `activating`, data loaded | ✅ OK |
| Stacks | Deactivate | `POST /v1/adapter-stacks/deactivate` | Data loaded | ✅ OK |
| Stacks | Create | `POST /v1/adapter-stacks` | `creating`, adapters loaded | ✅ OK |

**Implementation:** Activate is disabled when `inference_ready != True` (requires workers + model loaded).

---

### Adapters

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Adapters | Promote/Demote | `POST /v1/adapters/{id}/lifecycle/*` | `in_flight` warning, `loading` | ✅ OK |
| Adapters | Start Conversation | Navigation | — | ✅ OK |

---

### Routing

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Routing Rules | Add rule | `POST /v1/routing/rules` | `adapters_ready` (adapters Loaded), `can_submit` | ✅ OK |
| Routing Rules | Target adapter select | — | `disabled=!adapters_ready` | ✅ OK |
| Routing Decisions | Prompt | `GET /v1/routing/decisions` (or similar) | `prompt` non-empty, `loading` | ⚠️ May need inference |

**Note:** `adapters_ready` = `LoadingState::Loaded` for adapters list. Routing rules do not require inference; they configure routing. Decisions tab may call inference — verify.

---

### Welcome (Setup Wizard)

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Welcome | Run migrations | `POST /v1/setup/migrate` | Status loaded, `seeding` | ✅ OK |
| Welcome | Discover models | `GET /v1/setup/models/discover` | `discovering` | ✅ OK |
| Welcome | Seed models | `POST /v1/setup/models/seed` | `seeding`, selected paths | ✅ OK |
| Welcome | Spawn worker | `POST /v1/workers/spawn` | Plans/nodes loaded | ✅ OK |

**Note:** Welcome shows checklist based on `status`; steps are ordered. Migrations must run before seed/spawn.

---

### Datasets & Documents

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Datasets | Train (button) | Navigation to `/training?dataset_id=...` | `is_trainable` (status ready, validation ok, trust ok) | ✅ OK |
| Datasets | Delete | `DELETE /v1/datasets/{id}` | Data loaded, delete dialog | ✅ OK |
| Datasets | Upload | Various | `uploading` | ✅ OK |
| Documents | Upload | `POST /v1/documents/upload` | `uploading` | ✅ OK |
| Documents | Reprocess | `POST /v1/documents/{id}/reprocess` | `system_not_ready` (`readiness.overall`), data loaded | ✅ OK |

**Implementation:** Reprocess/Retry (list and detail) disabled when `readiness.overall != Ready`.

---

### Policies, Collections, Repositories, Admin, Audit, Errors

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Policies | Apply | `POST /v1/policies/apply` | `applying`, content | ✅ OK |
| Collections | Create, Add/Remove doc | Various | `creating`, `adding` | ✅ OK |
| Repositories | Register, Scan | Various | `submitting` | ✅ OK |
| Admin | Invite, API keys, Org actions | Various | `loading` | ✅ OK |
| Audit | Export | Client-side | Data loaded | ✅ OK |
| Errors | Create/Delete alert rule | Various | `submitting`, `toggling`, `deleting` | ✅ OK |

---

### System & Monitoring

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| System | Shutdown, Maintenance, Restart | Various | `shutdown_loading`, etc. | ✅ OK |
| System | Service Start/Stop/Drain | `POST /v1/services/*` | `action_loading` | ✅ OK |
| Monitoring | Refresh | Health endpoints | — | ✅ OK |

---

### Flight Recorder & Diff

| Page | Operation | Backend API | Current Gate | Gap? |
|------|-----------|-------------|--------------|------|
| Flight Recorder | Export, Execute replay | Diagnostics APIs | Data loaded, `executing` | ✅ OK |
| Diff | Compare | `POST /v1/diagnostics/diff` | `diff_loading`, run IDs | ✅ OK |

---

## Summary: Gaps and Recommendations

### Implemented (Readiness gates in place)

1. **Models — Load/Unload/Validate/Import** — Gated on `readiness.overall` via `system_not_ready`.
2. **Workers — Spawn** — Gated on `readiness.overall` via `system_not_ready`.
3. **Stacks — Activate** — Gated on `inference_ready` via `inference_not_ready`.
4. **Documents — Reprocess/Retry** — Gated on `readiness.overall` via `system_not_ready`.

### Remaining (Graceful degradation acceptable)

5. **Training — Create job**
   - **Current:** BackendReadinessPanel is informational; job creation proceeds. Backend auto-selects backend.
   - **Recommendation:** Keep as-is; optional: show warning when training backend readiness is Error.

### Low Priority (Navigation or low-impact)

6. **Routing Decisions**
   - Verify if this tab calls inference. If yes, gate prompt submission on `inference_ready`.

7. **Refresh buttons**
   - Most Refresh buttons have no readiness gate. They will show LoadingState::Error when backend is down. This is acceptable — user sees error and can retry.

---

## Implementation Pattern

For pages that need readiness gating:

```rust
// In page component
let (system_status, _) = use_system_status();

let is_ready = Signal::derive(move || {
    matches!(
        system_status.get(),
        LoadingState::Loaded(ref s) if matches!(s.readiness.overall, ApiStatusIndicator::Ready)
    )
});

let inference_ready = Signal::derive(move || {
    matches!(
        system_status.get(),
        LoadingState::Loaded(ref s) if matches!(s.inference_ready, InferenceReadyState::True)
    )
});

// On button
disabled=Signal::derive(move || !is_ready.get() || action_loading.get())
// Or for inference-dependent:
disabled=Signal::derive(move || !inference_ready.get() || action_loading.get())
```

For tooltips when disabled due to readiness:

```rust
title=move || {
    if !is_ready.get() {
        "Backend not ready. Check System page for status.".to_string()
    } else {
        String::new()
    }
}
```

---

## Implementation Reference (gates implemented)

| File | Implementation |
|------|-----------------|
| `crates/adapteros-ui/src/pages/models.rs` | `use_system_status`, `system_not_ready` gates Load/Unload/Validate/Import |
| `crates/adapteros-ui/src/pages/workers/mod.rs` | `use_system_status`, `system_not_ready` gates Spawn (quick + advanced) |
| `crates/adapteros-ui/src/pages/stacks/detail.rs` | `use_system_status`, `inference_not_ready` gates Activate |
| `crates/adapteros-ui/src/pages/documents.rs` | `use_system_status`, `system_not_ready` gates Reprocess/Retry (list + detail) |

---

## Backend API Route Reference (Quick)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/system/status` | System status, readiness, inference_ready |
| `GET /healthz` | Liveness |
| `GET /readyz` | Readiness |
| `GET /system/ready` | System gate |
| `GET /v1/training/backend/readiness` | Training backend (CoreML/Metal/MLX) |
| `POST /v1/models/{id}/load` | Load model (needs worker) |
| `POST /v1/workers/spawn` | Spawn worker (needs CP ready) |
| `POST /v1/adapter-stacks/{id}/activate` | Activate stack (needs inference) |
| `POST /v1/documents/{id}/reprocess` | Reprocess document (needs worker) |
