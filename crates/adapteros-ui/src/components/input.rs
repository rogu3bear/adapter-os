//! Input component

use crate::components::form_field::use_form_field_context;
use leptos::prelude::*;

/// Input component
#[component]
pub fn Input(
    #[prop(optional, into)] value: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] input_type: String,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] error: Option<String>,
) -> impl IntoView {
    let base_class = "input";

    let error_class = if error.is_some() { "input-error" } else { "" };

    let full_class = format!("{} {} {}", base_class, error_class, class);

    let input_type_val = if input_type.is_empty() {
        "text".to_string()
    } else {
        input_type
    };

    let field_ctx = use_form_field_context();
    let input_id = id.or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()));
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    view! {
        <div class="grid w-full gap-1.5">
            {if !label.is_empty() {
                Some(view! {
                    <label class="label" for=input_id.clone()>
                        {label.clone()}
                    </label>
                })
            } else {
                None
            }}
            <input
                id=input_id
                name=name
                type=input_type_val
                class=full_class
                placeholder=placeholder
                disabled=disabled
                aria-describedby=described_by
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
            {error.map(|e| view! {
                <p class="form-field-error">{e}</p>
            })}
        </div>
    }
}

/// Textarea component
#[component]
pub fn Textarea(
    #[prop(optional, into)] value: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] rows: Option<u32>,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] aria_label: String,
) -> impl IntoView {
    let base_class = "input input-textarea";

    let full_class = format!("{} {}", base_class, class);

    let field_ctx = use_form_field_context();
    let input_id = id.or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()));
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label" for=input_id.clone()>
                    {l}
                </label>
            })}
            <textarea
                id=input_id
                name=name
                class=full_class
                placeholder=placeholder
                disabled=disabled
                rows=rows.unwrap_or(3)
                aria-label=move || (!aria_label.is_empty()).then(|| aria_label.clone())
                aria-describedby=described_by
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
        </div>
    }
}
