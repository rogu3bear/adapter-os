# PRD-04: Adapters + Stacks Lifecycle Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make adapter and stack lifecycle management fully functional with in-flight guards, lifecycle transitions, and proper error handling.

**Architecture:** Add `/v1/adapters/in-flight` endpoint exposing InferenceStateTracker data. UI polls this to show "In Use" badges. Lifecycle transitions use existing backend handlers with new UI controls and confirmation dialogs.

**Tech Stack:** Rust (Axum backend, Leptos WASM frontend), async polling, confirmation dialogs

---

### Task 1: Add In-Flight Adapters Endpoint Handler

**Files:**
- Create: `crates/adapteros-server-api/src/handlers/adapters/in_flight.rs`
- Modify: `crates/adapteros-server-api/src/handlers/adapters/mod.rs`

**Step 1: Create handler file**

```rust
//! In-flight adapters handler
//!
//! Returns the set of adapter IDs currently being used for inference.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

/// Response for in-flight adapters endpoint
#[derive(Debug, Serialize, ToSchema)]
pub struct InFlightAdaptersResponse {
    /// Adapter IDs currently in use for inference
    pub adapter_ids: Vec<String>,
    /// Total count of in-flight inferences
    pub inference_count: usize,
}

/// GET /v1/adapters/in-flight
///
/// Returns adapter IDs currently being used for active inference requests.
#[utoipa::path(
    get,
    path = "/v1/adapters/in-flight",
    responses(
        (status = 200, description = "In-flight adapters", body = InFlightAdaptersResponse),
    ),
    tag = "adapters"
)]
pub async fn get_in_flight_adapters(
    State(state): State<Arc<AppState>>,
) -> Result<Json<InFlightAdaptersResponse>, StatusCode> {
    let (adapter_ids, inference_count) = if let Some(ref tracker) = state.inference_state_tracker {
        let ids: Vec<String> = tracker.adapters_in_flight().into_iter().collect();
        let count = tracker.active_count();
        (ids, count)
    } else {
        (vec![], 0)
    };

    Ok(Json(InFlightAdaptersResponse {
        adapter_ids,
        inference_count,
    }))
}
```

**Step 2: Add module export**

In `crates/adapteros-server-api/src/handlers/adapters/mod.rs`, add:

```rust
pub mod in_flight;
pub use in_flight::{get_in_flight_adapters, InFlightAdaptersResponse};
```

**Step 3: Verify compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-server-api`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-server-api/src/handlers/adapters/in_flight.rs
git add crates/adapteros-server-api/src/handlers/adapters/mod.rs
git commit -m "feat(api): add in-flight adapters handler"
```

---

### Task 2: Wire In-Flight Endpoint to Router

**Files:**
- Modify: `crates/adapteros-server-api/src/routes/adapters.rs`

**Step 1: Add route**

Find the adapters router function and add the in-flight route. Look for `pub fn adapters_routes()` or similar and add:

```rust
.route("/in-flight", get(handlers::adapters::get_in_flight_adapters))
```

**Step 2: Verify compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-server-api`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add crates/adapteros-server-api/src/routes/adapters.rs
git commit -m "feat(api): wire in-flight endpoint to router"
```

---

### Task 3: Add active_count Method to InferenceStateTracker

**Files:**
- Modify: `crates/adapteros-server-api/src/inference_state_tracker.rs`

**Step 1: Add method**

Add to `impl InferenceStateTracker`:

```rust
/// Get count of active (non-terminal) inferences
pub fn active_count(&self) -> usize {
    self.inferences
        .read()
        .values()
        .filter(|e| !e.state.is_terminal())
        .count()
}
```

**Step 2: Verify compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-server-api`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add crates/adapteros-server-api/src/inference_state_tracker.rs
git commit -m "feat(api): add active_count to InferenceStateTracker"
```

---

### Task 4: Add UI API Client Method for In-Flight

**Files:**
- Modify: `crates/adapteros-ui/src/api/client.rs`

**Step 1: Add response type and method**

Add near other adapter methods:

```rust
/// In-flight adapters response
#[derive(Debug, Clone, serde::Deserialize)]
pub struct InFlightAdaptersResponse {
    pub adapter_ids: Vec<String>,
    pub inference_count: usize,
}

impl ApiClient {
    // ... existing methods ...

    /// Get adapter IDs currently in use for inference
    pub async fn get_in_flight_adapters(&self) -> ApiResult<InFlightAdaptersResponse> {
        self.get("/v1/adapters/in-flight").await
    }
}
```

**Step 2: Re-export type**

In `crates/adapteros-ui/src/api/mod.rs`, add to the re-exports:

```rust
pub use client::InFlightAdaptersResponse;
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/api/client.rs
git add crates/adapteros-ui/src/api/mod.rs
git commit -m "feat(ui): add API client method for in-flight adapters"
```

---

### Task 5: Create InFlightProvider Context

**Files:**
- Create: `crates/adapteros-ui/src/contexts/in_flight.rs`
- Modify: `crates/adapteros-ui/src/contexts/mod.rs`

**Step 1: Create context file**

```rust
//! In-flight adapters context
//!
//! Polls the backend to track which adapters are currently being used
//! for inference. Components can check this to show "In Use" badges
//! and disable modification controls.

use crate::api::ApiClient;
use leptos::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

/// Polling interval for in-flight status (5 seconds)
const POLL_INTERVAL_MS: u32 = 5000;

/// Context providing in-flight adapter tracking
#[derive(Clone)]
pub struct InFlightContext {
    /// Set of adapter IDs currently in use
    pub adapter_ids: Signal<HashSet<String>>,
    /// Count of active inferences
    pub inference_count: Signal<usize>,
}

impl InFlightContext {
    /// Check if a specific adapter is in-flight
    pub fn is_in_flight(&self, adapter_id: &str) -> bool {
        self.adapter_ids.get().contains(adapter_id)
    }
}

/// Hook to access in-flight context
pub fn use_in_flight() -> InFlightContext {
    expect_context::<InFlightContext>()
}

/// Provider component that polls in-flight status
#[component]
pub fn InFlightProvider(children: Children) -> impl IntoView {
    let client = crate::hooks::use_api();

    let adapter_ids = RwSignal::new(HashSet::<String>::new());
    let inference_count = RwSignal::new(0usize);

    // Poll in-flight status
    let poll_client = Arc::clone(&client);
    Effect::new(move |_| {
        let client = Arc::clone(&poll_client);
        let handle = leptos::tachys::dom::set_interval(
            move || {
                let client = Arc::clone(&client);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(response) = client.get_in_flight_adapters().await {
                        adapter_ids.set(response.adapter_ids.into_iter().collect());
                        inference_count.set(response.inference_count);
                    }
                });
            },
            std::time::Duration::from_millis(POLL_INTERVAL_MS.into()),
        );

        // Initial fetch
        let client_init = Arc::clone(&client);
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(response) = client_init.get_in_flight_adapters().await {
                adapter_ids.set(response.adapter_ids.into_iter().collect());
                inference_count.set(response.inference_count);
            }
        });

        on_cleanup(move || {
            handle.clear();
        });
    });

    let context = InFlightContext {
        adapter_ids: adapter_ids.into(),
        inference_count: inference_count.into(),
    };

    provide_context(context);

    children()
}
```

**Step 2: Export from contexts module**

In `crates/adapteros-ui/src/contexts/mod.rs`, add:

```rust
pub mod in_flight;
pub use in_flight::{use_in_flight, InFlightContext, InFlightProvider};
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/contexts/in_flight.rs
git add crates/adapteros-ui/src/contexts/mod.rs
git commit -m "feat(ui): add InFlightProvider context with polling"
```

---

### Task 6: Add InFlightProvider to App Shell

**Files:**
- Modify: `crates/adapteros-ui/src/app.rs` or `crates/adapteros-ui/src/lib.rs`

**Step 1: Find app component and wrap with provider**

Locate the main App component. Wrap the router/shell with InFlightProvider:

```rust
use crate::contexts::InFlightProvider;

// Inside the App component view:
view! {
    <InFlightProvider>
        // existing shell/router content
    </InFlightProvider>
}
```

**Step 2: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add crates/adapteros-ui/src/app.rs  # or lib.rs
git commit -m "feat(ui): add InFlightProvider to app shell"
```

---

### Task 7: Add "In Use" Badge to Adapter List Rows

**Files:**
- Modify: `crates/adapteros-ui/src/pages/adapters.rs`

**Step 1: Import and use in-flight context**

At the top of the file, add:

```rust
use crate::contexts::use_in_flight;
```

**Step 2: Add badge to adapter row**

Find the adapter row rendering (likely in `AdaptersListInteractive` or similar). Add the in-flight badge next to lifecycle state:

```rust
// Inside the component
let in_flight = use_in_flight();

// In the row view, add after lifecycle badge:
{
    let adapter_id = adapter.id.clone();
    let is_in_flight = Signal::derive(move || in_flight.is_in_flight(&adapter_id));
    view! {
        {move || is_in_flight.get().then(|| view! {
            <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
        })}
    }
}
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/pages/adapters.rs
git commit -m "feat(ui): add 'In Use' badge to adapter list rows"
```

---

### Task 8: Add "In Use" Badge to Adapter Detail Panel

**Files:**
- Modify: `crates/adapteros-ui/src/components/adapter_detail_panel.rs`

**Step 1: Import and use in-flight context**

```rust
use crate::contexts::use_in_flight;
```

**Step 2: Add badge to detail header**

In the detail panel view, add near the adapter name/title:

```rust
// Inside the component
let in_flight = use_in_flight();

// Derive in-flight status for current adapter
let adapter_id_for_flight = adapter.clone();
let is_in_flight = Signal::derive(move || {
    adapter_id_for_flight.get()
        .map(|a| in_flight.is_in_flight(&a.id))
        .unwrap_or(false)
});

// In the view, add badge:
{move || is_in_flight.get().then(|| view! {
    <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
})}
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/components/adapter_detail_panel.rs
git commit -m "feat(ui): add 'In Use' badge to adapter detail panel"
```

---

### Task 9: Create Lifecycle Transition Dialog Component

**Files:**
- Create: `crates/adapteros-ui/src/components/lifecycle_transition_dialog.rs`
- Modify: `crates/adapteros-ui/src/components/mod.rs`

**Step 1: Create dialog component**

```rust
//! Lifecycle Transition Dialog
//!
//! Confirmation dialog for adapter lifecycle state transitions.
//! Requires a reason for audit trail.

use crate::components::{
    Button, ButtonVariant, Dialog, DialogContent, DialogFooter, DialogHeader, Input, Label,
};
use leptos::prelude::*;

/// Props for lifecycle transition dialog
#[derive(Clone)]
pub struct LifecycleTransitionDialogProps {
    pub adapter_name: String,
    pub current_state: String,
    pub new_state: String,
    pub is_in_flight: bool,
}

/// Lifecycle transition confirmation dialog
#[component]
pub fn LifecycleTransitionDialog(
    /// Whether dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Transition details
    #[prop(into)]
    props: Signal<Option<LifecycleTransitionDialogProps>>,
    /// Called with reason when confirmed
    on_confirm: Callback<String>,
    /// Loading state
    #[prop(into, default = Signal::derive(|| false))]
    loading: Signal<bool>,
) -> impl IntoView {
    let reason = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);

    let on_close = move || {
        open.set(false);
        reason.set(String::new());
        error.set(None);
    };

    let handle_confirm = move |_| {
        let r = reason.get();
        if r.trim().is_empty() {
            error.set(Some("Reason is required for audit trail".into()));
            return;
        }
        on_confirm.run(r);
    };

    view! {
        <Dialog open=open on_close=Callback::new(move |_| on_close())>
            {move || props.get().map(|p| {
                let in_flight_warning = p.is_in_flight;
                view! {
                    <DialogHeader>
                        <h2 class="text-lg font-semibold">"Transition Lifecycle State"</h2>
                    </DialogHeader>
                    <DialogContent>
                        <div class="space-y-4">
                            <p class="text-sm text-muted-foreground">
                                {format!("Transition '{}' from ", p.adapter_name)}
                                <span class="font-medium">{p.current_state.clone()}</span>
                                " to "
                                <span class="font-medium">{p.new_state.clone()}</span>
                                "?"
                            </p>

                            {in_flight_warning.then(|| view! {
                                <div class="p-3 bg-status-warning/10 border border-status-warning/20 rounded-md">
                                    <p class="text-sm text-status-warning">
                                        "Warning: This adapter is currently in use for inference. "
                                        "The transition may fail if requests are still active."
                                    </p>
                                </div>
                            })}

                            <div class="space-y-2">
                                <Label for_="reason">"Reason (required)"</Label>
                                <Input
                                    id="reason"
                                    placeholder="Enter reason for this transition..."
                                    value=reason
                                    on_input=move |e| reason.set(event_target_value(&e))
                                />
                                {move || error.get().map(|e| view! {
                                    <p class="text-sm text-destructive">{e}</p>
                                })}
                            </div>
                        </div>
                    </DialogContent>
                    <DialogFooter>
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(move |_| on_close())
                            disabled=loading
                        >
                            "Cancel"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(handle_confirm)
                            disabled=Signal::derive(move || loading.get() || reason.get().trim().is_empty())
                        >
                            {move || if loading.get() { "Transitioning..." } else { "Confirm" }}
                        </Button>
                    </DialogFooter>
                }
            })}
        </Dialog>
    }
}
```

**Step 2: Export component**

In `crates/adapteros-ui/src/components/mod.rs`, add:

```rust
mod lifecycle_transition_dialog;
pub use lifecycle_transition_dialog::{LifecycleTransitionDialog, LifecycleTransitionDialogProps};
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/components/lifecycle_transition_dialog.rs
git add crates/adapteros-ui/src/components/mod.rs
git commit -m "feat(ui): add LifecycleTransitionDialog component"
```

---

### Task 10: Create Adapter Lifecycle Controls Component

**Files:**
- Create: `crates/adapteros-ui/src/components/adapter_lifecycle_controls.rs`
- Modify: `crates/adapteros-ui/src/components/mod.rs`

**Step 1: Create controls component**

```rust
//! Adapter Lifecycle Controls
//!
//! Shows valid lifecycle transitions for an adapter based on current state.

use crate::api::ApiClient;
use crate::components::{Button, ButtonSize, ButtonVariant, LifecycleTransitionDialog, LifecycleTransitionDialogProps};
use crate::contexts::use_in_flight;
use leptos::prelude::*;
use std::sync::Arc;

/// Valid transitions from each state
fn valid_transitions(state: &str) -> Vec<(&'static str, &'static str)> {
    match state.to_lowercase().as_str() {
        "draft" => vec![("active", "Activate")],
        "active" => vec![("deprecated", "Deprecate")],
        "deprecated" => vec![
            ("active", "Reactivate"),
            ("retired", "Retire"),
        ],
        "retired" => vec![],
        _ => vec![],
    }
}

/// Lifecycle controls for adapter transitions
#[component]
pub fn AdapterLifecycleControls(
    /// Adapter ID
    adapter_id: String,
    /// Adapter name (for display)
    adapter_name: String,
    /// Current lifecycle state
    current_state: String,
    /// Callback when transition succeeds
    on_transition: Callback<()>,
) -> impl IntoView {
    let client = crate::hooks::use_api();
    let in_flight = use_in_flight();

    let show_dialog = RwSignal::new(false);
    let dialog_props = RwSignal::new(Option::<LifecycleTransitionDialogProps>::None);
    let loading = RwSignal::new(false);
    let pending_state = RwSignal::new(String::new());

    let adapter_id_for_flight = adapter_id.clone();
    let is_in_flight = Signal::derive(move || in_flight.is_in_flight(&adapter_id_for_flight));

    let transitions = valid_transitions(&current_state);

    let open_transition = {
        let adapter_name = adapter_name.clone();
        let current_state = current_state.clone();
        move |new_state: &str, _label: &str| {
            pending_state.set(new_state.to_string());
            dialog_props.set(Some(LifecycleTransitionDialogProps {
                adapter_name: adapter_name.clone(),
                current_state: current_state.clone(),
                new_state: new_state.to_string(),
                is_in_flight: is_in_flight.get_untracked(),
            }));
            show_dialog.set(true);
        }
    };

    let handle_confirm = {
        let client = Arc::clone(&client);
        let adapter_id = adapter_id.clone();
        Callback::new(move |reason: String| {
            let client = Arc::clone(&client);
            let adapter_id = adapter_id.clone();
            let new_state = pending_state.get();
            loading.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                match client.transition_adapter_lifecycle(&adapter_id, &new_state, &reason).await {
                    Ok(_) => {
                        show_dialog.set(false);
                        on_transition.run(());
                    }
                    Err(e) => {
                        // Error handling via toast is done in API client
                        tracing::error!("Lifecycle transition failed: {}", e);
                    }
                }
                loading.set(false);
            });
        })
    };

    if transitions.is_empty() {
        return view! {
            <span class="text-sm text-muted-foreground italic">"No transitions available"</span>
        }.into_any();
    }

    view! {
        <div class="flex items-center gap-2">
            {transitions.into_iter().map(|(new_state, label)| {
                let open = open_transition.clone();
                let ns = new_state.to_string();
                let lbl = label.to_string();
                view! {
                    <Button
                        size=ButtonSize::Small
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| open(&ns, &lbl))
                    >
                        {label}
                    </Button>
                }
            }).collect::<Vec<_>>()}

            <LifecycleTransitionDialog
                open=show_dialog
                props=dialog_props.into()
                on_confirm=handle_confirm
                loading=loading.into()
            />
        </div>
    }.into_any()
}
```

**Step 2: Export component**

In `crates/adapteros-ui/src/components/mod.rs`, add:

```rust
mod adapter_lifecycle_controls;
pub use adapter_lifecycle_controls::AdapterLifecycleControls;
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/components/adapter_lifecycle_controls.rs
git add crates/adapteros-ui/src/components/mod.rs
git commit -m "feat(ui): add AdapterLifecycleControls component"
```

---

### Task 11: Add Lifecycle Transition API Method

**Files:**
- Modify: `crates/adapteros-ui/src/api/client.rs`

**Step 1: Add request and response types**

```rust
/// Lifecycle transition request
#[derive(Debug, Clone, serde::Serialize)]
pub struct LifecycleTransitionRequest {
    pub reason: String,
}

/// Lifecycle transition response
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LifecycleTransitionResponse {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub reason: String,
    pub actor: String,
    pub timestamp: String,
}
```

**Step 2: Add API method**

```rust
impl ApiClient {
    /// Transition adapter lifecycle state
    pub async fn transition_adapter_lifecycle(
        &self,
        adapter_id: &str,
        new_state: &str,
        reason: &str,
    ) -> ApiResult<LifecycleTransitionResponse> {
        let request = LifecycleTransitionRequest {
            reason: reason.to_string(),
        };
        self.post(
            &format!("/v1/adapters/{}/lifecycle/{}", adapter_id, new_state),
            &request,
        ).await
    }
}
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/api/client.rs
git commit -m "feat(ui): add lifecycle transition API method"
```

---

### Task 12: Add Lifecycle Controls to Adapter Detail Panel

**Files:**
- Modify: `crates/adapteros-ui/src/components/adapter_detail_panel.rs`

**Step 1: Import component**

```rust
use crate::components::AdapterLifecycleControls;
```

**Step 2: Add controls to detail view**

In the detail panel, add after the lifecycle state badge:

```rust
// Add refetch callback prop if not present
// #[prop(optional)]
// on_refetch: Option<Callback<()>>,

// In the view, add lifecycle controls section:
<div class="space-y-2">
    <h3 class="text-sm font-medium">"Lifecycle"</h3>
    <div class="flex items-center gap-2">
        <Badge variant=lifecycle_badge_variant(&adapter.lifecycle_state)>
            {adapter.lifecycle_state.clone()}
        </Badge>
        <AdapterLifecycleControls
            adapter_id=adapter.id.clone()
            adapter_name=adapter.name.clone()
            current_state=adapter.lifecycle_state.clone()
            on_transition=Callback::new(move |_| {
                // Trigger refetch if callback provided
            })
        />
    </div>
</div>
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/components/adapter_detail_panel.rs
git commit -m "feat(ui): add lifecycle controls to adapter detail panel"
```

---

### Task 13: Add Stack Activate Confirmation Dialog

**Files:**
- Modify: `crates/adapteros-ui/src/pages/stacks/list.rs`

**Step 1: Add confirmation state**

Add state for activate confirmation:

```rust
let show_activate_confirm = RwSignal::new(false);
let pending_activate_id = RwSignal::new(Option::<String>::None);
let pending_activate_name = RwSignal::new(String::new());
let activating = RwSignal::new(false);
```

**Step 2: Add confirmation dialog**

Add after the delete confirmation dialog:

```rust
<ConfirmationDialog
    open=show_activate_confirm
    title="Activate Stack"
    description=Signal::derive(move || {
        format!(
            "Activating '{}' will route inference requests to this adapter stack. This may affect running workloads. Continue?",
            pending_activate_name.get()
        )
    })
    severity=ConfirmationSeverity::Warning
    confirm_text="Activate"
    on_confirm=Callback::new(move |_| {
        if let Some(id) = pending_activate_id.get() {
            activating.set(true);
            let client = Arc::clone(&client);
            wasm_bindgen_futures::spawn_local(async move {
                match client.activate_stack(&id).await {
                    Ok(_) => {
                        show_activate_confirm.set(false);
                        pending_activate_id.set(None);
                        pending_activate_name.set(String::new());
                        refetch_trigger.update(|n| *n = n.wrapping_add(1));
                    }
                    Err(e) => {
                        tracing::error!("Failed to activate stack: {}", e);
                    }
                }
                activating.set(false);
            });
        }
    })
    on_cancel=Callback::new(move |_| {
        show_activate_confirm.set(false);
        pending_activate_id.set(None);
        pending_activate_name.set(String::new());
    })
    loading=Signal::derive(move || activating.get())
/>
```

**Step 3: Update activate button to open dialog**

Replace direct activate call with dialog open:

```rust
// Instead of calling client.activate_stack directly:
on:click=move |_| {
    pending_activate_id.set(Some(stack_id.clone()));
    pending_activate_name.set(stack_name.clone());
    show_activate_confirm.set(true);
}
```

**Step 4: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add crates/adapteros-ui/src/pages/stacks/list.rs
git commit -m "feat(ui): add activate confirmation dialog for stacks"
```

---

### Task 14: Add ADAPTER_IN_FLIGHT Toast Handling

**Files:**
- Modify: `crates/adapteros-ui/src/api/error.rs`

**Step 1: Add specific error handling**

Find the error handling/display logic and add special case for ADAPTER_IN_FLIGHT:

```rust
impl ApiError {
    /// Check if this is an in-flight adapter error
    pub fn is_adapter_in_flight(&self) -> bool {
        self.code.as_deref() == Some("ADAPTER_IN_FLIGHT")
    }

    /// Get user-friendly message for display
    pub fn user_message(&self) -> String {
        if self.is_adapter_in_flight() {
            "This adapter is currently in use for inference. Please wait for active requests to complete before making changes.".to_string()
        } else {
            self.message.clone()
        }
    }
}
```

**Step 2: Update error reporter if needed**

In `crates/adapteros-ui/src/api/error_reporter.rs`, ensure toast uses user_message:

```rust
pub fn report_error_with_toast(error: &ApiError) {
    let message = error.user_message();
    let variant = if error.is_adapter_in_flight() {
        ToastVariant::Warning
    } else {
        ToastVariant::Error
    };
    // Show toast with message and variant
}
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/api/error.rs
git add crates/adapteros-ui/src/api/error_reporter.rs
git commit -m "feat(ui): add ADAPTER_IN_FLIGHT toast handling"
```

---

### Task 15: Add In-Flight Badges to Stack Detail

**Files:**
- Modify: `crates/adapteros-ui/src/pages/stacks/detail.rs`

**Step 1: Import context**

```rust
use crate::contexts::use_in_flight;
```

**Step 2: Add in-flight indicators to adapter list**

In the stack detail's adapter list section:

```rust
let in_flight = use_in_flight();

// For each adapter in the stack:
{
    let adapter_id = adapter.id.clone();
    let is_in_flight = Signal::derive(move || in_flight.is_in_flight(&adapter_id));
    view! {
        <div class="flex items-center gap-2">
            <span>{adapter.name.clone()}</span>
            {move || is_in_flight.get().then(|| view! {
                <Badge variant=BadgeVariant::Warning>"In Use"</Badge>
            })}
        </div>
    }
}
```

**Step 3: Verify WASM compilation**

Run: `CARGO_INCREMENTAL=0 cargo check -p adapteros-ui --target wasm32-unknown-unknown`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add crates/adapteros-ui/src/pages/stacks/detail.rs
git commit -m "feat(ui): add in-flight indicators to stack detail"
```

---

### Task 16: Run Full Test Suite

**Step 1: Run backend tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p adapteros-server-api`
Expected: All tests pass

**Step 2: Run UI tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p adapteros-ui --lib`
Expected: All tests pass

**Step 3: Full WASM build**

Run: `CARGO_INCREMENTAL=0 cargo build -p adapteros-ui --target wasm32-unknown-unknown --release`
Expected: Builds successfully

**Step 4: Commit any fixes**

If tests revealed issues, fix and commit:

```bash
git add -A
git commit -m "fix: address test failures from PRD-04 implementation"
```

---

### Task 17: Final Cleanup and Summary Commit

**Step 1: Run format check**

Run: `cargo fmt --all --check`
If needed: `cargo fmt --all`

**Step 2: Run clippy**

Run: `CARGO_INCREMENTAL=0 cargo clippy -p adapteros-ui -p adapteros-server-api -- -D warnings`
Fix any warnings.

**Step 3: Final commit**

```bash
git add -A
git commit -m "chore: format and lint PRD-04 implementation"
```

---

## Summary

This plan implements PRD-04 with:

1. **Backend**: New `/v1/adapters/in-flight` endpoint exposing InferenceStateTracker data
2. **Context**: InFlightProvider polling in-flight status every 5 seconds
3. **Components**: LifecycleTransitionDialog, AdapterLifecycleControls
4. **UI Updates**: "In Use" badges on adapter list/detail, lifecycle controls, stack activate confirmation
5. **Error Handling**: ADAPTER_IN_FLIGHT toast with user-friendly message

Total: 17 tasks, ~2-3 hours estimated implementation time
