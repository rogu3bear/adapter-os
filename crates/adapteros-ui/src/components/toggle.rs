//! Toggle/Switch component

use crate::components::form_field::use_form_field_context;
use leptos::prelude::*;

/// Toggle component (switch)
#[component]
pub fn Toggle(
    #[prop(into)] checked: RwSignal<bool>,
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] label: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let toggle = move |_| {
        if !disabled {
            checked.update(|v| *v = !*v);
        }
    };

    view! {
        <div class=format!("flex items-center justify-between {}", class)>
            <div class="space-y-0.5">
                {label.map(|l| view! {
                    <label class="label">
                        {l}
                    </label>
                })}
                {description.map(|d| view! {
                    <p class="text-sm text-muted-foreground">{d}</p>
                })}
            </div>
            <button
                type="button"
                role="switch"
                aria-checked=move || checked.get().to_string()
                disabled=disabled
                class=move || {
                    let base = "toggle";
                    let state = if checked.get() { "toggle-on" } else { "toggle-off" };
                    format!("{} {}", base, state)
                }
                on:click=toggle
            >
                <span
                    class=move || {
                        let base = "toggle-thumb";
                        let state = if checked.get() {
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
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] class: String,
    #[prop(optional)] on_change: Option<Callback<String>>,
) -> impl IntoView {
    let full_class = format!("select {}", class);
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
            <select
                id=input_id
                name=name
                class=full_class
                disabled=disabled
                aria-describedby=described_by
                prop:value=move || value.get()
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
                        <option value=val selected=move || value.get() == val_clone>
                            {label}
                        </option>
                    }
                }).collect::<Vec<_>>()}
            </select>
        </div>
    }
}
