//! AdapterOS Leptos UI
//!
//! A Leptos-based web frontend for the AdapterOS control plane.
//! This crate provides a CSR (Client-Side Rendered) application that
//! communicates with the AdapterOS backend via REST and SSE.
//!
//! # Architecture
//!
//! - `api/` - HTTP client and SSE infrastructure
//! - `components/` - Reusable UI components (buttons, dialogs, etc.)
//! - `pages/` - Route page components
//! - `signals/` - Reactive state management
//! - `hooks/` - Leptos-style hooks for common patterns
//!
//! # Features
//!
//! - `hydrate` - Enable client-side hydration (for SSR + hydration mode)
//! - `ssr` - Enable server-side rendering

// Leptos view! macro patterns that trigger clippy but are idiomatic
#![allow(clippy::unused_unit)]
#![allow(clippy::unit_arg)]
// Callback<T> is Copy but .clone() is often clearer in closures
#![allow(clippy::clone_on_copy)]

pub mod api;
pub mod components;
pub mod hooks;
pub mod pages;
pub mod search;
pub mod signals;
pub mod sse;
pub mod theme;
pub mod validation;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::api::ApiClient;
use components::{AuthProvider, CommandPalette, ProtectedRoute, Shell};
use signals::{provide_chat_context, provide_notifications_context, provide_search_context};
use std::sync::Arc;

/// PRD-UI-000: Redact sensitive information from panic messages.
///
/// Scrubs Bearer tokens, JWTs, passwords, and auth tokens from error messages
/// to prevent credential leakage in panic overlays.
///
/// Patterns redacted:
/// - `Bearer <token>` -> `Bearer [REDACTED]`
/// - `jwt=<token>` -> `jwt=[REDACTED]`
/// - `auth_token=<token>` -> `auth_token=[REDACTED]`
/// - `password=<value>` -> `password=[REDACTED]`
/// - `Authorization: <value>` -> `Authorization: [REDACTED]`
/// - `api_key=<value>` -> `api_key=[REDACTED]`
/// - `secret=<value>` -> `secret=[REDACTED]`
pub fn redact_sensitive_info(message: &str) -> String {
    let mut result = message.to_string();

    // Redact Bearer tokens (handles both header format and inline)
    // Matches: "Bearer eyJ..." or "bearer abc123..."
    let bearer_re = regex_lite::Regex::new(r"(?i)Bearer\s+[A-Za-z0-9\-_\.]+").unwrap();
    result = bearer_re
        .replace_all(&result, "Bearer [REDACTED]")
        .to_string();

    // Redact JWT patterns (base64.base64.base64)
    // This catches standalone JWTs that might not have Bearer prefix
    let jwt_re =
        regex_lite::Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+").unwrap();
    result = jwt_re.replace_all(&result, "[REDACTED_JWT]").to_string();

    // Redact key=value patterns for sensitive keys
    let patterns = [
        (r"(?i)jwt\s*=\s*[^\s&]+", "jwt=[REDACTED]"),
        (r"(?i)auth_token\s*=\s*[^\s&]+", "auth_token=[REDACTED]"),
        (r"(?i)password\s*=\s*[^\s&]+", "password=[REDACTED]"),
        (r"(?i)api_key\s*=\s*[^\s&]+", "api_key=[REDACTED]"),
        (r"(?i)secret\s*=\s*[^\s&]+", "secret=[REDACTED]"),
        (r"(?i)access_token\s*=\s*[^\s&]+", "access_token=[REDACTED]"),
        (
            r"(?i)refresh_token\s*=\s*[^\s&]+",
            "refresh_token=[REDACTED]",
        ),
        (
            r"(?i)Authorization:\s*[^\r\n]+",
            "Authorization: [REDACTED]",
        ),
    ];

    for (pattern, replacement) in patterns {
        if let Ok(re) = regex_lite::Regex::new(pattern) {
            result = re.replace_all(&result, replacement).to_string();
        }
    }

    result
}

/// Main application component - full app with all routes
#[component]
pub fn App() -> impl IntoView {
    web_sys::console::log_1(&"[App] Rendering App component...".into());
    provide_meta_context();
    web_sys::console::log_1(&"[App] Meta context provided, creating view...".into());

    view! {
        <Title text="AdapterOS"/>
        <Meta charset="utf-8"/>
        <Meta name="viewport" content="width=device-width, initial-scale=1"/>

        <AuthProvider>
            <NotificationsProvider>
                <SearchProvider>
                    <ChatProvider>
                        <Router>
                    <Routes fallback=|| view! { <pages::NotFound/> }>
                        <Route path=path!("/login") view=pages::Login/>
                        <Route path=path!("/") view=|| view! { <ProtectedRoute><Shell><pages::Dashboard/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/dashboard") view=|| view! { <ProtectedRoute><Shell><pages::Dashboard/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/adapters") view=|| view! { <ProtectedRoute><Shell><pages::Adapters/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/adapters/:id") view=|| view! { <ProtectedRoute><Shell><pages::AdapterDetail/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/chat") view=|| view! { <ProtectedRoute><Shell><pages::Chat/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/chat/:session_id") view=|| view! { <ProtectedRoute><Shell><pages::ChatSession/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/training") view=|| view! { <ProtectedRoute><Shell><pages::Training/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/system") view=|| view! { <ProtectedRoute><Shell><pages::System/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/settings") view=|| view! { <ProtectedRoute><Shell><pages::Settings/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/models") view=|| view! { <ProtectedRoute><Shell><pages::Models/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/policies") view=|| view! { <ProtectedRoute><Shell><pages::Policies/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/stacks") view=|| view! { <ProtectedRoute><Shell><pages::Stacks/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/stacks/:id") view=|| view! { <ProtectedRoute><Shell><pages::StackDetail/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/collections") view=|| view! { <ProtectedRoute><Shell><pages::Collections/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/collections/:id") view=|| view! { <ProtectedRoute><Shell><pages::CollectionDetail/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/documents") view=|| view! { <ProtectedRoute><Shell><pages::Documents/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/documents/:id") view=|| view! { <ProtectedRoute><Shell><pages::DocumentDetail/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/admin") view=|| view! { <ProtectedRoute><Shell><pages::Admin/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/audit") view=|| view! { <ProtectedRoute><Shell><pages::Audit/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/workers") view=|| view! { <ProtectedRoute><Shell><pages::Workers/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/workers/:id") view=|| view! { <ProtectedRoute><Shell><pages::WorkerDetail/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/repositories") view=|| view! { <ProtectedRoute><Shell><pages::Repositories/></Shell></ProtectedRoute> }/>
                        <Route path=path!("/repositories/:id") view=|| view! { <ProtectedRoute><Shell><pages::RepositoryDetail/></Shell></ProtectedRoute> }/>
                        // PRD-UI-000: Safe mode route (no auth required, no API calls)
                        <Route path=path!("/safe") view=pages::Safe/>
                        // PRD-UI-003: Style audit (dev tool, no sensitive data)
                        <Route path=path!("/style-audit") view=pages::StyleAudit/>
                    </Routes>
                        // Global Command Palette overlay
                        <CommandPalette/>
                        </Router>
                    </ChatProvider>
                </SearchProvider>
            </NotificationsProvider>
        </AuthProvider>
    }
}

#[component]
fn ChatProvider(children: Children) -> impl IntoView {
    provide_chat_context();
    children()
}

#[component]
fn NotificationsProvider(children: Children) -> impl IntoView {
    provide_notifications_context();
    children()
}

#[component]
fn SearchProvider(children: Children) -> impl IntoView {
    let client = Arc::new(ApiClient::new());
    provide_search_context(client);
    children()
}

// PRD-UI-000: JS interop for boot diagnostics
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "aosSignalWasmLoaded")]
    fn signal_wasm_loaded();
    #[wasm_bindgen(js_name = "aosSignalMounted")]
    fn signal_mounted();
    #[wasm_bindgen(js_name = "aosShowPanic")]
    fn show_panic(message: &str, stack_trace: &str);
}

/// PRD-UI-000: Custom panic hook that displays errors in the DOM with redaction
fn set_dom_panic_hook() {
    use std::panic;
    use std::sync::Once;
    static SET_HOOK: Once = Once::new();
    SET_HOOK.call_once(|| {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            previous_hook(info);

            // Extract the panic message
            let raw_message = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };

            // Redact sensitive information from the message
            let message = redact_sensitive_info(&raw_message);

            // Build location info for the main message
            let location = if let Some(loc) = info.location() {
                format!(" at {}:{}:{}", loc.file(), loc.line(), loc.column())
            } else {
                String::new()
            };

            // Build a stack trace (location details go in the collapsible section)
            let stack_trace = if let Some(loc) = info.location() {
                format!(
                    "Panic occurred:\n  File: {}\n  Line: {}\n  Column: {}\n\nNote: Full stack traces require debug builds with RUST_BACKTRACE=1",
                    loc.file(),
                    loc.line(),
                    loc.column()
                )
            } else {
                "No location information available.\n\nNote: Full stack traces require debug builds with RUST_BACKTRACE=1".to_string()
            };

            // Redact the stack trace as well (in case it contains sensitive paths)
            let redacted_stack = redact_sensitive_info(&stack_trace);

            show_panic(&format!("{}{}", message, location), &redacted_stack);
        }));
    });
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn mount() {
    console_error_panic_hook::set_once();
    set_dom_panic_hook();
    signal_wasm_loaded();
    tracing_wasm::set_as_global_default();
    web_sys::console::log_1(&"[mount] Starting app mount...".into());
    leptos::mount::mount_to_body(App);
    signal_mounted();
    web_sys::console::log_1(&"[mount] App mounted successfully".into());
}
