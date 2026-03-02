//! HUD keyboard shortcuts.
//!
//! Global keydown handler for the HUD shell:
//! - Cmd/Ctrl+K: Command palette
//! - Cmd/Ctrl+N: New conversation
//! - Escape: Close panel / dismiss
//! - Cmd/Ctrl+.: System panel
//! - Cmd/Ctrl+,: Settings panel
//! - / (not in input): Focus chat input

use crate::signals::use_search;
use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    static HUD_KEYDOWN_LISTENER: RefCell<Option<Closure<dyn FnMut(web_sys::KeyboardEvent)>>> =
        RefCell::new(None);
}

fn register_hud_keydown_listener(listener: Closure<dyn FnMut(web_sys::KeyboardEvent)>) {
    HUD_KEYDOWN_LISTENER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(window) = web_sys::window() {
            if let Some(existing) = slot.take() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    existing.as_ref().unchecked_ref(),
                );
            }
            let _ = window
                .add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
            *slot = Some(listener);
        }
    });
}

fn clear_hud_keydown_listener() {
    HUD_KEYDOWN_LISTENER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(existing) = slot.take() {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    existing.as_ref().unchecked_ref(),
                );
            }
        }
    });
}

/// Returns `true` when the active element is a text input, textarea, or
/// contenteditable — contexts where single-key shortcuts should not fire.
fn focus_is_in_text_field(element: &web_sys::HtmlElement) -> bool {
    let tag = element.tag_name().to_lowercase();
    if tag == "input" || tag == "textarea" {
        return true;
    }
    element
        .get_attribute("contenteditable")
        .is_some_and(|v| v == "true" || v.is_empty())
}

/// Register HUD-specific keyboard shortcuts on the global `keydown` event.
///
/// Call this once from `HudShell`. The listener is stored in a `thread_local!`
/// and cleaned up via `on_cleanup` to avoid accumulating handlers on remount.
pub fn use_hud_keyboard() {
    let search = use_search();
    let navigate = use_navigate();
    let location = use_location();

    Effect::new(move || {
        let search = search.clone();
        let navigate = navigate.clone();
        let location_pathname = location.pathname;

        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            let ctrl_or_cmd = event.ctrl_key() || event.meta_key();

            // Determine if focus is in a text field
            let in_text_field = event
                .target()
                .and_then(|t| t.dyn_ref::<web_sys::HtmlElement>().cloned())
                .is_some_and(|el| focus_is_in_text_field(&el));

            // --- Escape: always allowed, even in text fields ---
            if key == "Escape" {
                // If in a text field, just blur it
                if in_text_field {
                    if let Some(el) = event
                        .target()
                        .and_then(|t| t.dyn_ref::<web_sys::HtmlElement>().cloned())
                    {
                        let _ = el.blur();
                    }
                    event.prevent_default();
                    return;
                }

                // If on a non-chat route (a slide panel is open), go home
                let pathname = location_pathname.try_get_untracked().unwrap_or_default();
                let is_home_or_chat = matches!(pathname.as_str(), "/" | "/dashboard")
                    || pathname.starts_with("/chat");
                if !is_home_or_chat {
                    event.prevent_default();
                    navigate("/", Default::default());
                }
                return;
            }

            // --- Cmd/Ctrl+K: command palette (allowed even in text fields) ---
            if ctrl_or_cmd && key == "k" {
                event.prevent_default();
                search.toggle();
                return;
            }

            // --- All remaining shortcuts are suppressed while in a text field ---
            if in_text_field {
                return;
            }

            // --- Cmd/Ctrl+N: new conversation ---
            if ctrl_or_cmd && key == "n" {
                event.prevent_default();
                navigate("/chat", Default::default());
                return;
            }

            // --- Cmd/Ctrl+.: system panel ---
            if ctrl_or_cmd && key == "." {
                event.prevent_default();
                navigate("/system", Default::default());
                return;
            }

            // --- Cmd/Ctrl+,: settings panel ---
            if ctrl_or_cmd && key == "," {
                event.prevent_default();
                navigate("/settings", Default::default());
                return;
            }

            // --- / (bare): focus chat input ---
            if key == "/" {
                // Don't steal focus if the command palette is already open
                if search
                    .command_palette_open
                    .try_get_untracked()
                    .unwrap_or(false)
                {
                    return;
                }

                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Ok(Some(el)) = document.query_selector(".hud-input-textarea") {
                        if let Some(html_el) = el.dyn_ref::<web_sys::HtmlElement>() {
                            event.prevent_default();
                            let _ = html_el.focus();
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);

        register_hud_keydown_listener(closure);
        on_cleanup(clear_hud_keydown_listener);
    });
}
