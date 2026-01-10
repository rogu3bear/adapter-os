//! Admin page
//!
//! User and role management for administrators.

use crate::api::{ApiClient, ApiKeyInfo, CreateApiKeyRequest, UserResponse};
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, ErrorDisplay, Spinner, Table, TableBody,
    TableCell, TableHead, TableHeader, TableRow,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::use_auth;
use leptos::prelude::*;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

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

/// Users section - fetches real user data from API
#[component]
fn UsersSection() -> impl IntoView {
    // Fetch users from API
    let (users, refetch) = use_api_resource(move |client: Arc<ApiClient>| async move {
        client.list_users(Some(1), Some(50)).await
    });

    let refetch_signal = StoredValue::new(refetch);

    view! {
        <Card>
            {move || {
                match users.get() {
                    LoadingState::Idle | LoadingState::Loading => {
                        view! {
                            <div class="flex items-center justify-center py-12">
                                <Spinner/>
                            </div>
                        }.into_any()
                    }
                    LoadingState::Loaded(data) => {
                        if data.users.is_empty() {
                            view! {
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
                                    <h3 class="text-lg font-medium mb-2">"No Users Found"</h3>
                                    <p class="text-muted-foreground max-w-md mx-auto">
                                        "No users are registered in the system yet."
                                    </p>
                                </div>
                            }.into_any()
                        } else {
                            let users_list: Vec<UserResponse> = data.users;
                            view! {
                                <div>
                                    <div class="flex items-center justify-between mb-4">
                                        <span class="text-sm text-muted-foreground">
                                            {format!("{} users total", data.total)}
                                        </span>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                                        >
                                            "Refresh"
                                        </Button>
                                    </div>
                                    <Table>
                                        <TableHeader>
                                            <TableRow>
                                                <TableHead>"Email"</TableHead>
                                                <TableHead>"Display Name"</TableHead>
                                                <TableHead>"Role"</TableHead>
                                                <TableHead>"Last Login"</TableHead>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            {users_list.into_iter().map(|user| {
                                                let role_variant = match user.role.as_str() {
                                                    "admin" => BadgeVariant::Destructive,
                                                    "operator" => BadgeVariant::Warning,
                                                    _ => BadgeVariant::Secondary,
                                                };
                                                let last_login = user.last_login_at.clone().unwrap_or_else(|| "Never".to_string());
                                                view! {
                                                    <TableRow>
                                                        <TableCell>
                                                            <span>{user.email.clone()}</span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span>{user.display_name.clone()}</span>
                                                        </TableCell>
                                                        <TableCell>
                                                            <Badge variant=role_variant>{user.role.clone()}</Badge>
                                                        </TableCell>
                                                        <TableCell>
                                                            <span class="text-sm text-muted-foreground">{last_login}</span>
                                                        </TableCell>
                                                    </TableRow>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </TableBody>
                                    </Table>
                                </div>
                            }.into_any()
                        }
                    }
                    LoadingState::Error(e) => {
                        view! {
                            <ErrorDisplay
                                error=e
                                on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                            />
                        }.into_any()
                    }
                }
            }}
        </Card>
    }
}

/// Roles section
#[component]
fn RolesSection() -> impl IntoView {
    // Define the roles with their descriptions
    let roles = vec![
        (
            "Admin",
            "Full access to all features including user management, policies, and system settings",
            vec![
                "Manage users and roles",
                "Configure policies",
                "Access audit logs",
                "Manage federation",
            ],
        ),
        (
            "Operator",
            "Can run inference, training, and manage adapters. Cannot modify system settings",
            vec![
                "Create/cancel training jobs",
                "Load/unload models",
                "Create adapter stacks",
                "View system metrics",
            ],
        ),
        (
            "Viewer",
            "Read-only access to dashboards and status. Cannot modify any resources",
            vec![
                "View dashboard",
                "View system status",
                "Run approved inferences",
                "View training jobs",
            ],
        ),
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

/// API Keys section - LM Studio-style API key management
#[component]
fn ApiKeysSection() -> impl IntoView {
    // Fetch API keys from API
    let (keys, refetch) =
        use_api_resource(move |client: Arc<ApiClient>| async move { client.list_api_keys().await });

    let refetch_signal = StoredValue::new(refetch);

    // Dialog state
    let show_create_dialog = RwSignal::new(false);
    let new_key_name = RwSignal::new(String::new());
    let selected_scopes = RwSignal::new(vec!["viewer".to_string()]);
    let creating = RwSignal::new(false);
    let create_error = RwSignal::new(Option::<String>::None);

    // Created key display (shown once after creation)
    let created_key_token = RwSignal::new(Option::<String>::None);
    let copied = RwSignal::new(false);

    // Revoking state
    let revoking_id = RwSignal::new(Option::<String>::None);

    // Create key handler
    let on_create = move |_| {
        let name = new_key_name.get();
        if name.trim().is_empty() {
            create_error.set(Some("Name is required".to_string()));
            return;
        }

        creating.set(true);
        create_error.set(None);

        let scopes = selected_scopes.get();
        let request = CreateApiKeyRequest {
            name: name.trim().to_string(),
            scopes,
        };

        spawn_local(async move {
            let client = ApiClient::new();
            match client.create_api_key(&request).await {
                Ok(response) => {
                    created_key_token.set(Some(response.token));
                    new_key_name.set(String::new());
                    selected_scopes.set(vec!["viewer".to_string()]);
                    show_create_dialog.set(false);
                    refetch_signal.with_value(|f| f());
                }
                Err(e) => {
                    create_error.set(Some(e.to_string()));
                }
            }
            creating.set(false);
        });
    };

    // Copy to clipboard using wasm-bindgen
    let on_copy = move |_| {
        if let Some(token) = created_key_token.get() {
            spawn_local(async move {
                if copy_to_clipboard(&token).await {
                    copied.set(true);
                    // Reset after 2 seconds
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    copied.set(false);
                }
            });
        }
    };

    // Revoke key handler
    let on_revoke = move |id: String| {
        revoking_id.set(Some(id.clone()));

        spawn_local(async move {
            let client = ApiClient::new();
            match client.revoke_api_key(&id).await {
                Ok(_) => {
                    refetch_signal.with_value(|f| f());
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to revoke key: {}", e).into());
                }
            }
            revoking_id.set(None);
        });
    };

    // Toggle scope selection
    let toggle_scope = move |scope: String| {
        let mut current = selected_scopes.get();
        if current.contains(&scope) {
            current.retain(|s| s != &scope);
        } else {
            current.push(scope);
        }
        if current.is_empty() {
            current.push("viewer".to_string());
        }
        selected_scopes.set(current);
    };

    view! {
        <div class="space-y-4">
            // Created key display (show once after creation)
            {move || {
                if let Some(token) = created_key_token.get() {
                    view! {
                        <div class="glass-panel border-green-500/50 bg-green-500/10 p-4 rounded-lg">
                            <div class="flex items-start gap-3">
                                <div class="rounded-full bg-green-500/20 p-2">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                        <polyline points="20 6 9 17 4 12"/>
                                    </svg>
                                </div>
                                <div class="flex-1 min-w-0">
                                    <h4 class="font-medium text-green-400">"API Key Created"</h4>
                                    <p class="text-sm text-muted-foreground mt-1">
                                        "Copy this key now. You won't be able to see it again."
                                    </p>
                                    <div class="mt-3 flex items-center gap-2">
                                        <code class="flex-1 bg-background/50 px-3 py-2 rounded font-mono text-sm truncate">
                                            {token.clone()}
                                        </code>
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=Callback::new(on_copy)
                                        >
                                            {move || if copied.get() { "Copied!" } else { "Copy" }}
                                        </Button>
                                    </div>
                                </div>
                                <button
                                    class="text-muted-foreground hover:text-foreground"
                                    on:click=move |_| created_key_token.set(None)
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                        <line x1="18" y1="6" x2="6" y2="18"/>
                                        <line x1="6" y1="6" x2="18" y2="18"/>
                                    </svg>
                                </button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}

            // Create dialog
            {move || {
                if show_create_dialog.get() {
                    view! {
                        <Card>
                            <div class="space-y-4">
                                <div class="flex items-center justify-between">
                                    <h3 class="text-lg font-medium">"Generate New API Key"</h3>
                                    <button
                                        class="text-muted-foreground hover:text-foreground"
                                        on:click=move |_| {
                                            show_create_dialog.set(false);
                                            create_error.set(None);
                                        }
                                    >
                                        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                            <line x1="18" y1="6" x2="6" y2="18"/>
                                            <line x1="6" y1="6" x2="18" y2="18"/>
                                        </svg>
                                    </button>
                                </div>

                                <div class="space-y-3">
                                    <div>
                                        <label class="text-sm font-medium mb-1 block">"Key Name"</label>
                                        <input
                                            type="text"
                                            class="w-full px-3 py-2 rounded-md border bg-background focus:outline-none focus:ring-2 focus:ring-primary"
                                            placeholder="e.g., Production API, Development Key"
                                            prop:value=move || new_key_name.get()
                                            on:input=move |ev| new_key_name.set(event_target_value(&ev))
                                        />
                                    </div>

                                    <div>
                                        <label class="text-sm font-medium mb-2 block">"Permissions"</label>
                                        <div class="flex flex-wrap gap-2">
                                            {["admin", "operator", "viewer"].into_iter().map(|scope| {
                                                let scope_str = scope.to_string();
                                                let scope_for_check = scope_str.clone();
                                                let scope_for_toggle = scope_str.clone();
                                                view! {
                                                    <button
                                                        class=move || {
                                                            let is_selected = selected_scopes.get().contains(&scope_for_check);
                                                            if is_selected {
                                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-primary text-primary-foreground"
                                                            } else {
                                                                "px-3 py-1.5 rounded-md text-sm font-medium bg-muted text-muted-foreground hover:bg-muted/80"
                                                            }
                                                        }
                                                        on:click={
                                                            let scope = scope_for_toggle.clone();
                                                            move |_| toggle_scope(scope.clone())
                                                        }
                                                    >
                                                        {scope_str.clone()}
                                                    </button>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </div>
                                        <p class="text-xs text-muted-foreground mt-1">
                                            "Select the permission levels this key should have"
                                        </p>
                                    </div>
                                </div>

                                {move || {
                                    if let Some(error) = create_error.get() {
                                        view! {
                                            <div class="text-sm text-destructive">{error}</div>
                                        }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}

                                <div class="flex justify-end gap-2 pt-2">
                                    <Button
                                        variant=ButtonVariant::Ghost
                                        on_click=Callback::new(move |_| {
                                            show_create_dialog.set(false);
                                            create_error.set(None);
                                        })
                                    >
                                        "Cancel"
                                    </Button>
                                    <Button
                                        variant=ButtonVariant::Primary
                                        on_click=Callback::new(on_create)
                                        disabled=creating.get()
                                    >
                                        {move || if creating.get() { "Generating..." } else { "Generate Key" }}
                                    </Button>
                                </div>
                            </div>
                        </Card>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}

            // Keys list
            <Card>
                {move || {
                    match keys.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center justify-center py-12">
                                    <Spinner/>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            let active_keys: Vec<ApiKeyInfo> = data.api_keys.into_iter()
                                .filter(|k| k.revoked_at.is_none())
                                .collect();

                            if active_keys.is_empty() && !show_create_dialog.get() {
                                view! {
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
                                        <h3 class="text-lg font-medium mb-2">"No API Keys"</h3>
                                        <p class="text-muted-foreground max-w-md mx-auto mb-4">
                                            "Generate API keys for programmatic access to the AdapterOS API."
                                        </p>
                                        <Button
                                            variant=ButtonVariant::Primary
                                            on_click=Callback::new(move |_| show_create_dialog.set(true))
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                <line x1="12" y1="5" x2="12" y2="19"/>
                                                <line x1="5" y1="12" x2="19" y2="12"/>
                                            </svg>
                                            "Generate New Key"
                                        </Button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        <div class="flex items-center justify-between mb-4">
                                            <span class="text-sm text-muted-foreground">
                                                {format!("{} active key{}", active_keys.len(), if active_keys.len() == 1 { "" } else { "s" })}
                                            </span>
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::Outline
                                                    on_click=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                                                >
                                                    "Refresh"
                                                </Button>
                                                <Button
                                                    variant=ButtonVariant::Primary
                                                    on_click=Callback::new(move |_| show_create_dialog.set(true))
                                                >
                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 mr-2" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                        <line x1="12" y1="5" x2="12" y2="19"/>
                                                        <line x1="5" y1="12" x2="19" y2="12"/>
                                                    </svg>
                                                    "Generate New Key"
                                                </Button>
                                            </div>
                                        </div>
                                        <Table>
                                            <TableHeader>
                                                <TableRow>
                                                    <TableHead>"Name"</TableHead>
                                                    <TableHead>"Permissions"</TableHead>
                                                    <TableHead>"Created"</TableHead>
                                                    <TableHead class="text-right".to_string()>"Actions"</TableHead>
                                                </TableRow>
                                            </TableHeader>
                                            <TableBody>
                                                {active_keys.into_iter().map(|key| {
                                                    let key_id = key.id.clone();
                                                    let key_id_for_revoke = key.id.clone();
                                                    let is_revoking = move || revoking_id.get() == Some(key_id.clone());

                                                    view! {
                                                        <TableRow>
                                                            <TableCell>
                                                                <div class="flex items-center gap-2">
                                                                    <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 5.25a3 3 0 013 3m3 0a6 6 0 01-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1121.75 8.25z"/>
                                                                    </svg>
                                                                    <span class="font-medium">{key.name.clone()}</span>
                                                                </div>
                                                            </TableCell>
                                                            <TableCell>
                                                                <div class="flex gap-1 flex-wrap">
                                                                    {key.scopes.clone().into_iter().map(|scope| {
                                                                        let variant = match scope.as_str() {
                                                                            "admin" => BadgeVariant::Destructive,
                                                                            "operator" => BadgeVariant::Warning,
                                                                            _ => BadgeVariant::Secondary,
                                                                        };
                                                                        view! {
                                                                            <Badge variant=variant>{scope}</Badge>
                                                                        }
                                                                    }).collect::<Vec<_>>()}
                                                                </div>
                                                            </TableCell>
                                                            <TableCell>
                                                                <span class="text-sm text-muted-foreground">
                                                                    {format_date(&key.created_at)}
                                                                </span>
                                                            </TableCell>
                                                            <TableCell class="text-right".to_string()>
                                                                <Button
                                                                    variant=ButtonVariant::Ghost
                                                                    on_click=Callback::new({
                                                                        let id = key_id_for_revoke.clone();
                                                                        move |_| on_revoke(id.clone())
                                                                    })
                                                                    disabled=is_revoking()
                                                                >
                                                                    {move || if is_revoking() { "Revoking..." } else { "Revoke" }}
                                                                </Button>
                                                            </TableCell>
                                                        </TableRow>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </TableBody>
                                        </Table>
                                    </div>
                                }.into_any()
                            }
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <ErrorDisplay
                                    error=e
                                    on_retry=Callback::new(move |_| refetch_signal.with_value(|f| f()))
                                />
                            }.into_any()
                        }
                    }
                }}
            </Card>
        </div>
    }
}

/// Format ISO date to a more readable format
fn format_date(iso: &str) -> String {
    // Simple date formatting - just show the date part
    if let Some(date_part) = iso.split('T').next() {
        date_part.to_string()
    } else {
        iso.to_string()
    }
}

/// Copy text to clipboard using the Clipboard API
async fn copy_to_clipboard(text: &str) -> bool {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };

    let navigator = window.navigator();

    // Get clipboard from navigator using JS reflection
    let clipboard = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .ok()
        .filter(|v| !v.is_undefined());

    let clipboard = match clipboard {
        Some(c) => c,
        None => return false,
    };

    // Call writeText method
    let write_text_fn =
        match js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")) {
            Ok(f) => f,
            Err(_) => return false,
        };

    let write_text_fn = match write_text_fn.dyn_ref::<js_sys::Function>() {
        Some(f) => f,
        None => return false,
    };

    let promise = match write_text_fn.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let promise = match promise.dyn_into::<js_sys::Promise>() {
        Ok(p) => p,
        Err(_) => return false,
    };

    JsFuture::from(promise).await.is_ok()
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
