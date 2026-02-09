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

use crate::components::{PageBreadcrumbItem, PageScaffold, PageScaffoldActions, TabNav, TabPanel};
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
        <PageScaffold
            title="Settings"
            subtitle="Manage your profile, preferences, and API configuration."
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Org", "/settings"),
                PageBreadcrumbItem::current("Settings"),
            ]
        >
            <PageScaffoldActions slot>
                <div class="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-xs text-muted-foreground">
                    <span class="inline-flex h-2 w-2 rounded-full bg-emerald-500"></span>
                    "Settings auto-save"
                </div>
            </PageScaffoldActions>

            <TabNav
                tabs=vec![
                    ("profile", "Profile"),
                    ("preferences", "Preferences"),
                    ("api", "API"),
                    ("system", "System"),
                ]
                active=active_tab
            />

            <TabPanel tab="profile" active=active_tab>
                <ProfileSection/>
            </TabPanel>

            <TabPanel tab="preferences" active=active_tab>
                <PreferencesSection/>
            </TabPanel>

            <TabPanel tab="api" active=active_tab>
                <ApiConfigSection/>
            </TabPanel>

            <TabPanel tab="system" active=active_tab>
                <SystemInfoSection/>
            </TabPanel>

            // Scope info footer
            <div class="rounded-lg border border-border bg-muted/30 p-4">
                <h3 class="text-sm font-semibold mb-2">"Scope"</h3>
                <ul class="space-y-1 text-xs text-muted-foreground">
                    <li>"Preferences are stored in this browser only."</li>
                    <li>"Profile fields are read-only today."</li>
                    <li>"API changes apply immediately."</li>
                </ul>
            </div>
        </PageScaffold>
    }
}
