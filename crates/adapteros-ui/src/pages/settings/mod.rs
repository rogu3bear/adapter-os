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
mod security;
mod system_info;

use crate::components::{
    Badge, BadgeVariant, ButtonLink, ButtonVariant, Card, PageBreadcrumbItem, PageScaffold,
    PageScaffoldActions, TabNav, TabPanel,
};
use crate::constants::ui_language;
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
            title="Control Room Settings"
            subtitle="Tune workspace behavior, trust guarantees, and kernel integrations."
            breadcrumbs=vec![
                PageBreadcrumbItem::label("Org"),
                PageBreadcrumbItem::current("Settings"),
            ]
        >
            <PageScaffoldActions slot>
                <div class="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-xs text-muted-foreground">
                    <span class="inline-flex h-2 w-2 rounded-full bg-emerald-500"></span>
                    "Changes auto-save"
                </div>
            </PageScaffoldActions>

            <TabNav
                tabs=vec![
                    ("profile", "Operator Profile"),
                    ("preferences", "Workspace"),
                    ("api", "Network"),
                    ("security", "Safety"),
                    ("system", "Kernel"),
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

            <TabPanel tab="security" active=active_tab>
                <security::SecuritySection/>
            </TabPanel>

            <TabPanel tab="system" active=active_tab>
                <SystemInfoSection/>
                <KernelSettingsSection/>
            </TabPanel>

            // Scope info footer
            <div class="rounded-lg border border-border bg-muted/30 p-4">
                <h3 class="text-sm font-semibold mb-2">"How these settings apply"</h3>
                <ul class="space-y-1 text-xs text-muted-foreground">
                    <li>"Workspace preferences are stored in this browser only."</li>
                    <li>"Profile values are read-only in this release."</li>
                    <li>"Network and safety updates apply immediately."</li>
                </ul>
            </div>
        </PageScaffold>
    }
}

#[component]
fn KernelSettingsSection() -> impl IntoView {
    view! {
        <Card>
            <div class="flex items-start justify-between gap-4">
                <div class="space-y-2">
                    <div class="flex items-center gap-2">
                        <h3 class="text-sm font-semibold">{ui_language::BASE_MODEL_REGISTRY}</h3>
                        <Badge variant=BadgeVariant::Outline>{ui_language::REPRODUCIBLE_MODE}</Badge>
                    </div>
                    <p class="text-xs text-muted-foreground">
                        "Register and validate base models before activation. Compatibility checks are enforced before live traffic."
                    </p>
                    <p class="text-xs text-muted-foreground">
                        "Use this to confirm update readiness and preserve safe rollback behavior."
                    </p>
                </div>
                <ButtonLink
                    href="/models"
                    variant=ButtonVariant::Primary
                >
                    {ui_language::REGISTER_NEW_BASE}
                </ButtonLink>
            </div>
        </Card>
    }
}
