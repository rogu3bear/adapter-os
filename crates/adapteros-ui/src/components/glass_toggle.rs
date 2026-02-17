//! Toggle for the Liquid Glass theme.

use crate::signals::settings::{update_setting, use_settings};
use leptos::prelude::*;

/// Toggle control for the glass theme.
#[component]
pub fn GlassThemeToggle() -> impl IntoView {
    let settings = use_settings();

    view! {
        <button
            class=move || {
                let base = "btn btn-ghost btn-icon-sm";
                let on = settings.try_get().map(|s| s.glass_enabled).unwrap_or(true);
                let state = if on { "glass-toggle-on" } else { "glass-toggle-off" };
                format!("{} {}", base, state)
            }
            on:click=move |_| {
                update_setting(settings, |s| s.glass_enabled = !s.glass_enabled);
                settings.get_untracked().apply_glass();
            }
            title="Toggle glass theme"
            aria-pressed=move || settings.try_get().map(|s| s.glass_enabled).unwrap_or(true).to_string()
        >
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
                <path d="M12 2l1.4 4.2L18 7l-4.6 2.8L12 14l-1.4-4.2L6 7l4.6-.8L12 2z" />
                <path d="M5 16l.8 2.4L8 19l-2.2 1.4L5 23l-.8-2.6L2 19l2.2-.6L5 16z" />
            </svg>
        </button>
    }
}
