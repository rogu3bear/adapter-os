//! ConfirmationDialog component for destructive actions
//!
//! Implements "typed intent" pattern for high-risk operations.
//! User must type a confirmation phrase to proceed.
//!
//! Wraps the base [`Dialog`] component for focus trap, focus restoration,
//! unique ARIA IDs, and Escape-to-close behavior.
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

use crate::components::icons::{IconInfo, IconTrash, IconWarning};
use crate::components::{Button, ButtonType, ButtonVariant, Dialog, DialogSize};
use leptos::prelude::*;

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
            Self::Warning => "text-status-warning",
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

    // Check if confirmation is valid
    let can_confirm = {
        let typed_confirmation = typed_confirmation.clone();
        Memo::new(move |_| match (&severity, &typed_confirmation) {
            (ConfirmationSeverity::Destructive, Some(required)) => {
                typed_input.try_get().unwrap_or_default() == *required
            }
            _ => true,
        })
    };

    let disabled = Signal::derive(move || {
        !can_confirm.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false)
    });

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

    let handle_cancel = move |_| {
        if !loading.try_get().unwrap_or(false) {
            handle_close();
        }
    };

    let handle_confirm = move |_| {
        if can_confirm.try_get().unwrap_or(false) && !loading.try_get().unwrap_or(false) {
            on_confirm.run(());
        }
    };

    // Reset typed input when dialog opens
    Effect::new(move || {
        if open.try_get().unwrap_or(false) {
            let _ = typed_input.try_set(String::new());
        }
    });

    let icon_class = format!("flex-shrink-0 {}", severity.icon_class());

    view! {
        <Dialog
            open=open
            title=title.clone()
            description=description.clone()
            size=DialogSize::Sm
            scrollable=true
        >
            <form
                on:submit=move |ev| {
                    ev.prevent_default();
                    handle_confirm(());
                }
            >
                // Severity icon
                <div class="flex items-start gap-4 mb-4">
                    <div class=icon_class.clone()>
                        {match severity {
                            ConfirmationSeverity::Destructive => view! {
                                <IconTrash class="h-6 w-6".to_string() />
                            }.into_any(),
                            ConfirmationSeverity::Warning => view! {
                                <IconWarning class="h-6 w-6".to_string() />
                            }.into_any(),
                            ConfirmationSeverity::Normal => view! {
                                <IconInfo class="h-6 w-6".to_string() />
                            }.into_any(),
                        }}
                    </div>
                    <div class="flex-1">
                        <p class=format!("heading-3 {}", severity.header_class())>
                            {title.clone()}
                        </p>
                        <p class="mt-2 text-sm text-muted-foreground">
                            {description.clone()}
                        </p>
                    </div>
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
                                        type="text"
                                        id="confirm-dialog-input"
                                        class="input"
                                        placeholder=format!("Type {} to confirm", confirm_phrase)
                                        prop:value=move || typed_input.try_get().unwrap_or_default()
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
                        button_type=ButtonType::Submit
                        disabled=disabled
                        loading=loading
                    >
                        {confirm_btn_text.clone()}
                    </Button>
                </div>
            </form>
        </Dialog>
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
