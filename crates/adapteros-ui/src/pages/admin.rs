//! Admin page
//!
//! User and role management for administrators.

use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Shell, Table,
    TableBody, TableCell, TableHead, TableHeader, TableRow,
};
use crate::signals::use_auth;
use leptos::prelude::*;

/// Admin page for user and role management
#[component]
pub fn Admin() -> impl IntoView {
    // Get current user info to display admin context
    let (auth_state, _) = use_auth();

    // Active tab
    let active_tab = RwSignal::new("users".to_string());

    view! {
        <Shell>
            <div class="space-y-6">
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
                                <Badge variant=BadgeVariant::Outline>
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
        </Shell>
    }
}

/// Tab button component
#[component]
fn TabButton(
    tab: String,
    label: String,
    active: RwSignal<String>,
) -> impl IntoView {
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

/// Users section - placeholder until API endpoint exists
#[component]
fn UsersSection() -> impl IntoView {
    view! {
        <Card>
            <div class="py-8 text-center">
                <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-8 w-8 text-muted-foreground"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M15 19.128a9.38 9.38 0 002.625.372 9.337 9.337 0 004.121-.952 4.125 4.125 0 00-7.533-2.493M15 19.128v-.003c0-1.113-.285-2.16-.786-3.07M15 19.128v.106A12.318 12.318 0 018.624 21c-2.331 0-4.512-.645-6.374-1.766l-.001-.109a6.375 6.375 0 0111.964-3.07M12 6.375a3.375 3.375 0 11-6.75 0 3.375 3.375 0 016.75 0zm8.25 2.25a2.625 2.625 0 11-5.25 0 2.625 2.625 0 015.25 0z"/>
                    </svg>
                </div>
                <h3 class="text-lg font-medium mb-2">"User Management"</h3>
                <p class="text-muted-foreground max-w-md mx-auto">
                    "User management API endpoint is not yet available. Once implemented, you'll be able to view, create, and manage users here."
                </p>

                // Demo data preview
                <div class="mt-6">
                    <h4 class="text-sm font-medium mb-3 text-left">"Preview (demo data)"</h4>
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>"Email"</TableHead>
                                <TableHead>"Role"</TableHead>
                                <TableHead>"Status"</TableHead>
                                <TableHead>"Last Login"</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            <TableRow>
                                <TableCell>
                                    <span class="text-muted-foreground">"admin@example.com"</span>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=BadgeVariant::Destructive>"Admin"</Badge>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=BadgeVariant::Success>"Active"</Badge>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">"2 hours ago"</span>
                                </TableCell>
                            </TableRow>
                            <TableRow>
                                <TableCell>
                                    <span class="text-muted-foreground">"dev@example.com"</span>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=BadgeVariant::Default>"Operator"</Badge>
                                </TableCell>
                                <TableCell>
                                    <Badge variant=BadgeVariant::Success>"Active"</Badge>
                                </TableCell>
                                <TableCell>
                                    <span class="text-sm text-muted-foreground">"1 day ago"</span>
                                </TableCell>
                            </TableRow>
                        </TableBody>
                    </Table>
                </div>
            </div>
        </Card>
    }
}

/// Roles section
#[component]
fn RolesSection() -> impl IntoView {
    // Define the roles with their descriptions
    let roles = vec![
        ("Admin", "Full access to all features including user management, policies, and system settings", vec![
            "Manage users and roles",
            "Configure policies",
            "Access audit logs",
            "Manage federation",
        ]),
        ("Operator", "Can run inference, training, and manage adapters. Cannot modify system settings", vec![
            "Create/cancel training jobs",
            "Load/unload models",
            "Create adapter stacks",
            "View system metrics",
        ]),
        ("Viewer", "Read-only access to dashboards and status. Cannot modify any resources", vec![
            "View dashboard",
            "View system status",
            "Run approved inferences",
            "View training jobs",
        ]),
    ];

    view! {
        <div class="grid gap-4">
            {roles.into_iter().map(|(name, desc, perms)| {
                let variant = match name {
                    "Admin" => BadgeVariant::Destructive,
                    "Operator" => BadgeVariant::Default,
                    _ => BadgeVariant::Secondary,
                };

                view! {
                    <Card>
                        <div class="flex items-start justify-between">
                            <div class="flex-1">
                                <div class="flex items-center gap-2 mb-2">
                                    <Badge variant=variant>{name}</Badge>
                                </div>
                                <p class="text-sm text-muted-foreground mb-4">{desc}</p>
                                <div class="space-y-1">
                                    {perms.into_iter().map(|perm| view! {
                                        <div class="flex items-center gap-2 text-sm">
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-4 w-4 text-green-500"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                            >
                                                <polyline points="20 6 9 17 4 12"/>
                                            </svg>
                                            <span>{perm}</span>
                                        </div>
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        </div>
                    </Card>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// API Keys section - placeholder
#[component]
fn ApiKeysSection() -> impl IntoView {
    view! {
        <Card>
            <div class="py-8 text-center">
                <div class="rounded-full bg-muted p-3 mx-auto w-fit mb-4">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-8 w-8 text-muted-foreground"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/>
                    </svg>
                </div>
                <h3 class="text-lg font-medium mb-2">"API Keys"</h3>
                <p class="text-muted-foreground max-w-md mx-auto">
                    "Generate and manage API keys for programmatic access. API key management will be available in a future update."
                </p>
                <div class="mt-4">
                    <Button variant=ButtonVariant::Outline disabled=true>
                        "Generate New Key"
                    </Button>
                </div>
            </div>
        </Card>
    }
}

/// Organization section - placeholder
#[component]
fn OrgSection() -> impl IntoView {
    let (auth_state, _) = use_auth();

    view! {
        <div class="max-w-2xl">
            <Card title="Organization Settings".to_string() description="Configure your organization's AdapterOS instance.".to_string()>
                <div class="space-y-4">
                    {move || {
                        let state = auth_state.get();
                        if let Some(user) = state.user() {
                            let tenant_id = user.tenant_id.clone();
                            let email = user.email.clone();
                            // Build tenant badges before the view! macro to avoid lifetime issues
                            let tenant_badges: Vec<_> = user.admin_tenants.iter().map(|t| {
                                let tenant = t.clone();
                                view! {
                                    <Badge variant=BadgeVariant::Outline>{tenant}</Badge>
                                }
                            }).collect();
                            view! {
                                <div class="grid gap-3 text-sm">
                                    <div class="flex justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Tenant ID"</span>
                                        <span class="font-mono text-xs">{tenant_id}</span>
                                    </div>
                                    <div class="flex justify-between py-2 border-b">
                                        <span class="text-muted-foreground">"Admin Email"</span>
                                        <span>{email}</span>
                                    </div>
                                    <div class="flex justify-between py-2">
                                        <span class="text-muted-foreground">"Admin Tenants"</span>
                                        <div class="flex gap-1">
                                            {tenant_badges}
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p class="text-muted-foreground">"Loading..."</p>
                            }.into_any()
                        }
                    }}
                </div>
            </Card>

            <Card title="Danger Zone".to_string() class="mt-6 border-destructive".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="font-medium">"Revoke All Sessions"</p>
                            <p class="text-sm text-muted-foreground">
                                "Force all users to re-authenticate. Use with caution."
                            </p>
                        </div>
                        <Button variant=ButtonVariant::Destructive disabled=true>
                            "Revoke All"
                        </Button>
                    </div>
                </div>
            </Card>
        </div>
    }
}
