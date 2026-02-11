//! ConfirmationDialog component for destructive actions
//!
//! Implements "typed intent" pattern for high-risk operations.
//! User must type a confirmation phrase to proceed.
//!
//! # PRD-UI-150: Risk Threshold Policy
//!
//! Operations requiring typed confirmation (Destructive severity):
//! - **Irreversible deletions**: adapters, training data, models
//! - **Bulk operations**: deleting multiple items
//! - **Security-sensitive**: API key rotation, access revocation
//!
//! Operations requiring simple confirmation (Normal/Warning severity):
//! - Logout
//! - Navigation with unsaved changes
//! - Training job cancellation
//!
//! Confirmation phrase format:
//! - Use resource name (e.g., "my-adapter-v2") for targeted deletions
//! - Use action verb (e.g., "DELETE", "REVOKE") for bulk or anonymous operations
//!
//! # Impact Summaries (PRD-UI-150)
//!
//! For destructive actions, dialogs should display an impact summary showing
//! what will be affected. Use the `impact_items` prop to list affected resources:
//!
//! ```rust
//! ConfirmationDialog {
//!     title: "Delete Stack",
//!     impact_items: vec![
//!         ("Adapter associations", "3 adapters will be disassociated"),
//!         ("Inference sessions", "Active sessions will be terminated"),
//!     ],
//!     ..
//! }
//! ```
//!
//! # Accessibility features
//! - Escape key closes the dialog (when not loading)
//! - Enter key confirms (when valid and not loading)
//! - ARIA `role="alertdialog"` with labelledby/describedby
//! - Focus management via Leptos

use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use web_sys::KeyboardEvent;

/// Severity level for confirmation dialogs
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ConfirmationSeverity {
    /// Standard confirmation (blue styling)
    #[default]
    Normal,
    /// Warning confirmation (yellow/amber styling)
    Warning,
    /// Destructive confirmation (red styling, requires typed intent)
    Destructive,
}

impl ConfirmationSeverity {
    fn header_class(&self) -> &'static str {
        match self {
            Self::Normal => "text-foreground",
            Self::Warning => "text-warning",
            Self::Destructive => "text-destructive",
        }
    }

    fn icon_class(&self) -> &'static str {
        match self {
            Self::Normal => "text-primary",
            Self::Warning => "text-status-warning",
            Self::Destructive => "text-destructive",
        }
    }

    fn button_variant(&self) -> ButtonVariant {
        match self {
            Self::Normal => ButtonVariant::Primary,
            Self::Warning => ButtonVariant::Primary,
            Self::Destructive => ButtonVariant::Destructive,
        }
    }
}

/// Impact item for destructive action summaries
#[derive(Clone, Debug)]
pub struct ImpactItem {
    /// Category/type of impact (e.g., "Adapters", "Stack associations")
    pub label: String,
    /// Description of what will happen (e.g., "3 adapters will be removed")
    pub description: String,
}

impl ImpactItem {
    /// Create a new impact item
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

/// ConfirmationDialog component
///
/// For destructive operations, requires the user to type a confirmation phrase.
///
/// # Example
/// ```rust
/// let show_delete = RwSignal::new(false);
///
/// view! {
///     <ConfirmationDialog
///         open=show_delete
///         title="Delete Adapter"
///         description="This will permanently delete the adapter and all associated data."
///         severity=ConfirmationSeverity::Destructive
///         confirm_text="DELETE"
///         typed_confirmation=Some("my-adapter".to_string())
///         impact_items=vec![
///             ImpactItem::new("Stack associations", "2 stacks will lose this adapter"),
///         ]
///         on_confirm=Callback::new(move |_| {
///             // Perform deletion
///         })
///     />
/// }
/// ```
#[component]
pub fn ConfirmationDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Dialog title
    #[prop(into)]
    title: String,
    /// Description text explaining the action
    #[prop(into)]
    description: String,
    /// Severity level (affects styling and behavior)
    #[prop(optional)]
    severity: ConfirmationSeverity,
    /// Text for the confirm button
    #[prop(optional, into)]
    confirm_text: Option<String>,
    /// Text for the cancel button
    #[prop(optional, into)]
    cancel_text: Option<String>,
    /// If set, user must type this exact text to enable confirmation
    /// Only applies to Destructive severity
    #[prop(optional, into)]
    typed_confirmation: Option<String>,
    /// Impact summary items showing what will be affected
    /// Displayed as a list before the confirmation input
    #[prop(optional)]
    impact_items: Vec<ImpactItem>,
    /// Callback when user confirms
    on_confirm: Callback<()>,
    /// Optional callback when user cancels
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
    /// Whether the action is loading/in-progress
    #[prop(optional, into)]
    loading: Signal<bool>,
) -> impl IntoView {
    let confirm_btn_text = confirm_text.unwrap_or_else(|| "Confirm".to_string());
    let cancel_btn_text = cancel_text.unwrap_or_else(|| "Cancel".to_string());

    // Typed confirmation input
    let typed_input = RwSignal::new(String::new());

    // Check if confirmation is valid (derived signal)
    let can_confirm = {
        let typed_confirmation = typed_confirmation.clone();
        Memo::new(move |_| match (&severity, &typed_confirmation) {
            (ConfirmationSeverity::Destructive, Some(required)) => typed_input.get() == *required,
            _ => true,
        })
    };

    // Close handler that also calls on_cancel callback
    let handle_close = {
        move || {
            if let Some(ref cb) = on_cancel {
                cb.run(());
            }
            open.set(false);
            typed_input.set(String::new());
        }
    };

    let handle_cancel = {
        move |_| {
            handle_close();
        }
    };

    let handle_confirm = move |_| {
        if can_confirm.get() {
            on_confirm.run(());
            // Don't close automatically - let the caller handle it after async operation
        }
    };

    // Reset typed input when dialog opens
    Effect::new(move || {
        if open.try_get().unwrap_or(false) {
            let _ = typed_input.try_set(String::new());
        }
    });

    // Keyboard event handler for accessibility
    let handle_keydown = {
        move |ev: KeyboardEvent| match ev.key().as_str() {
            "Escape" => {
                if !loading.get() {
                    ev.prevent_default();
                    handle_close();
                }
            }
            "Enter" => {
                if can_confirm.get() && !loading.get() {
                    ev.prevent_default();
                    on_confirm.run(());
                }
            }
            _ => {}
        }
    };

    view! {
        // Backdrop
        <div
            class=move || {
                if open.get() {
                    "fixed inset-0 z-50 bg-black/80 flex items-center justify-center"
                } else {
                    "hidden"
                }
            }
            aria-hidden=move || (!open.get()).to_string()
            on:click=move |_| if !loading.get() { handle_close() }
        >
            // Dialog content (stop propagation to prevent closing on content click)
            <div
                class=move || {
                    if open.get() {
                        "relative w-full max-w-md bg-background border rounded-lg shadow-lg p-6"
                    } else {
                        "hidden"
                    }
                }
                on:click=|ev| ev.stop_propagation()
                on:keydown=handle_keydown
                role="alertdialog"
                aria-modal="true"
                aria-labelledby="confirm-dialog-title"
                aria-describedby="confirm-dialog-description"
                aria-busy=move || loading.get().to_string()
            >
                // Icon and header
                <div class="flex items-start gap-4">
                    // Icon
                    <div class=format!("flex-shrink-0 {}", severity.icon_class())>
                        {match severity {
                            ConfirmationSeverity::Destructive => view! {
                                <svg
                                    aria-hidden="true"
                                    xmlns="http://www.w3.org/2000/svg"
                                    width="24"
                                    height="24"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M3 6h18"/>
                                    <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
                                    <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
                                    <line x1="10" y1="11" x2="10" y2="17"/>
                                    <line x1="14" y1="11" x2="14" y2="17"/>
                                </svg>
                            }.into_any(),
                            ConfirmationSeverity::Warning => view! {
                                <svg
                                    aria-hidden="true"
                                    xmlns="http://www.w3.org/2000/svg"
                                    width="24"
                                    height="24"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/>
                                    <line x1="12" y1="9" x2="12" y2="13"/>
                                    <line x1="12" y1="17" x2="12.01" y2="17"/>
                                </svg>
                            }.into_any(),
                            ConfirmationSeverity::Normal => view! {
                                <svg
                                    aria-hidden="true"
                                    xmlns="http://www.w3.org/2000/svg"
                                    width="24"
                                    height="24"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <circle cx="12" cy="12" r="10"/>
                                    <path d="M12 16v-4"/>
                                    <path d="M12 8h.01"/>
                                </svg>
                            }.into_any(),
                        }}
                    </div>

                    // Title and description
                    <div class="flex-1">
                        <h2
                            id="confirm-dialog-title"
                            class=format!("heading-3 {}", severity.header_class())
                        >
                            {title.clone()}
                        </h2>
                        <p
                            id="confirm-dialog-description"
                            class="mt-2 text-sm text-muted-foreground"
                        >
                            {description.clone()}
                        </p>
                    </div>

                    // Close button
                    <button
                        class="absolute right-4 top-4 rounded-sm opacity-70 hover:opacity-100 disabled:pointer-events-none disabled:opacity-50"
                        aria-label="Close dialog"
                        disabled=move || loading.get()
                        on:click=move |_| if !loading.get() { handle_close() }
                    >
                        <svg
                            aria-hidden="true"
                            xmlns="http://www.w3.org/2000/svg"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M18 6 6 18"/>
                            <path d="m6 6 12 12"/>
                        </svg>
                    </button>
                </div>

                // Impact summary for destructive actions
                {if !impact_items.is_empty() {
                    let items = impact_items.clone();
                    Some(view! {
                        <div class="mt-4 rounded-md border border-destructive/30 bg-destructive/5 p-3">
                            <h3 class="text-sm font-medium text-destructive mb-2">"Impact Summary"</h3>
                            <ul class="space-y-1">
                                {items.into_iter().map(|item| {
                                    view! {
                                        <li class="flex items-start gap-2 text-sm">
                                            <span class="text-destructive mt-0.5">
                                                <svg aria-hidden="true" class="h-3 w-3" fill="currentColor" viewBox="0 0 8 8">
                                                    <circle cx="4" cy="4" r="3" />
                                                </svg>
                                            </span>
                                            <span>
                                                <span class="font-medium">{item.label}</span>
                                                ": "
                                                <span class="text-muted-foreground">{item.description}</span>
                                            </span>
                                        </li>
                                    }
                                }).collect::<Vec<_>>()}
                            </ul>
                        </div>
                    })
                } else {
                    None
                }}

                // Typed confirmation input (for destructive actions)
                {move || {
                    if severity == ConfirmationSeverity::Destructive {
                        typed_confirmation.clone().map(|confirm_phrase| {
                            view! {
                                <div class="mt-6">
                                    <label class="block text-sm font-medium text-foreground mb-2" for="confirm-dialog-input">
                                        "To confirm, type "
                                        <span class="font-mono bg-muted px-1.5 py-0.5 rounded text-destructive">
                                            {confirm_phrase.clone()}
                                        </span>
                                        " below:"
                                    </label>
                                    <input
                                        id="confirm-dialog-input"
                                        type="text"
                                        class="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                                        placeholder=format!("Type {} to confirm", confirm_phrase)
                                        prop:value=move || typed_input.get()
                                        on:input=move |ev| {
                                            typed_input.set(event_target_value(&ev));
                                        }
                                    />
                                </div>
                            }
                        })
                    } else {
                        None
                    }
                }}

                // Actions
                <div class="mt-6 flex justify-end gap-3">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(handle_cancel)
                    >
                        {cancel_btn_text.clone()}
                    </Button>
                    <Button
                        variant=severity.button_variant()
                        disabled=Signal::derive(move || !can_confirm.get() || loading.get())
                        loading=loading
                        on_click=Callback::new(handle_confirm)
                    >
                        {confirm_btn_text.clone()}
                    </Button>
                </div>
            </div>
        </div>
    }
}

/// Simple confirmation dialog without typed intent
/// For non-destructive confirmations like "Are you sure you want to leave?"
#[component]
pub fn SimpleConfirmDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Dialog title
    #[prop(into)]
    title: String,
    /// Description text
    #[prop(into)]
    description: String,
    /// Callback when user confirms
    on_confirm: Callback<()>,
) -> impl IntoView {
    view! {
        <ConfirmationDialog
            open=open
            title=title
            description=description
            severity=ConfirmationSeverity::Normal
            on_confirm=on_confirm
        />
    }
}
