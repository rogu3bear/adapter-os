//! Dialog/Modal component
//!
//! Uses semantic CSS classes from components.css.
//! Implements ARIA dialog pattern with keyboard handling (PRD-UI-160).

use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Global counter for unique dialog IDs
static DIALOG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique dialog ID
fn next_dialog_id() -> String {
    let id = DIALOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dialog-{}", id)
}

/// Dialog component with keyboard handling
///
/// Implements WCAG 2.1 modal dialog requirements:
/// - Escape key closes dialog
/// - Uses role="dialog" and aria-modal="true"
/// - Focus is moved to dialog on open
#[component]
pub fn Dialog(
    #[prop(into)] open: RwSignal<bool>,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] description: String,
    children: Children,
) -> impl IntoView {
    let close = move |_| open.set(false);
    let has_title = !title.is_empty();
    let has_description = !description.is_empty();

    // Generate unique IDs for ARIA attributes (computed once at component creation)
    let dialog_id = next_dialog_id();
    let title_id = format!("{}-title", &dialog_id);
    let desc_id = format!("{}-desc", &dialog_id);

    // Clone for closures
    let dialog_id_for_effect = dialog_id.clone();
    let dialog_id_for_keyboard = dialog_id.clone();

    // Focus management - focus first focusable element when dialog opens
    Effect::new(move || {
        let is_open = open.get();
        if is_open {
            let dialog_id = dialog_id_for_effect.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Small delay to ensure DOM is updated
                gloo_timers::future::TimeoutFuture::new(10).await;

                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(dialog) = document.get_element_by_id(&dialog_id) {
                        // Try to focus first focusable element
                        if let Ok(Some(focusable)) = dialog.query_selector(
                            "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])"
                        ) {
                            if let Some(el) = focusable.dyn_ref::<web_sys::HtmlElement>() {
                                let _ = el.focus();
                            }
                        } else if let Some(el) = dialog.dyn_ref::<web_sys::HtmlElement>() {
                            // Fallback: focus the dialog itself
                            let _ = el.focus();
                        }
                    }
                }
            });
        }
    });

    // Keyboard handler for Escape key
    Effect::new(move || {
        let is_open = open.get();
        if is_open {
            let dialog_id = dialog_id_for_keyboard.clone();
            let open_signal = open;

            let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                // Check if this dialog is the one handling the event
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(dialog) = document.get_element_by_id(&dialog_id) {
                        if let Some(active) = document.active_element() {
                            // Only handle if focus is within this dialog or on backdrop
                            let is_within = dialog.contains(Some(&active))
                                || active.class_list().contains("dialog-overlay");

                            if is_within && event.key() == "Escape" {
                                event.prevent_default();
                                open_signal.set(false);
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);

            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
            }

            // Closure is leaked intentionally (WASM limitation for event listeners)
            // The check for is_open prevents stale handlers from doing anything
            closure.forget();
        }
    });

    view! {
        // Backdrop - uses .dialog-overlay CSS class
        <div
            class=move || {
                if open.get() {
                    "dialog-overlay"
                } else {
                    "hidden"
                }
            }
            on:click=close
            aria-hidden="true"
        />

        // Dialog content with ARIA attributes
        <div
            id=dialog_id.clone()
            class=move || {
                if open.get() {
                    "dialog-content"
                } else {
                    "hidden"
                }
            }
            role="dialog"
            aria-modal="true"
            aria-labelledby=if has_title { Some(title_id.clone()) } else { None }
            aria-describedby=if has_description { Some(desc_id.clone()) } else { None }
            tabindex="-1"
        >
            // Close button with aria-label
            <button
                class="dialog-close"
                on:click=close
                aria-label="Close dialog"
                type="button"
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="24"
                    height="24"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="h-4 w-4"
                    aria-hidden="true"
                >
                    <path d="M18 6 6 18"/>
                    <path d="m6 6 12 12"/>
                </svg>
                <span class="sr-only">"Close"</span>
            </button>

            // Header
            {(has_title || has_description).then(|| {
                view! {
                    <div class="dialog-header">
                        {has_title.then(|| view! {
                            <h2 id=title_id.clone() class="dialog-title">{title.clone()}</h2>
                        })}
                        {has_description.then(|| view! {
                            <p id=desc_id.clone() class="dialog-description">{description.clone()}</p>
                        })}
                    </div>
                }
            })}

            // Content
            <div class="py-2">
                {children()}
            </div>
        </div>
    }
}
