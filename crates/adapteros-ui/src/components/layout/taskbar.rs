//! Taskbar - Bottom navigation bar
//!
//! Module-level bottom taskbar with start button, module shortcuts, and system tray.
//! Navigation follows the 6-module structure: Operate, Build, Configure, Data, Verify, Org
//!
//! Personal settings (User, Settings) are accessed via the user menu in the topbar,
//! not the taskbar, to separate personal preferences from system/org navigation.

use super::start_menu::StartMenu;
use super::system_tray::SystemTray;
use crate::signals::{use_chat, DockState};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Module navigation item for taskbar
struct ModuleItem {
    label: &'static str,
    /// Primary route for the module (clicked to navigate)
    href: &'static str,
    icon: &'static str,
    /// All routes that belong to this module (for active state)
    routes: &'static [&'static str],
}

impl ModuleItem {
    const fn new(
        label: &'static str,
        href: &'static str,
        icon: &'static str,
        routes: &'static [&'static str],
    ) -> Self {
        Self {
            label,
            href,
            icon,
            routes,
        }
    }
}

/// Module navigation items - showing the 6 primary modules
/// Each module links to its primary page
///
/// Navigation philosophy:
/// - Personal settings (User, Settings) are accessed via the user menu in the topbar
/// - Admin/Org functions (users, roles, API keys) are organization-wide, not personal
/// - Chat is elevated to first-class status as a core interaction feature
/// - Tools (Routing Debug, Run Diff) remain accessible via Start Menu
const MODULE_ITEMS: &[ModuleItem] = &[
    ModuleItem::new(
        "Operate",
        "/",
        // Dashboard/metrics icon
        "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z",
        &["/", "/dashboard", "/system", "/workers", "/monitoring", "/errors"],
    ),
    ModuleItem::new(
        "Build",
        "/training",
        // Flask/training icon
        "M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z",
        &["/training", "/agents"],
    ),
    ModuleItem::new(
        "Configure",
        "/adapters",
        // Gear/settings icon
        "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z",
        &["/adapters", "/stacks", "/policies", "/models"],
    ),
    ModuleItem::new(
        "Data",
        "/datasets",
        // Database icon
        "M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4",
        &["/datasets", "/documents", "/repositories", "/collections"],
    ),
    ModuleItem::new(
        "Verify",
        "/audit",
        // Shield/verify icon
        "M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z",
        &["/audit", "/runs", "/reviews"],
    ),
    ModuleItem::new(
        "Org",
        "/admin",
        // Building/organization icon
        "M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4",
        &["/admin"],
    ),
];

/// Bottom taskbar with start button, module shortcuts, and system tray.
/// Touch targets meet minimum 44x44px on touch-capable viewports.
#[component]
pub fn Taskbar() -> impl IntoView {
    let (start_menu_open, set_start_menu_open) = signal(false);
    let location = use_location();
    let (chat_state, chat_action) = use_chat();

    view! {
        <nav class="taskbar h-12 flex items-center justify-between border-t border-border bg-background/95 backdrop-blur-sm shrink-0">
            // Left: Start button
            <div class="relative">
                <button
                    class=move || format!(
                        "start-btn taskbar-btn flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
                        if start_menu_open.get() {
                            "bg-primary text-primary-foreground"
                        } else {
                            "hover:bg-muted/50 text-foreground"
                        }
                    )
                    on:click=move |_| set_start_menu_open.update(|v| *v = !*v)
                    title="Open Start Menu"
                    aria-label="Open Start Menu - access all pages and settings"
                    aria-expanded=move || start_menu_open.get().to_string()
                    aria-haspopup="menu"
                >
                    <svg class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                        <rect x="3" y="3" width="8" height="8" rx="1"/>
                        <rect x="13" y="3" width="8" height="8" rx="1"/>
                        <rect x="3" y="13" width="8" height="8" rx="1"/>
                        <rect x="13" y="13" width="8" height="8" rx="1"/>
                    </svg>
                    <span class="text-sm font-medium hidden sm:block">"Menu"</span>
                </button>

                // Start menu dropdown
                <Show when=move || start_menu_open.get()>
                    <StartMenu on_close=move || set_start_menu_open.set(false)/>
                </Show>
            </div>

            // Center: Module shortcuts
            <div class="flex items-center gap-1">
                {MODULE_ITEMS.iter().map(|item| {
                    let href = item.href;
                    let label = item.label;
                    let icon_path = item.icon;
                    let routes = item.routes;

                    view! {
                        <ModuleButton
                            href=href
                            label=label
                            icon_path=icon_path
                            is_active=move || {
                                let path = location.pathname.get();
                                // Check if current path matches any route in this module
                                routes.iter().any(|r| {
                                    if *r == "/" {
                                        path == "/" || path == "/dashboard"
                                    } else if r.ends_with('/') {
                                        // Pattern like "/runs/" matches "/runs/abc"
                                        path.starts_with(r)
                                    } else {
                                        path == *r || path.starts_with(&format!("{}/", r))
                                    }
                                })
                            }
                        />
                    }
                }).collect::<Vec<_>>()}

                // Separator
                <div class="w-px h-6 bg-border/50 mx-1"></div>

                // Chat toggle button with unread badge
                {
                    let action = chat_action.clone();
                    view! {
                        <button
                            class=move || format!(
                                "taskbar-btn relative flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
                                if chat_state.get().dock_state == DockState::Docked {
                                    "bg-primary/20 text-primary"
                                } else {
                                    "hover:bg-muted/50 text-muted-foreground hover:text-foreground"
                                }
                            )
                            on:click={
                                let action = action.clone();
                                move |_| action.toggle_dock()
                            }
                            title="Toggle chat panel"
                            aria-label=move || {
                                if chat_state.get().dock_state == DockState::Docked {
                                    "Close chat panel".to_string()
                                } else {
                                    let unread = chat_state.get().unread_count();
                                    if unread > 0 {
                                        format!("Open chat panel ({} unread messages)", unread)
                                    } else {
                                        "Open chat panel".to_string()
                                    }
                                }
                            }
                            aria-expanded=move || (chat_state.get().dock_state == DockState::Docked).to_string()
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                            </svg>

                            // Unread badge
                            {move || {
                                let unread = chat_state.get().unread_count();
                                if unread > 0 && chat_state.get().dock_state != DockState::Docked {
                                    view! {
                                        <span class="absolute -top-1 -right-1 flex h-4 w-4 items-center justify-center rounded-full bg-destructive text-[9px] font-medium text-destructive-foreground">
                                            {if unread > 9 { "9+".to_string() } else { unread.to_string() }}
                                        </span>
                                    }.into_any()
                                } else {
                                    view! {}.into_any()
                                }
                            }}
                        </button>
                    }
                }
            </div>

            // Right: System tray
            <SystemTray/>
        </nav>
    }
}

/// Module button for taskbar - represents a navigation module.
/// Touch targets meet minimum 44x44px on touch-capable viewports.
#[component]
fn ModuleButton(
    href: &'static str,
    label: &'static str,
    icon_path: &'static str,
    is_active: impl Fn() -> bool + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <a
            href=href
            class=move || format!(
                "module-btn group flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors relative {}",
                if is_active() {
                    "bg-muted text-foreground"
                } else {
                    "hover:bg-muted/50 text-muted-foreground hover:text-foreground"
                }
            )
            title=label
            aria-label=format!("Go to {} module", label)
            aria-current=move || if is_active() { Some("page") } else { None }
        >
            <svg
                class="w-4 h-4"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                stroke-width="2"
                aria-hidden="true"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d=icon_path/>
            </svg>
            <span class="text-sm hidden lg:block">{label}</span>

            // Active indicator
            {move || {
                if is_active() {
                    view! {
                        <span class="absolute bottom-0 left-1/2 -translate-x-1/2 w-4 h-0.5 bg-primary rounded-full" aria-hidden="true"></span>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </a>
    }
}
