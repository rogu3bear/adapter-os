//! Toast notification components
//!
//! Provides Toast and ToastContainer for displaying notifications.

use crate::signals::notifications::{use_notification_context, Toast as ToastData, ToastSeverity};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

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
                        let details_for_copy = toast.details.clone().unwrap_or_default();
                        let title_for_copy = toast.title.clone();
                        let message_for_copy = toast.message.clone();
                        let copy_clicked = RwSignal::new(false);
                        view! {
                            <div class="toast-details">
                                <pre class="toast-details-text">{toast.details.clone()}</pre>
                                <button
                                    class="toast-copy-btn"
                                    on:click=move |_| {
                                        // If details look like JSON (diagnostic bundle), copy as-is
                                        // Otherwise format as plain text
                                        let full_details = if details_for_copy.trim_start().starts_with('{') {
                                            details_for_copy.clone()
                                        } else {
                                            format!(
                                                "Error: {}\nMessage: {}\n\nDetails:\n{}",
                                                title_for_copy, message_for_copy, details_for_copy
                                            )
                                        };
                                        let copy_clicked = copy_clicked.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            if copy_to_clipboard(&full_details).await {
                                                copy_clicked.set(true);
                                                // Reset after 2 seconds
                                                let copy_clicked_reset = copy_clicked.clone();
                                                let handle = gloo_timers::callback::Timeout::new(2000, move || {
                                                    copy_clicked_reset.set(false);
                                                });
                                                handle.forget();
                                            }
                                        });
                                    }
                                    title="Copy diagnostic bundle to clipboard for error reporting"
                                >
                                    {move || if copy_clicked.get() { "Copied!" } else { "Copy Diagnostic Bundle" }}
                                </button>
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
///
/// Uses aria-live="polite" to announce notifications to screen readers
/// without interrupting the current task.
#[component]
pub fn ToastContainer() -> impl IntoView {
    let (state, action) = use_notification_context();

    view! {
        <div
            class="toast-container"
            aria-label="Notifications"
            aria-live="polite"
            aria-atomic="false"
        >
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

/// Copy text to clipboard using the Clipboard API
async fn copy_to_clipboard(text: &str) -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };

    let navigator = window.navigator();

    // Get clipboard from navigator using JS reflection
    let clipboard = js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard"))
        .ok()
        .filter(|v| !v.is_undefined());

    let clipboard = match clipboard {
        Some(c) => c,
        None => return false,
    };

    // Call writeText method
    let write_text_fn =
        match js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")) {
            Ok(f) => f,
            Err(_) => return false,
        };

    let write_text_fn = match write_text_fn.dyn_ref::<js_sys::Function>() {
        Some(f) => f,
        None => return false,
    };

    let promise = match write_text_fn.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let promise = match promise.dyn_into::<js_sys::Promise>() {
        Ok(p) => p,
        Err(_) => return false,
    };

    JsFuture::from(promise).await.is_ok()
}
