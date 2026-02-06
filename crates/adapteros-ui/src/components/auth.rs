//! Authentication components
//!
//! Route guards and auth-related UI components.

use crate::components::{Button, ButtonVariant, Card, Spinner};
use crate::signals::{use_auth, AuthState};
use leptos::prelude::*;

/// Protected route wrapper
///
/// Redirects to login if not authenticated.
/// Shows error UI for auth failures (with retry for transient errors).
/// Shows timeout error if auth check takes too long.
/// Uses pathname check to prevent redirect loops when already on login page.
#[component]
pub fn ProtectedRoute(children: ChildrenFn) -> impl IntoView {
    web_sys::console::log_1(&"[ProtectedRoute] Rendering...".into());
    let (auth_state, auth_action) = use_auth();
    web_sys::console::log_1(
        &format!(
            "[ProtectedRoute] Auth state: {:?}",
            auth_state.get_untracked()
        )
        .into(),
    );

    // Track if we should show content vs loading/redirect
    let show_content = Memo::new(move |_| matches!(auth_state.get(), AuthState::Authenticated(_)));

    let is_loading =
        Memo::new(move |_| matches!(auth_state.get(), AuthState::Unknown | AuthState::Loading));

    let is_timeout = Memo::new(move |_| matches!(auth_state.get(), AuthState::Timeout));

    // Track error state - show UI instead of silent redirect
    let error_info = Memo::new(move |_| {
        if let AuthState::Error(ref err) = auth_state.get() {
            Some((
                err.message().to_string(),
                err.is_retryable(),
                err.requires_login(),
            ))
        } else {
            None
        }
    });

    // Only redirect for Unauthenticated (not Error - we show UI for errors)
    let needs_redirect = Memo::new(move |_| matches!(auth_state.get(), AuthState::Unauthenticated));

    // Handle redirect for unauthenticated
    // Only redirect if not already on login page to prevent infinite loop
    // Capture current path as returnUrl so user returns after login
    Effect::new(move || {
        if needs_redirect.get() {
            if let Some(window) = web_sys::window() {
                if let Ok(current) = window.location().pathname() {
                    // Only redirect if we're not already on login page
                    if current != "/login" {
                        // Encode current path as returnUrl query param
                        let encoded_path = js_sys::encode_uri_component(&current);
                        let login_url = format!("/login?returnUrl={}", encoded_path);
                        let _ = window.location().set_href(&login_url);
                    }
                }
            }
        }
    });

    // Retry handler for timeout/error states
    let retry_auth = {
        let action = auth_action.clone();
        move |_| {
            let action = action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                action.check_auth().await;
            });
        }
    };
    let retry_auth_error = retry_auth.clone();

    view! {
        // Loading spinner (only during initial check)
        <div
            class="flex min-h-screen items-center justify-center"
            style:display=move || if is_loading.get() { "flex" } else { "none" }
        >
            <div class="text-center">
                <Spinner/>
                <p class="mt-4 text-sm font-medium text-foreground">"adapterOS"</p>
                <p class="text-xs text-muted-foreground">"Signing you in"</p>
            </div>
        </div>

        // Timeout error state - user can retry or go to login
        <div
            class="flex min-h-screen items-center justify-center bg-muted/40"
            style:display=move || if is_timeout.get() { "flex" } else { "none" }
        >
            <Card class="max-w-md text-center".to_string()>
                <div class="text-destructive text-4xl mb-4">"!"</div>
                <h2 class="heading-4 text-destructive mb-2">"Authentication Timeout"</h2>
                <p class="text-sm text-muted-foreground mb-4">
                    "The authentication check is taking too long. The server may be unavailable."
                </p>
                <div class="flex gap-3 justify-center">
                    <Button
                        on_click=Callback::new({
                            let retry = retry_auth.clone();
                            move |_| retry(())
                        })
                    >
                        "Retry"
                    </Button>
                    <Button
                        variant=ButtonVariant::Outline
                        on_click=Callback::new(move |_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().set_href("/login");
                            }
                        })
                    >
                        "Go to Login"
                    </Button>
                </div>
            </Card>
        </div>

        // Auth error state - show user-friendly message with appropriate actions
        <div
            class="flex min-h-screen items-center justify-center bg-muted/40"
            style:display=move || if error_info.get().is_some() { "flex" } else { "none" }
        >
            {move || {
                error_info.get().map(|(message, is_retryable, requires_login)| {
                    let retry_handler = retry_auth_error.clone();
                    view! {
                        <Card class="max-w-md text-center".to_string()>
                            <div class="text-destructive text-4xl mb-4">"!"</div>
                            <h2 class="heading-4 text-destructive mb-2">"Authentication Error"</h2>
                            <p class="text-sm text-muted-foreground mb-4">
                                {message}
                            </p>
                            <div class="flex gap-3 justify-center">
                                {is_retryable.then(|| view! {
                                    <Button
                                        on_click=Callback::new(move |_| retry_handler(()))
                                    >
                                        "Retry"
                                    </Button>
                                })}
                                {requires_login.then(|| view! {
                                    <Button
                                        on_click=Callback::new(move |_| {
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.location().set_href("/login");
                                            }
                                        })
                                    >
                                        "Log In"
                                    </Button>
                                })}
                                {(!is_retryable && !requires_login).then(|| view! {
                                    <Button
                                        variant=ButtonVariant::Outline
                                        on_click=Callback::new(move |_| {
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.location().set_href("/login");
                                            }
                                        })
                                    >
                                        "Go to Login"
                                    </Button>
                                })}
                            </div>
                        </Card>
                    }
                })
            }}
        </div>

        // Protected content - only render children when authenticated
        // Using Show ensures children() is only called when auth completes
        <Show when=move || show_content.get() fallback=|| ()>
            {children()}
        </Show>
    }
}

/// Auth provider wrapper
///
/// Provides auth context to the app and checks initial auth state.
#[component]
pub fn AuthProvider(children: Children) -> impl IntoView {
    web_sys::console::log_1(&"[AuthProvider] Initializing...".into());
    use crate::signals::provide_auth_context;

    // Provide auth context at the app level
    provide_auth_context();
    web_sys::console::log_1(&"[AuthProvider] Auth context provided".into());

    // Note: Chat context is provided by ChatProvider in lib.rs
    // Do NOT call provide_chat_context() here - that would create duplicate contexts

    children()
}
