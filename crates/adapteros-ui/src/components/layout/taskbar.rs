//! Taskbar - Bottom navigation bar
//!
//! Module-level bottom taskbar with start button, module shortcuts, and system tray.
//! Navigation follows runtime IA taxonomy from the shared registry:
//! Infer, Data, Train, Deploy, Route, Observe, Govern, Org.
//!
//! Personal settings (User, Settings) are accessed via the user menu in the topbar,
//! not the taskbar, to separate personal preferences from system/org navigation.

use super::nav_registry::build_taskbar_modules;
use super::sidebar::use_sidebar;
use super::start_menu::StartMenu;
use super::system_tray::SystemTray;
use crate::components::responsive::use_is_mobile;
use crate::signals::{use_chat, use_ui_profile, DockState};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Bottom taskbar with start button, module shortcuts, and system tray.
/// Touch targets meet minimum 44x44px on touch-capable viewports.
#[component]
pub fn Taskbar() -> impl IntoView {
    let (start_menu_open, set_start_menu_open) = signal(false);
    let location = use_location();
    let (chat_state, chat_action) = use_chat();
    let ui_profile = use_ui_profile();
    let sidebar = use_sidebar();
    let is_mobile = use_is_mobile();
    let modules = Signal::derive(move || {
        ui_profile
            .try_get()
            .map(build_taskbar_modules)
            .unwrap_or_default()
    });

    // On desktop: toggle sidebar. On mobile: toggle start menu popup.
    let on_menu_click = move |_| {
        if is_mobile.try_get().unwrap_or(false) {
            set_start_menu_open.update(|v| *v = !*v);
        } else {
            sidebar.update(|s| *s = s.toggle());
        }
    };

    view! {
        <nav class="taskbar h-12 flex items-center justify-between border-t border-border bg-background/95 backdrop-blur-sm shrink-0">
            // Left: Start button (toggles sidebar on desktop, start menu on mobile)
            <div class="relative">
                <button
                    class=move || format!(
                        "start-btn taskbar-btn flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
                        if start_menu_open.try_get().unwrap_or(false) || sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false) {
                            "bg-primary text-primary-foreground"
                        } else {
                            "hover:bg-muted/50 text-foreground"
                        }
                    )
                    on:click=on_menu_click
                    title="Toggle navigation"
                    aria-label="Toggle navigation sidebar"
                    aria-expanded=move || (start_menu_open.try_get().unwrap_or(false) || sidebar.try_get().map(|s| s.is_expanded()).unwrap_or(false)).to_string()
                >
                    <svg class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
                        <rect x="3" y="3" width="8" height="8" rx="1"/>
                        <rect x="13" y="3" width="8" height="8" rx="1"/>
                        <rect x="3" y="13" width="8" height="8" rx="1"/>
                        <rect x="13" y="13" width="8" height="8" rx="1"/>
                    </svg>
                    <span class="text-sm font-medium hidden sm:block">"Menu"</span>
                </button>

                // Start menu dropdown (mobile fallback only)
                <Show when=move || start_menu_open.try_get().unwrap_or(false)>
                    <StartMenu on_close=move || set_start_menu_open.set(false)/>
                </Show>
            </div>

            // Center: Module shortcuts
            <div class="flex items-center gap-1 flex-1 min-w-0 overflow-auto">
                {move || {
                    modules
                        .try_get()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|item| {
                            let href = item.href;
                            let label = item.label;
                            let icon_path = item.icon;
                            // Use StoredValue for routes so the closure can be Copy
                            let routes = StoredValue::new(item.routes);

                            view! {
                                <ModuleButton
                                    href=href
                                    label=label
                                    icon_path=icon_path
                                    is_active=move || {
                                        let path = location.pathname.try_get().unwrap_or_default();
                                        // Check if current path matches any route in this module
                                        // Use try_with_value to avoid panic when StoredValue is
                                        // disposed during SPA navigation re-renders
                                        routes.try_with_value(|routes| {
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
                                        }).unwrap_or(false)
                                    }
                                />
                            }
                        })
                        .collect::<Vec<_>>()
                }}

                // Separator
                <div class="w-px h-6 bg-border/50 mx-1"></div>

                // Chat toggle button with unread badge
                {
                    let action = chat_action.clone();
                    view! {
                        <button
                            class=move || format!(
                                "taskbar-btn shrink-0 relative flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
                                if chat_state.try_get().unwrap_or_default().dock_state == DockState::Docked {
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
                                if chat_state.try_get().unwrap_or_default().dock_state == DockState::Docked {
                                    "Close chat panel".to_string()
                                } else {
                                    let unread = chat_state.try_get().unwrap_or_default().unread_count();
                                    if unread > 0 {
                                        format!("Open chat panel ({} unread messages)", unread)
                                    } else {
                                        "Open chat panel".to_string()
                                    }
                                }
                            }
                            aria-expanded=move || (chat_state.try_get().unwrap_or_default().dock_state == DockState::Docked).to_string()
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                            </svg>

                            // Unread badge
                            {move || {
                                let unread = chat_state.try_get().unwrap_or_default().unread_count();
                                if unread > 0 && chat_state.try_get().unwrap_or_default().dock_state != DockState::Docked {
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
                "module-btn group shrink-0 flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors relative {}",
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
