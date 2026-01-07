//! Authentication components
//!
//! Route guards and auth-related UI components.

use crate::components::Spinner;
use crate::signals::{use_auth, AuthState};
use leptos::prelude::*;

/// Protected route wrapper
///
/// Redirects to login if not authenticated.
/// Shows timeout error if auth check takes too long.
/// Uses leptos_router navigation instead of full page reload to prevent infinite loops.
#[component]
pub fn ProtectedRoute(children: Children) -> impl IntoView {
    web_sys::console::log_1(&"[ProtectedRoute] Rendering...".into());
    let (auth_state, auth_action) = use_auth();
    web_sys::console::log_1(&format!("[ProtectedRoute] Auth state: {:?}", auth_state.get()).into());

    // Render children once - they'll be shown/hidden based on auth state
    let rendered_children = children();

    // Track if we should show content vs loading/redirect
    let show_content = Memo::new(move |_| matches!(auth_state.get(), AuthState::Authenticated(_)));

    let is_loading =
        Memo::new(move |_| matches!(auth_state.get(), AuthState::Unknown | AuthState::Loading));

    let is_timeout = Memo::new(move |_| matches!(auth_state.get(), AuthState::Timeout));

    // Track if we need to redirect (but don't redirect yet)
    let needs_redirect = Memo::new(move |_| {
        matches!(
            auth_state.get(),
            AuthState::Unauthenticated | AuthState::Error(_)
        )
    });

    // Handle redirect for unauthenticated
    // Only redirect if not already on login page to prevent infinite loop
    Effect::new(move || {
        if needs_redirect.get() {
            if let Some(window) = web_sys::window() {
                if let Ok(current) = window.location().pathname() {
                    // Only redirect if we're not already on login page
                    if current != "/login" {
                        let _ = window.location().set_href("/login");
                    }
                }
            }
        }
    });

    // Retry handler for timeout state
    let retry_auth = {
        let action = auth_action.clone();
        move |_| {
            let action = action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                action.check_auth().await;
            });
        }
    };

    view! {
        // Loading spinner (only during initial check, not timeout)
        <div
            class="flex min-h-screen items-center justify-center"
            style:display=move || if is_loading.get() { "flex" } else { "none" }
        >
            <div class="text-center">
                <Spinner/>
                <p class="mt-4 text-sm text-muted-foreground">"Checking authentication..."</p>
            </div>
        </div>

        // Timeout error state - user can retry or go to login
        <div
            class="flex min-h-screen items-center justify-center"
            style:display=move || if is_timeout.get() { "flex" } else { "none" }
        >
            <div class="text-center max-w-md p-6 rounded-lg border border-destructive/50 bg-destructive/10">
                <div class="text-destructive text-4xl mb-4">"⚠"</div>
                <h2 class="text-lg font-semibold text-destructive mb-2">"Authentication Timeout"</h2>
                <p class="text-sm text-muted-foreground mb-4">
                    "The authentication check is taking too long. The server may be unavailable."
                </p>
                <div class="flex gap-3 justify-center">
                    <button
                        class="px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90"
                        on:click=retry_auth.clone()
                    >
                        "Retry"
                    </button>
                    <a
                        href="/login"
                        class="px-4 py-2 rounded-md border border-input bg-background hover:bg-accent"
                    >
                        "Go to Login"
                    </a>
                </div>
            </div>
        </div>

        // Protected content
        <div style:display=move || if show_content.get() { "contents" } else { "none" }>
            {rendered_children}
        </div>
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
