//! Authentication components
//!
//! Route guards and auth-related UI components.

use leptos::prelude::*;
use crate::signals::{use_auth, AuthState, provide_chat_context};
use crate::components::Spinner;

/// Protected route wrapper
///
/// Redirects to login if not authenticated.
#[component]
pub fn ProtectedRoute(children: Children) -> impl IntoView {
    let (auth_state, _) = use_auth();

    // Render children once - they'll be shown/hidden based on auth state
    let rendered_children = children();

    // Track if we should show content vs loading/redirect
    let show_content = Memo::new(move |_| {
        matches!(auth_state.get(), AuthState::Authenticated(_))
    });

    let is_loading = Memo::new(move |_| {
        matches!(auth_state.get(), AuthState::Unknown | AuthState::Loading)
    });

    // Handle redirect for unauthenticated
    Effect::new(move || {
        let state = auth_state.get();
        if matches!(state, AuthState::Unauthenticated | AuthState::Error(_)) {
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/login");
            }
        }
    });

    view! {
        // Loading spinner
        <div
            class="flex min-h-screen items-center justify-center"
            style:display=move || if is_loading.get() { "flex" } else { "none" }
        >
            <Spinner/>
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
    use crate::signals::provide_auth_context;

    // Provide auth context at the app level
    provide_auth_context();

    // Also provide chat context (for the persistent dock)
    provide_chat_context();

    children()
}
