# PRD-04: Adapters + Stacks Lifecycle E2E Design

## Overview

Make adapter and stack management fully functional across UI and backend, with proper lifecycle state display, transitions, and in-flight guard integration.

## Decisions

| Decision | Choice |
|----------|--------|
| In-flight error handling | Toast notification with actionable message |
| Adapter lifecycle transitions | Full controls with confirmation dialogs |
| Stack activate confirmation | Confirm activate only; deactivate is instant |
| In-flight visibility | Proactive "In Use" badge via polling |

## Architecture

### Data Flow

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│   UI Components │────▶│   API Client     │────▶│  Backend Handlers   │
│                 │◀────│                  │◀────│                     │
└─────────────────┘     └──────────────────┘     └─────────────────────┘
        │                                                   │
        │ polls /v1/inference/in-flight                    │
        │◀──────────────────────────────────────────────────│
        │                                                   │
        ▼                                                   ▼
┌─────────────────┐                              ┌─────────────────────┐
│  In-Flight Set  │                              │ InferenceStateTracker│
│  (adapter_ids)  │                              │ is_adapter_in_flight│
└─────────────────┘                              └─────────────────────┘
```

### Existing Backend Support

The backend already provides:

1. **InferenceStateTracker** (`inference_state_tracker.rs:147`)
   - `is_adapter_in_flight(adapter_id)` - checks if adapter is in use
   - `adapters_in_flight()` - returns HashSet of all in-flight adapter IDs
   - AUDIT metrics: `in_flight_guard_blocks`, `in_flight_guard_allows`

2. **Lifecycle Guard** (`handlers/adapters/lifecycle.rs:46`)
   - `check_adapter_not_in_flight()` returns HTTP 409 with `ADAPTER_IN_FLIGHT` code
   - Error includes actionable message about waiting for drain

3. **Stack KV Operations** (`stacks_kv.rs`)
   - Full CRUD: create, get, update, delete
   - `activate_stack()`, `deactivate_stack()`
   - Lifecycle state management

## Implementation Plan

### 1. Add In-Flight Adapters Endpoint

**File:** `crates/adapteros-server-api/src/routes/adapters.rs`

Add endpoint to expose in-flight adapter IDs:

```rust
GET /v1/adapters/in-flight -> { adapter_ids: Vec<String> }
```

Returns the set from `InferenceStateTracker::adapters_in_flight()`.

### 2. Add API Client Method

**File:** `crates/adapteros-ui/src/api/client.rs`

```rust
pub async fn get_in_flight_adapters(&self) -> ApiResult<Vec<String>>
```

### 3. Create In-Flight Context Provider

**File:** `crates/adapteros-ui/src/contexts/in_flight.rs`

Polling context that maintains the set of in-flight adapter IDs:

```rust
#[component]
pub fn InFlightProvider(children: Children) -> impl IntoView {
    // Poll every 5 seconds
    // Provide: in_flight_adapters: Signal<HashSet<String>>
    // Provide: is_adapter_in_flight(id: &str) -> bool
}
```

### 4. Add "In Use" Badge to Adapter Components

**Files:**
- `crates/adapteros-ui/src/pages/adapters.rs` - list rows
- `crates/adapteros-ui/src/components/adapter_detail_panel.rs` - detail view

Show badge when `in_flight_adapters.contains(adapter.id)`:

```rust
{is_in_flight.then(|| view! {
    <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
})}
```

### 5. Add Lifecycle Transition Controls

**File:** `crates/adapteros-ui/src/components/adapter_lifecycle_controls.rs` (new)

Component showing valid transitions based on current state:

| Current State | Valid Transitions |
|---------------|-------------------|
| draft | → active |
| active | → deprecated |
| deprecated | → retired, → active (reactivate) |
| retired | (none) |

Each transition opens confirmation dialog requiring reason input.

**File:** `crates/adapteros-ui/src/components/lifecycle_transition_dialog.rs` (new)

Reusable confirmation dialog with:
- Current state → New state display
- Required reason text input
- Warning if adapter is in-flight
- Confirm/Cancel buttons

### 6. Add Lifecycle Transition API Methods

**File:** `crates/adapteros-ui/src/api/client.rs`

```rust
pub async fn transition_adapter_lifecycle(
    &self,
    adapter_id: &str,
    new_state: &str,
    reason: &str,
) -> ApiResult<LifecycleTransitionResponse>
```

### 7. Enhance Stack Activate with Confirmation

**File:** `crates/adapteros-ui/src/pages/stacks/list.rs`

Add confirmation dialog for activate action:

```rust
<ConfirmationDialog
    open=show_activate_confirm
    title="Activate Stack"
    description=format!(
        "Activating '{}' will route inference requests to this adapter stack. Continue?",
        stack_name
    )
    severity=ConfirmationSeverity::Warning
    confirm_text="Activate"
    on_confirm=on_confirm_activate
    on_cancel=on_cancel_activate
/>
```

### 8. Handle In-Flight Errors with Toast

**File:** `crates/adapteros-ui/src/api/error.rs`

Add specific handling for `ADAPTER_IN_FLIGHT` error code:

```rust
pub fn handle_api_error(error: &ApiError) -> Option<ToastMessage> {
    if error.code == Some("ADAPTER_IN_FLIGHT".into()) {
        return Some(ToastMessage {
            variant: ToastVariant::Warning,
            title: "Adapter In Use".into(),
            description: "This adapter is being used for inference. Try again when complete.".into(),
            duration: 5000,
        });
    }
    // ... other error handling
}
```

### 9. Update Stack Detail Page

**File:** `crates/adapteros-ui/src/pages/stacks/detail.rs`

- Show lifecycle state badge prominently
- Add activate/deactivate controls with confirmation
- Show "In Use" indicators for contained adapters
- Disable edit when any adapter is in-flight

## File Changes Summary

| File | Change |
|------|--------|
| `crates/adapteros-server-api/src/routes/adapters.rs` | Add `/v1/adapters/in-flight` endpoint |
| `crates/adapteros-server-api/src/handlers/adapters/mod.rs` | Add handler for in-flight endpoint |
| `crates/adapteros-ui/src/api/client.rs` | Add `get_in_flight_adapters()`, `transition_adapter_lifecycle()` |
| `crates/adapteros-ui/src/contexts/mod.rs` | Export new in_flight module |
| `crates/adapteros-ui/src/contexts/in_flight.rs` | New: InFlightProvider context |
| `crates/adapteros-ui/src/components/mod.rs` | Export new components |
| `crates/adapteros-ui/src/components/adapter_lifecycle_controls.rs` | New: Lifecycle transition buttons |
| `crates/adapteros-ui/src/components/lifecycle_transition_dialog.rs` | New: Transition confirmation dialog |
| `crates/adapteros-ui/src/pages/adapters.rs` | Add in-flight badge, lifecycle controls |
| `crates/adapteros-ui/src/components/adapter_detail_panel.rs` | Add in-flight badge, lifecycle controls |
| `crates/adapteros-ui/src/pages/stacks/list.rs` | Add activate confirmation dialog |
| `crates/adapteros-ui/src/pages/stacks/detail.rs` | Add lifecycle display, in-flight indicators |
| `crates/adapteros-ui/src/api/error.rs` | Add ADAPTER_IN_FLIGHT toast handling |

## Testing

### Manual Test Cases

1. **Adapter list shows lifecycle state** - Load adapters page, verify badge shows current state
2. **In-flight badge appears** - Start inference, verify "In Use" badge appears on adapter row
3. **Lifecycle transition works** - Open adapter detail, click transition button, enter reason, confirm
4. **In-flight blocks transition** - Try to deprecate adapter during inference, verify toast error
5. **Stack activate confirms** - Click activate on stack, verify confirmation dialog appears
6. **Stack deactivate is instant** - Click deactivate, verify no dialog, toast confirms success
7. **Error toast for in-flight** - Try to modify in-flight adapter, verify warning toast

### Automated Tests

- WASM unit tests for new components
- Integration test: lifecycle transition with reason audit trail
- Integration test: in-flight guard returns 409

## Non-Goals

- Adding new lifecycle states (per PRD)
- Real-time SSE for in-flight status (polling is sufficient)
- Batch lifecycle transitions
