//! API Configuration section component

use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Card, Input, Spinner};
use crate::components::{IconCheck, IconX};
use crate::signals::{update_setting, use_auth, use_settings, AuthState};
use leptos::prelude::*;

/// API Configuration section
#[component]
pub fn ApiConfigSection() -> impl IntoView {
    let settings = use_settings();
    let (auth_state, _) = use_auth();

    // Local state for API endpoint editing
    let api_endpoint = RwSignal::new(
        settings
            .get_untracked()
            .api_endpoint
            .unwrap_or_else(get_default_api_endpoint),
    );

    // Connection test state
    let test_status = RwSignal::new(ConnectionTestStatus::Idle);

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
                                <div class="flex items-center gap-2 text-sm text-status-success">
                                    <IconCheck/>
                                    "Connection successful"
                                </div>
                            }.into_any(),
                            ConnectionTestStatus::Error(ref msg) => view! {
                                <div class="flex items-center gap-2 text-sm text-destructive">
                                    <IconX/>
                                    {format!("Connection failed: {}", msg)}
                                </div>
                            }.into_any(),
                            ConnectionTestStatus::Saved => view! {
                                <div class="flex items-center gap-2 text-sm text-status-success">
                                    <IconCheck/>
                                    "Settings saved"
                                </div>
                            }.into_any(),
                        }
                    }}
                </div>
            </Card>

            // Auth Status
            <Card title="Authentication Status".to_string() description="Current session authentication details.".to_string()>
                <div class="space-y-3">
                    {move || {
                        match auth_state.get() {
                            AuthState::Authenticated(user) => {
                                let user = user.clone();
                                view! {
                                    <div class="space-y-3">
                                        // User identity
                                        <div class="space-y-1">
                                            <div class="text-sm font-medium">{user.display_name.clone()}</div>
                                            <div class="text-xs text-muted-foreground">{user.email.clone()}</div>
                                        </div>

                                        // Session details grid
                                        <div class="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
                                            <div class="text-muted-foreground">"Role"</div>
                                            <div class="font-mono">{user.role.clone()}</div>

                                            <div class="text-muted-foreground">"Tenant"</div>
                                            <div class="font-mono truncate" title=user.tenant_id.clone()>{user.tenant_id.clone()}</div>

                                            <div class="text-muted-foreground">"User ID"</div>
                                            <div class="font-mono truncate" title=user.user_id.clone()>{user.user_id.clone()}</div>

                                            {user.last_login_at.clone().map(|t| view! {
                                                <div class="text-muted-foreground">"Last Login"</div>
                                                <div class="font-mono">{format_timestamp(&t)}</div>
                                            })}

                                            {user.mfa_enabled.map(|enabled| view! {
                                                <div class="text-muted-foreground">"MFA"</div>
                                                <div>{if enabled { "Enabled" } else { "Disabled" }}</div>
                                            })}
                                        </div>

                                        // Permissions
                                        {(!user.permissions.is_empty()).then(|| {
                                            let perms = user.permissions.clone();
                                            view! {
                                                <div class="space-y-1">
                                                    <div class="text-xs text-muted-foreground">"Permissions"</div>
                                                    <div class="flex flex-wrap gap-1">
                                                        {perms.into_iter().map(|p| view! {
                                                            <span class="inline-flex items-center rounded-md bg-muted px-2 py-0.5 text-xs font-mono">
                                                                {p}
                                                            </span>
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            }
                                        })}
                                    </div>
                                }.into_any()
                            }
                            AuthState::Unauthenticated => view! {
                                <p class="text-sm text-muted-foreground">"Not authenticated."</p>
                            }.into_any(),
                            AuthState::Error(err) => view! {
                                <p class="text-sm text-destructive">{format!("Authentication error: {}", err.message())}</p>
                            }.into_any(),
                            AuthState::Timeout => view! {
                                <p class="text-sm text-destructive">"Authentication check timed out."</p>
                            }.into_any(),
                            AuthState::Unknown | AuthState::Loading => view! {
                                <div class="flex items-center gap-2 text-sm text-muted-foreground">
                                    <Spinner/>
                                    "Checking authentication..."
                                </div>
                            }.into_any(),
                        }
                    }}

                    <p class="text-xs text-muted-foreground">
                        "Session uses secure httpOnly cookies."
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

/// Format ISO timestamp to human-readable form
fn format_timestamp(iso: &str) -> String {
    // Parse and display in a compact form
    // ISO: 2024-01-15T10:30:00Z -> Jan 15 10:30
    if let Some(date_part) = iso.get(..10) {
        if let Some(time_part) = iso.get(11..16) {
            let parts: Vec<&str> = date_part.split('-').collect();
            if parts.len() == 3 {
                let month = match parts[1] {
                    "01" => "Jan",
                    "02" => "Feb",
                    "03" => "Mar",
                    "04" => "Apr",
                    "05" => "May",
                    "06" => "Jun",
                    "07" => "Jul",
                    "08" => "Aug",
                    "09" => "Sep",
                    "10" => "Oct",
                    "11" => "Nov",
                    "12" => "Dec",
                    _ => return iso.to_string(),
                };
                let day = parts[2].trim_start_matches('0');
                return format!("{} {} {}", month, day, time_part);
            }
        }
    }
    iso.to_string()
}
