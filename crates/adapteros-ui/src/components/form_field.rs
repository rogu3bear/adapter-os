//! Form field wrappers for consistent labels, help, and errors.
//!
//! # Accessibility Requirements (PRD-UI-150)
//!
//! Every form field MUST have a visible label. The `FormField` component enforces this:
//! - The `label` prop is required and renders a visible `<label>` element
//! - For inputs where layout constraints prevent a visible label, use `aria-label` on the Input
//! - Placeholder text is NOT a substitute for a label
//!
//! Optional hint text can be added below the label for additional context.

use leptos::prelude::*;

/// Context provided by FormField for child inputs.
#[derive(Clone)]
pub struct FormFieldContext {
    /// Unique ID for the input element
    pub field_id: String,
    /// Combined IDs for aria-describedby (help text + error)
    pub described_by: Option<String>,
    /// Whether this field is required
    pub required: bool,
}

/// Read the current form field context (if any).
pub fn use_form_field_context() -> Option<FormFieldContext> {
    use_context::<FormFieldContext>()
}

/// Help tooltip icon.
#[component]
pub fn HelpTooltip(#[prop(into)] text: String) -> impl IntoView {
    let aria_label = format!("Help: {}", text);
    view! {
        <span
            class="help-tooltip"
            title=text
            aria-label=aria_label
            tabindex="0"
            role="button"
        >
            "?"
        </span>
    }
}

/// Label with optional help tooltip and required indicator.
#[component]
pub fn LabelWithHelp(
    #[prop(into)] label: String,
    #[prop(into)] for_id: String,
    #[prop(optional)] required: bool,
    help: Option<String>,
) -> impl IntoView {
    view! {
        <label class="label" for=for_id>
            <span class="flex items-center gap-2">
                <span>{label}</span>
                {move || {
                    if required {
                        view! { <span class="form-field-required">"*"</span> }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
                {help.clone().map(|text| view! { <HelpTooltip text=text/> })}
            </span>
        </label>
    }
}

/// Form field wrapper with label, help text, and error display.
///
/// # Label Requirement
///
/// The `label` prop is required. Every form field must have a visible label for accessibility.
/// Placeholder text is not a substitute for a proper label.
///
/// # Example
///
/// ```rust
/// <FormField
///     label="Email Address"
///     name="email"
///     required=true
///     help="We'll never share your email."
///     error=email_error_signal
/// >
///     <Input value=email placeholder="you@example.com" />
/// </FormField>
/// ```
#[component]
pub fn FormField(
    /// Visible label text (required for accessibility)
    #[prop(into)]
    label: String,
    /// Field name used for ID generation and form submission
    #[prop(into)]
    name: String,
    /// Whether the field is required (shows asterisk indicator)
    #[prop(optional)]
    required: bool,
    /// Help text displayed below the input (for additional context)
    #[prop(optional, into)]
    help: Option<String>,
    /// Reactive error signal (error shown when Some)
    #[prop(optional)]
    error: Option<Signal<Option<String>>>,
    children: Children,
) -> impl IntoView {
    let field_id = format!("field-{}", name);
    let help_id = format!("{}-help", field_id);
    let error_id = format!("{}-error", field_id);

    let described_by = {
        let mut parts = Vec::new();
        if help.is_some() {
            parts.push(help_id.clone());
        }
        if error.is_some() {
            parts.push(error_id.clone());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    };

    provide_context(FormFieldContext {
        field_id: field_id.clone(),
        described_by: described_by.clone(),
        required,
    });

    view! {
        <div class="form-field">
            <LabelWithHelp
                label=label
                for_id=field_id.clone()
                required=required
                help=help.clone()
            />
            <div class="form-field-control">
                {children()}
            </div>
            {help.map(|text| view! {
                <p id=help_id class="form-field-help">{text}</p>
            })}
            {error.map(|signal| {
                let error_id = error_id.clone();
                view! {
                    {move || signal.get().map(|text| view! {
                        <p id=error_id.clone() class="form-field-error" role="alert">{text}</p>
                    })}
                }
            })}
        </div>
    }
}
