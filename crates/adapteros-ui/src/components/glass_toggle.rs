//! Glass Theme Toggle Component
//!
//! PRD-UI-100: Toggles the `.theme-glass` class on the document body
//! to enable/disable the liquid glass morphism theme.

use leptos::prelude::*;

/// Local storage key for persisting glass theme preference
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const GLASS_THEME_KEY: &str = "aos-glass-theme";

/// Glass theme toggle component
///
/// Toggles the `.theme-glass` class on the document body to enable
/// the liquid glass morphism visual effects. State is persisted to
/// localStorage for consistency across page reloads.
#[component]
pub fn GlassThemeToggle(
    /// Optional additional CSS class
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    // Initialize from localStorage or default to off
    let (is_enabled, set_is_enabled) = signal(read_glass_preference());

    // Apply initial state on mount
    Effect::new(move || {
        let enabled = is_enabled.get();
        apply_glass_class(enabled);
    });

    // Toggle handler
    let toggle = move |_| {
        set_is_enabled.update(|v| {
            let new_value = !*v;
            *v = new_value;
            save_glass_preference(new_value);
            apply_glass_class(new_value);
        });
    };

    view! {
        <button
            type="button"
            class=format!(
                "flex items-center gap-2 px-2 py-1 rounded-md text-xs transition-colors hover:bg-muted/50 {}",
                class
            )
            on:click=toggle
            title="Toggle glass theme"
            aria-pressed=move || is_enabled.get().to_string()
        >
            // Glass icon (stylized layers)
            <svg
                class="w-3.5 h-3.5"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                stroke-width="1.5"
            >
                // Outer layer
                <rect
                    x="3"
                    y="3"
                    width="18"
                    height="18"
                    rx="3"
                    stroke-opacity=move || if is_enabled.get() { "1" } else { "0.5" }
                />
                // Middle layer (offset for depth)
                <rect
                    x="6"
                    y="6"
                    width="12"
                    height="12"
                    rx="2"
                    stroke-opacity=move || if is_enabled.get() { "0.7" } else { "0.3" }
                />
                // Inner layer
                <rect
                    x="9"
                    y="9"
                    width="6"
                    height="6"
                    rx="1"
                    fill=move || if is_enabled.get() { "currentColor" } else { "none" }
                    fill-opacity="0.3"
                />
            </svg>

            // Label
            <span class="hidden sm:inline text-muted-foreground">
                "Glass: "
                <span class=move || {
                    if is_enabled.get() {
                        "text-primary font-medium"
                    } else {
                        "text-muted-foreground"
                    }
                }>
                    {move || if is_enabled.get() { "On" } else { "Off" }}
                </span>
            </span>
        </button>
    }
}

/// Read glass theme preference from localStorage
fn read_glass_preference() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(GLASS_THEME_KEY).ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Save glass theme preference to localStorage
fn save_glass_preference(enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item(GLASS_THEME_KEY, if enabled { "true" } else { "false" });
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = enabled; // Silence unused warning
    }
}

/// Apply or remove the theme-glass class on the document body
fn apply_glass_class(enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(body) = document.body() {
                let class_list = body.class_list();
                if enabled {
                    let _ = class_list.add_1("theme-glass");
                } else {
                    let _ = class_list.remove_1("theme-glass");
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = enabled; // Silence unused warning
    }
}
