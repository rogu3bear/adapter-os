//! User page
//!
//! Redirects to /settings for consolidated account management.
//! This page exists for backward compatibility with existing bookmarks/links.

use leptos::prelude::*;
use leptos_router::components::Redirect;

/// User page - redirects to Settings
///
/// The User page previously duplicated settings content. Now it redirects
/// to /settings which is the canonical location for all account management:
/// - Profile information
/// - UI preferences
/// - API configuration
/// - System information
#[component]
pub fn User() -> impl IntoView {
    view! {
        <Redirect path="/settings"/>
    }
}
