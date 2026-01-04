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

    let dismissible = toast.dismissible;

    view! {
        <div
            class=format!("toast {}", severity_class)
            role="alert"
            aria-live={if toast.severity == ToastSeverity::Error { "assertive" } else { "polite" }}
        >
            // Icon
            <div class="toast__icon">
                <svg
                    class="toast__icon-svg"
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
            <div class="toast__content">
                <div class="toast__header">
                    <span class="toast__title">{toast.title.clone()}</span>
                    {move || {
                        if has_details {
                            let is_expanded = expanded.get();
                            view! {
                                <button
                                    class="toast__expand-btn"
                                    on:click=move |_| expanded.update(|e| *e = !*e)
                                    aria-expanded=is_expanded.to_string()
                                    aria-label={if is_expanded { "Collapse details" } else { "Expand details" }}
                                >
                                    <svg
                                        class=move || if expanded.get() { "toast__expand-icon toast__expand-icon--expanded" } else { "toast__expand-icon" }
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
                <p class="toast__message">{toast.message.clone()}</p>
                {move || {
                    if has_details && expanded.get() {
                        view! {
                            <div class="toast__details">
                                <pre class="toast__details-text">{toast.details.clone()}</pre>
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
                            class="toast__dismiss"
                            on:click=move |_| {
                                if let Some(ref cb) = on_dismiss {
                                    cb.run(id.clone());
                                }
                            }
                            aria-label="Dismiss notification"
                        >
                            <svg
                                class="toast__dismiss-icon"
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

        // Inline styles for toast components
        <style>
            {r#"
            /* Toast Container */
            .toast-container {
                position: fixed;
                bottom: 1rem;
                right: 1rem;
                display: flex;
                flex-direction: column-reverse;
                gap: 0.75rem;
                z-index: 50;
                pointer-events: none;
            }

            @media (max-width: 640px) {
                .toast-container {
                    left: 1rem;
                    right: 1rem;
                    bottom: 1rem;
                }

                .toast-container .toast {
                    min-width: auto;
                    max-width: none;
                }
            }

            /* Toast Base */
            .toast {
                display: flex;
                align-items: flex-start;
                gap: 0.75rem;
                padding: 1rem;
                border-radius: var(--radius, 0.5rem);
                background-color: var(--color-card, white);
                border: 1px solid var(--color-border);
                box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -2px rgba(0, 0, 0, 0.05);
                min-width: 320px;
                max-width: 420px;
                pointer-events: auto;
                animation: toast-slide-in 0.3s ease-out;
            }

            @keyframes toast-slide-in {
                from {
                    transform: translateX(100%);
                    opacity: 0;
                }
                to {
                    transform: translateX(0);
                    opacity: 1;
                }
            }

            /* Severity Variants */
            .toast--info {
                border-left: 4px solid hsl(200, 80%, 50%);
            }

            .toast--info .toast__icon {
                color: hsl(200, 80%, 50%);
                background-color: hsl(200, 80%, 95%);
            }

            .toast--success {
                border-left: 4px solid hsl(142, 76%, 36%);
            }

            .toast--success .toast__icon {
                color: hsl(142, 76%, 36%);
                background-color: hsl(142, 76%, 95%);
            }

            .toast--warning {
                border-left: 4px solid hsl(38, 92%, 50%);
            }

            .toast--warning .toast__icon {
                color: hsl(38, 92%, 40%);
                background-color: hsl(38, 92%, 95%);
            }

            .toast--error {
                border-left: 4px solid hsl(0, 84%, 60%);
            }

            .toast--error .toast__icon {
                color: hsl(0, 84%, 60%);
                background-color: hsl(0, 84%, 95%);
            }

            /* Toast Icon */
            .toast__icon {
                display: flex;
                align-items: center;
                justify-content: center;
                width: 2rem;
                height: 2rem;
                border-radius: 9999px;
                flex-shrink: 0;
            }

            .toast__icon-svg {
                width: 1.25rem;
                height: 1.25rem;
            }

            /* Toast Content */
            .toast__content {
                flex: 1;
                min-width: 0;
            }

            .toast__header {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 0.5rem;
            }

            .toast__title {
                font-weight: 600;
                font-size: 0.875rem;
                color: var(--color-foreground);
            }

            .toast__expand-btn {
                display: flex;
                align-items: center;
                justify-content: center;
                width: 1.25rem;
                height: 1.25rem;
                border: none;
                background: transparent;
                cursor: pointer;
                color: var(--color-muted-foreground);
                padding: 0;
                border-radius: 0.25rem;
            }

            .toast__expand-btn:hover {
                background-color: var(--color-accent);
            }

            .toast__expand-icon {
                width: 1rem;
                height: 1rem;
                transition: transform 0.2s ease;
            }

            .toast__expand-icon--expanded {
                transform: rotate(180deg);
            }

            .toast__message {
                font-size: 0.875rem;
                color: var(--color-muted-foreground);
                margin-top: 0.25rem;
                line-height: 1.4;
            }

            .toast__details {
                margin-top: 0.5rem;
                padding: 0.5rem;
                background-color: var(--color-muted);
                border-radius: 0.25rem;
                overflow: hidden;
            }

            .toast__details-text {
                font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                font-size: 0.75rem;
                color: var(--color-muted-foreground);
                white-space: pre-wrap;
                word-break: break-all;
                margin: 0;
                max-height: 150px;
                overflow-y: auto;
            }

            /* Toast Dismiss Button */
            .toast__dismiss {
                display: flex;
                align-items: center;
                justify-content: center;
                width: 1.5rem;
                height: 1.5rem;
                border: none;
                background: transparent;
                cursor: pointer;
                color: var(--color-muted-foreground);
                padding: 0;
                border-radius: 0.25rem;
                flex-shrink: 0;
            }

            .toast__dismiss:hover {
                background-color: var(--color-accent);
                color: var(--color-foreground);
            }

            .toast__dismiss-icon {
                width: 1rem;
                height: 1rem;
            }

            /* Dark mode adjustments */
            .dark .toast--info .toast__icon {
                background-color: hsl(200, 80%, 20%);
            }

            .dark .toast--success .toast__icon {
                background-color: hsl(142, 76%, 20%);
            }

            .dark .toast--warning .toast__icon {
                background-color: hsl(38, 92%, 20%);
            }

            .dark .toast--error .toast__icon {
                background-color: hsl(0, 84%, 20%);
            }
            "#}
        </style>
    }
}
