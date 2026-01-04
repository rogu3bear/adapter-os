//! Responsive hooks for breakpoint-aware layouts
//!
//! Provides reactive breakpoint detection for building responsive UIs.
//! Uses window resize events to track viewport changes.

use leptos::prelude::*;

/// Breakpoint enum representing viewport widths
///
/// Follows common responsive design breakpoints:
/// - Mobile: < 640px
/// - Tablet: >= 640px and < 1024px
/// - Desktop: >= 1024px and < 1280px
/// - Wide: >= 1280px
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Breakpoint {
    /// Small screens, phones (< 640px)
    Mobile,
    /// Medium screens, tablets (>= 640px, < 1024px)
    Tablet,
    /// Large screens, desktop (>= 1024px, < 1280px)
    Desktop,
    /// Extra large screens (>= 1280px)
    Wide,
}

impl Breakpoint {
    /// Determine breakpoint from viewport width in pixels
    pub fn from_width(width: u32) -> Self {
        if width < 640 {
            Breakpoint::Mobile
        } else if width < 1024 {
            Breakpoint::Tablet
        } else if width < 1280 {
            Breakpoint::Desktop
        } else {
            Breakpoint::Wide
        }
    }

    /// Check if this breakpoint is at least the given size
    pub fn at_least(&self, other: Breakpoint) -> bool {
        match (self, other) {
            (_, Breakpoint::Mobile) => true,
            (Breakpoint::Mobile, _) => false,
            (_, Breakpoint::Tablet) => true,
            (Breakpoint::Tablet, _) => false,
            (_, Breakpoint::Desktop) => true,
            (Breakpoint::Desktop, _) => false,
            (Breakpoint::Wide, Breakpoint::Wide) => true,
        }
    }

    /// Check if this breakpoint is at most the given size
    pub fn at_most(&self, other: Breakpoint) -> bool {
        match (self, other) {
            (Breakpoint::Mobile, _) => true,
            (_, Breakpoint::Mobile) => false,
            (Breakpoint::Tablet, _) => true,
            (_, Breakpoint::Tablet) => false,
            (Breakpoint::Desktop, _) => true,
            (_, Breakpoint::Desktop) => false,
            (Breakpoint::Wide, Breakpoint::Wide) => true,
        }
    }
}

/// Get the current viewport width
#[cfg(target_arch = "wasm32")]
fn get_viewport_width() -> u32 {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .map(|w| w as u32)
        .unwrap_or(1024) // Default to desktop if unavailable
}

#[cfg(not(target_arch = "wasm32"))]
fn get_viewport_width() -> u32 {
    1024 // Default to desktop for SSR
}

/// Reactive hook that returns the current breakpoint
///
/// Listens to window resize events and updates automatically.
///
/// # Example
///
/// ```ignore
/// let breakpoint = use_breakpoint();
///
/// view! {
///     {move || match breakpoint.get() {
///         Breakpoint::Mobile => view! { <MobileLayout/> }.into_any(),
///         Breakpoint::Tablet => view! { <TabletLayout/> }.into_any(),
///         _ => view! { <DesktopLayout/> }.into_any(),
///     }}
/// }
/// ```
pub fn use_breakpoint() -> ReadSignal<Breakpoint> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let (breakpoint, set_breakpoint) = signal(Breakpoint::from_width(get_viewport_width()));

        // Create closure for resize handler
        let handler = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let width = get_viewport_width();
            let new_breakpoint = Breakpoint::from_width(width);
            set_breakpoint.set(new_breakpoint);
        }) as Box<dyn FnMut(_)>);

        // Attach resize listener immediately (not in Effect since we leak the closure)
        // This runs once per component mount and the closure lives for app lifetime
        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("resize", handler.as_ref().unchecked_ref());
        }
        // Leak the closure - it lives for app lifetime
        // This is intentional since the resize listener should persist
        handler.forget();

        breakpoint
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let (breakpoint, _set_breakpoint) = signal(Breakpoint::from_width(get_viewport_width()));
        breakpoint
    }
}

/// Reactive hook that returns true if the viewport is mobile-sized
///
/// This is a convenience wrapper around `use_breakpoint()` for the common
/// case of checking for mobile layouts.
///
/// # Example
///
/// ```ignore
/// let is_mobile = use_is_mobile();
///
/// view! {
///     <Show when=move || is_mobile.get() fallback=|| view! { <DesktopNav/> }>
///         <MobileNav/>
///     </Show>
/// }
/// ```
pub fn use_is_mobile() -> Memo<bool> {
    let breakpoint = use_breakpoint();
    Memo::new(move |_| breakpoint.get() == Breakpoint::Mobile)
}

/// Reactive hook that returns true if the viewport is tablet-sized or smaller
///
/// Useful for layouts that collapse to single column on smaller screens.
pub fn use_is_tablet_or_smaller() -> Memo<bool> {
    let breakpoint = use_breakpoint();
    Memo::new(move |_| breakpoint.get().at_most(Breakpoint::Tablet))
}

/// Reactive hook that returns true if the viewport is desktop-sized or larger
///
/// Useful for showing multi-column layouts and additional UI elements.
pub fn use_is_desktop_or_larger() -> Memo<bool> {
    let breakpoint = use_breakpoint();
    Memo::new(move |_| breakpoint.get().at_least(Breakpoint::Desktop))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_from_width() {
        assert_eq!(Breakpoint::from_width(320), Breakpoint::Mobile);
        assert_eq!(Breakpoint::from_width(639), Breakpoint::Mobile);
        assert_eq!(Breakpoint::from_width(640), Breakpoint::Tablet);
        assert_eq!(Breakpoint::from_width(1023), Breakpoint::Tablet);
        assert_eq!(Breakpoint::from_width(1024), Breakpoint::Desktop);
        assert_eq!(Breakpoint::from_width(1279), Breakpoint::Desktop);
        assert_eq!(Breakpoint::from_width(1280), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(1920), Breakpoint::Wide);
    }

    #[test]
    fn test_at_least() {
        assert!(Breakpoint::Wide.at_least(Breakpoint::Mobile));
        assert!(Breakpoint::Wide.at_least(Breakpoint::Tablet));
        assert!(Breakpoint::Wide.at_least(Breakpoint::Desktop));
        assert!(Breakpoint::Wide.at_least(Breakpoint::Wide));

        assert!(Breakpoint::Desktop.at_least(Breakpoint::Mobile));
        assert!(Breakpoint::Desktop.at_least(Breakpoint::Tablet));
        assert!(Breakpoint::Desktop.at_least(Breakpoint::Desktop));
        assert!(!Breakpoint::Desktop.at_least(Breakpoint::Wide));

        assert!(Breakpoint::Mobile.at_least(Breakpoint::Mobile));
        assert!(!Breakpoint::Mobile.at_least(Breakpoint::Tablet));
    }

    #[test]
    fn test_at_most() {
        assert!(Breakpoint::Mobile.at_most(Breakpoint::Mobile));
        assert!(Breakpoint::Mobile.at_most(Breakpoint::Tablet));
        assert!(Breakpoint::Mobile.at_most(Breakpoint::Desktop));
        assert!(Breakpoint::Mobile.at_most(Breakpoint::Wide));

        assert!(!Breakpoint::Desktop.at_most(Breakpoint::Mobile));
        assert!(!Breakpoint::Desktop.at_most(Breakpoint::Tablet));
        assert!(Breakpoint::Desktop.at_most(Breakpoint::Desktop));
        assert!(Breakpoint::Desktop.at_most(Breakpoint::Wide));
    }
}
