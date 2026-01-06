//! Global search trigger components.

use crate::signals::use_search;
use leptos::prelude::*;

/// Button that opens the command palette.
#[component]
pub fn SearchTriggerButton(#[prop(optional, into)] placeholder: Option<String>) -> impl IntoView {
    let search = use_search();
    let label = placeholder.unwrap_or_else(|| "Search...".to_string());

    view! {
        <button
            class="search-trigger"
            on:click=move |_| search.open()
            aria-label="Open search"
        >
            <span class="search-trigger-icon">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="w-4 h-4"
                >
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
            </span>
            <span class="search-trigger-label">{label}</span>
            <span class="search-trigger-kbd">
                <kbd>"Ctrl"</kbd>
                <kbd>"K"</kbd>
            </span>
        </button>
    }
}

/// Global search box used in the top bar.
#[component]
pub fn GlobalSearchBox() -> impl IntoView {
    view! {
        <div class="global-search">
            <SearchTriggerButton/>
        </div>
    }
}
