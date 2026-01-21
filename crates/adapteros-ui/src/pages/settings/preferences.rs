//! UI Preferences section component

use super::icons::CheckIcon;
use crate::components::{Card, Select, Toggle};
use crate::signals::{update_setting, use_settings, DefaultPage, Theme};
use leptos::prelude::*;

/// UI Preferences section
#[component]
pub fn PreferencesSection() -> impl IntoView {
    let settings = use_settings();

    // Local signals bound to settings
    let theme = RwSignal::new(settings.get_untracked().theme.as_str().to_string());
    let compact_mode = RwSignal::new(settings.get_untracked().compact_mode);
    let show_timestamps = RwSignal::new(settings.get_untracked().show_timestamps);
    let default_page = RwSignal::new(settings.get_untracked().default_page.as_str().to_string());

    // Save feedback
    let save_feedback = RwSignal::new(false);

    // Effect to sync theme changes
    Effect::new(move || {
        let new_theme = Theme::parse(&theme.get());
        update_setting(settings, |s| {
            s.theme = new_theme;
            s.apply_theme();
        });
    });

    // Effect to sync compact mode changes
    Effect::new(move || {
        let value = compact_mode.get();
        update_setting(settings, |s| {
            s.compact_mode = value;
        });
    });

    // Effect to sync show timestamps changes
    Effect::new(move || {
        let value = show_timestamps.get();
        update_setting(settings, |s| {
            s.show_timestamps = value;
        });
    });

    // Effect to sync default page changes
    Effect::new(move || {
        let new_page = DefaultPage::parse(&default_page.get());
        update_setting(settings, |s| {
            s.default_page = new_page;
        });
    });

    // Show save feedback briefly with proper cleanup
    // Use raw web_sys setTimeout/clearTimeout with atomic ID for Send+Sync compatibility
    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::atomic::{AtomicI32, Ordering};
        use std::sync::Arc;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let timeout_id = Arc::new(AtomicI32::new(-1));
        let timeout_id_cleanup = Arc::clone(&timeout_id);

        // Clear any pending timeout on component unmount
        on_cleanup(move || {
            let id = timeout_id_cleanup.swap(-1, Ordering::SeqCst);
            if id >= 0 {
                if let Some(window) = web_sys::window() {
                    window.clear_timeout_with_handle(id);
                }
            }
        });

        Effect::new(move || {
            let _ = theme.get();
            let _ = compact_mode.get();
            let _ = show_timestamps.get();
            let _ = default_page.get();

            save_feedback.set(true);

            // Cancel previous timeout if any
            let old_id = timeout_id.swap(-1, Ordering::SeqCst);
            if old_id >= 0 {
                if let Some(window) = web_sys::window() {
                    window.clear_timeout_with_handle(old_id);
                }
            }

            // Set new timeout to hide feedback after 2 seconds
            if let Some(window) = web_sys::window() {
                let callback = Closure::once_into_js(move || {
                    save_feedback.set(false);
                });
                if let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.unchecked_ref(),
                    2000,
                ) {
                    timeout_id.store(id, Ordering::SeqCst);
                }
            }
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    Effect::new(move || {
        let _ = theme.get();
        let _ = compact_mode.get();
        let _ = show_timestamps.get();
        let _ = default_page.get();
        save_feedback.set(true);
    });

    // Theme options
    let theme_options = vec![
        ("light".to_string(), "Light".to_string()),
        ("dark".to_string(), "Dark".to_string()),
        ("system".to_string(), "System".to_string()),
    ];

    // Default page options
    let page_options = vec![
        ("dashboard".to_string(), "Dashboard".to_string()),
        ("adapters".to_string(), "Adapters".to_string()),
        ("chat".to_string(), "Chat".to_string()),
        ("training".to_string(), "Training".to_string()),
        ("system".to_string(), "System".to_string()),
    ];

    view! {
        <div class="space-y-6 max-w-2xl">
            // Theme
            <Card title="Appearance".to_string() description="Customize the look and feel of the interface.".to_string()>
                <div class="space-y-6">
                    <Select
                        value=theme
                        options=theme_options
                        label="Theme".to_string()
                    />

                    <Toggle
                        checked=compact_mode
                        label="Compact Mode".to_string()
                        description="Reduce spacing and padding for a denser layout".to_string()
                    />

                    <Toggle
                        checked=show_timestamps
                        label="Show Timestamps".to_string()
                        description="Display timestamps in lists, messages, and activity logs".to_string()
                    />
                </div>
            </Card>

            // Navigation
            <Card title="Navigation".to_string() description="Configure navigation behavior.".to_string()>
                <Select
                    value=default_page
                    options=page_options
                    label="Default Page After Login".to_string()
                />
            </Card>

            // Save indicator
            {move || {
                if save_feedback.get() {
                    view! {
                        <div class="flex items-center gap-2 text-sm text-green-600">
                            <CheckIcon/>
                            "Changes saved automatically"
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}
