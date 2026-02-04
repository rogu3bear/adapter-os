//! UI Preferences section component

use crate::components::IconCheck;
use crate::components::{Button, ButtonSize, ButtonVariant, Card, Select, Toggle};
use crate::signals::{update_setting, use_settings, use_ui_profile_state, DefaultPage, Theme};
use adapteros_api_types::UiProfile;
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
    let ui_profile_state = use_ui_profile_state();
    let ui_profile_value = RwSignal::new(
        settings
            .get_untracked()
            .ui_profile
            .or(ui_profile_state.get_untracked().runtime_profile)
            .unwrap_or(UiProfile::Full)
            .as_str()
            .to_string(),
    );

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

    // Keep UI profile selection in sync with runtime default when no override exists
    Effect::new(move || {
        let override_profile = settings.get().ui_profile;
        let runtime_profile = ui_profile_state.get().runtime_profile;
        if override_profile.is_none() {
            let effective = runtime_profile.unwrap_or(UiProfile::Full);
            ui_profile_value.set(effective.as_str().to_string());
        }
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
            let _ = ui_profile_value.get();

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
        let _ = ui_profile_value.get();
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

    let ui_profile_options = vec![
        ("primary".to_string(), "Primary".to_string()),
        ("full".to_string(), "Full".to_string()),
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
                <div class="mt-4 space-y-2">
                    <Select
                        value=ui_profile_value
                        options=ui_profile_options
                        label="UI Profile".to_string()
                        on_change=Callback::new(move |value: String| {
                            let profile = UiProfile::parse(&value);
                            update_setting(settings, |s| {
                                s.ui_profile = Some(profile);
                            });
                        })
                    />
                    <div class="flex items-center justify-between text-xs text-muted-foreground">
                        <span>
                            {move || {
                                ui_profile_state
                                    .get()
                                    .runtime_profile
                                    .map(|profile| format!("Server default: {}", profile.display()))
                                    .unwrap_or_else(|| "Server default: unavailable".to_string())
                            }}
                        </span>
                        {move || {
                            settings.get().ui_profile.is_some().then(|| view! {
                                <Button
                                    variant=ButtonVariant::Ghost
                                    size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| {
                                        update_setting(settings, |s| {
                                            s.ui_profile = None;
                                        });
                                    })
                                >
                                    "Use server default"
                                </Button>
                            })
                        }}
                    </div>
                </div>
            </Card>

            // Save indicator
            {move || {
                if save_feedback.get() {
                    view! {
                        <div class="flex items-center gap-2 text-sm text-status-success">
                            <IconCheck/>
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
