use leptos::prelude::*;

/// Shared wrapper for the session list rail.
#[component]
pub fn ChatSessionListShell(children: Children) -> impl IntoView {
    view! {
        <div class="chat-session-sidebar border-r border-border flex-shrink-0 flex flex-col h-full overflow-hidden">
            {children()}
        </div>
    }
}
