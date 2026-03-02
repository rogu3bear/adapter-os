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

use crate::components::{
    Badge, PageBreadcrumbItem, PageScaffold, PageScaffoldActions, TabNav, TabPanel,
};
use crate::signals::use_auth;
use api_keys::ApiKeysSection;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use org::OrgSection;
use roles::RolesSection;
use users::UsersSection;

const VALID_TABS: &[&str] = &["users", "roles", "keys", "org"];

fn tab_from_query(value: Option<String>) -> &'static str {
    match value.as_deref() {
        Some("roles") => "roles",
        Some("keys") => "keys",
        Some("org") => "org",
        _ => "users",
    }
}

/// Admin page for user and role management
#[component]
pub fn Admin() -> impl IntoView {
    let (auth_state, _) = use_auth();

    // Read initial tab from URL query parameter
    let query = use_query_map();
    let initial_tab = tab_from_query(
        query
            .try_get_untracked()
            .and_then(|q| q.get("tab").filter(|t| VALID_TABS.contains(&t.as_str()))),
    );

    let active_tab = RwSignal::new(initial_tab);

    // Sync tab changes to URL
    let navigate = leptos_router::hooks::use_navigate();
    let synced = RwSignal::new(false);
    Effect::new(move || {
        let tab = active_tab.get();
        // Skip the initial effect run to avoid replacing the URL on mount
        if !synced.get_untracked() {
            synced.set(true);
            return;
        }
        let path = if tab == "users" {
            "/admin".to_string()
        } else {
            format!("/admin?tab={}", tab)
        };
        navigate(&path, Default::default());
    });

    view! {
        <PageScaffold
            title="Administration"
            subtitle="Manage users, roles, and organization settings"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("System", "/admin"),
                PageBreadcrumbItem::current("Administration"),
            ]
            full_width=true
        >
            <PageScaffoldActions slot>
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
            </PageScaffoldActions>

            <TabNav
                tabs=vec![
                    ("users", "Users"),
                    ("roles", "Roles"),
                    ("keys", "API Keys"),
                    ("org", "Organization"),
                ]
                active=active_tab
            />

            <TabPanel tab="users" active=active_tab>
                <UsersSection/>
            </TabPanel>

            <TabPanel tab="roles" active=active_tab>
                <RolesSection/>
            </TabPanel>

            <TabPanel tab="keys" active=active_tab>
                <ApiKeysSection/>
            </TabPanel>

            <TabPanel tab="org" active=active_tab>
                <OrgSection/>
            </TabPanel>
        </PageScaffold>
    }
}
