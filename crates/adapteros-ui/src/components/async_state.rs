//! Async state handling components
//!
//! Standard components for handling async data loading states,
//! ensuring consistent UI patterns and no infinite spinners.

use crate::api::ApiError;
use crate::components::{Badge, BadgeVariant, Spinner};
use adapteros_api_types::FailureCode;
use leptos::prelude::*;

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

/// Error display component with retry functionality
///
/// Enhanced to show:
/// - Error message
/// - Error code (if available)
/// - Failure code badge (if available, color-coded by severity)
/// - Collapsible details section (if available)
#[component]
pub fn ErrorDisplay(
    /// The error to display
    error: ApiError,
    /// Optional retry callback
    #[prop(optional)]
    on_retry: Option<Callback<()>>,
) -> impl IntoView {
    let error_code = error.code().map(|s| s.to_string());
    let error_message = error.to_string();
    let failure_code = error.failure_code();

    // Extract details if this is a structured error
    let details = match &error {
        ApiError::Structured { details, .. } => details.clone(),
        _ => None,
    };

    // Signal for collapsible details state
    let (details_expanded, set_details_expanded) = signal(false);

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
                        {error_message}
                    </p>

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
                            on:click=move |_| set_details_expanded.update(|e| *e = !*e)
                            aria-expanded=move || details_expanded.get().to_string()
                        >
                            <span class="font-medium">"Error Details"</span>
                            <svg
                                class=move || format!(
                                    "w-4 h-4 transition-transform {}",
                                    if details_expanded.get() { "rotate-180" } else { "" }
                                )
                                fill="none"
                                stroke="currentColor"
                                viewBox="0 0 24 24"
                            >
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                            </svg>
                        </button>

                        {move || if details_expanded.get() {
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

            {on_retry.map(|retry| view! {
                <div class="flex justify-end pt-2 border-t border-destructive/20">
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
                </div>
            })}
        </div>
    }
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
                            let cb = on_action.clone();
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
                    <h1 class="text-3xl font-bold tracking-tight">{title}</h1>
                    {subtitle.map(|s| view! {
                        <p class="text-muted-foreground mt-1">{s}</p>
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
    let is_loading = move || loading.map(|l| l.get()).unwrap_or(false);

    view! {
        <button
            class="inline-flex items-center justify-center rounded-md text-sm font-medium h-10 px-4 py-2 border border-input bg-background hover:bg-accent hover:text-accent-foreground disabled:opacity-50"
            disabled=is_loading
            on:click=move |_| on_click.run(())
        >
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 mr-2"
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
        </button>
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
        <div class="flex justify-between py-1">
            <span class="text-muted-foreground">{label}</span>
            <span class=value_class>{value}</span>
        </div>
    }
}

/// Async boundary component for handling LoadingState
///
/// Replaces repetitive `match loading_state` patterns across pages.
/// Automatically renders LoadingDisplay, ErrorDisplay, or children based on state.
///
/// # Example
/// ```rust,ignore
/// let (data, refetch) = use_api_resource(|client| async move { client.list_items().await });
///
/// view! {
///     <AsyncBoundary state=data on_retry=refetch>
///         {move |items| view! {
///             <ItemList items=items />
///         }}
///     </AsyncBoundary>
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
    /// Children render function - receives the loaded data
    children: F,
) -> impl IntoView
where
    T: Clone + 'static,
    V: IntoView + 'static,
    F: Fn(T) -> V + Clone + 'static,
{
    let children = children.clone();

    view! {
        {move || {
            let children = children.clone();
            match state.get() {
                crate::hooks::LoadingState::Idle | crate::hooks::LoadingState::Loading => {
                    view! {
                        <LoadingDisplay message=loading_message.clone() />
                    }.into_any()
                }
                crate::hooks::LoadingState::Loaded(data) => {
                    children(data).into_any()
                }
                crate::hooks::LoadingState::Error(e) => {
                    view! {
                        <ErrorDisplay error=e on_retry=on_retry />
                    }.into_any()
                }
            }
        }}
    }
}

/// Async boundary with empty state handling
///
/// Like AsyncBoundary but also handles the case when loaded data is empty.
/// Useful for list views where you want to show a specific empty state.
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
    /// Children render function - receives the loaded data
    children: F,
) -> impl IntoView
where
    T: Clone + 'static,
    V: IntoView + 'static,
    F: Fn(T) -> V + Clone + 'static,
    E: Fn(&T) -> bool + Clone + 'static,
{
    let children = children.clone();
    let is_empty = is_empty.clone();
    let empty_title = empty_title.clone();
    let empty_description = empty_description.clone();

    view! {
        {move || {
            let children = children.clone();
            let is_empty = is_empty.clone();
            let empty_title = empty_title.clone();
            let empty_desc = empty_description.clone();

            match state.get() {
                crate::hooks::LoadingState::Idle | crate::hooks::LoadingState::Loading => {
                    view! {
                        <LoadingDisplay message=loading_message.clone() />
                    }.into_any()
                }
                crate::hooks::LoadingState::Loaded(data) => {
                    if is_empty(&data) {
                        view! {
                            <EmptyState
                                title=empty_title
                                description=empty_desc
                                variant=empty_variant
                            />
                        }.into_any()
                    } else {
                        children(data).into_any()
                    }
                }
                crate::hooks::LoadingState::Error(e) => {
                    view! {
                        <ErrorDisplay error=e on_retry=on_retry />
                    }.into_any()
                }
            }
        }}
    }
}
