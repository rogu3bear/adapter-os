//! API Configuration section component

use super::icons::{CheckIcon, EyeIcon, EyeOffIcon, XIcon};
use crate::api::ApiClient;
use crate::components::{Button, ButtonVariant, Card, Input, Spinner};
use crate::signals::{update_setting, use_settings};
use leptos::prelude::*;

/// API Configuration section
#[component]
pub fn ApiConfigSection() -> impl IntoView {
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

/// Get auth token info for display
///
/// With httpOnly cookie-based auth, we can't read the token directly.
/// Returns a placeholder indicating secure cookie storage.
fn get_stored_token() -> Option<String> {
    // Auth tokens are now stored in httpOnly cookies for security.
    // We can't read them from JavaScript, which is the point.
    // Return a placeholder to indicate auth is via secure cookies.
    Some("[Stored in secure httpOnly cookie]".to_string())
}

/// Mask token for display
fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        "*".repeat(token.len())
    } else {
        format!("{}...{}", &token[..6], &token[token.len() - 6..])
    }
}
