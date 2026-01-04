//! Notification provider component
//!
//! Wraps the application to provide notification context and render toasts.

use crate::components::toast::ToastContainer;
use crate::signals::notifications::provide_notifications_context;
use leptos::prelude::*;

/// Notification provider component
///
/// Provides notification context to all children and renders the toast container.
/// Should be placed high in the component tree, typically wrapping the main app.
///
/// # Example
///
/// ```rust,ignore
/// use adapteros_ui::components::NotificationProvider;
/// use adapteros_ui::signals::use_notifications;
///
/// #[component]
/// fn App() -> impl IntoView {
///     view! {
///         <NotificationProvider>
///             <MyApp />
///         </NotificationProvider>
///     }
/// }
///
/// #[component]
/// fn MyApp() -> impl IntoView {
///     let notifications = use_notifications();
///
///     view! {
///         <button on:click=move |_| {
///             notifications.success("Success", "Operation completed!");
///         }>
///             "Show Toast"
///         </button>
///     }
/// }
/// ```
#[component]
pub fn NotificationProvider(children: Children) -> impl IntoView {
    // Initialize notification context
    provide_notifications_context();

    view! {
        {children()}
        <ToastContainer />
    }
}
