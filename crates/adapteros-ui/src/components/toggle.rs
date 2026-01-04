//! Toggle/Switch component

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
                    let base = "peer inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50";
                    let bg = if checked.get() {
                        "bg-primary"
                    } else {
                        "bg-input"
                    };
                    format!("{} {}", base, bg)
                }
                on:click=toggle
            >
                <span
                    class=move || {
                        let base = "pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform";
                        let translate = if checked.get() {
                            "translate-x-5"
                        } else {
                            "translate-x-0"
                        };
                        format!("{} {}", base, translate)
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
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let base_class = "flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

    let full_class = format!("{} {}", base_class, class);

    view! {
        <div class="grid w-full gap-1.5">
            {label.map(|l| view! {
                <label class="label">
                    {l}
                </label>
            })}
            <select
                class=full_class
                disabled=disabled
                prop:value=move || value.get()
                on:change=move |ev| {
                    value.set(event_target_value(&ev));
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
