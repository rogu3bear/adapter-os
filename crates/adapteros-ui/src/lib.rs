//! adapterOS Leptos UI
//!
//! A Leptos-based web frontend for the adapterOS control plane.
//! This crate provides a CSR (Client-Side Rendered) application that
//! communicates with the adapterOS backend via REST and SSE.
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
#![allow(non_snake_case)]

pub mod api;
pub mod components;
pub mod constants;
pub mod contexts;
pub mod hooks;
pub mod pages;
pub mod search;
pub mod signals;
pub mod sse;
pub mod utils;
pub mod validation;

use leptos::prelude::*;
use leptos::tachys::view::any_view::IntoAny;
use leptos_router::components::*;
use leptos_router::path;

use crate::api::{report_ui_panic, ApiClient};
use crate::contexts::InFlightProvider;
use components::{
    AuthProvider, CommandPalette, ProtectedRoute, RouteErrorBoundary, Shell, ToastContainer,
};
use signals::{
    provide_chat_context, provide_notifications_context, provide_refetch_context,
    provide_search_context, provide_settings_context, provide_ui_profile_context,
};
use std::sync::Arc;

/// Pre-compiled regex patterns for sensitive data redaction.
///
/// Using OnceLock ensures patterns are compiled exactly once,
/// avoiding the overhead of regex compilation on every call.
struct RedactionPatterns {
    bearer: regex_lite::Regex,
    jwt: regex_lite::Regex,
    key_value: Vec<(regex_lite::Regex, &'static str)>,
}

impl RedactionPatterns {
    fn new() -> Self {
        Self {
            // Matches: "Bearer eyJ..." or "bearer abc123..."
            bearer: regex_lite::Regex::new(r"(?i)Bearer\s+[A-Za-z0-9\-_\.]+").unwrap(),
            // Matches standalone JWTs (base64.base64.base64)
            jwt: regex_lite::Regex::new(r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+")
                .unwrap(),
            // Key=value patterns for sensitive keys
            key_value: vec![
                (
                    regex_lite::Regex::new(r"(?i)jwt\s*=\s*[^\s&]+").unwrap(),
                    "jwt=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)auth_token\s*=\s*[^\s&]+").unwrap(),
                    "auth_token=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)password\s*=\s*[^\s&]+").unwrap(),
                    "password=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)api_key\s*=\s*[^\s&]+").unwrap(),
                    "api_key=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)secret\s*=\s*[^\s&]+").unwrap(),
                    "secret=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)access_token\s*=\s*[^\s&]+").unwrap(),
                    "access_token=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)refresh_token\s*=\s*[^\s&]+").unwrap(),
                    "refresh_token=[REDACTED]",
                ),
                (
                    regex_lite::Regex::new(r"(?i)Authorization:\s*[^\r\n]+").unwrap(),
                    "Authorization: [REDACTED]",
                ),
            ],
        }
    }
}

/// Static storage for pre-compiled redaction patterns.
static REDACTION_PATTERNS: std::sync::OnceLock<RedactionPatterns> = std::sync::OnceLock::new();

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
    let patterns = REDACTION_PATTERNS.get_or_init(RedactionPatterns::new);

    let mut result = message.to_string();

    // Redact Bearer tokens (handles both header format and inline)
    result = patterns
        .bearer
        .replace_all(&result, "Bearer [REDACTED]")
        .to_string();

    // Redact JWT patterns (base64.base64.base64)
    // This catches standalone JWTs that might not have Bearer prefix
    result = patterns
        .jwt
        .replace_all(&result, "[REDACTED_JWT]")
        .to_string();

    // Redact key=value patterns for sensitive keys
    for (re, replacement) in &patterns.key_value {
        result = re.replace_all(&result, *replacement).to_string();
    }

    result
}

/// Main application component - full app with all routes
#[component]
pub fn App() -> impl IntoView {
    web_sys::console::log_1(&"[App] Rendering App component...".into());

    // Ensure API base URL is configured; fail fast with a clear banner instead of a blank screen
    if let Err(err) = crate::api::api_base_url_checked() {
        return view! { <BaseUrlError reason=err.to_string()/> }.into_any();
    }

    view! {
        <>
            <AuthProvider>
                <AppProviders>
                    <SearchProvider>
                        <InFlightProvider>
                                <RouteErrorBoundary>
                                <Router>
                            // Toast container for app-wide notifications
                            <ToastContainer/>
                        <Routes fallback=|| view! { <pages::NotFound/> }>
                        // Non-shell routes (no auth, no Shell wrapper)
                        <Route path=path!("/login") view=pages::Login/>
                        // PRD-UI-000: Safe mode route (no auth required, no API calls)
                        <Route path=path!("/safe") view=pages::Safe/>
                        // PRD-UI-003: Style audit (dev tool, no sensitive data)
                        <Route path=path!("/style-audit") view=pages::StyleAudit/>
                        // Backward compatibility redirects
                        <Route path=path!("/flight-recorder") view=|| view! { <ProtectedRoute><Redirect path="/runs"/></ProtectedRoute> }/>
                        <Route path=path!("/flight-recorder/:id") view=|| view! { <ProtectedRoute><FlightRecorderIdRedirect/></ProtectedRoute> }/>

                        // All protected routes share a single Shell instance via ParentRoute.
                        // This prevents Shell (TopBar, Taskbar, SystemTray) from being
                        // disposed and recreated on every SPA navigation.
                        <ParentRoute path=path!("") view=|| view! { <ProtectedRoute><Shell/></ProtectedRoute> }>
                            <Route path=path!("/") view=pages::Dashboard/>
                            <Route path=path!("/dashboard") view=pages::Dashboard/>
                            <Route path=path!("/adapters") view=pages::Adapters/>
                            <Route path=path!("/adapters/:id") view=pages::AdapterDetail/>
                            <Route path=path!("/update-center") view=pages::UpdateCenter/>
                            <Route path=path!("/chat") view=pages::Chat/>
                            <Route path=path!("/chat/:session_id") view=pages::ChatSession/>
                            <Route path=path!("/system") view=pages::System/>
                            <Route path=path!("/settings") view=pages::Settings/>
                            <Route path=path!("/user") view=pages::User/>
                            <Route path=path!("/models") view=pages::Models/>
                            <Route path=path!("/models/:id") view=pages::ModelDetail/>
                            <Route path=path!("/policies") view=pages::Policies/>
                            <Route path=path!("/training") view=pages::Training/>
                            <Route path=path!("/training/:id") view=pages::training::TrainingDetailRoute/>
                            <Route path=path!("/stacks") view=pages::Stacks/>
                            <Route path=path!("/stacks/:id") view=pages::StackDetail/>
                            <Route path=path!("/collections") view=pages::Collections/>
                            <Route path=path!("/collections/:id") view=pages::CollectionDetail/>
                            <Route path=path!("/documents") view=pages::Documents/>
                            <Route path=path!("/documents/:id") view=pages::DocumentDetail/>
                            <Route path=path!("/datasets") view=pages::Datasets/>
                            <Route path=path!("/datasets/:id") view=pages::DatasetDetail/>
                            <Route path=path!("/admin") view=pages::Admin/>
                            <Route path=path!("/audit") view=pages::Audit/>
                            <Route path=path!("/runs") view=pages::FlightRecorder/>
                            <Route path=path!("/runs/:id") view=pages::FlightRecorderDetail/>
                            <Route path=path!("/diff") view=pages::Diff/>
                            <Route path=path!("/workers") view=pages::Workers/>
                            <Route path=path!("/workers/:id") view=pages::WorkerDetail/>
                            <Route path=path!("/monitoring") view=pages::Monitoring/>
                            <Route path=path!("/errors") view=pages::Errors/>
                            <Route path=path!("/routing") view=pages::Routing/>
                            <Route path=path!("/repositories") view=pages::Repositories/>
                            <Route path=path!("/repositories/:id") view=pages::RepositoryDetail/>
                            <Route path=path!("/reviews/:pause_id") view=pages::ReviewDetail/>
                            <Route path=path!("/reviews") view=pages::Reviews/>
                            <Route path=path!("/welcome") view=pages::Welcome/>
                            <Route path=path!("/agents") view=pages::Agents/>
                            <Route path=path!("/files") view=pages::FileBrowser/>
                        </ParentRoute>
                    </Routes>
                            // Global Command Palette overlay
                            <CommandPalette/>
                                </Router>
                                </RouteErrorBoundary>
                        </InFlightProvider>
                    </SearchProvider>
                </AppProviders>
            </AuthProvider>
        </>
    }
    .into_any()
}

/// Fatal error surface for missing API base URL configuration
#[component]
fn BaseUrlError(reason: String) -> impl IntoView {
    view! {
        <div class="min-h-screen flex items-center justify-center bg-background">
            <div class="card max-w-xl w-full mx-4 space-y-3 border-destructive p-6">
                <div class="flex items-center gap-2">
                    <span class="text-lg font-semibold text-destructive">"adapterOS UI cannot start"</span>
                </div>
                <p class="text-sm text-foreground/80">
                    "We couldn't determine the API base URL. Set AOS_API_BASE_URL at build time or serve the UI via ./start so the origin is correct."
                </p>
                <p class="text-xs text-foreground/60 font-mono break-all">{"Detail: "}{reason}</p>
                <div class="text-xs text-foreground/70 space-y-1">
                    <p>"Quick fixes:"</p>
                    <ul class="list-disc list-inside space-y-0.5">
                        <li>"Run with ./start up to serve UI and API from the same origin."</li>
                        <li>"Or export AOS_API_BASE_URL=https://your-api-host before trunk build/serve."</li>
                    </ul>
                </div>
            </div>
        </div>
    }
}

#[component]
fn AppProviders(children: Children) -> impl IntoView {
    provide_context(Arc::new(ApiClient::new()));
    provide_settings_context();
    provide_ui_profile_context();
    provide_notifications_context();
    provide_chat_context();
    provide_refetch_context();
    children()
}

#[component]
fn SearchProvider(children: Children) -> impl IntoView {
    let client = crate::api::use_api_client();
    provide_search_context(client);
    children()
}

/// Redirect component for /flight-recorder/:id -> /runs/:id (preserves query params)
#[component]
fn FlightRecorderIdRedirect() -> impl IntoView {
    use leptos_router::hooks::use_params_map;
    let params = use_params_map();
    let path = move || {
        let id = params.get().get("id").unwrap_or_default();
        // Get query string directly from window location
        let query_string = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();

        if query_string.is_empty() {
            format!("/runs/{}", id)
        } else {
            format!("/runs/{}{}", id, query_string)
        }
    };
    view! { <Redirect path=path()/> }
}

// PRD-UI-000: JS interop for boot diagnostics
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    /// Signal that WASM compilation is complete (called when wasm_bindgen start runs)
    #[wasm_bindgen(js_name = "aosWasmCompileDone")]
    fn signal_wasm_compile_done();
    /// Signal that WASM runtime is initialized (panic hooks, tracing set up)
    #[wasm_bindgen(js_name = "aosSignalWasmLoaded")]
    fn signal_wasm_loaded();
    /// Signal that the Leptos app has been mounted to the DOM
    #[wasm_bindgen(js_name = "aosSignalMounted")]
    fn signal_mounted();
    /// Show a panic overlay with error message and stack trace
    #[wasm_bindgen(js_name = "aosShowPanic")]
    fn show_panic(message: &str, stack_trace: &str);
    /// Get high-resolution timestamp (performance.now())
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

/// Boot timeline event logger with high-resolution timestamps.
/// Format: [boot T+{ms}ms] {phase}: {message}
pub fn boot_log(phase: &str, message: &str) {
    // Use a static to track boot start time
    static BOOT_START: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    let start = *BOOT_START.get_or_init(now);
    let elapsed = now() - start;
    web_sys::console::log_1(&format!("[boot T+{:.0}ms] {}: {}", elapsed, phase, message).into());
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
            let panic_message = format!("{}{}", message, location);

            // Persist panic to client error telemetry; ignore reporter failures.
            let _ = std::panic::catch_unwind({
                let panic_message = panic_message.clone();
                let redacted_stack = redacted_stack.clone();
                move || report_ui_panic(&panic_message, None, Some(&redacted_stack))
            });

            show_panic(&panic_message, &redacted_stack);
        }));
    });
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn mount() {
    // Boot timeline: T+0ms - WASM binary executing
    boot_log("wasm", "binary loaded, executing start");
    signal_wasm_compile_done();

    // Set up panic hooks FIRST - before any code that might panic
    console_error_panic_hook::set_once();
    set_dom_panic_hook();
    boot_log("wasm", "panic hooks installed");

    // PRD-UI-000: Initialize redaction patterns AFTER panic hooks
    // This ensures any regex compilation panic is caught by the hook
    let _ = REDACTION_PATTERNS.get_or_init(RedactionPatterns::new);
    boot_log("wasm", "redaction patterns compiled");

    // Initialize tracing (safe to call multiple times - ignore if already set)
    // Use OnceLock to ensure we only set tracing once, even if mount() is called multiple times
    static TRACING_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    TRACING_INIT.get_or_init(|| {
        // tracing_wasm::set_as_global_default() panics if called twice, so catch it
        let _ = std::panic::catch_unwind(|| {
            tracing_wasm::set_as_global_default();
        });
    });
    boot_log("wasm", "tracing initialized");

    // PRD-UI-000: Signal runtime is initialized
    signal_wasm_loaded();
    boot_log("mount", "runtime ready, mounting Leptos app");

    // Mount the Leptos app
    leptos::mount::mount_to_body(App);

    // PRD-UI-000: Signal app is mounted (triggers backend health check)
    signal_mounted();
    boot_log("mount", "app mounted to DOM");
}
