//! Admin page
//!
//! User and role management for administrators.
//!
//! This module is structured for future extraction into sub-modules:
//! - users.rs: UsersSection
//! - roles.rs: RolesSection  
//! - api_keys.rs: ApiKeysSection
//! - org.rs: OrgSection

mod api_keys;
mod org;
mod roles;
mod users;

use crate::components::Badge;
use crate::signals::use_auth;
use api_keys::ApiKeysSection;
use leptos::prelude::*;
use org::OrgSection;
use roles::RolesSection;
use users::UsersSection;

/// Admin page for user and role management
#[component]
pub fn Admin() -> impl IntoView {
    // Get current user info to display admin context
    let (auth_state, _) = use_auth();

    // Active tab
    let active_tab = RwSignal::new("users".to_string());

    view! {
        <div class="p-6 space-y-6">
                <div class="flex items-center justify-between">
                    <div>
                        <h1 class="text-3xl font-bold tracking-tight">"Administration"</h1>
                        <p class="text-muted-foreground mt-1">"Manage users, roles, and organization settings"</p>
                    </div>
                    {move || {
                        let state = auth_state.get();
                        if let Some(user) = state.user() {
                            let tenant = user.tenant_id.clone();
                            view! {
                                <Badge variant=crate::components::BadgeVariant::Outline>
                                    "Tenant: "{tenant}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}
                </div>

                // Tab navigation
                <div class="border-b">
                    <nav class="-mb-px flex space-x-8">
                        <TabButton
                            tab="users".to_string()
                            label="Users".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="roles".to_string()
                            label="Roles".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="keys".to_string()
                            label="API Keys".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="org".to_string()
                            label="Organization".to_string()
                            active=active_tab
                        />
                    </nav>
                </div>

                // Tab content
                <div class="py-4">
                    {move || {
                        match active_tab.get().as_str() {
                            "users" => view! { <UsersSection/> }.into_any(),
                            "roles" => view! { <RolesSection/> }.into_any(),
                            "keys" => view! { <ApiKeysSection/> }.into_any(),
                            "org" => view! { <OrgSection/> }.into_any(),
                            _ => view! { <UsersSection/> }.into_any(),
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
            class=move || {
                let base = "whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm transition-colors";
                if is_active() {
                    format!("{} border-primary text-primary", base)
                } else {
                    format!("{} border-transparent text-muted-foreground hover:text-foreground hover:border-muted", base)
                }
            }
            on:click={
                let tab = tab.clone();
                move |_| active.set(tab.clone())
            }
        >
            {label}
        </button>
    }
}
