//! Status Center Panel
//!
//! A comprehensive system status panel accessible via Ctrl+Shift+S.
//! Displays readiness checks, inference status, memory usage, and node health.
//!
//! ## Usage
//!
//! ```rust
//! use leptos::prelude::*;
//! use adapteros_ui::components::status_center::StatusCenter;
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     view! {
//!         // Add StatusCenter to your app root
//!         <StatusCenter />
//!         // ... rest of your app
//!     }
//! }
//! ```
//!
//! The Status Center can be toggled with Ctrl+Shift+S keyboard shortcut.

pub mod hooks;
pub mod items;
pub mod panel;
pub mod sections;

pub use hooks::{
    use_escape_key, use_keyboard_shortcut, use_status_center_shortcut, use_status_data,
    CombinedStatus, StatusLoadingState,
};
pub use items::{StatusItem, StatusItemAvailability, StatusItemMemory, StatusItemSeverity};
pub use panel::StatusCenterPanel;
pub use sections::{StatusDivider, StatusSection, StatusSectionBadgeVariant, StatusSectionLabel};

use leptos::prelude::*;

/// Main Status Center component
///
/// This component should be placed at the app root level.
/// It listens for Ctrl+Shift+S keyboard shortcut to toggle the panel.
#[component]
pub fn StatusCenter() -> impl IntoView {
    let open = RwSignal::new(false);

    // Listen for keyboard shortcut
    let shortcut_count = use_status_center_shortcut();

    // Toggle on shortcut
    Effect::new(move || {
        let Some(count) = shortcut_count.try_get() else {
            return;
        };
        if count > 0 {
            let _ = open.try_update(|o| *o = !*o);
        }
    });

    view! {
        <StatusCenterPanel open=open />
    }
}

/// Provider for Status Center context
///
/// Allows child components to programmatically control the Status Center.
#[component]
pub fn StatusCenterProvider(children: Children) -> impl IntoView {
    let open = RwSignal::new(false);
    provide_context(StatusCenterContext { open });

    let shortcut_count = use_status_center_shortcut();
    Effect::new(move || {
        let Some(count) = shortcut_count.try_get() else {
            return;
        };
        if count > 0 {
            let _ = open.try_update(|o| *o = !*o);
        }
    });

    view! {
        <StatusCenterPanel open=open />
        {children()}
    }
}

/// Context for controlling Status Center from child components
#[derive(Clone, Copy)]
pub struct StatusCenterContext {
    /// Signal to control panel open state
    pub open: RwSignal<bool>,
}

impl StatusCenterContext {
    /// Open the status center panel
    pub fn open(&self) {
        self.open.set(true);
    }

    /// Close the status center panel
    pub fn close(&self) {
        self.open.set(false);
    }

    /// Toggle the status center panel
    pub fn toggle(&self) {
        self.open.update(|o| *o = !*o);
    }
}

/// Hook to access Status Center context
pub fn use_status_center() -> Option<StatusCenterContext> {
    use_context::<StatusCenterContext>()
}
