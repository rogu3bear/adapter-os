//! Async state handling components
//!
//! Standard components for handling async data loading states,
//! ensuring consistent UI patterns and no infinite spinners.

use crate::api::ApiError;
use crate::components::{Badge, BadgeVariant, Button, ButtonVariant, Spinner};
use adapteros_api_types::FailureCode;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Breadcrumb item for navigation hierarchy
#[derive(Debug, Clone)]
pub struct Breadcrumb {
    /// Display label for the breadcrumb
    pub label: String,
    /// Optional navigation href (None for current page)
    pub href: Option<String>,
}

impl Breadcrumb {
    /// Create a new breadcrumb with a link
    pub fn new(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    /// Create a breadcrumb for the current page (no link)
    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

/// Get the appropriate badge variant for a failure code
fn failure_code_variant(code: FailureCode) -> BadgeVariant {
    use FailureCode::*;

    match code {
        // Critical - Red (Destructive)
        MigrationInvalid
        | MigrationChecksumMismatch
        | MigrationOutOfOrder
        | DownMigrationBlocked
        | BootMigrationFailed
        | BootSeedFailed
        | BootBootstrapFailed
        | TenantAccessDenied
        | TlsCertificateError
        | ReceiptMismatch
        | PolicyDivergence => BadgeVariant::Destructive,

        // Retryable/Resource - Yellow (Warning)
        WorkerOverloaded
        | CpuThrottled
        | FileDescriptorExhausted
        | ThreadPoolSaturated
        | GpuUnavailable
        | OutOfMemory
        | KvQuotaExceeded
        | BootDbUnreachable
        | BootDependencyTimeout
        | CacheStale
        | DnsResolutionFailed
        | ProxyConnectionFailed
        | ThunderingHerdRejected => BadgeVariant::Warning,

        // Operational - Outline
        BackendFallback
        | CacheInvalidationFailed
        | CacheKeyNondeterministic
        | CacheSerializationError
        | MigrationFileMissing => BadgeVariant::Outline,

        // Informational - Default (blue)
        BootConfigInvalid
        | BootNoWorkers
        | BootNoModels
        | BootBackgroundTaskFailed
        | SchemaVersionAhead
        | EnvironmentMismatch
        | ModelLoadFailed
        | TraceWriteFailed
        | RateLimiterNotConfigured
        | InvalidRateLimitConfig => BadgeVariant::Default,
    }
}

/// Display a failure code as a badge with Title Case formatting
fn failure_code_label(code: FailureCode) -> String {
    // Use the canonical SCREAMING_SNAKE_CASE representation and convert to Title Case
    code.as_str()
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Check if an error is a server/network error that warrants "Copy error" action
fn is_copyable_error(error: &ApiError) -> bool {
    matches!(
        error,
        ApiError::Server(_)
            | ApiError::Network(_)
            | ApiError::Http {
                status: 500..=599,
                ..
            }
    ) || matches!(error, ApiError::Structured { .. } if error.code().is_some_and(|c| c.starts_with("5") || c.contains("SERVER") || c.contains("INTERNAL")))
}

/// Build a sanitized error payload for clipboard copy
fn build_error_payload(error: &ApiError) -> String {
    let mut payload = String::new();

    payload.push_str("=== AdapterOS Error Report ===\n\n");
    payload.push_str(&format!("Error: {}\n", error));

    if let Some(code) = error.code() {
        payload.push_str(&format!("Code: {}\n", code));
    }

    if let Some(fc) = error.failure_code() {
        payload.push_str(&format!("Failure Code: {}\n", fc.as_str()));
    }

    if let ApiError::Structured {
        details: Some(d), ..
    } = error
    {
        if let Ok(json) = serde_json::to_string_pretty(d) {
            payload.push_str(&format!("\nDetails:\n{}\n", json));
        }
    }

    // Add timestamp
    if let Some(window) = web_sys::window() {
        let now = js_sys::Date::new_0();
        payload.push_str(&format!("\nTimestamp: {}\n", now.to_iso_string()));

        // Add URL for context (sanitized - no query params)
        if let Ok(location) = window.location().pathname() {
            payload.push_str(&format!("Page: {}\n", location));
        }
    }

    payload
}

/// Copy text to clipboard using the Clipboard API
fn copy_to_clipboard(
    text: &str,
    on_success: impl Fn() + 'static,
    on_error: impl Fn(String) + 'static,
) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = if js_sys::Reflect::has(&navigator, &"clipboard".into()).unwrap_or(false) {
            js_sys::Reflect::get(&navigator, &"clipboard".into()).ok()
        } else {
            None
        };

        let Some(clipboard) = clipboard else {
            on_error("Clipboard API not available".to_string());
            return;
        };

        let text = text.to_string();
        let on_success = std::rc::Rc::new(std::cell::RefCell::new(Some(on_success)));
        let on_error = std::rc::Rc::new(std::cell::RefCell::new(Some(on_error)));

        let success_cb = on_success.clone();
        let error_cb = on_error.clone();

        let success_closure =
            wasm_bindgen::closure::Closure::once(Box::new(move |_: wasm_bindgen::JsValue| {
                if let Some(cb) = success_cb.borrow_mut().take() {
                    cb();
                }
            })
                as Box<dyn FnOnce(wasm_bindgen::JsValue)>);

        let error_closure =
            wasm_bindgen::closure::Closure::once(Box::new(move |e: wasm_bindgen::JsValue| {
                if let Some(cb) = error_cb.borrow_mut().take() {
                    cb(format!("{:?}", e));
                }
            })
                as Box<dyn FnOnce(wasm_bindgen::JsValue)>);

        let promise = js_sys::Promise::resolve(&clipboard);
        let _ = promise.then(&success_closure).catch(&error_closure);

        // Attempt write_text if available; ignore if missing.
        if let Ok(write_fn) = js_sys::Reflect::get(&clipboard, &"writeText".into()) {
            if let Ok(write_fn) = write_fn.dyn_into::<js_sys::Function>() {
                let _ = write_fn.call1(&clipboard, &text.into());
            }
        }

        success_closure.forget();
        error_closure.forget();
    }
}

/// Error display component with retry functionality
///
/// Enhanced to show:
/// - Error message
/// - Error code (if available)
/// - Failure code badge (if available, color-coded by severity)
/// - Collapsible details section (if available)
/// - Copy error action for server/network errors (5xx)
#[component]
pub fn ErrorDisplay(
    /// The error to display
    error: ApiError,
    /// Optional retry callback
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
) -> impl IntoView {
    let error_code = error.code().map(|s| s.to_string());
    let error_message = error.user_message();
    let raw_error_message = error.to_string();
    let failure_code = error.failure_code();
    let show_copy_action = is_copyable_error(&error);
    let error_for_copy = error.clone();

    // Extract details if this is a structured error
    let details = match &error {
        ApiError::Structured { details, .. } => details.clone(),
        _ => None,
    };

    // Signal for collapsible details state
    let (details_expanded, set_details_expanded) = signal(false);

    // Signal for copy button state
    let (copy_state, set_copy_state) = signal(CopyState::Idle);

    view! {
        <div class="rounded-lg border border-destructive bg-destructive/10 p-4 space-y-3">
            <div class="flex items-start gap-3">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-5 w-5 text-destructive shrink-0 mt-0.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <circle cx="12" cy="12" r="10"/>
                    <line x1="12" y1="8" x2="12" y2="12"/>
                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                </svg>
                <div class="flex-1 min-w-0 space-y-2">
                    <p class="text-sm font-medium text-destructive">
                        "Error"
                    </p>
                    <p class="text-sm text-destructive/80 break-words">
                        {error_message.clone()}
                    </p>
                    {(!raw_error_message.is_empty() && raw_error_message != error_message).then(|| view! {
                        <p class="text-xs text-muted-foreground break-words">
                            {raw_error_message.clone()}
                        </p>
                    })}

                    // Failure code badge
                    {failure_code.map(|fc| {
                        let variant = failure_code_variant(fc);
                        let label = failure_code_label(fc);
                        view! {
                            <div class="mt-2">
                                <Badge variant=variant>
                                    {label}
                                </Badge>
                            </div>
                        }
                    })}

                    // Error code (if different from failure_code)
                    {error_code.filter(|c| !c.is_empty() && failure_code.is_none()).map(|code| view! {
                        <p class="text-xs text-muted-foreground mt-2 font-mono">
                            "Code: "{code}
                        </p>
                    })}
                </div>
            </div>

            // Collapsible details section
            {details.map(|d| {
                let details_clone = d.clone();
                view! {
                    <div class="border-t border-destructive/20 pt-3">
                        <button
                            class="w-full flex items-center justify-between text-sm text-muted-foreground hover:text-foreground transition-colors"
                            on:click=move |_| { let _ = set_details_expanded.try_update(|e| *e = !*e); }
                            aria-expanded=move || details_expanded.try_get().unwrap_or(false).to_string()
                        >
                            <span class="font-medium">"Error Details"</span>
                            <svg
                                class=move || format!(
                                    "w-4 h-4 transition-transform {}",
                                    if details_expanded.try_get().unwrap_or(false) { "rotate-180" } else { "" }
                                )
                                fill="none"
                                stroke="currentColor"
                                viewBox="0 0 24 24"
                            >
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                            </svg>
                        </button>

                        {move || if details_expanded.try_get().unwrap_or(false) {
                            let json_pretty = serde_json::to_string_pretty(&details_clone)
                                .unwrap_or_else(|e| {
                                    format!(
                                        "[Error: JSON formatting failed - {}. Raw value: {:?}]",
                                        e, details_clone
                                    )
                                });

                            Some(view! {
                                <div class="mt-3 rounded bg-muted/50 p-3 max-h-64 overflow-auto">
                                    <pre class="font-mono text-xs text-foreground whitespace-pre-wrap break-words">
                                        {json_pretty}
                                    </pre>
                                </div>
                            })
                        } else {
                            None
                        }}
                    </div>
                }
            })}

            // Action buttons row (retry + copy error)
            {move || {
                let has_retry = on_retry.is_some();
                let has_copy = show_copy_action;

                if !has_retry && !has_copy {
                    return None;
                }

                let error_clone = error_for_copy.clone();

                Some(view! {
                    <div class="flex justify-end gap-3 pt-2 border-t border-destructive/20">
                        // Copy error button (for 5xx/network errors)
                        {has_copy.then(|| {
                            let error_for_handler = error_clone.clone();
                            view! {
                                <button
                                    class="inline-flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground font-medium transition-colors"
                                    on:click=move |_| {
                                        let payload = build_error_payload(&error_for_handler);
                                        let _ = set_copy_state.try_set(CopyState::Copying);
                                        copy_to_clipboard(
                                            &payload,
                                            move || { let _ = set_copy_state.try_set(CopyState::Copied); },
                                            move |_| { let _ = set_copy_state.try_set(CopyState::Failed); },
                                        );
                                    }
                                    disabled=move || matches!(copy_state.try_get().unwrap_or(CopyState::Idle), CopyState::Copying)
                                >
                                    {move || match copy_state.try_get().unwrap_or(CopyState::Idle) {
                                        CopyState::Idle => view! {
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
                                                <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
                                                <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
                                            </svg>
                                            "Copy error"
                                        }.into_any(),
                                        CopyState::Copying => view! {
                                            <Spinner/>
                                            "Copying..."
                                        }.into_any(),
                                        CopyState::Copied => view! {
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-4 w-4 text-success"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                            >
                                                <polyline points="20 6 9 17 4 12"/>
                                            </svg>
                                            "Copied!"
                                        }.into_any(),
                                        CopyState::Failed => view! {
                                            <svg
                                                xmlns="http://www.w3.org/2000/svg"
                                                class="h-4 w-4 text-destructive"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                            >
                                                <circle cx="12" cy="12" r="10"/>
                                                <line x1="15" y1="9" x2="9" y2="15"/>
                                                <line x1="9" y1="9" x2="15" y2="15"/>
                                            </svg>
                                            "Copy failed"
                                        }.into_any(),
                                    }}
                                </button>
                            }
                        })}

                        // Retry button
                        {on_retry.map(|retry| view! {
                            <button
                                class="inline-flex items-center gap-2 text-sm text-destructive hover:text-destructive/80 font-medium"
                                on:click=move |_| retry.run(())
                            >
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
                                    <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                                    <path d="M21 3v5h-5"/>
                                    <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                                    <path d="M3 21v-5h5"/>
                                </svg>
                                "Retry"
                            </button>
                        })}
                    </div>
                })
            }}
        </div>
    }
}

/// Copy button state
#[derive(Debug, Clone, Copy, PartialEq)]
enum CopyState {
    Idle,
    Copying,
    Copied,
    Failed,
}

/// Empty state variants for different contexts
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum EmptyStateVariant {
    /// Default empty state - no data exists yet (folder/inbox icon)
    #[default]
    Empty,
    /// Search/filter returned no results (search icon, warning color)
    NoResults,
    /// User lacks permission to view content (lock icon, error color)
    NoPermission,
    /// Content is not available (slash-circle icon, muted)
    Unavailable,
}

impl EmptyStateVariant {
    /// Default icon path for each variant
    fn default_icon(&self) -> &'static str {
        match self {
            Self::Empty => "M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4",
            Self::NoResults => "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z",
            Self::NoPermission => "M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z",
            Self::Unavailable => "M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636",
        }
    }

    /// CSS class for variant-specific styling
    fn css_class(&self) -> &'static str {
        match self {
            Self::Empty => "empty-state-empty",
            Self::NoResults => "empty-state-no-results",
            Self::NoPermission => "empty-state-no-permission",
            Self::Unavailable => "empty-state-unavailable",
        }
    }
}

/// Empty state display with variants, icons, and action support
#[component]
pub fn EmptyState(
    /// Title for the empty state
    #[prop(into)]
    title: String,
    /// Optional description/guidance
    #[prop(optional, into)]
    description: Option<String>,
    /// Empty state variant (determines default icon and styling)
    #[prop(optional)]
    variant: EmptyStateVariant,
    /// Optional custom icon SVG path (overrides variant default)
    #[prop(optional)]
    icon: Option<&'static str>,
    /// Optional action button label
    #[prop(optional, into)]
    action_label: Option<String>,
    /// Optional action button callback
    #[prop(optional)]
    on_action: Option<Callback<()>>,
    /// Optional secondary action label (e.g., "Learn more")
    #[prop(optional, into)]
    secondary_label: Option<String>,
    /// Optional secondary action href (renders as link)
    #[prop(optional, into)]
    secondary_href: Option<String>,
) -> impl IntoView {
    let icon_path = icon.unwrap_or_else(|| variant.default_icon());
    let variant_class = variant.css_class();
    let has_actions = action_label.is_some() || secondary_label.is_some();

    view! {
        <div class=format!("empty-state {}", variant_class)>
            <div class="empty-state-icon-wrapper">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="empty-state-icon"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d=icon_path/>
                </svg>
            </div>
            <h3 class="empty-state-title">{title}</h3>
            {description.map(|desc| view! {
                <p class="empty-state-description">{desc}</p>
            })}

            // Action buttons
            {has_actions.then(|| {
                let action_label_clone = action_label.clone();
                let secondary_label_clone = secondary_label.clone();
                let secondary_href_clone = secondary_href.clone();

                view! {
                    <div class="empty-state-actions">
                        {action_label_clone.map(|label| {
                            let cb = on_action;
                            view! {
                                <button
                                    class="btn btn-primary btn-md"
                                    on:click=move |_| {
                                        if let Some(ref callback) = cb {
                                            callback.run(());
                                        }
                                    }
                                >
                                    {label}
                                </button>
                            }
                        })}
                        {secondary_label_clone.map(|label| {
                            if let Some(href) = secondary_href_clone.clone() {
                                view! {
                                    <a href=href class="btn btn-ghost btn-md">
                                        {label}
                                    </a>
                                }.into_any()
                            } else {
                                view! {
                                    <span class="text-sm text-muted-foreground">{label}</span>
                                }.into_any()
                            }
                        })}
                    </div>
                }
            })}
        </div>
    }
}

/// Loading state display
#[component]
pub fn LoadingDisplay(
    /// Optional loading message
    #[prop(optional, into)]
    message: Option<String>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center justify-center py-12">
            <Spinner/>
            {message.map(|msg| view! {
                <p class="text-sm text-muted-foreground mt-3">{msg}</p>
            })}
        </div>
    }
}

/// Page header with title, breadcrumbs, and optional actions
#[component]
pub fn PageHeader(
    /// Page title
    #[prop(into)]
    title: String,
    /// Optional subtitle/description
    #[prop(optional, into)]
    subtitle: Option<String>,
    /// Optional breadcrumb navigation
    #[prop(optional)]
    breadcrumbs: Option<Vec<Breadcrumb>>,
    /// Optional action buttons (rendered on the right)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="page-header">
            // Breadcrumb navigation
            {breadcrumbs.map(|crumbs| {
                view! {
                    <nav class="page-header-breadcrumbs" aria-label="Breadcrumb">
                        <ol class="page-header-breadcrumb-list">
                            {crumbs.into_iter().enumerate().map(|(idx, crumb)| {
                                let label = crumb.label.clone();
                                let href = crumb.href.clone();

                                view! {
                                    <li class="page-header-breadcrumb-item">
                                        {if idx > 0 {
                                            Some(view! {
                                                <span class="page-header-breadcrumb-separator" aria-hidden="true">
                                                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7"/>
                                                    </svg>
                                                </span>
                                            })
                                        } else {
                                            None
                                        }}
                                        {if let Some(href) = href {
                                            view! {
                                                <a href=href class="page-header-breadcrumb-link">
                                                    {label}
                                                </a>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <span class="page-header-breadcrumb-current" aria-current="page">
                                                    {label}
                                                </span>
                                            }.into_any()
                                        }}
                                    </li>
                                }
                            }).collect::<Vec<_>>()}
                        </ol>
                    </nav>
                }
            })}

            // Title and actions row
            <div class="flex items-center justify-between">
                <div>
                    <h1 class="heading-1">{title}</h1>
                    {subtitle.map(|s| view! {
                        <p class="body-small text-muted-foreground mt-1">{s}</p>
                    })}
                </div>
                {children.map(|c| view! {
                    <div class="flex items-center gap-2">
                        {c()}
                    </div>
                })}
            </div>
        </div>
    }
}

/// Refresh button component
#[component]
pub fn RefreshButton(
    /// Callback when clicked
    on_click: Callback<()>,
    /// Optional loading state
    #[prop(optional)]
    loading: Option<RwSignal<bool>>,
) -> impl IntoView {
    let is_loading = move || loading.and_then(|l| l.try_get()).unwrap_or(false);

    view! {
        <Button
            variant=ButtonVariant::Secondary
            disabled=Signal::derive(is_loading)
            on_click=Callback::new(move |_| on_click.run(()))
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4"
                class:animate-spin=is_loading
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/>
                <path d="M21 3v5h-5"/>
                <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/>
                <path d="M3 21v-5h5"/>
            </svg>
            {move || if is_loading() { "Refreshing..." } else { "Refresh" }}
        </Button>
    }
}

/// Detail row component for key-value displays
#[component]
pub fn DetailRow(
    /// Label for the row
    label: &'static str,
    /// Value to display
    #[prop(into)]
    value: String,
    /// Optional mono font for value
    #[prop(optional)]
    mono: bool,
) -> impl IntoView {
    let value_class = if mono {
        "font-medium font-mono text-sm"
    } else {
        "font-medium"
    };

    view! {
        <div class="flex items-start justify-between gap-3 min-w-0 py-1">
            <span class="text-muted-foreground shrink-0">{label}</span>
            <span class=format!("{} min-w-0 text-right break-all", value_class)>{value}</span>
        </div>
    }
}

/// Async boundary component for handling LoadingState
///
/// Replaces repetitive `match loading_state` patterns across pages.
/// Automatically renders LoadingDisplay, ErrorDisplay, or loaded content based on state.
///
/// # Example
/// ```rust,ignore
/// let (data, refetch) = use_api_resource(|client| async move { client.list_items().await });
///
/// view! {
///     <AsyncBoundary
///         state=data
///         on_retry=Callback::new(move |_| refetch.run(()))
///         render=move |items| view! { <ItemList items=items /> }
///     />
/// }
/// ```
#[component]
pub fn AsyncBoundary<T, V, F>(
    /// The loading state signal
    state: ReadSignal<crate::hooks::LoadingState<T>>,
    /// Optional retry callback for error state
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
    /// Optional loading message
    #[prop(optional, into)]
    loading_message: Option<String>,
    /// Render function called with loaded data
    render: F,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T) -> V + Clone + Send + 'static,
{
    let render = render.clone();

    view! {
        {move || {
            let render = render.clone();
            let current = match state.try_get() {
                Some(s) => s,
                None => return view! { <LoadingDisplay /> }.into_any(),
            };
            match current {
                crate::hooks::LoadingState::Idle | crate::hooks::LoadingState::Loading => {
                    match loading_message.clone() {
                        Some(msg) => view! { <LoadingDisplay message=msg /> }.into_any(),
                        None => view! { <LoadingDisplay /> }.into_any(),
                    }
                }
                crate::hooks::LoadingState::Loaded(data) => {
                    render(data).into_any()
                }
                crate::hooks::LoadingState::Error(e) => {
                    match on_retry {
                        Some(retry) => view! { <ErrorDisplay error=e on_retry=retry /> }.into_any(),
                        None => view! { <ErrorDisplay error=e /> }.into_any(),
                    }
                }
            }
        }}
    }
}

/// Async boundary with custom error rendering
///
/// Like AsyncBoundary but allows custom error rendering for cases where
/// different error types need different UI treatment (e.g., validation errors).
///
/// # Example
/// ```rust,ignore
/// <AsyncBoundaryWithErrorRender
///     state=data
///     on_retry=Callback::new(move |_| refetch.run(()))
///     render=move |data| view! { <DataView data=data /> }
///     render_error=move |e, retry| {
///         if let ApiError::Validation(msg) = &e {
///             view! { <ValidationErrorView message=msg.clone() /> }.into_any()
///         } else {
///             view! { <ErrorDisplay error=e on_retry=retry /> }.into_any()
///         }
///     }
/// />
/// ```
#[component]
pub fn AsyncBoundaryWithErrorRender<T, V, F, EV, EF>(
    /// The loading state signal
    state: ReadSignal<crate::hooks::LoadingState<T>>,
    /// Optional retry callback for error state
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
    /// Optional loading message
    #[prop(optional, into)]
    loading_message: Option<String>,
    /// Render function called with loaded data
    render: F,
    /// Custom error render function - receives error and optional retry callback
    render_error: EF,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T) -> V + Clone + Send + 'static,
    EV: IntoView + 'static,
    EF: Fn(ApiError, Option<Callback<()>>) -> EV + Clone + Send + 'static,
{
    let render = render.clone();
    let render_error = render_error.clone();

    view! {
        {move || {
            let render = render.clone();
            let render_error = render_error.clone();
            let current = match state.try_get() {
                Some(s) => s,
                None => return view! { <LoadingDisplay /> }.into_any(),
            };
            match current {
                crate::hooks::LoadingState::Idle | crate::hooks::LoadingState::Loading => {
                    match loading_message.clone() {
                        Some(msg) => view! { <LoadingDisplay message=msg /> }.into_any(),
                        None => view! { <LoadingDisplay /> }.into_any(),
                    }
                }
                crate::hooks::LoadingState::Loaded(data) => {
                    render(data).into_any()
                }
                crate::hooks::LoadingState::Error(e) => {
                    render_error(e, on_retry).into_any()
                }
            }
        }}
    }
}

/// Async boundary with empty state handling
///
/// Like AsyncBoundary but also handles the case when loaded data is empty.
/// Useful for list views where you want to show a specific empty state.
///
/// # Example
/// ```rust,ignore
/// let (data, _) = use_api_resource(|client| async move { client.list_items().await });
///
/// view! {
///     <AsyncBoundaryWithEmpty
///         state=data
///         is_empty=|items: &Vec<Item>| items.is_empty()
///         empty_title="No items"
///         empty_description="Create your first item to get started."
///         render=move |items| view! { <ItemList items=items /> }
///     />
/// }
/// ```
#[component]
pub fn AsyncBoundaryWithEmpty<T, V, F, E>(
    /// The loading state signal
    state: ReadSignal<crate::hooks::LoadingState<T>>,
    /// Function to check if data is empty
    is_empty: E,
    /// Optional retry callback for error state
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
    /// Optional loading message
    #[prop(optional, into)]
    loading_message: Option<String>,
    /// Empty state title
    #[prop(into)]
    empty_title: String,
    /// Optional empty state description
    #[prop(optional, into)]
    empty_description: Option<String>,
    /// Optional empty state variant
    #[prop(optional)]
    empty_variant: EmptyStateVariant,
    /// Render function called with loaded data (when not empty)
    render: F,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    V: IntoView + 'static,
    F: Fn(T) -> V + Clone + Send + 'static,
    E: Fn(&T) -> bool + Clone + Send + 'static,
{
    let render = render.clone();
    let is_empty = is_empty.clone();
    let empty_title = empty_title.clone();
    let empty_description = empty_description.clone();

    view! {
        {move || {
            let render = render.clone();
            let is_empty = is_empty.clone();
            let empty_title = empty_title.clone();
            let empty_desc = empty_description.clone();

            let current = match state.try_get() {
                Some(s) => s,
                None => return view! { <LoadingDisplay /> }.into_any(),
            };
            match current {
                crate::hooks::LoadingState::Idle | crate::hooks::LoadingState::Loading => {
                    match loading_message.clone() {
                        Some(msg) => view! { <LoadingDisplay message=msg /> }.into_any(),
                        None => view! { <LoadingDisplay /> }.into_any(),
                    }
                }
                crate::hooks::LoadingState::Loaded(data) => {
                    if is_empty(&data) {
                        match empty_desc {
                            Some(desc) => view! {
                                <EmptyState
                                    title=empty_title
                                    description=desc
                                    variant=empty_variant
                                />
                            }.into_any(),
                            None => view! {
                                <EmptyState
                                    title=empty_title
                                    variant=empty_variant
                                />
                            }.into_any(),
                        }
                    } else {
                        render(data).into_any()
                    }
                }
                crate::hooks::LoadingState::Error(e) => {
                    match on_retry {
                        Some(retry) => view! { <ErrorDisplay error=e on_retry=retry /> }.into_any(),
                        None => view! { <ErrorDisplay error=e /> }.into_any(),
                    }
                }
            }
        }}
    }
}
