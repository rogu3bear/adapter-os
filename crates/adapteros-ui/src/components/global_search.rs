//! Global Search component
//!
//! An inline search box in the TopBar that opens the Command Palette.

use crate::signals::search::use_search;
use leptos::prelude::*;

/// Global search box for the TopBar
///
/// Clicking this opens the full Command Palette.
#[component]
pub fn GlobalSearchBox() -> impl IntoView {
    let search = use_search();

    // Detect platform for keyboard hint
    let is_mac = {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .map(|w| w.navigator())
                .and_then(|n| n.platform().ok())
                .map(|p| p.to_lowercase().contains("mac"))
                .unwrap_or(false)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    };

    let modifier_key = if is_mac { "⌘" } else { "Ctrl" };

    view! {
        <button
            class="flex items-center gap-2 px-3 py-1.5 rounded-md border border-border/50 bg-muted/30 hover:bg-muted/50 text-muted-foreground text-sm transition-colors search-trigger"
            on:click=move |_| search.open()
            title="Open command palette"
        >
            // Search icon
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
            </svg>

            // Placeholder text
            <span class="flex-1 text-left truncate">"Search..."</span>

            // Keyboard hint
            <kbd class="hidden sm:inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-background border border-border/50 text-2xs font-mono">
                <span>{modifier_key}</span>
                <span>"K"</span>
            </kbd>
        </button>
    }
}

/// Compact search trigger for mobile
#[component]
pub fn SearchTriggerButton() -> impl IntoView {
    let search = use_search();

    view! {
        <button
            class="p-1.5 rounded-md hover:bg-muted/50 text-muted-foreground hover:text-foreground transition-colors"
            on:click=move |_| search.open()
            title="Search (Ctrl+K)"
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-5 w-5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"/>
            </svg>
        </button>
    }
}
