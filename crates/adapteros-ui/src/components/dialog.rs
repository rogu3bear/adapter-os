//! Dialog/Modal component
//!
//! Uses semantic CSS classes from components.css.
//! Implements ARIA dialog pattern with keyboard handling (PRD-UI-160).
//!
//! Accessibility features:
//! - Focus trap: Tab cycles through focusable elements within dialog
//! - Escape closes: Press Escape to close the dialog
//! - Focus restoration: Returns focus to trigger element on close
//! - ARIA: role="dialog", aria-modal="true", aria-labelledby, aria-describedby

use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Global counter for unique dialog IDs
static DIALOG_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique dialog ID
fn next_dialog_id() -> String {
    let id = DIALOG_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dialog-{}", id)
}

/// Selector for focusable elements within a dialog
const FOCUSABLE_SELECTOR: &str =
    "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1']):not([disabled])";

/// Get all focusable elements within a container element
fn get_focusable_elements(
    document: &web_sys::Document,
    dialog_id: &str,
) -> Vec<web_sys::HtmlElement> {
    let mut elements = Vec::new();

    // Build a selector that scopes to this dialog
    let scoped_selector = format!("#{} {}", dialog_id, FOCUSABLE_SELECTOR);

    if let Ok(node_list) = document.query_selector_all(&scoped_selector) {
        for i in 0..node_list.length() {
            if let Some(node) = node_list.item(i) {
                if let Ok(el) = node.dyn_into::<web_sys::HtmlElement>() {
                    elements.push(el);
                }
            }
        }
    }
    elements
}

/// Dialog size variants
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DialogSize {
    /// Small dialog (384px)
    Sm,
    /// Medium dialog (512px) - default
    #[default]
    Md,
    /// Large dialog (672px)
    Lg,
    /// Extra large dialog (896px)
    Xl,
    /// Full width (viewport - margins)
    Full,
}

impl DialogSize {
    fn class(&self) -> &'static str {
        match self {
            DialogSize::Sm => "dialog-sm",
            DialogSize::Md => "dialog-md",
            DialogSize::Lg => "dialog-lg",
            DialogSize::Xl => "dialog-xl",
            DialogSize::Full => "dialog-full",
        }
    }
}

/// Dialog component with keyboard handling and focus trap
///
/// Implements WCAG 2.1 modal dialog requirements:
/// - Escape key closes dialog
/// - Uses role="dialog" and aria-modal="true"
/// - Focus is moved to dialog on open
/// - Focus trap keeps Tab cycling within dialog
/// - Focus returns to trigger element on close
#[component]
pub fn Dialog(
    #[prop(into)] open: RwSignal<bool>,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] description: String,
    /// Dialog size variant (default: Md)
    #[prop(optional)]
    size: DialogSize,
    /// Enable scrollable content with max-height constraint
    #[prop(optional)]
    scrollable: bool,
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
    let dialog_id_for_focus = dialog_id.clone();
    let dialog_id_for_keyboard = dialog_id.clone();

    // Track the element ID that had focus before dialog opened (for focus restoration)
    // We store the ID as a String since HtmlElement is not Send+Sync
    let trigger_element_id: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let trigger_element_id_for_effect = trigger_element_id.clone();

    // Focus management - focus first focusable element when dialog opens
    // and restore focus when it closes
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let dialog_id = dialog_id_for_focus.clone();
        let trigger_id = trigger_element_id_for_effect.clone();

        if is_open {
            // Store the currently focused element's ID before opening
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                if let Some(active) = document.active_element() {
                    // Try to get the element's ID, or generate one
                    let id = active.id();
                    if !id.is_empty() {
                        *trigger_id.borrow_mut() = Some(id);
                    } else {
                        // Generate a temporary ID for focus restoration
                        let temp_id =
                            format!("dialog-trigger-{}", DIALOG_COUNTER.load(Ordering::Relaxed));
                        active.set_id(&temp_id);
                        *trigger_id.borrow_mut() = Some(temp_id);
                    }
                }
            }

            wasm_bindgen_futures::spawn_local(async move {
                // Small delay to ensure DOM is updated
                gloo_timers::future::TimeoutFuture::new(10).await;

                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(dialog) = document.get_element_by_id(&dialog_id) {
                        // Try to focus first focusable element
                        if let Ok(Some(focusable)) = dialog.query_selector(FOCUSABLE_SELECTOR) {
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
        } else {
            // Restore focus to trigger element when dialog closes
            if let Some(trigger_id_str) = trigger_id.borrow_mut().take() {
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(el) = document.get_element_by_id(&trigger_id_str) {
                        if let Some(html_el) = el.dyn_ref::<web_sys::HtmlElement>() {
                            let _ = html_el.focus();
                        }
                        // Clean up temporary IDs we created
                        if trigger_id_str.starts_with("dialog-trigger-") {
                            el.remove_attribute("id").ok();
                        }
                    }
                }
            }
        }
    });

    // Keyboard handler for Escape and Tab (focus trap)
    // Track if the handler should be active (Send+Sync for on_cleanup)
    let handler_active = Arc::new(AtomicBool::new(true));
    let handler_active_for_cleanup = Arc::clone(&handler_active);

    // Track if we've already registered the listener (prevent duplicate registration)
    let handler_registered = Arc::new(AtomicBool::new(false));

    // Register cleanup to disable the keyboard handler on unmount
    on_cleanup(move || {
        handler_active_for_cleanup.store(false, Ordering::SeqCst);
    });

    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };

        if is_open {
            // Only register if we haven't already
            if handler_registered.swap(true, Ordering::SeqCst) {
                return;
            }

            let dialog_id = dialog_id_for_keyboard.clone();
            let open_signal = open;
            let handler_active = Arc::clone(&handler_active);

            let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                // Check if handler is still active (component not unmounted)
                if !handler_active.load(Ordering::SeqCst) {
                    return;
                }

                // Check if this dialog is the one handling the event
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(dialog) = document.get_element_by_id(&dialog_id) {
                        if let Some(active) = document.active_element() {
                            // Only handle if focus is within this dialog or on backdrop
                            let is_within = dialog.contains(Some(&active))
                                || active.class_list().contains("dialog-overlay");

                            if !is_within {
                                return;
                            }

                            match event.key().as_str() {
                                "Escape" => {
                                    event.prevent_default();
                                    open_signal.set(false);
                                }
                                "Tab" => {
                                    // Implement focus trap
                                    let focusables = get_focusable_elements(&document, &dialog_id);
                                    if focusables.is_empty() {
                                        // No focusable elements, prevent Tab from leaving
                                        event.prevent_default();
                                        return;
                                    }

                                    let first = &focusables[0];
                                    let last = &focusables[focusables.len() - 1];

                                    if event.shift_key() {
                                        // Shift+Tab: if on first element, wrap to last
                                        if active == *first.as_ref() {
                                            event.prevent_default();
                                            let _ = last.focus();
                                        }
                                    } else {
                                        // Tab: if on last element, wrap to first
                                        if active == *last.as_ref() {
                                            event.prevent_default();
                                            let _ = first.focus();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);

            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
            }

            // Closure must be leaked (WASM limitation), but handler becomes no-op after cleanup
            closure.forget();
        }
    });

    view! {
        // Backdrop - uses .dialog-overlay CSS class
        <div
            class=move || {
                if open.try_get().unwrap_or(false) {
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
                if open.try_get().unwrap_or(false) {
                    let mut classes = vec!["dialog-content", size.class()];
                    if scrollable {
                        classes.push("dialog-scrollable");
                    }
                    classes.join(" ")
                } else {
                    "hidden".to_string()
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
