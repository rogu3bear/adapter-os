use leptos::prelude::*;

/// Shared split layout shell for chat list + conversation.
#[component]
pub fn ChatWorkspaceLayout(children: Children) -> impl IntoView {
    view! { <div class="flex h-full min-h-0">{children()}</div> }
}
