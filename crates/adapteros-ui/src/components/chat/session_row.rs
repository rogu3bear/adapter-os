use leptos::prelude::*;

/// Shared row wrapper for session list entries.
#[component]
pub fn ChatSessionRowShell(class: Signal<String>, children: Children) -> impl IntoView {
    view! { <div class=move || class.try_get().unwrap_or_else(|| "chat-session-row".to_string())>{children()}</div> }
}
