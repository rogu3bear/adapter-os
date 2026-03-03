use leptos::prelude::*;

/// Shared wrapper for chat header control groups.
#[component]
pub fn ChatHeaderControls(children: Children) -> impl IntoView {
    view! { <div class="chat-header-controls">{children()}</div> }
}
