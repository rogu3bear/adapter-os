//! Settings state management with localStorage persistence
//!
//! Provides reactive settings that persist to localStorage.

use adapteros_api_types::UiProfile;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
const SETTINGS_KEY: &str = "adapteros_settings";

/// Theme preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }
}

/// Default page on login
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultPage {
    #[default]
    Dashboard,
    Adapters,
    Chat,
    Training,
    System,
}

impl DefaultPage {
    pub fn as_str(&self) -> &'static str {
        match self {
            DefaultPage::Dashboard => "dashboard",
            DefaultPage::Adapters => "adapters",
            DefaultPage::Chat => "chat",
            DefaultPage::Training => "training",
            DefaultPage::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "adapters" => DefaultPage::Adapters,
            "chat" => DefaultPage::Chat,
            "training" => DefaultPage::Training,
            "system" => DefaultPage::System,
            _ => DefaultPage::Dashboard,
        }
    }

    pub fn path(&self) -> &'static str {
        match self {
            DefaultPage::Dashboard => "/",
            DefaultPage::Adapters => "/adapters",
            DefaultPage::Chat => "/chat",
            DefaultPage::Training => "/training",
            DefaultPage::System => "/system",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            DefaultPage::Dashboard => "Dashboard",
            DefaultPage::Adapters => "Adapters",
            DefaultPage::Chat => "Chat",
            DefaultPage::Training => "Training",
            DefaultPage::System => "System",
        }
    }
}

/// User settings that persist to localStorage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    /// Theme preference (light/dark/system)
    pub theme: Theme,
    /// Compact mode for denser UI
    pub compact_mode: bool,
    /// Show timestamps in lists and messages
    pub show_timestamps: bool,
    /// Default page after login (consumed by login.rs redirect)
    pub default_page: DefaultPage,
    /// Custom API endpoint (if overridden)
    pub api_endpoint: Option<String>,
    /// Show telemetry overlay in corner (off by default for clean UI)
    #[serde(default)]
    pub show_telemetry_overlay: bool,
    /// Optional UI profile override (primary/full)
    #[serde(default)]
    pub ui_profile: Option<UiProfile>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            compact_mode: false,
            show_timestamps: true,
            default_page: DefaultPage::Dashboard,
            api_endpoint: None,
            show_telemetry_overlay: false,
            ui_profile: None,
        }
    }
}

impl UserSettings {
    /// Load settings from localStorage
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(SETTINGS_KEY).ok().flatten())
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::default()
        }
    }

    /// Save settings to localStorage
    pub fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            if let Ok(json) = serde_json::to_string(self) {
                let _ = storage.set_item(SETTINGS_KEY, &json);
            }
        }
    }

    /// Apply theme to document
    pub fn apply_theme(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(html) = document.document_element() {
                let _ = html.class_list().remove_2("light", "dark");

                match self.theme {
                    Theme::Light => {
                        let _ = html.class_list().add_1("light");
                    }
                    Theme::Dark => {
                        let _ = html.class_list().add_1("dark");
                    }
                    Theme::System => {
                        // Check system preference
                        if let Some(window) = web_sys::window() {
                            if let Ok(Some(media)) =
                                window.match_media("(prefers-color-scheme: dark)")
                            {
                                if media.matches() {
                                    let _ = html.class_list().add_1("dark");
                                } else {
                                    let _ = html.class_list().add_1("light");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Settings context type
pub type SettingsContext = RwSignal<UserSettings>;

/// Provide settings context to the application
pub fn provide_settings_context() {
    let settings = RwSignal::new(UserSettings::load());

    // Apply theme on initial load
    settings.get_untracked().apply_theme();

    provide_context(settings);
}

/// Use settings context
pub fn use_settings() -> SettingsContext {
    use_context::<SettingsContext>().unwrap_or_else(|| {
        let settings = RwSignal::new(UserSettings::load());
        provide_context(settings);
        settings
    })
}

/// Whether perf logging is enabled for UI-only diagnostics.
///
/// This is intentionally tied to the existing telemetry overlay setting
/// to avoid adding new configuration surfaces.
pub fn perf_logging_enabled() -> bool {
    UserSettings::load().show_telemetry_overlay
}

/// Update and save a single setting
pub fn update_setting<F>(settings: SettingsContext, f: F)
where
    F: FnOnce(&mut UserSettings),
{
    settings.update(|s| {
        f(s);
        s.save();
    });
}
