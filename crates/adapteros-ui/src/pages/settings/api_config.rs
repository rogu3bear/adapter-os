//! API Configuration section component

use super::icons::{CheckIcon, XIcon};
use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Card, Input, Spinner};
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
                                <div class="flex items-center gap-2 text-sm text-status-success">
                                    <CheckIcon/>
                                    "Settings saved"
                                </div>
                            }.into_any(),
                        }
                    }}
                </div>
            </Card>

            // Auth Status
            <Card title="Authentication Status".to_string() description="Current authenticated user for this session.".to_string()>
                <div class="space-y-3">
                    {move || {
                        match auth_state.get() {
                            AuthState::Authenticated(user) => {
                                let user = user.clone();
                                let email = user.email.clone();
                                let role = user.role.clone();
                                view! {
                                    <div class="space-y-1">
                                        <div class="text-sm font-medium">{email}</div>
                                        <div class="text-xs text-muted-foreground">{format!("Role: {}", role)}</div>
                                    </div>
                                }.into_any()
                            }
                            AuthState::Unauthenticated => view! {
                                <p class="text-sm text-muted-foreground">"Not authenticated."</p>
                            }.into_any(),
                            AuthState::Error(msg) => view! {
                                <p class="text-sm text-destructive">{format!("Authentication error: {}", msg)}</p>
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
                        "Session authentication uses secure httpOnly cookies; no bearer token is stored in the browser."
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
