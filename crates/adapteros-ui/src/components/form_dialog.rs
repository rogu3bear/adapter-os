//! FormDialog component for form submission patterns
//!
//! A standardized dialog for forms with submit/cancel actions,
//! loading states, and form validation integration.

use crate::components::{Button, ButtonVariant, Dialog, DialogSize};
use leptos::prelude::*;

/// Form dialog with submit/cancel actions
///
/// Standardizes the dialog pattern used across Stacks, Admin, Repositories, Training.
/// Provides consistent form submission UX with loading states.
///
/// # Example
/// ```rust,ignore
/// let show_dialog = RwSignal::new(false);
/// let name = RwSignal::new(String::new());
///
/// view! {
///     <FormDialog
///         open=show_dialog
///         title="Create Adapter"
///         submit_label="Create"
///         on_submit=Callback::new(move |_| {
///             // Handle form submission
///         })
///     >
///         <Input
///             label="Name"
///             value=name
///             on_input=move |v| name.set(v)
///         />
///     </FormDialog>
/// }
/// ```
#[component]
pub fn FormDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Dialog title
    #[prop(into)]
    title: String,
    /// Optional dialog description
    #[prop(optional, into)]
    description: Option<String>,
    /// Submit button label
    #[prop(optional, into)]
    submit_label: Option<String>,
    /// Cancel button label
    #[prop(optional, into)]
    cancel_label: Option<String>,
    /// Whether the form is currently submitting
    #[prop(optional)]
    loading: Option<Signal<bool>>,
    /// Whether the submit button should be disabled (e.g., invalid form)
    #[prop(optional)]
    submit_disabled: Option<Signal<bool>>,
    /// Submit button variant
    #[prop(optional)]
    submit_variant: Option<ButtonVariant>,
    /// Callback when form is submitted
    on_submit: Callback<()>,
    /// Optional callback when dialog is cancelled
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
    /// Dialog size variant (default: Md)
    #[prop(optional)]
    size: DialogSize,
    /// Enable scrollable content with max-height constraint
    #[prop(optional)]
    scrollable: bool,
    /// Form content (children)
    children: Children,
) -> impl IntoView {
    let submit_text = submit_label.unwrap_or_else(|| "Submit".to_string());
    let cancel_text = cancel_label.unwrap_or_else(|| "Cancel".to_string());
    let variant = submit_variant.unwrap_or(ButtonVariant::Primary);

    // Convert Option<Signal> to Signal with default false
    let loading_signal = Signal::derive(move || loading.map(|l| l.get()).unwrap_or(false));
    let disabled_signal = Signal::derive(move || {
        submit_disabled.map(|d| d.get()).unwrap_or(false)
            || loading.map(|l| l.get()).unwrap_or(false)
    });

    let handle_cancel = {
        let on_cancel = on_cancel.clone();
        move |_| {
            if !loading_signal.get() {
                if let Some(ref cb) = on_cancel {
                    cb.run(());
                }
                open.set(false);
            }
        }
    };

    let handle_submit = move |_| {
        if !loading_signal.get() && !disabled_signal.get() {
            on_submit.run(());
        }
    };

    let desc = description.clone().unwrap_or_default();

    view! {
        <Dialog
            open=open
            title=title.clone()
            description=desc
            size=size
            scrollable=scrollable
        >
            <form
                class="space-y-4"
                on:submit=move |ev| {
                    ev.prevent_default();
                    handle_submit(());
                }
            >
                // Form content
                <div class="form-dialog-content">
                    {children()}
                </div>

                // Form actions
                <div class="form-dialog-actions flex justify-end gap-3 pt-4 border-t border-border">
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(handle_cancel.clone())
                        disabled=loading_signal
                    >
                        {cancel_text.clone()}
                    </Button>
                    <Button
                        variant=variant
                        disabled=disabled_signal
                        loading=loading_signal
                    >
                        {submit_text.clone()}
                    </Button>
                </div>
            </form>
        </Dialog>
    }
}

/// Form dialog with multiple steps (wizard pattern)
///
/// For complex multi-step forms like Training configuration.
#[component]
pub fn StepFormDialog(
    /// Whether the dialog is open
    #[prop(into)]
    open: RwSignal<bool>,
    /// Dialog title
    #[prop(into)]
    title: String,
    /// Current step (0-indexed)
    current_step: Signal<usize>,
    /// Total number of steps
    total_steps: usize,
    /// Step labels
    step_labels: Vec<String>,
    /// Whether the current step is valid
    #[prop(optional)]
    step_valid: Option<Signal<bool>>,
    /// Whether the form is currently submitting
    #[prop(optional)]
    loading: Option<Signal<bool>>,
    /// Callback when moving to next step
    on_next: Callback<()>,
    /// Callback when moving to previous step
    on_back: Callback<()>,
    /// Callback when form is submitted (on final step)
    on_submit: Callback<()>,
    /// Optional callback when dialog is cancelled
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
    /// Dialog size variant (default: Md)
    #[prop(optional)]
    size: DialogSize,
    /// Enable scrollable content with max-height constraint
    #[prop(optional)]
    scrollable: bool,
    /// Step content (children)
    children: Children,
) -> impl IntoView {
    // Convert Optional Signals to derived Signals with defaults
    let loading_signal = Signal::derive(move || loading.map(|l| l.get()).unwrap_or(false));
    let valid_signal = Signal::derive(move || step_valid.map(|v| v.get()).unwrap_or(true));
    let invalid_signal = Signal::derive(move || !valid_signal.get());
    let submit_disabled = Signal::derive(move || loading_signal.get() || !valid_signal.get());

    let is_first = move || current_step.get() == 0;
    let is_last = move || current_step.get() == total_steps - 1;

    let handle_cancel = {
        let on_cancel = on_cancel.clone();
        move |_| {
            if !loading_signal.get() {
                if let Some(ref cb) = on_cancel {
                    cb.run(());
                }
                open.set(false);
            }
        }
    };

    view! {
        <Dialog open=open title=title.clone() size=size scrollable=scrollable>
            // Step indicator
            <div class="step-indicator flex items-center justify-center gap-2 mb-6">
                {step_labels.iter().enumerate().map(|(idx, label)| {
                    let is_current = move || current_step.get() == idx;
                    let is_complete = move || current_step.get() > idx;

                    view! {
                        <div class="flex items-center">
                            {(idx > 0).then(|| view! {
                                <div class=move || format!(
                                    "w-8 h-0.5 {}",
                                    if is_complete() || is_current() { "bg-primary" } else { "bg-muted" }
                                ) />
                            })}
                            <div class="flex flex-col items-center">
                                <div class=move || format!(
                                    "w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium {}",
                                    if is_current() { "bg-primary text-primary-foreground" }
                                    else if is_complete() { "bg-primary/20 text-primary" }
                                    else { "bg-muted text-muted-foreground" }
                                )>
                                    {idx + 1}
                                </div>
                                <span class="text-xs mt-1 text-muted-foreground">
                                    {label.clone()}
                                </span>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>

            // Step content
            <div class="step-content min-h-[200px]">
                {children()}
            </div>

            // Actions
            <div class="step-actions flex justify-between pt-4 border-t border-border mt-4">
                <Button
                    variant=ButtonVariant::Ghost
                    on_click=Callback::new(handle_cancel.clone())
                    disabled=loading_signal
                >
                    "Cancel"
                </Button>

                <div class="flex gap-2">
                    {move || (!is_first()).then(|| view! {
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(move |_| on_back.run(()))
                            disabled=loading_signal
                        >
                            "Back"
                        </Button>
                    })}

                    {move || if is_last() {
                        view! {
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(move |_| on_submit.run(()))
                                disabled=submit_disabled
                                loading=loading_signal
                            >
                                "Submit"
                            </Button>
                        }.into_any()
                    } else {
                        view! {
                            <Button
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(move |_| on_next.run(()))
                                disabled=invalid_signal
                            >
                                "Next"
                            </Button>
                        }.into_any()
                    }}
                </div>
            </div>
        </Dialog>
    }
}
