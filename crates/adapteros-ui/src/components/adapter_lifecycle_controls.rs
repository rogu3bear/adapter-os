//! Adapter Lifecycle Controls
//!
//! Shows valid lifecycle transitions for an adapter based on current state.
//!
//! # Lifecycle States and Transitions
//!
//! | Current State | Valid Transitions |
//! |---------------|-------------------|
//! | draft | active |
//! | active | deprecated |
//! | deprecated | active (reactivate), retired |
//! | retired | (none) |
//!
//! # Usage
//!
//! ```rust,ignore
//! <AdapterLifecycleControls
//!     adapter_id="adapter-123".to_string()
//!     adapter_name="my-adapter".to_string()
//!     current_state="active".to_string()
//!     on_transition=Callback::new(move |()| {
//!         // Refresh adapter data after transition
//!     })
//! />
//! ```

use crate::api::ApiClient;
use crate::components::{
    Button, ButtonSize, ButtonVariant, LifecycleTransitionDialog, LifecycleTransitionInfo,
};
use crate::contexts::use_in_flight;
use crate::signals::notifications::use_notifications;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::Arc;

/// Valid transitions from each state
///
/// Returns a list of (target_state, button_label) tuples for the given current state.
fn valid_transitions(state: &str) -> Vec<(&'static str, &'static str)> {
    match state.to_lowercase().as_str() {
        "draft" => vec![("active", "Activate")],
        "active" => vec![("deprecated", "Deprecate")],
        "deprecated" => vec![("active", "Reactivate"), ("retired", "Retire")],
        "retired" => vec![],
        _ => vec![],
    }
}

/// Get button variant based on the target state
fn button_variant_for_transition(target_state: &str) -> ButtonVariant {
    match target_state {
        "active" => ButtonVariant::Primary,
        "deprecated" => ButtonVariant::Secondary,
        "retired" => ButtonVariant::Destructive,
        _ => ButtonVariant::Secondary,
    }
}

/// Lifecycle controls for adapter transitions
///
/// Displays buttons for valid lifecycle transitions based on the adapter's current state.
/// Opens a confirmation dialog before making changes, which requires a reason for audit trail.
#[component]
pub fn AdapterLifecycleControls(
    /// The adapter's unique identifier
    #[prop(into)]
    adapter_id: String,
    /// The adapter's display name
    #[prop(into)]
    adapter_name: String,
    /// Current lifecycle state (draft, active, deprecated, retired)
    #[prop(into)]
    current_state: String,
    /// Callback invoked after successful transition
    on_transition: Callback<()>,
) -> impl IntoView {
    // Dialog state
    let show_dialog = RwSignal::new(false);
    let selected_transition = RwSignal::new(None::<LifecycleTransitionInfo>);
    let loading = RwSignal::new(false);

    // Get in-flight context to check if adapter is in use
    let in_flight = use_in_flight();
    let notifications = use_notifications();

    // Create API client
    let client = Arc::new(ApiClient::new());

    // Get valid transitions for current state
    let transitions = valid_transitions(&current_state);

    // Clone values needed in closures
    let adapter_id_clone = adapter_id.clone();
    let adapter_name_clone = adapter_name.clone();
    let current_state_clone = current_state.clone();

    // Check if adapter is in-flight
    let is_in_flight = {
        let adapter_id = adapter_id.clone();
        move || in_flight.is_in_flight(&adapter_id)
    };

    // Handle button click - open dialog with transition info
    let open_transition_dialog = {
        let adapter_name = adapter_name_clone.clone();
        let current_state = current_state_clone.clone();
        move |target_state: &'static str| {
            let adapter_name = adapter_name.clone();
            let current_state = current_state.clone();
            let is_flying = is_in_flight();
            move || {
                selected_transition.set(Some(LifecycleTransitionInfo {
                    adapter_name: adapter_name.clone(),
                    current_state: current_state.clone(),
                    new_state: target_state.to_string(),
                    is_in_flight: is_flying,
                }));
                show_dialog.set(true);
            }
        }
    };

    // Handle confirm from dialog
    let handle_confirm = {
        let client = Arc::clone(&client);
        let adapter_id = adapter_id_clone.clone();
        let notifications = notifications.clone();
        Callback::new(move |reason: String| {
            let client = Arc::clone(&client);
            let adapter_id = adapter_id.clone();
            let notifications = notifications.clone();

            // Get target state from selected transition
            let target_state = selected_transition
                .get()
                .map(|t| t.new_state.clone())
                .unwrap_or_default();

            loading.set(true);

            spawn_local(async move {
                // Call the API to transition adapter lifecycle
                // Note: transition_adapter_lifecycle will be added in Task 11
                match client
                    .transition_adapter_lifecycle(&adapter_id, &target_state, &reason)
                    .await
                {
                    Ok(_) => {
                        let detail_href = format!("/adapters/{}", adapter_id);
                        notifications.success_with_action(
                            "Lifecycle Updated",
                            &format!("Adapter transitioned to {}", target_state),
                            "View Adapter",
                            &detail_href,
                        );
                        show_dialog.set(false);
                        loading.set(false);
                        selected_transition.set(None);
                        on_transition.run(());
                    }
                    Err(err) => {
                        notifications.error("Transition Failed", &err.to_string());
                        loading.set(false);
                    }
                }
            });
        })
    };

    // If no valid transitions, show a message or nothing
    if transitions.is_empty() {
        return view! {
            <div class="text-sm text-muted-foreground italic">
                "No lifecycle transitions available"
            </div>
        }
        .into_any();
    }

    view! {
        <div class="flex flex-wrap gap-2">
            // Render a button for each valid transition
            {transitions.into_iter().map(|(target_state, label)| {
                let on_click = open_transition_dialog(target_state);
                let variant = button_variant_for_transition(target_state);
                view! {
                    <Button
                        variant=variant
                        size=ButtonSize::Sm
                        on_click=Callback::new(move |()| on_click())
                    >
                        {label}
                    </Button>
                }
            }).collect_view()}

            // Lifecycle transition confirmation dialog
            <LifecycleTransitionDialog
                open=show_dialog
                transition=Signal::derive(move || selected_transition.get())
                on_confirm=handle_confirm
                loading=Signal::derive(move || loading.get())
            />
        </div>
    }
    .into_any()
}
