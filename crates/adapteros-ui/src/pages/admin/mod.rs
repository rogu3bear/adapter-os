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

use crate::components::{Badge, TabButton};
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
    let active_tab = RwSignal::new("users");

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
                            tab="users"
                            label="Users".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="roles"
                            label="Roles".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="keys"
                            label="API Keys".to_string()
                            active=active_tab
                        />
                        <TabButton
                            tab="org"
                            label="Organization".to_string()
                            active=active_tab
                        />
                    </nav>
                </div>

                // Tab content
                <div class="py-4">
                    {move || {
                        match active_tab.get() {
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
