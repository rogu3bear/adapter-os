//! Route-level error boundary component
//!
//! Catches panics in route components and displays a recovery UI.
//! Uses Leptos `ErrorBoundary` with custom fallback rendering.
//! Errors are logged with a correlation ID for diagnostics.

use leptos::prelude::*;

use crate::components::{Button, ButtonVariant, Card, IconWarning};

/// Generate a short correlation ID for error boundary events.
fn generate_correlation_id() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let ts = js_sys::Date::now() as u64;
        let rand = (js_sys::Math::random() * 0xFFFF as f64) as u16;
        format!("eb-{:x}-{:04x}", ts % 0xFFFFFF, rand)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "eb-test".to_string()
    }
}

/// Route-level error boundary with recovery UI
///
/// Wraps route content and catches any panics or errors,
/// displaying a user-friendly recovery interface instead of a blank screen.
/// Each error occurrence is assigned a correlation ID and logged to the console.
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
            let correlation_id = generate_correlation_id();

            // Collect error messages at render time (try_get for disposal safety)
            let error_messages: Vec<String> = errors
                .try_get()
                .map(|errs| errs.into_iter().map(|(_, e)| e.to_string()).collect())
                .unwrap_or_default();

            // Log errors with correlation ID for diagnostics
            #[cfg(target_arch = "wasm32")]
            {
                let build_id = option_env!("AOS_BUILD_ID").unwrap_or("unknown");
                for msg in &error_messages {
                    web_sys::console::error_1(
                        &format!("[ErrorBoundary] corr={} build={} error={}", correlation_id, build_id, msg).into(),
                    );
                }
            }

            let cid = correlation_id.clone();
            view! {
                <ErrorRecoveryPanel error_messages=error_messages correlation_id=cid/>
            }
        }>
            {children()}
        </ErrorBoundary>
    }
}

/// Error recovery panel displayed when an error occurs.
///
/// Shows a user-friendly card with recovery actions and a correlation ID.
/// Error details are collapsed by default to avoid exposing raw internals.
#[component]
fn ErrorRecoveryPanel(
    error_messages: Vec<String>,
    /// Correlation ID for support/diagnostics
    #[prop(into)]
    correlation_id: String,
) -> impl IntoView {
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
        <div class="error-recovery-panel">
            <Card title="Something went wrong".to_string()>
                <div class="space-y-4">
                    // Error icon
                    <div class="flex items-center gap-3 text-destructive">
                        <IconWarning class="h-6 w-6".to_string()/>
                        <span class="font-medium">"An error occurred while loading this page"</span>
                    </div>

                    // Error details (collapsible, sanitized)
                    {(!error_messages.is_empty()).then(|| {
                        let msgs = error_messages.clone();
                        view! {
                            <details class="text-sm">
                                <summary class="cursor-pointer text-muted-foreground hover:text-foreground">
                                    "Show error details"
                                </summary>
                                <div class="mt-2 p-3 rounded-lg bg-muted/50 font-mono text-xs overflow-x-auto">
                                    {msgs.iter().map(|msg| view! {
                                        <p class="text-destructive">{crate::redact_sensitive_info(msg)}</p>
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
                            "Go to Home"
                        </Button>
                    </div>

                    // Correlation ID for support
                    <p class="text-xs text-muted-foreground">
                        "If this issue persists, reference ID: "
                        <code class="font-mono">{correlation_id}</code>
                    </p>
                </div>
            </Card>
        </div>
    }
}

/// Minimal error boundary for inline error capture
///
/// Lighter-weight version that shows inline error message
/// without full recovery UI. Logs the error with a correlation ID.
#[component]
pub fn InlineErrorBoundary(
    /// Fallback message when error occurs
    #[prop(default = "An error occurred")]
    fallback_message: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=move |errors| {
            let cid = generate_correlation_id();

            // Log inline boundary errors
            #[cfg(target_arch = "wasm32")]
            {
                let build_id = option_env!("AOS_BUILD_ID").unwrap_or("unknown");
                if let Some(errs) = errors.try_get() {
                    for (_, e) in errs.iter() {
                        web_sys::console::error_1(
                            &format!("[InlineErrorBoundary] corr={} build={} error={}", cid, build_id, e).into(),
                        );
                    }
                }
            }
            // Suppress unused variable warning on non-wasm targets
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (&errors, &cid);

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
