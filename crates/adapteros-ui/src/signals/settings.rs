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
    #[default]
    Light,
    Dark,
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

/// Interface density preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "compact" => Self::Compact,
            _ => Self::Comfortable,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Comfortable => "Comfortable",
            Self::Compact => "Compact",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }

    pub fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
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
            DefaultPage::Dashboard => "Home",
            DefaultPage::Adapters => "Adapters",
            DefaultPage::Chat => "Chat",
            DefaultPage::Training => "Build",
            DefaultPage::System => "System",
        }
    }
}

/// Trust provenance display mode for chat messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustDisplay {
    #[default]
    Full,
    Compact,
    Off,
}

impl TrustDisplay {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "compact" => Self::Compact,
            "off" => Self::Off,
            _ => Self::Full,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Compact => "Compact",
            Self::Off => "Off",
        }
    }
}

/// User settings that persist to localStorage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    /// Theme preference (light/dark/system)
    pub theme: Theme,
    /// Legacy compact mode flag kept for localStorage back-compat.
    pub compact_mode: bool,
    /// UI density preference (comfortable/compact)
    #[serde(default)]
    pub density: Density,
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
    /// Glass theme enabled (Liquid Glass morphism)
    #[serde(default = "default_glass_enabled")]
    pub glass_enabled: bool,
    /// Trust provenance display level
    #[serde(default)]
    pub trust_display: TrustDisplay,
    /// Persistent knowledge collection used as base for chat sessions.
    #[serde(default)]
    pub knowledge_collection_id: Option<String>,
}

fn default_glass_enabled() -> bool {
    true
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            compact_mode: false,
            density: Density::Comfortable,
            show_timestamps: true,
            default_page: DefaultPage::Chat,
            api_endpoint: None,
            show_telemetry_overlay: false,
            ui_profile: None,
            glass_enabled: true,
            trust_display: TrustDisplay::Full,
            knowledge_collection_id: None,
        }
    }
}

impl UserSettings {
    /// Load settings from localStorage
    pub fn load() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let mut settings: Self = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(SETTINGS_KEY).ok().flatten())
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            // Migrate old aos_glass_theme localStorage key into unified settings
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
            {
                if let Ok(Some(old_value)) = storage.get_item("aos_glass_theme") {
                    settings.glass_enabled = old_value == "true";
                    settings.save();
                    let _ = storage.remove_item("aos_glass_theme");
                }
            }

            // Backward compatibility: old settings used compact_mode only.
            if settings.density == Density::Comfortable && settings.compact_mode {
                settings.density = Density::Compact;
            }
            settings.compact_mode = settings.density.is_compact();

            settings
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

    /// Apply glass theme class to document body
    pub fn apply_glass(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(body) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
        {
            let _ = if self.glass_enabled {
                body.class_list().add_1("theme-glass")
            } else {
                body.class_list().remove_1("theme-glass")
            };
        }
    }

    /// Apply density attribute to the root document element.
    pub fn apply_density(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(html) = document.document_element() {
                let _ = html.set_attribute("data-density", self.density.as_str());
            }
        }
    }
}

/// Settings context type
pub type SettingsContext = RwSignal<UserSettings>;

/// Provide settings context to the application
pub fn provide_settings_context() {
    let settings = RwSignal::new(UserSettings::load());

    // Apply visual state on initial load
    settings.get_untracked().apply_theme();
    settings.get_untracked().apply_glass();
    settings.get_untracked().apply_density();

    provide_context(settings);

    // Listen for OS color scheme changes when theme is System
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        if let Some(mql) = web_sys::window()
            .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        {
            let cb = Closure::<dyn Fn()>::new(move || {
                let current = settings.get_untracked();
                if current.theme == Theme::System {
                    current.apply_theme();
                }
            });
            let _ = mql.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref());
            cb.forget(); // intentional leak — lives for app lifetime
        }
    }
}

/// Use settings context
pub fn use_settings() -> SettingsContext {
    expect_context::<SettingsContext>()
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
        // Keep legacy compact_mode synchronized for backwards compatibility.
        s.compact_mode = s.density.is_compact();
        s.apply_density();
        s.save();
    });
}

#[cfg(test)]
mod tests {
    use super::{DefaultPage, Density, Theme, UserSettings};

    #[test]
    fn user_settings_default_theme_is_light() {
        assert_eq!(UserSettings::default().theme, Theme::Light);
    }

    #[test]
    fn user_settings_default_page_is_chat() {
        assert_eq!(UserSettings::default().default_page, DefaultPage::Chat);
    }

    #[test]
    fn user_settings_default_density_is_comfortable() {
        assert_eq!(UserSettings::default().density, Density::Comfortable);
    }
}
