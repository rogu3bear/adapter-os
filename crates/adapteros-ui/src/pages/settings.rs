//! Settings page
//!
//! Complete settings page with Profile, API Configuration, UI Preferences,
//! and System Info sections. Settings persist to localStorage.

use crate::api::ApiClient;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonVariant, Card, Input, Select, Spinner, Toggle,
};
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{update_setting, use_auth, use_settings, DefaultPage, Theme};
use adapteros_api_types::HealthResponse;
use leptos::prelude::*;
use std::sync::Arc;

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
                    <nav class="-mb-px flex space-x-8">
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

/// Profile section
#[component]
fn ProfileSection() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();

    // Logout handler
    let logout_loading = RwSignal::new(false);
    let handle_logout = move |_| {
        logout_loading.set(true);
        let action = auth_action.clone();
        wasm_bindgen_futures::spawn_local(async move {
            action.logout().await;
            // Redirect will happen via auth context
        });
    };

    view! {
        <div class="space-y-6 max-w-2xl">
            <Card title="User Profile".to_string() description="Your account information and session details.".to_string()>
                {move || {
                    if let Some(user) = auth_state.get().user() {
                        let user = user.clone();
                        view! {
                            <div class="space-y-4">
                                // Display Name
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Display Name"</span>
                                    <span class="col-span-2 text-sm">{user.display_name.clone()}</span>
                                </div>

                                // Email
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Email"</span>
                                    <span class="col-span-2 text-sm">{user.email.clone()}</span>
                                </div>

                                // User ID
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"User ID"</span>
                                    <span class="col-span-2 text-sm font-mono text-xs">{user.user_id.clone()}</span>
                                </div>

                                // Role
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Role"</span>
                                    <div class="col-span-2">
                                        <Badge variant=role_to_variant(&user.role)>
                                            {user.role.clone()}
                                        </Badge>
                                    </div>
                                </div>

                                // Tenant
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Tenant ID"</span>
                                    <span class="col-span-2 text-sm font-mono text-xs">{user.tenant_id.clone()}</span>
                                </div>

                                // Permissions
                                <div class="grid grid-cols-3 gap-4 items-start">
                                    <span class="text-sm font-medium text-muted-foreground">"Permissions"</span>
                                    <div class="col-span-2 flex flex-wrap gap-1">
                                        {if user.permissions.is_empty() {
                                            view! {
                                                <span class="text-sm text-muted-foreground">"No explicit permissions"</span>
                                            }.into_any()
                                        } else {
                                            let permissions = user.permissions.clone();
                                            view! {
                                                {permissions.into_iter().map(|p| {
                                                    view! {
                                                        <Badge variant=BadgeVariant::Outline>
                                                            {p}
                                                        </Badge>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            }.into_any()
                                        }}
                                    </div>
                                </div>

                                // MFA Status
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"MFA Status"</span>
                                    <div class="col-span-2">
                                        {if user.mfa_enabled.unwrap_or(false) {
                                            view! {
                                                <Badge variant=BadgeVariant::Success>"Enabled"</Badge>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <Badge variant=BadgeVariant::Secondary>"Disabled"</Badge>
                                            }.into_any()
                                        }}
                                    </div>
                                </div>

                                // Last Login
                                {user.last_login_at.clone().map(|last| view! {
                                    <div class="grid grid-cols-3 gap-4 items-center">
                                        <span class="text-sm font-medium text-muted-foreground">"Last Login"</span>
                                        <span class="col-span-2 text-sm">{last}</span>
                                    </div>
                                })}

                                // Member Since
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Member Since"</span>
                                    <span class="col-span-2 text-sm">{user.created_at.clone()}</span>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <p class="text-muted-foreground">"Loading user information..."</p>
                        }.into_any()
                    }
                }}
            </Card>

            // Session Actions
            <Card title="Session".to_string() description="Manage your current session.".to_string()>
                <div class="flex items-center justify-between">
                    <div>
                        <p class="text-sm text-muted-foreground">
                            "Sign out of your current session. You will need to log in again."
                        </p>
                    </div>
                    <Button
                        variant=ButtonVariant::Destructive
                        loading=logout_loading.get_untracked()
                        on_click=Callback::new(handle_logout)
                    >
                        "Logout"
                    </Button>
                </div>
            </Card>
        </div>
    }
}

/// Convert role string to badge variant
fn role_to_variant(role: &str) -> BadgeVariant {
    match role.to_lowercase().as_str() {
        "admin" => BadgeVariant::Destructive,
        "developer" | "sre" => BadgeVariant::Default,
        "operator" | "compliance" => BadgeVariant::Warning,
        "auditor" | "viewer" => BadgeVariant::Secondary,
        _ => BadgeVariant::Outline,
    }
}

/// API Configuration section
#[component]
fn ApiConfigSection() -> impl IntoView {
    let settings = use_settings();

    // Local state for API endpoint editing
    let api_endpoint = RwSignal::new(
        settings
            .get_untracked()
            .api_endpoint
            .unwrap_or_else(get_default_api_endpoint),
    );

    // Connection test state
    let test_status = RwSignal::new(ConnectionTestStatus::Idle);

    // Masked token display
    let token_visible = RwSignal::new(false);

    // Test connection handler
    let test_connection = move |_| {
        test_status.set(ConnectionTestStatus::Testing);
        let endpoint = api_endpoint.get();

        wasm_bindgen_futures::spawn_local(async move {
            let client = ApiClient::with_base_url(&endpoint);
            match client.health().await {
                Ok(_) => test_status.set(ConnectionTestStatus::Success),
                Err(e) => test_status.set(ConnectionTestStatus::Error(e.to_string())),
            }
        });
    };

    // Save endpoint handler
    let save_endpoint = move |_| {
        let endpoint = api_endpoint.get();
        update_setting(settings, |s| {
            s.api_endpoint = if endpoint == get_default_api_endpoint() {
                None
            } else {
                Some(endpoint)
            };
        });
        test_status.set(ConnectionTestStatus::Saved);
    };

    // Reset to default handler
    let reset_endpoint = move |_| {
        let default = get_default_api_endpoint();
        api_endpoint.set(default.clone());
        update_setting(settings, |s| {
            s.api_endpoint = None;
        });
        test_status.set(ConnectionTestStatus::Idle);
    };

    view! {
        <div class="space-y-6 max-w-2xl">
            // API Endpoint
            <Card title="API Endpoint".to_string() description="Configure the backend API endpoint URL.".to_string()>
                <div class="space-y-4">
                    <Input
                        value=api_endpoint
                        label="API URL".to_string()
                        placeholder="https://api.example.com".to_string()
                    />

                    <div class="flex items-center gap-2">
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(test_connection)
                        >
                            "Test Connection"
                        </Button>
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(save_endpoint)
                        >
                            "Save"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            on_click=Callback::new(reset_endpoint)
                        >
                            "Reset to Default"
                        </Button>
                    </div>

                    // Connection test status
                    {move || {
                        match test_status.get() {
                            ConnectionTestStatus::Idle => view! {}.into_any(),
                            ConnectionTestStatus::Testing => view! {
                                <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                    <Spinner/>
                                    "Testing connection..."
                                </div>
                            }.into_any(),
                            ConnectionTestStatus::Success => view! {
                                <div class="flex items-center gap-2 text-sm text-green-600">
                                    <CheckIcon/>
                                    "Connection successful"
                                </div>
                            }.into_any(),
                            ConnectionTestStatus::Error(ref msg) => view! {
                                <div class="flex items-center gap-2 text-sm text-destructive">
                                    <XIcon/>
                                    {format!("Connection failed: {}", msg)}
                                </div>
                            }.into_any(),
                            ConnectionTestStatus::Saved => view! {
                                <div class="flex items-center gap-2 text-sm text-green-600">
                                    <CheckIcon/>
                                    "Settings saved"
                                </div>
                            }.into_any(),
                        }
                    }}
                </div>
            </Card>

            // Auth Token
            <Card title="Authentication Token".to_string() description="Your current authentication token (stored in browser).".to_string()>
                <div class="space-y-4">
                    <div class="flex items-center gap-2">
                        <div class="flex-1 font-mono text-sm bg-muted p-3 rounded-md overflow-hidden">
                            {move || {
                                if let Some(token) = get_stored_token() {
                                    if token_visible.get() {
                                        token
                                    } else {
                                        mask_token(&token)
                                    }
                                } else {
                                    "No token stored".to_string()
                                }
                            }}
                        </div>
                        <Button
                            variant=ButtonVariant::Outline
                            size=crate::components::ButtonSize::Icon
                            on_click=Callback::new(move |_| token_visible.update(|v| *v = !*v))
                        >
                            {move || {
                                if token_visible.get() {
                                    view! { <EyeOffIcon/> }.into_any()
                                } else {
                                    view! { <EyeIcon/> }.into_any()
                                }
                            }}
                        </Button>
                    </div>

                    <p class="text-xs text-muted-foreground">
                        "Token is managed automatically during login/logout. Refresh if experiencing authentication issues."
                    </p>
                </div>
            </Card>
        </div>
    }
}

/// Connection test status
#[derive(Clone)]
enum ConnectionTestStatus {
    Idle,
    Testing,
    Success,
    Error(String),
    Saved,
}

/// Get default API endpoint
fn get_default_api_endpoint() -> String {
    crate::api::api_base_url()
}

/// Get stored auth token
fn get_stored_token() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("auth_token").ok().flatten())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Mask token for display
fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        "*".repeat(token.len())
    } else {
        format!("{}...{}", &token[..6], &token[token.len() - 6..])
    }
}

/// UI Preferences section
#[component]
fn PreferencesSection() -> impl IntoView {
    let settings = use_settings();

    // Local signals bound to settings
    let theme = RwSignal::new(settings.get_untracked().theme.as_str().to_string());
    let compact_mode = RwSignal::new(settings.get_untracked().compact_mode);
    let show_timestamps = RwSignal::new(settings.get_untracked().show_timestamps);
    let default_page = RwSignal::new(settings.get_untracked().default_page.as_str().to_string());

    // Save feedback
    let save_feedback = RwSignal::new(false);

    // Effect to sync theme changes
    Effect::new(move || {
        let new_theme = Theme::parse(&theme.get());
        update_setting(settings, |s| {
            s.theme = new_theme;
            s.apply_theme();
        });
    });

    // Effect to sync compact mode changes
    Effect::new(move || {
        let value = compact_mode.get();
        update_setting(settings, |s| {
            s.compact_mode = value;
        });
    });

    // Effect to sync show timestamps changes
    Effect::new(move || {
        let value = show_timestamps.get();
        update_setting(settings, |s| {
            s.show_timestamps = value;
        });
    });

    // Effect to sync default page changes
    Effect::new(move || {
        let new_page = DefaultPage::parse(&default_page.get());
        update_setting(settings, |s| {
            s.default_page = new_page;
        });
    });

    // Show save feedback briefly
    Effect::new(move || {
        let _ = theme.get();
        let _ = compact_mode.get();
        let _ = show_timestamps.get();
        let _ = default_page.get();

        save_feedback.set(true);

        // Hide after 2 seconds
        #[cfg(target_arch = "wasm32")]
        {
            let handle = gloo_timers::callback::Timeout::new(2000, move || {
                save_feedback.set(false);
            });
            handle.forget();
        }
    });

    // Theme options
    let theme_options = vec![
        ("light".to_string(), "Light".to_string()),
        ("dark".to_string(), "Dark".to_string()),
        ("system".to_string(), "System".to_string()),
    ];

    // Default page options
    let page_options = vec![
        ("dashboard".to_string(), "Dashboard".to_string()),
        ("adapters".to_string(), "Adapters".to_string()),
        ("chat".to_string(), "Chat".to_string()),
        ("training".to_string(), "Training".to_string()),
        ("system".to_string(), "System".to_string()),
    ];

    view! {
        <div class="space-y-6 max-w-2xl">
            // Theme
            <Card title="Appearance".to_string() description="Customize the look and feel of the interface.".to_string()>
                <div class="space-y-6">
                    <Select
                        value=theme
                        options=theme_options
                        label="Theme".to_string()
                    />

                    <Toggle
                        checked=compact_mode
                        label="Compact Mode".to_string()
                        description="Reduce spacing and padding for a denser layout".to_string()
                    />

                    <Toggle
                        checked=show_timestamps
                        label="Show Timestamps".to_string()
                        description="Display timestamps in lists, messages, and activity logs".to_string()
                    />
                </div>
            </Card>

            // Navigation
            <Card title="Navigation".to_string() description="Configure navigation behavior.".to_string()>
                <Select
                    value=default_page
                    options=page_options
                    label="Default Page After Login".to_string()
                />
            </Card>

            // Save indicator
            {move || {
                if save_feedback.get() {
                    view! {
                        <div class="flex items-center gap-2 text-sm text-green-600">
                            <CheckIcon/>
                            "Changes saved automatically"
                        </div>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

/// System Info section
#[component]
fn SystemInfoSection() -> impl IntoView {
    // Fetch health info
    let (health, refetch) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.health().await });

    view! {
        <div class="space-y-6 max-w-2xl">
            // UI Version
            <Card title="UI Version".to_string() description="Frontend application version.".to_string()>
                <div class="space-y-2">
                    <div class="grid grid-cols-3 gap-4 items-center">
                        <span class="text-sm font-medium text-muted-foreground">"Version"</span>
                        <span class="col-span-2 text-sm font-mono">{env!("CARGO_PKG_VERSION")}</span>
                    </div>
                    <div class="grid grid-cols-3 gap-4 items-center">
                        <span class="text-sm font-medium text-muted-foreground">"Framework"</span>
                        <span class="col-span-2 text-sm">"Leptos 0.7 (CSR)"</span>
                    </div>
                    <div class="grid grid-cols-3 gap-4 items-center">
                        <span class="text-sm font-medium text-muted-foreground">"Target"</span>
                        <span class="col-span-2 text-sm font-mono">"wasm32-unknown-unknown"</span>
                    </div>
                </div>
            </Card>

            // API Version
            <Card title="API Version".to_string() description="Backend API and runtime information.".to_string()>
                <div class="flex items-center justify-between mb-4">
                    <span class="text-sm text-muted-foreground">"Backend health status from /healthz"</span>
                    <Button
                        variant=ButtonVariant::Outline
                        size=crate::components::ButtonSize::Sm
                        on_click=Callback::new(move |_| refetch())
                    >
                        "Refresh"
                    </Button>
                </div>

                {move || {
                    match health.get() {
                        LoadingState::Idle | LoadingState::Loading => {
                            view! {
                                <div class="flex items-center gap-2">
                                    <Spinner/>
                                    <span class="text-sm text-muted-foreground">"Loading..."</span>
                                </div>
                            }.into_any()
                        }
                        LoadingState::Loaded(data) => {
                            view! { <HealthInfo health=data/> }.into_any()
                        }
                        LoadingState::Error(e) => {
                            view! {
                                <div class="rounded-lg border border-destructive bg-destructive/10 p-4">
                                    <p class="text-sm text-destructive">
                                        {format!("Failed to fetch: {}", e)}
                                    </p>
                                </div>
                            }.into_any()
                        }
                    }
                }}
            </Card>

            // Build Info
            <Card title="Build Information".to_string() description="Compilation and environment details.".to_string()>
                <div class="space-y-2">
                    <div class="grid grid-cols-3 gap-4 items-center">
                        <span class="text-sm font-medium text-muted-foreground">"API Schema Version"</span>
                        <span class="col-span-2 text-sm font-mono">{adapteros_api_types::API_SCHEMA_VERSION}</span>
                    </div>
                    <div class="grid grid-cols-3 gap-4 items-center">
                        <span class="text-sm font-medium text-muted-foreground">"Build Profile"</span>
                        <span class="col-span-2 text-sm">
                            {if cfg!(debug_assertions) { "Debug" } else { "Release" }}
                        </span>
                    </div>
                </div>
            </Card>
        </div>
    }
}

/// Health info display
#[component]
fn HealthInfo(health: HealthResponse) -> impl IntoView {
    let status_variant = match health.status.as_str() {
        "ok" | "healthy" => BadgeVariant::Success,
        "degraded" | "warning" => BadgeVariant::Warning,
        _ => BadgeVariant::Destructive,
    };

    view! {
        <div class="space-y-2">
            <div class="grid grid-cols-3 gap-4 items-center">
                <span class="text-sm font-medium text-muted-foreground">"Status"</span>
                <div class="col-span-2">
                    <Badge variant=status_variant>
                        {health.status.clone()}
                    </Badge>
                </div>
            </div>
            <div class="grid grid-cols-3 gap-4 items-center">
                <span class="text-sm font-medium text-muted-foreground">"Version"</span>
                <span class="col-span-2 text-sm font-mono">{health.version.clone()}</span>
            </div>
            <div class="grid grid-cols-3 gap-4 items-center">
                <span class="text-sm font-medium text-muted-foreground">"Schema Version"</span>
                <span class="col-span-2 text-sm font-mono">{health.schema_version.clone()}</span>
            </div>

            // Model runtime health
            {health.models.map(|models| view! {
                <div class="mt-4 pt-4 border-t">
                    <h4 class="text-sm font-medium mb-2">"Model Runtime"</h4>
                    <div class="space-y-2">
                        <div class="grid grid-cols-3 gap-4 items-center">
                            <span class="text-sm font-medium text-muted-foreground">"Models Loaded"</span>
                            <span class="col-span-2 text-sm">
                                {format!("{} / {}", models.loaded_count, models.total_models)}
                            </span>
                        </div>
                        <div class="grid grid-cols-3 gap-4 items-center">
                            <span class="text-sm font-medium text-muted-foreground">"Health"</span>
                            <div class="col-span-2">
                                {if models.healthy {
                                    view! {
                                        <Badge variant=BadgeVariant::Success>"Healthy"</Badge>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Badge variant=BadgeVariant::Destructive>"Unhealthy"</Badge>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                        {if models.inconsistencies_count > 0 {
                            Some(view! {
                                <div class="grid grid-cols-3 gap-4 items-center">
                                    <span class="text-sm font-medium text-muted-foreground">"Inconsistencies"</span>
                                    <span class="col-span-2 text-sm text-destructive">
                                        {models.inconsistencies_count.to_string()}
                                    </span>
                                </div>
                            })
                        } else {
                            None
                        }}
                    </div>
                </div>
            })}
        </div>
    }
}

// Icon components

#[component]
fn CheckIcon() -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <polyline points="20 6 9 17 4 12"/>
        </svg>
    }
}

#[component]
fn XIcon() -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
    }
}

#[component]
fn EyeIcon() -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
            <circle cx="12" cy="12" r="3"/>
        </svg>
    }
}

#[component]
fn EyeOffIcon() -> impl IntoView {
    view! {
        <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/>
            <line x1="1" y1="1" x2="23" y2="23"/>
        </svg>
    }
}
