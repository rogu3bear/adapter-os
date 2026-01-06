//! Responsive helpers and breakpoint hooks.

use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

/// Screen size breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    Mobile,
    Tablet,
    Desktop,
    Wide,
}

#[cfg(target_arch = "wasm32")]
fn breakpoint_for_width(width: f64) -> Breakpoint {
    if width < 640.0 {
        Breakpoint::Mobile
    } else if width < 1024.0 {
        Breakpoint::Tablet
    } else if width < 1280.0 {
        Breakpoint::Desktop
    } else {
        Breakpoint::Wide
    }
}

fn current_breakpoint() -> Breakpoint {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(width) = window.inner_width() {
                if let Some(width) = width.as_f64() {
                    return breakpoint_for_width(width);
                }
            }
        }
    }
    Breakpoint::Desktop
}

/// Track the current breakpoint.
pub fn use_breakpoint() -> ReadSignal<Breakpoint> {
    let (breakpoint, set_breakpoint) = signal(current_breakpoint());

    #[cfg(target_arch = "wasm32")]
    {
        let set_breakpoint = set_breakpoint.clone();
        let handler = Closure::wrap(Box::new(move || {
            set_breakpoint.set(current_breakpoint());
        }) as Box<dyn FnMut()>);

        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("resize", handler.as_ref().unchecked_ref());
        }

        handler.forget();
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_breakpoint;

    breakpoint
}

/// True when breakpoint is mobile.
pub fn use_is_mobile() -> Memo<bool> {
    let bp = use_breakpoint();
    Memo::new(move |_| bp.get() == Breakpoint::Mobile)
}

/// True when breakpoint is tablet or smaller.
pub fn use_is_tablet_or_smaller() -> Memo<bool> {
    let bp = use_breakpoint();
    Memo::new(move |_| matches!(bp.get(), Breakpoint::Mobile | Breakpoint::Tablet))
}

/// True when breakpoint is desktop or larger.
pub fn use_is_desktop_or_larger() -> Memo<bool> {
    let bp = use_breakpoint();
    Memo::new(move |_| matches!(bp.get(), Breakpoint::Desktop | Breakpoint::Wide))
}
