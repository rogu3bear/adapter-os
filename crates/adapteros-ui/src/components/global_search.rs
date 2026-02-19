//! Global search trigger components.

use crate::signals::use_search;
use leptos::prelude::*;

/// Detect if the user is on macOS (for keyboard shortcut display)
fn is_macos() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(navigator) = window.navigator().platform() {
                return navigator.to_lowercase().contains("mac");
            }
        }
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        cfg!(target_os = "macos")
    }
}

/// Button that opens the command palette.
#[component]
pub fn SearchTriggerButton(#[prop(optional, into)] placeholder: Option<String>) -> impl IntoView {
    let search = use_search();
    let label = placeholder.unwrap_or_else(|| "Open Command Deck...".to_string());

    // Determine platform-appropriate modifier key
    let modifier_key = if is_macos() { "\u{2318}" } else { "Ctrl" };

    view! {
        <button
            class="search-trigger"
            on:click=move |_| search.open()
            aria-label="Open Command Deck"
            title="Open Command Deck"
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
            <span class="search-trigger-kbd" aria-label=format!("{} K to open Command Deck", modifier_key)>
                <kbd>{modifier_key}</kbd>
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
