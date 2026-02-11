//! Toggle for the Liquid Glass theme.

use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "aos_glass_theme";

fn load_glass_enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            if let Ok(Some(value)) = storage.get_item(STORAGE_KEY) {
                return value == "true";
            }
        }
    }
    true
}

fn apply_glass(enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(body) = document.body() {
            let _ = if enabled {
                body.class_list().add_1("theme-glass")
            } else {
                body.class_list().remove_1("theme-glass")
            };
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = enabled;
}

fn persist_glass(enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(STORAGE_KEY, if enabled { "true" } else { "false" });
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = enabled;
}

/// Toggle control for the glass theme.
#[component]
pub fn GlassThemeToggle() -> impl IntoView {
    let enabled = RwSignal::new(load_glass_enabled());

    Effect::new(move || {
        let Some(value) = enabled.try_get() else {
            return;
        };
        apply_glass(value);
        persist_glass(value);
    });

    view! {
        <button
            class=move || {
                let base = "btn btn-ghost btn-icon-sm";
                let state = if enabled.get() { "glass-toggle-on" } else { "glass-toggle-off" };
                format!("{} {}", base, state)
            }
            on:click=move |_| enabled.update(|v| *v = !*v)
            title="Toggle glass theme"
            aria-pressed=move || enabled.get().to_string()
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
