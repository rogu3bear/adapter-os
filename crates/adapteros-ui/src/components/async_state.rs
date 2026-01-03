//! Async state handling components
//!
//! Standard components for handling async data loading states,
//! ensuring consistent UI patterns and no infinite spinners.

use crate::api::ApiError;
use crate::components::Spinner;
use leptos::prelude::*;

/// Error display component with retry functionality
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
                <div class="flex-1 min-w-0">
                    <p class="text-sm font-medium text-destructive">
                        "Error loading data"
                    </p>
                    <p class="text-sm text-destructive/80 mt-1 break-words">
                        {error_message}
                    </p>
                    {error_code.filter(|c| !c.is_empty()).map(|code| view! {
                        <p class="text-xs text-muted-foreground mt-2 font-mono">
                            "Code: "{code}
                        </p>
                    })}
                </div>
            </div>

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

/// Empty state display with optional guidance
#[component]
pub fn EmptyState(
    /// Title for the empty state
    #[prop(into)]
    title: String,
    /// Optional description/guidance
    #[prop(optional, into)]
    description: Option<String>,
    /// Optional icon (SVG path)
    #[prop(optional)]
    icon: Option<&'static str>,
) -> impl IntoView {
    let icon_path = icon.unwrap_or("M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4");

    view! {
        <div class="flex flex-col items-center justify-center py-12 text-center">
            <div class="rounded-full bg-muted p-3 mb-4">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="h-8 w-8 text-muted-foreground"
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
            <h3 class="text-lg font-medium text-foreground mb-1">{title}</h3>
            {description.map(|desc| view! {
                <p class="text-sm text-muted-foreground max-w-sm">{desc}</p>
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

/// Page header with title and optional actions
#[component]
pub fn PageHeader(
    /// Page title
    #[prop(into)]
    title: String,
    /// Optional subtitle/description
    #[prop(optional, into)]
    subtitle: Option<String>,
    /// Optional action buttons (rendered on the right)
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
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
