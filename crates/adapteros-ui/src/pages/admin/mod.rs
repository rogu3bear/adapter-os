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
        <PageScaffold
            title="Administration"
            subtitle="Manage users, roles, and organization settings"
            breadcrumbs=vec![
                PageBreadcrumbItem::new("Org", "/admin"),
                PageBreadcrumbItem::current("Administration"),
            ]
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
