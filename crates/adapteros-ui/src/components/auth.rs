//! Authentication components
//!
//! Route guards and auth-related UI components.

use crate::components::{Button, ButtonVariant, Card, Spinner};
use crate::signals::{use_auth, AuthState};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
fn current_pathname() -> Option<String> {
    web_sys::window().and_then(|window| window.location().pathname().ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn current_pathname() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn redirect_to(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn redirect_to(path: &str) {
    let _ = path;
}

/// Protected route wrapper
///
/// Redirects to login if not authenticated.
/// Shows error UI for auth failures (with retry for transient errors).
/// Shows timeout error if auth check takes too long.
/// Uses pathname check to prevent redirect loops when already on login page.
#[component]
pub fn ProtectedRoute(children: ChildrenFn) -> impl IntoView {
    let (auth_state, auth_action) = use_auth();

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
    let needs_redirect =
        Memo::new(move |_| matches!(auth_state.try_get(), Some(AuthState::Unauthenticated)));

    // Handle redirect for unauthenticated
    // Only redirect if not already on login page to prevent infinite loop
    // Capture current path as returnUrl so user returns after login
    Effect::new(move || {
        if needs_redirect.try_get().unwrap_or(false) {
            if let Some(current) = current_pathname() {
                // Only redirect if we're not already on login page
                if current != "/login" {
                    // Encode current path as returnUrl query param
                    let encoded_path = urlencoding::encode(&current);
                    let login_url = format!("/login?returnUrl={}", encoded_path);
                    redirect_to(&login_url);
                }
            }
        }
    });

    // Retry handler for timeout/error states
    let retry_auth: Callback<()> = {
        let action = auth_action.clone();
        Callback::new(move |_| {
            let action = action.clone();
            #[cfg(all(feature = "hydrate", target_arch = "wasm32"))]
            wasm_bindgen_futures::spawn_local(async move {
                action.check_auth().await;
            });
            #[cfg(not(all(feature = "hydrate", target_arch = "wasm32")))]
            let _ = action;
        })
    };
    let retry_auth_error = retry_auth;

    // Self-heal guard: if auth remains loading unexpectedly, run one
    // explicit check to avoid stale loading overlays.
    let loading_recheck_scheduled = StoredValue::new(false);
    Effect::new(move || {
        if is_loading.get() {
            if !loading_recheck_scheduled.get_value() {
                loading_recheck_scheduled.set_value(true);
                let action = auth_action.clone();
                let state = auth_state;
                #[cfg(all(feature = "hydrate", target_arch = "wasm32"))]
                {
                    gloo_timers::callback::Timeout::new(1500, move || {
                        if !state.get_untracked().is_loading() {
                            return;
                        }
                        let action = action.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            action.check_auth().await;
                        });
                    })
                    .forget();
                }
                #[cfg(not(all(feature = "hydrate", target_arch = "wasm32")))]
                {
                    let _ = action;
                    let _ = state;
                }
            }
        } else if loading_recheck_scheduled.get_value() {
            loading_recheck_scheduled.set_value(false);
        }
    });

    view! {
        // Mount/unmount auth UI states (avoid hidden overlays intercepting clicks).
        <Show when=move || is_loading.get() fallback=|| ()>
            <div class="flex min-h-screen items-center justify-center">
                <div class="text-center">
                    <Spinner/>
                    <p class="mt-4 text-sm font-medium text-foreground">"AdapterOS"</p>
                    <p class="text-xs text-muted-foreground">"Signing you in"</p>
                </div>
            </div>
        </Show>

        <Show when=move || is_timeout.get() fallback=|| ()>
            <div class="flex min-h-screen items-center justify-center bg-muted/40">
                    <Card class="max-w-md text-center".to_string()>
                        <div class="text-destructive text-4xl mb-4">"!"</div>
                        <h2 class="heading-4 text-destructive mb-2">"Authentication Timeout"</h2>
                    <p class="text-sm text-muted-foreground mb-4">
                        "The authentication check is taking too long. The server may be unavailable."
                    </p>
                    <div class="flex gap-3 justify-center">
                        <Button
                            on_click=Callback::new({
                                let retry = retry_auth;
                                move |_| retry.run(())
                            })
                        >
                            "Retry"
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            on_click=Callback::new(move |_| {
                                redirect_to("/login");
                            })
                        >
                            "Go to Login"
                        </Button>
                    </div>
                </Card>
            </div>
        </Show>

        <Show when=move || error_info.get().is_some() fallback=|| ()>
            <div class="flex min-h-screen items-center justify-center bg-muted/40">
                {move || {
                    error_info.get().map(|(message, is_retryable, requires_login)| {
                        let retry_handler = retry_auth_error;
                        view! {
                            <Card class="max-w-md text-center".to_string()>
                                <div class="text-destructive text-4xl mb-4">"!"</div>
                                <h2 class="heading-4 text-destructive mb-2">"Authentication Error"</h2>
                                <p class="text-sm text-muted-foreground mb-4">
                                    {message}
                                </p>
                                <div class="flex gap-3 justify-center">
                                    {is_retryable.then(|| view! {
                                        <Button on_click=Callback::new(move |_| retry_handler.run(()))>
                                            "Retry"
                                        </Button>
                                    })}
                                    {requires_login.then(|| view! {
                                        <Button
                                            on_click=Callback::new(move |_| {
                                                redirect_to("/login");
                                            })
                                        >
                                            "Log In"
                                        </Button>
                                    })}
                                    {(!is_retryable && !requires_login).then(|| view! {
                                        <Button
                                            variant=ButtonVariant::Outline
                                            on_click=Callback::new(move |_| {
                                                redirect_to("/login");
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
        </Show>

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
    use crate::signals::{AuthAction, AuthContext, AuthState};

    // Provide auth context at the app level
    #[cfg(all(feature = "hydrate", target_arch = "wasm32"))]
    crate::signals::provide_auth_context();
    #[cfg(not(all(feature = "hydrate", target_arch = "wasm32")))]
    {
        use crate::api::ApiClient;
        use std::sync::Arc;

        let state = RwSignal::new(AuthState::Unknown);
        let action = AuthAction::new(Arc::new(ApiClient::new()), state);
        provide_context::<AuthContext>((state.read_only(), action));
    }

    // Note: Chat context is provided by AppProviders in lib.rs
    // Do NOT call provide_chat_context() here - that would create duplicate contexts

    children()
}
