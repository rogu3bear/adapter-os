//! Input component

use leptos::prelude::*;

/// Input component
#[component]
pub fn Input(
    #[prop(optional, into)] value: RwSignal<String>,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] label: String,
    #[prop(optional, into)] input_type: String,
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] error: Option<String>,
) -> impl IntoView {
    let base_class = "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

    let error_class = if error.is_some() {
        "border-destructive"
    } else {
        ""
    };

    let full_class = format!("{} {} {}", base_class, error_class, class);

    let input_type_val = if input_type.is_empty() {
        "text".to_string()
    } else {
        input_type
    };

    view! {
        <div class="grid w-full gap-1.5">
            {if !label.is_empty() {
                Some(view! {
                    <label class="label">
                        {label.clone()}
                    </label>
                })
            } else {
                None
            }}
            <input
                type=input_type_val
                class=full_class
                placeholder=placeholder
                disabled=disabled
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
            {error.map(|e| view! {
                <p class="text-sm text-destructive">{e}</p>
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
    #[prop(optional)] disabled: bool,
    #[prop(optional)] rows: Option<u32>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let base_class = "flex min-h-textarea w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

    let full_class = format!("{} {}", base_class, class);

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label">
                    {l}
                </label>
            })}
            <textarea
                class=full_class
                placeholder=placeholder
                disabled=disabled
                rows=rows.unwrap_or(3)
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
        </div>
    }
}
