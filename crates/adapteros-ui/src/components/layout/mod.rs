//! Layout components
//!
//! Global shell layout with top bar, sidebar, and main workspace.

pub mod hud_keyboard;
pub mod hud_shell;
pub mod nav_registry;
mod page_scaffold;
mod shell;
mod shell_dispatch;
pub mod sidebar;
mod system_tray;
mod topbar;

// Main exports
pub use page_scaffold::{
    BreadcrumbItem, PageScaffold, PageScaffoldActions, PageScaffoldInspector,
    PageScaffoldPrimaryAction, PageScaffoldStatus,
};
pub use shell::Shell;
pub use shell_dispatch::ShellDispatch;
pub use sidebar::{provide_sidebar_context, use_sidebar, SidebarNav, SidebarState};
pub use topbar::TopBar;

// HUD-specific exports
pub use hud_keyboard::use_hud_keyboard;

// Re-export nav registry for external use
pub use nav_registry::{
    all_nav_items, nav_group_label_for_route, nav_groups, route_for_alt_shortcut, NavGroup, NavItem,
};
