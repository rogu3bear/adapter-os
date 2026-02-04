//! Layout components
//!
//! Global shell layout with top bar, bottom taskbar, and main workspace.
//! Designed like a Windows taskbar + modern control plane aesthetic.

mod nav_registry;
mod shell;
mod start_menu;
mod system_tray;
mod taskbar;
mod topbar;

// Main exports
pub use shell::Shell;
pub use taskbar::Taskbar;
pub use topbar::TopBar;

// Legacy exports for backward compatibility
use leptos::prelude::*;

/// Header component (legacy, now part of Shell)
#[component]
pub fn Header() -> impl IntoView {
    view! { <TopBar/> }
}

/// Sidebar navigation (legacy, replaced by taskbar)
#[component]
pub fn Sidebar() -> impl IntoView {
    // Legacy sidebar is now replaced by the bottom taskbar
    // This component is kept for backwards compatibility but renders nothing
    view! {}
}
