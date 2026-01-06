//! Form field wrappers for consistent labels, help, and errors.

use leptos::prelude::*;

/// Context provided by FormField for child inputs.
#[derive(Clone)]
pub struct FormFieldContext {
    pub field_id: String,
    pub described_by: Option<String>,
}

/// Read the current form field context (if any).
pub fn use_form_field_context() -> Option<FormFieldContext> {
    use_context::<FormFieldContext>()
}

/// Help tooltip icon.
#[component]
pub fn HelpTooltip(#[prop(into)] text: String) -> impl IntoView {
    view! {
        <span class="help-tooltip" title=text aria-label="Help">
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
#[component]
pub fn FormField(
    #[prop(into)] label: String,
    #[prop(into)] name: String,
    #[prop(optional)] required: bool,
    #[prop(optional, into)] help: Option<String>,
    #[prop(optional)] error: Option<Signal<Option<String>>>,
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
