//! Input component

use crate::components::form_field::use_form_field_context;
use leptos::prelude::*;

/// Input component with validation states
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
    /// Show success state styling (green border)
    #[prop(optional)]
    valid: bool,
    /// Mark field as required (shows * indicator)
    #[prop(optional)]
    required: bool,
    /// Maximum character count (shows counter when set)
    #[prop(optional)]
    max_length: Option<usize>,
) -> impl IntoView {
    let base_class = "input";

    let state_class = if error.is_some() {
        "input-error"
    } else if valid {
        "input-success"
    } else {
        ""
    };

    let full_class = format!("{} {} {}", base_class, state_class, class);

    let input_type_val = if input_type.is_empty() {
        "text".to_string()
    } else {
        input_type
    };

    let field_ctx = use_form_field_context();
    let input_id = id.or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()));
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    // Character counter
    let char_count = move || value.get().len();
    let show_counter = max_length.is_some();

    view! {
        <div class="grid w-full gap-1.5">
            {if !label.is_empty() {
                Some(view! {
                    <label class="label" for=input_id.clone()>
                        {label.clone()}
                        {required.then(|| view! {
                            <span class="form-field-required" aria-hidden="true">"*"</span>
                        })}
                    </label>
                })
            } else {
                None
            }}
            <div class="relative">
                <input
                    id=input_id
                    name=name
                    type=input_type_val
                    class=full_class
                    placeholder=placeholder
                    disabled=disabled
                    required=required
                    maxlength=max_length.map(|n| n.to_string())
                    aria-invalid=error.is_some().to_string()
                    aria-describedby=described_by
                    prop:value=move || value.get()
                    on:input=move |ev| {
                        value.set(event_target_value(&ev));
                    }
                />
                {valid.then(|| view! {
                    <span class="input-icon input-icon-success">
                        <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <polyline
                                points="20 6 9 17 4 12"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                            />
                        </svg>
                    </span>
                })}
            </div>
            {show_counter.then(|| {
                let max = max_length.unwrap();
                view! {
                    <div class="flex justify-end">
                        <span class=move || {
                            format!(
                                "input-char-counter {}",
                                if char_count() > max { "over-limit" } else { "" },
                            )
                        }>{move || format!("{}/{}", char_count(), max)}</span>
                    </div>
                }
            })}
            {error.map(|e| view! {
                <p class="form-field-error" role="alert">{e}</p>
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
