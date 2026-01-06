//! Toast notification components
//!
//! Provides Toast and ToastContainer for displaying notifications.

use crate::signals::notifications::{use_notification_context, Toast as ToastData, ToastSeverity};
use leptos::prelude::*;

/// Single toast notification component
#[component]
pub fn ToastItem(
    /// The toast data
    toast: ToastData,
    /// Callback when dismissed
    #[prop(optional)]
    on_dismiss: Option<Callback<String>>,
) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let has_details = toast.details.is_some();
    let toast_id_dismiss = toast.id.clone();

    // Severity-based CSS classes
    let severity_class = toast.severity.class();
    let icon_class = toast.severity.icon_class();

    let dismissible = toast.dismissible;

    view! {
        <div
            class=format!("toast {}", severity_class)
            role="alert"
            aria-live={if toast.severity == ToastSeverity::Error { "assertive" } else { "polite" }}
        >
            // Icon
            <div class=icon_class>
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        d=toast.severity.icon_path()
                    />
                </svg>
            </div>

            // Content
            <div class="toast-content">
                <div class="toast-header">
                    <span class="toast-title">{toast.title.clone()}</span>
                    {move || {
                        if has_details {
                            let is_expanded = expanded.get();
                            view! {
                                <button
                                    class="toast-expand"
                                    on:click=move |_| expanded.update(|e| *e = !*e)
                                    aria-expanded=is_expanded.to_string()
                                    aria-label={if is_expanded { "Collapse details" } else { "Expand details" }}
                                >
                                    <svg
                                        class=move || if expanded.get() { "toast-expand-icon toast-expand-icon-open" } else { "toast-expand-icon" }
                                        xmlns="http://www.w3.org/2000/svg"
                                        fill="none"
                                        viewBox="0 0 24 24"
                                        stroke="currentColor"
                                        stroke-width="2"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
                                    </svg>
                                </button>
                            }.into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}
                </div>
                <p class="toast-message">{toast.message.clone()}</p>
                {move || {
                    if has_details && expanded.get() {
                        view! {
                            <div class="toast-details">
                                <pre class="toast-details-text">{toast.details.clone()}</pre>
                            </div>
                        }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }}
            </div>

            // Dismiss button
            {move || {
                if dismissible {
                    let id = toast_id_dismiss.clone();
                    view! {
                        <button
                            class="toast-close"
                            on:click=move |_| {
                                if let Some(ref cb) = on_dismiss {
                                    cb.run(id.clone());
                                }
                            }
                            aria-label="Dismiss notification"
                        >
                            <svg
                                xmlns="http://www.w3.org/2000/svg"
                                fill="none"
                                viewBox="0 0 24 24"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </div>
    }
}

/// Container for toast notifications
#[component]
pub fn ToastContainer() -> impl IntoView {
    let (state, action) = use_notification_context();

    view! {
        <div class="toast-container" aria-label="Notifications">
            <For
                each=move || state.get().toasts.clone()
                key=|toast| toast.id.clone()
                children=move |toast| {
                    let action = action.clone();
                    view! {
                        <ToastItem
                            toast=toast
                            on_dismiss=Callback::new(move |id: String| {
                                action.dismiss(&id);
                            })
                        />
                    }
                }
            />
        </div>
    }
}
