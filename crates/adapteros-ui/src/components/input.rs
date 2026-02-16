//! Input component

use crate::components::form_field::use_form_field_context;
use leptos::prelude::*;
use uuid::Uuid;

/// Input component with validation states
#[component]
pub fn Input(
    #[prop(optional, into)] value: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] input_type: String,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
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
    /// Callback fired on blur (for validation timing)
    #[prop(optional)]
    on_blur: Option<Callback<()>>,
    /// Hint text displayed below the input
    #[prop(optional, into)]
    hint: Option<String>,
    /// Accessible label for inputs without visible label (layout constraints)
    #[prop(optional, into)]
    aria_label: Option<String>,
    #[prop(optional, into)] data_testid: Option<String>,
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
    let input_id = id
        .or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()))
        .or_else(|| Some(format!("input-{}", Uuid::new_v4().simple())));
    let has_field_context = field_ctx.is_some(); // FormField provides its own label
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    // Character counter
    let char_count = move || value.try_get().unwrap_or_default().len();

    // Determine if we have any form of accessible label
    let has_visible_label = label.is_some();
    let effective_aria_label = if has_visible_label || has_field_context {
        None // Label is visible, no need for aria-label
    } else {
        aria_label.clone()
    };

    // Build hint ID for aria-describedby if hint is provided
    let hint_id = hint
        .as_ref()
        .map(|_| format!("{}-hint", input_id.as_deref().unwrap_or("input"),));

    // Combine described_by with hint_id
    let full_described_by = match (&described_by, &hint_id) {
        (Some(d), Some(h)) => Some(format!("{} {}", d, h)),
        (Some(d), None) => Some(d.clone()),
        (None, Some(h)) => Some(h.clone()),
        (None, None) => None,
    };

    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label" for=input_id.clone()>
                    {l}
                    {required.then(|| view! {
                        <span class="form-field-required" aria-hidden="true">"*"</span>
                    })}
                </label>
            })}
            <div class="relative">
                <input
                    id=input_id
                    name=name
                    type=input_type_val
                    class=full_class
                    placeholder=placeholder
                    disabled=move || disabled.try_get().unwrap_or(false)
                    required=required
                    maxlength=max_length.map(|n| n.to_string())
                    aria-invalid=error.is_some().to_string()
                    aria-describedby=full_described_by
                    aria-label=effective_aria_label
                    data-testid=move || data_testid.clone()
                    prop:value=move || value.try_get().unwrap_or_default()
                    on:input=move |ev| {
                        value.set(event_target_value(&ev));
                    }
                    on:blur=move |_| {
                        if let Some(ref cb) = on_blur {
                            cb.run(());
                        }
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
            {hint.clone().map(|text| {
                let id = hint_id.clone();
                view! {
                    <p id=id class="form-field-hint text-xs text-muted-foreground">{text}</p>
                }
            })}
            {max_length.map(|max| {
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
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional)] rows: Option<u32>,
    #[prop(optional, into)] class: String,
    /// Accessible label for textareas without visible label (layout constraints)
    #[prop(optional, into)]
    aria_label: Option<String>,
    #[prop(optional, into)] data_testid: Option<String>,
    /// Mark field as required (shows * indicator)
    #[prop(optional)]
    required: bool,
    /// Callback fired on blur (for validation timing)
    #[prop(optional)]
    on_blur: Option<Callback<()>>,
    /// Callback fired on keydown (for Enter-to-submit handling)
    #[prop(optional)]
    on_keydown: Option<Callback<web_sys::KeyboardEvent>>,
    /// Hint text displayed below the textarea
    #[prop(optional, into)]
    hint: Option<String>,
    /// Error message to display
    #[prop(optional, into)]
    error: Option<String>,
) -> impl IntoView {
    let base_class = "input input-textarea";

    let state_class = if error.is_some() { "input-error" } else { "" };

    let full_class = format!("{} {} {}", base_class, state_class, class);

    let field_ctx = use_form_field_context();
    let input_id = id
        .or_else(|| field_ctx.as_ref().map(|ctx| ctx.field_id.clone()))
        .or_else(|| Some(format!("textarea-{}", Uuid::new_v4().simple())));
    let has_field_context = field_ctx.is_some();
    let described_by = field_ctx.and_then(|ctx| ctx.described_by.clone());

    // Determine if we have any form of accessible label
    let has_visible_label = label.is_some();
    let effective_aria_label = if has_visible_label || has_field_context {
        None // Label is visible, no need for aria-label
    } else {
        aria_label.clone()
    };

    // Build hint ID for aria-describedby if hint is provided
    let hint_id = hint
        .as_ref()
        .map(|_| format!("{}-hint", input_id.as_deref().unwrap_or("textarea"),));

    // Combine described_by with hint_id
    let full_described_by = match (&described_by, &hint_id) {
        (Some(d), Some(h)) => Some(format!("{} {}", d, h)),
        (Some(d), None) => Some(d.clone()),
        (None, Some(h)) => Some(h.clone()),
        (None, None) => None,
    };

    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label" for=input_id.clone()>
                    {l}
                    {required.then(|| view! {
                        <span class="form-field-required" aria-hidden="true">"*"</span>
                    })}
                </label>
            })}
            <textarea
                id=input_id
                name=name
                class=full_class
                placeholder=placeholder
                disabled=move || disabled.try_get().unwrap_or(false)
                required=required
                rows=rows.unwrap_or(3)
                aria-label=effective_aria_label
                aria-describedby=full_described_by
                aria-invalid=error.is_some().to_string()
                data-testid=move || data_testid.clone()
                prop:value=move || value.try_get().unwrap_or_default()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
                on:blur=move |_| {
                    if let Some(ref cb) = on_blur {
                        cb.run(());
                    }
                }
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if let Some(ref cb) = on_keydown {
                        cb.run(ev);
                    }
                }
            />
            {hint.clone().map(|text| {
                let id = hint_id.clone();
                view! {
                    <p id=id class="form-field-hint text-xs text-muted-foreground">{text}</p>
                }
            })}
            {error.map(|e| view! {
                <p class="form-field-error" role="alert">{e}</p>
            })}
        </div>
    }
}
