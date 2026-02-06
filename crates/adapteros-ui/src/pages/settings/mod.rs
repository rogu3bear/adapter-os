//! Settings page
//!
//! Complete settings page with Profile, API Configuration, UI Preferences,
//! and System Info sections. Settings persist to localStorage.
//!
//! This is the canonical location for all user/account settings. The /user
//! route redirects here for backward compatibility.

mod api_config;
mod preferences;
mod profile;
mod system_info;

use crate::components::TabButton;
use api_config::ApiConfigSection;
pub use api_config::ApiConfigSection as SettingsApiConfigSection;
use leptos::prelude::*;
use preferences::PreferencesSection;
pub use preferences::PreferencesSection as SettingsPreferencesSection;
use profile::ProfileSection;
pub use profile::ProfileSection as SettingsProfileSection;
use system_info::SystemInfoSection;

/// Settings page - canonical location for all user/account settings
#[component]
pub fn Settings() -> impl IntoView {
    // Active tab state
    let active_tab = RwSignal::new("profile");

    view! {
        <div class="p-6 space-y-6">
            // Header with title and auto-save indicator
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="heading-1">"Settings"</h1>
                    <p class="text-sm text-muted-foreground">
                        "Manage your profile, preferences, and API configuration."
                    </p>
                </div>
                <div class="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-xs text-muted-foreground">
                    <span class="inline-flex h-2 w-2 rounded-full bg-emerald-500"></span>
                    "Settings auto-save"
                </div>
            </div>

            // Tab navigation
            <div class="border-b border-border">
                <nav class="-mb-px flex space-x-8" role="tablist">
                    <TabButton
                        tab="profile"
                        label="Profile".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="preferences"
                        label="Preferences".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="api"
                        label="API".to_string()
                        active=active_tab
                    />
                    <TabButton
                        tab="system"
                        label="System".to_string()
                        active=active_tab
                    />
                </nav>
            </div>

            // Tab content
            <div class="py-4">
                {move || {
                    match active_tab.get() {
                        "profile" => view! { <ProfileSection/> }.into_any(),
                        "preferences" => view! { <PreferencesSection/> }.into_any(),
                        "api" => view! { <ApiConfigSection/> }.into_any(),
                        "system" => view! { <SystemInfoSection/> }.into_any(),
                        _ => view! { <ProfileSection/> }.into_any(),
                    }
                }}
            </div>

            // Scope info footer
            <div class="rounded-lg border border-border bg-muted/30 p-4">
                <h3 class="text-sm font-semibold mb-2">"Scope"</h3>
                <ul class="space-y-1 text-xs text-muted-foreground">
                    <li>"Preferences are stored in this browser only."</li>
                    <li>"Profile fields are read-only today."</li>
                    <li>"API changes apply immediately."</li>
                </ul>
            </div>
        </div>
    }
}
