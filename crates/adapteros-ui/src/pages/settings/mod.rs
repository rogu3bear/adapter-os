//! Settings page
//!
//! Complete settings page with Profile, API Configuration, UI Preferences,
//! and System Info sections. Settings persist to localStorage.

mod api_config;
mod icons;
mod preferences;
mod profile;
mod system_info;

use api_config::ApiConfigSection;
use leptos::prelude::*;
use preferences::PreferencesSection;
use profile::ProfileSection;
use system_info::SystemInfoSection;

/// Settings page
#[component]
pub fn Settings() -> impl IntoView {
    // Active tab state
    let active_tab = RwSignal::new("profile".to_string());

    view! {
        <div class="p-6 space-y-6">
            <h1 class="text-3xl font-bold tracking-tight">"Settings"</h1>

                // Tab navigation
                <div class="border-b">
                    <nav class="-mb-px flex space-x-8" role="tablist">
                        <TabButton
                            tab="profile".to_string()
                            label="Profile".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="api".to_string()
                            label="API Configuration".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="preferences".to_string()
                            label="UI Preferences".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="system".to_string()
                            label="System Info".to_string()
                            active=active_tab
                        />
                    </nav>
                </div>

                // Tab content
                <div class="py-4">
                    {move || {
                        match active_tab.get().as_str() {
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

/// Tab button component
#[component]
fn TabButton(tab: String, label: String, active: RwSignal<String>) -> impl IntoView {
    let tab_value = tab.clone();
    let is_active = move || active.get() == tab_value;

    view! {
        <button
            class={
                let is_active = is_active.clone();
                move || {
                    let base = "whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded-t-sm";
                    if is_active() {
                        format!("{} border-primary text-primary", base)
                    } else {
                        format!("{} border-transparent text-muted-foreground hover:text-foreground hover:border-muted", base)
                    }
                }
            }
            type="button"
            role="tab"
            aria-selected=is_active
            on:click={
                let tab = tab.clone();
                move |_| active.set(tab.clone())
            }
        >
            {label}
        </button>
    }
}
