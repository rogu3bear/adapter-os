//! Toggle/Switch component

use crate::components::form_field::use_form_field_context;
use leptos::prelude::*;

/// Toggle component (switch)
#[component]
pub fn Toggle(
    #[prop(into)] checked: RwSignal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    /// Accessible label for screen readers (used when no visible label is provided)
    #[prop(optional, into)]
    aria_label: Option<String>,
    /// Optional ID for the toggle button element
    #[prop(optional, into)]
    id: Option<String>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    // Generate a stable ID once per component instance using StoredValue
    let button_id =
        StoredValue::new(id.unwrap_or_else(|| format!("toggle-{}", uuid::Uuid::new_v4())));

    let toggle = move |_| {
        if !disabled.try_get().unwrap_or(false) {
            checked.update(|v| *v = !*v);
        }
    };

    // Only apply aria-label when no visible label is provided
    let effective_aria_label = aria_label.filter(|_| label.is_none());

    view! {
        <div class=format!("flex items-center justify-between {}", class)>
            <div class="space-y-0.5">
                {label.map(|l| {
                    let for_id = button_id.get_value();
                    view! {
                        <label class="label" for=for_id>
                            {l}
                        </label>
                    }
                })}
                {description.map(|d| view! {
                    <p class="text-sm text-muted-foreground">{d}</p>
                })}
            </div>
            <button
                type="button"
                id=button_id.get_value()
                role="switch"
                aria-checked=move || checked.try_get().unwrap_or(false).to_string()
                aria-label=effective_aria_label
                disabled=move || disabled.try_get().unwrap_or(false)
                class=move || {
                    let base = "toggle";
                    let state = if checked.try_get().unwrap_or(false) { "toggle-on" } else { "toggle-off" };
                    format!("{} {}", base, state)
                }
                on:click=toggle
            >
                <span
                    class=move || {
                        let base = "toggle-thumb";
                        let state = if checked.try_get().unwrap_or(false) {
                            "toggle-thumb-on"
                        } else {
                            "toggle-thumb-off"
                        };
                        format!("{} {}", base, state)
                    }
                />
            </button>
        </div>
    }
}

/// Select component for dropdowns
#[component]
pub fn Select(
    #[prop(into)] value: RwSignal<String>,
    #[prop(into)] options: Vec<(String, String)>,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_change: Option<Callback<String>>,
    /// Accessible label for screen readers (used when no visible label is provided)
    #[prop(optional, into)]
    aria_label: Option<String>,
) -> impl IntoView {
    let full_class = format!("select {}", class);
    let field_ctx = use_form_field_context();
    let input_id = id.or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()));
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    // Only apply aria-label when no visible label is provided
    let effective_aria_label = aria_label.filter(|_| label.is_none());

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label" for=input_id.clone()>
                    {l}
                </label>
            })}
            <select
                id=input_id
                name=name
                class=full_class
                disabled=move || disabled.try_get().unwrap_or(false)
                aria-describedby=described_by
                aria-label=effective_aria_label
                prop:value=move || value.try_get().unwrap_or_default()
                on:change=move |ev| {
                    let next = event_target_value(&ev);
                    value.set(next.clone());
                    if let Some(ref callback) = on_change {
                        callback.run(next);
                    }
                }
            >
                {options.into_iter().map(|(val, label)| {
                    let val_clone = val.clone();
                    view! {
                        <option value=val selected=move || value.try_get().unwrap_or_default() == val_clone>
                            {label}
                        </option>
                    }
                }).collect::<Vec<_>>()}
            </select>
        </div>
    }
}
