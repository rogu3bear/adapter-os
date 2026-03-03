//! Checkbox component
//!
//! Styled checkbox with label support for consistent form controls.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Checkbox component
///
/// A styled checkbox input with optional label text.
/// Accepts both direct signals and derived values for the checked state.
#[component]
pub fn Checkbox(
    /// Whether the checkbox is checked (reactive)
    #[prop(into)]
    checked: Signal<bool>,
    /// Callback when checked state changes
    #[prop(optional)]
    on_change: Option<Callback<bool>>,
    /// Optional label text
    #[prop(optional, into)]
    label: Option<String>,
    /// Accessible label for screen readers (used when no visible label is provided)
    #[prop(optional, into)]
    aria_label: Option<String>,
    /// Optional ID for the checkbox input
    #[prop(optional, into)]
    id: Option<String>,
    /// Optional name for form submission and accessibility tooling
    #[prop(optional, into)]
    name: Option<String>,
    /// Whether the checkbox is disabled
    #[prop(optional, into)]
    disabled: Signal<bool>,
    /// Additional CSS classes for the container label
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let label_class = if class.is_empty() {
        "checkbox-label".to_string()
    } else {
        format!("checkbox-label {class}")
    };

    let handle_change = move |ev: web_sys::Event| {
        if let Some(cb) = on_change {
            // Defensive: event target may be None in edge cases (detached DOM, etc.)
            let Some(target) = ev.target() else {
                return;
            };
            let input: web_sys::HtmlInputElement = target.unchecked_into();
            cb.run(input.checked());
        }
    };

    // Only apply aria-label when no visible label is provided
    let effective_aria_label = aria_label.filter(|_| label.is_none());
    let input_id =
        StoredValue::new(id.unwrap_or_else(|| format!("checkbox-{}", uuid::Uuid::new_v4())));

    view! {
        <label class=label_class>
            <input
                id=input_id.get_value()
                name=name
                type="checkbox"
                class="checkbox"
                prop:checked=checked
                disabled=disabled
                aria-label=effective_aria_label
                on:change=handle_change
            />
            {label.map(|text| view! {
                <span class="checkbox-text">{text}</span>
            })}
        </label>
    }
}
