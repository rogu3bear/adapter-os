//! Layout components
//!
//! Global shell layout with top bar, bottom taskbar, and main workspace.
//! Designed like a Windows taskbar + modern control plane aesthetic.
//!
//! Navigation follows the workflow spine:
//! Data → Train → Deploy → Route → Infer → Observe → Govern → Org

mod logical_rail;
pub mod nav_registry;
mod page_scaffold;
mod shell;
pub mod sidebar;
mod start_menu;
mod system_tray;
mod taskbar;
mod topbar;

// Main exports
pub use logical_rail::LogicalControlRail;
pub use page_scaffold::{
    BreadcrumbItem, PageScaffold, PageScaffoldActions, PageScaffoldPrimaryAction,
};
pub use shell::Shell;
pub use sidebar::{provide_sidebar_context, use_sidebar, SidebarNav, SidebarState};
pub use taskbar::Taskbar;
pub use topbar::TopBar;

// Re-export nav registry for external use
pub use nav_registry::{all_nav_items, nav_groups, route_for_alt_shortcut, NavGroup, NavItem};
