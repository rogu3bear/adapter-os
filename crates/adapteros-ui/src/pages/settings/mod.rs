//! Settings page
//!
//! Complete settings page with Profile, API Configuration, UI Preferences,
//! and System Info sections. Settings persist to localStorage.

mod api_config;
mod preferences;
mod profile;
mod system_info;

use crate::components::TabButton;
use api_config::ApiConfigSection;
use leptos::prelude::*;
use preferences::PreferencesSection;
use profile::ProfileSection;
use system_info::SystemInfoSection;
pub use api_config::ApiConfigSection as SettingsApiConfigSection;
pub use preferences::PreferencesSection as SettingsPreferencesSection;
pub use profile::ProfileSection as SettingsProfileSection;

/// Settings page
#[component]
pub fn Settings() -> impl IntoView {
    // Active tab state
    let active_tab = RwSignal::new("profile");

    view! {
        <div class="p-6 space-y-6">
            <h1 class="text-3xl font-bold tracking-tight">"Settings"</h1>

                // Tab navigation
                <div class="border-b">
                    <nav class="-mb-px flex space-x-8" role="tablist">
                        <TabButton
                            tab="profile"
                            label="Profile".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="api"
                            label="API Configuration".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="preferences"
                            label="UI Preferences".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="system"
                            label="System Info".to_string()
                            active=active_tab
                        />
                    </nav>
                </div>

                // Tab content
                <div class="py-4">
                    {move || {
                        match active_tab.get() {
                            "profile" => view! { <ProfileSection/> }.into_any(),
                            "api" => view! { <ApiConfigSection/> }.into_any(),
                            "preferences" => view! { <PreferencesSection/> }.into_any(),
                            "system" => view! { <SystemInfoSection/> }.into_any(),
                            _ => view! { <ProfileSection/> }.into_any(),
                        }
                    }}
                </div>
        </div>
    }
}
