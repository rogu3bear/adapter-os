//! Route-level error boundary component
//!
//! Catches panics in route components and displays a recovery UI.
//! Uses Leptos `ErrorBoundary` with custom fallback rendering.

use leptos::prelude::*;

use crate::components::{Button, ButtonVariant, Card, IconWarning};

/// Route-level error boundary with recovery UI
///
/// Wraps route content and catches any panics or errors,
/// displaying a user-friendly recovery interface instead of a blank screen.
///
/// # Example
/// ```ignore
/// <RouteErrorBoundary>
///     <MyPageComponent/>
/// </RouteErrorBoundary>
/// ```
#[component]
pub fn RouteErrorBoundary(children: Children) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            // Collect error messages at render time
            let error_messages: Vec<String> = errors
                .get()
                .into_iter()
                .map(|(_, e)| e.to_string())
                .collect();

            view! {
                <ErrorRecoveryPanel error_messages=error_messages/>
            }
        }>
            {children()}
        </ErrorBoundary>
    }
}

/// Error recovery panel displayed when an error occurs
#[component]
fn ErrorRecoveryPanel(error_messages: Vec<String>) -> impl IntoView {
    let on_reload = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        }
    };

    let on_go_home = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/");
            }
        }
    };

    view! {
        <div class="flex items-center justify-center min-h-[400px] p-6">
            <Card title="Something went wrong".to_string()>
                <div class="space-y-4">
                    // Error icon
                    <div class="flex items-center gap-3 text-destructive">
                        <IconWarning class="h-6 w-6".to_string()/>
                        <span class="font-medium">"An error occurred while loading this page"</span>
                    </div>

                    // Error details (collapsible)
                    {(!error_messages.is_empty()).then(|| {
                        let msgs = error_messages.clone();
                        view! {
                            <details class="text-sm">
                                <summary class="cursor-pointer text-muted-foreground hover:text-foreground">
                                    "Show error details"
                                </summary>
                                <div class="mt-2 p-3 rounded-lg bg-muted/50 font-mono text-xs overflow-x-auto">
                                    {msgs.iter().map(|msg| view! {
                                        <p class="text-destructive">{msg.clone()}</p>
                                    }).collect::<Vec<_>>()}
                                </div>
                            </details>
                        }
                    })}

                    // Recovery actions
                    <div class="flex items-center gap-3 pt-2">
                        <Button
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(on_reload)
                        >
                            "Reload Page"
                        </Button>
                        <Button
                            variant=ButtonVariant::Secondary
                            on_click=Callback::new(on_go_home)
                        >
                            "Go to Dashboard"
                        </Button>
                    </div>

                    // Help text
                    <p class="text-xs text-muted-foreground">
                        "If this issue persists, please contact support or check the system logs."
                    </p>
                </div>
            </Card>
        </div>
    }
}

/// Minimal error boundary for inline error capture
///
/// Lighter-weight version that shows inline error message
/// without full recovery UI.
#[component]
pub fn InlineErrorBoundary(
    /// Fallback message when error occurs
    #[prop(default = "An error occurred")]
    fallback_message: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=move |_errors| {
            view! {
                <div class="flex items-center gap-2 text-sm text-destructive p-2 rounded bg-destructive/10">
                    <IconWarning class="h-4 w-4".to_string()/>
                    <span>{fallback_message}</span>
                </div>
            }
        }>
            {children()}
        </ErrorBoundary>
    }
}
