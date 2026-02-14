//! Lifecycle Transition Dialog
//!
//! Confirmation dialog for adapter lifecycle state transitions.
//! Requires a reason for audit trail.
//!
//! # Usage
//!
//! ```rust,ignore
//! let show_dialog = RwSignal::new(false);
//! let transition_props = RwSignal::new(None);
//!
//! view! {
//!     <LifecycleTransitionDialog
//!         open=show_dialog
//!         transition=Signal::derive(move || transition_props.get())
//!         on_confirm=Callback::new(move |reason: String| {
//!             // Perform transition with reason for audit
//!         })
//!     />
//! }
//! ```

use crate::components::{Button, ButtonVariant, Dialog, DialogSize, Textarea};
use leptos::prelude::*;
use web_sys::KeyboardEvent;

/// Details for a lifecycle transition
#[derive(Clone, Debug, PartialEq)]
pub struct LifecycleTransitionInfo {
    /// Name of the adapter being transitioned
    pub adapter_name: String,
    /// Current lifecycle state
    pub current_state: String,
    /// Target lifecycle state
    pub new_state: String,
    /// Whether the adapter is currently in-flight (serving requests)
    pub is_in_flight: bool,
}

/// Lifecycle transition confirmation dialog
///
/// Displays a confirmation dialog for adapter lifecycle transitions.
/// Requires a reason input for audit trail purposes.
/// Shows a warning if the adapter is currently in-flight.
#[component]
pub fn LifecycleTransitionDialog(
    /// Whether dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Transition details (None hides content)
    #[prop(into)]
    transition: Signal<Option<LifecycleTransitionInfo>>,
    /// Called with reason when confirmed
    on_confirm: Callback<String>,
    /// Loading state during transition
    #[prop(into, default = Signal::derive(|| false))]
    loading: Signal<bool>,
) -> impl IntoView {
    // Reason input signal
    let reason = RwSignal::new(String::new());

    // Validate reason is not empty
    let can_confirm = Memo::new(move |_| {
        let r = reason.try_get().unwrap_or_default();
        !r.trim().is_empty()
    });

    // Reset reason when dialog opens
    Effect::new(move || {
        if open.try_get().unwrap_or(false) {
            let _ = reason.try_set(String::new());
        }
    });

    let handle_cancel = move |_| {
        if !loading.try_get().unwrap_or(false) {
            open.set(false);
            reason.set(String::new());
        }
    };

    let handle_confirm = move |_| {
        if can_confirm.try_get().unwrap_or(false) && !loading.try_get().unwrap_or(false) {
            on_confirm.run(reason.try_get().unwrap_or_default());
        }
    };

    // Keyboard handler for Ctrl+Enter to confirm
    let handle_keydown = move |ev: KeyboardEvent| {
        if ev.key() == "Enter"
            && ev.ctrl_key()
            && can_confirm.try_get().unwrap_or(false)
            && !loading.try_get().unwrap_or(false)
        {
            ev.prevent_default();
            on_confirm.run(reason.try_get().unwrap_or_default());
        }
    };

    view! {
        <Dialog
            open=open
            title="Confirm Lifecycle Transition".to_string()
            size=DialogSize::Md
        >
            <div class="space-y-4" on:keydown=handle_keydown>
                // Transition summary
                {move || transition.try_get().flatten().map(|t| {
                    view! {
                        <div class="space-y-4">
                            // Adapter name
                            <div class="text-sm">
                                <span class="text-muted-foreground">"Adapter: "</span>
                                <span class="font-medium">{t.adapter_name.clone()}</span>
                            </div>

                            // State transition visualization
                            <div class="flex items-center gap-3 p-3 bg-muted/50 rounded-lg">
                                <span class="px-2 py-1 text-sm font-medium bg-secondary rounded">
                                    {t.current_state.clone()}
                                </span>
                                <svg
                                    class="h-4 w-4 text-muted-foreground"
                                    xmlns="http://www.w3.org/2000/svg"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                    aria-hidden="true"
                                >
                                    <path d="M5 12h14"/>
                                    <path d="m12 5 7 7-7 7"/>
                                </svg>
                                <span class="px-2 py-1 text-sm font-medium bg-primary text-primary-foreground rounded">
                                    {t.new_state.clone()}
                                </span>
                            </div>

                            // In-flight warning
                            {t.is_in_flight.then(|| view! {
                                <div class="flex items-start gap-2 p-3 bg-warning/10 border border-warning/20 rounded-lg text-warning">
                                    <svg
                                        class="h-5 w-5 flex-shrink-0 mt-0.5"
                                        xmlns="http://www.w3.org/2000/svg"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        aria-hidden="true"
                                    >
                                        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>
                                        <line x1="12" y1="9" x2="12" y2="13"/>
                                        <line x1="12" y1="17" x2="12.01" y2="17"/>
                                    </svg>
                                    <div class="text-sm">
                                        <p class="font-medium">"Adapter is currently in use"</p>
                                        <p class="text-warning/80 mt-1">
                                            "This adapter is serving active requests. Changing its state may affect ongoing inference operations."
                                        </p>
                                    </div>
                                </div>
                            })}

                            // Reason input
                            <div class="space-y-2">
                                <label class="label" for="transition-reason">
                                    "Reason for transition"
                                    <span class="form-field-required" aria-hidden="true">"*"</span>
                                </label>
                                <Textarea
                                    value=reason
                                    placeholder="Enter the reason for this state change (required for audit trail)..."
                                    id="transition-reason".to_string()
                                    rows=3
                                    disabled=loading.try_get().unwrap_or(false)
                                />
                                <p class="text-xs text-muted-foreground">
                                    "This reason will be recorded in the audit log."
                                </p>
                            </div>
                        </div>
                    }
                })}

                // Actions
                <div class="flex justify-end gap-3 pt-4 border-t border-border">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(handle_cancel)
                        disabled=loading
                    >
                        "Cancel"
                    </Button>
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Callback::new(handle_confirm)
                        disabled=Signal::derive(move || !can_confirm.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false))
                        loading=loading
                    >
                        "Confirm Transition"
                    </Button>
                </div>
            </div>
        </Dialog>
    }
}
