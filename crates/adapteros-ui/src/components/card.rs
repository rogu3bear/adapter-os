//! Card component

use leptos::prelude::*;

/// Card component
#[component]
pub fn Card(
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional, into)] data_testid: Option<String>,
    children: Children,
) -> impl IntoView {
    let full_class = format!("card {}", class);
    let has_header = title.is_some() || description.is_some();
    let content_class = if has_header {
        "card-content"
    } else {
        "card-content card-content--full"
    };
    let data_testid = data_testid.filter(|value| !value.is_empty());

    view! {
        <div class=full_class data-testid=move || data_testid.clone()>
            {move || {
                if has_header {
                    view! {
                        <div class="card-header">
                            {title.clone().map(|t| view! {
                                <h3 class="card-title">{t}</h3>
                            })}
                            {description.clone().map(|d| view! {
                                <p class="card-description">{d}</p>
                            })}
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
            <div class=content_class>
                {children()}
            </div>
        </div>
    }
}
