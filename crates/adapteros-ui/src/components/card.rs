//! Card component

use leptos::prelude::*;

/// Card component
#[component]
pub fn Card(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    children: Children,
) -> impl IntoView {
    let base_class = "rounded-lg border bg-card text-card-foreground shadow-sm";
    let full_class = format!("{} {}", base_class, class);

    view! {
        <div class=full_class>
            {move || {
                if title.is_some() || description.is_some() {
                    view! {
                        <div class="flex flex-col space-y-1.5 p-6">
                            {title.clone().map(|t| view! {
                                <h3 class="text-2xl font-semibold leading-none tracking-tight">{t}</h3>
                            })}
                            {description.clone().map(|d| view! {
                                <p class="text-sm text-muted-foreground">{d}</p>
                            })}
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
            <div class="p-6 pt-0">
                {children()}
            </div>
        </div>
    }
}

/// Card header component
#[component]
pub fn CardHeader(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let full_class = format!("flex flex-col space-y-1.5 p-6 {}", class);
    view! {
        <div class=full_class>
            {children()}
        </div>
    }
}

/// Card content component
#[component]
pub fn CardContent(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let full_class = format!("p-6 pt-0 {}", class);
    view! {
        <div class=full_class>
            {children()}
        </div>
    }
}

/// Card footer component
#[component]
pub fn CardFooter(
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let full_class = format!("flex items-center p-6 pt-0 {}", class);
    view! {
        <div class=full_class>
            {children()}
        </div>
    }
}
