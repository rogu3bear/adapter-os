//! Layout components
//!
//! Global shell layout with top bar, bottom taskbar, and main workspace.
//! Designed like a Windows taskbar + modern control plane aesthetic.

use crate::api::ApiClient;
use crate::components::chat_dock::{ChatDockPanel, MobileChatOverlay, NarrowChatDock};
use crate::components::glass_toggle::GlassThemeToggle;
use crate::components::global_search::GlobalSearchBox;
use crate::components::offline_banner::OfflineBanner;
use crate::components::status::{Badge, BadgeVariant, StatusColor, StatusIndicator};
use crate::components::status_center::{use_status_center, StatusCenterProvider};
use crate::components::version_skew_banner::VersionSkewBanner;
use crate::components::workspace::Workspace;
use crate::hooks::{use_api_resource, LoadingState};
use crate::signals::{use_auth, use_chat, use_search, DockState};
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// ============================================================================
// Shell - Main Application Frame
// ============================================================================

/// Application shell with top bar, bottom taskbar, and main workspace
#[component]
pub fn Shell(children: Children) -> impl IntoView {
    web_sys::console::log_1(&"[Shell] Rendering...".into());
    let (chat_state, _chat_action) = use_chat();
    let search = use_search();
    web_sys::console::log_1(&"[Shell] Got chat context".into());

    // Global keyboard handler for Command Palette
    let keyboard_handler_set = StoredValue::new(false);
    Effect::new(move || {
        if keyboard_handler_set.get_value() {
            return;
        }
        keyboard_handler_set.set_value(true);

        let search = search.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            let key = event.key();
            let ctrl_or_cmd = event.ctrl_key() || event.meta_key();

            // Check if we're in an input field
            if let Some(target) = event.target() {
                if let Some(element) = target.dyn_ref::<web_sys::HtmlElement>() {
                    let tag = element.tag_name().to_lowercase();
                    if tag == "input" || tag == "textarea" {
                        // Allow Escape to blur
                        if key == "Escape" {
                            let _ = element.blur();
                            event.prevent_default();
                            return;
                        }
                        // Don't intercept other keys in inputs (except Ctrl+K)
                        if !(ctrl_or_cmd && key == "k") {
                            return;
                        }
                    }
                }
            }

            // Ctrl+K or Cmd+K opens command palette
            if ctrl_or_cmd && key == "k" {
                event.prevent_default();
                search.toggle();
                return;
            }

            // "/" opens command palette when not in input
            if key == "/" && !search.command_palette_open.get_untracked() {
                event.prevent_default();
                search.open();
            }
        }) as Box<dyn FnMut(_)>);

        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        }
        closure.forget();
    });

    view! {
        <StatusCenterProvider>
            <div class="shell">
                // PRD-UI-000: Offline banner for API connectivity status
                <OfflineBanner/>
                // Version drift banner prompts reload when backend/frontend versions differ
                <VersionSkewBanner/>
                // Streaming health indicator (SSE) could go here if needed

                // Top bar
                <TopBar/>

                // Main content area with workspace
                <div class="shell-content">
                    // Main workspace wrapper
                    <Workspace class="shell-workspace">
                        <main class="shell-main">
                            {children()}
                        </main>
                    </Workspace>

                    // Chat dock (collapsible right panel)
                    {move || {
                        match chat_state.get().dock_state {
                            DockState::Docked => view! { <ChatDockPanel/> }.into_any(),
                            DockState::Narrow => view! { <NarrowChatDock/> }.into_any(),
                            DockState::Hidden => view! {}.into_any(),
                        }
                    }}
                </div>

                // Bottom taskbar
                <Taskbar/>

                // Mobile chat overlay
                <MobileChatOverlay/>
            </div>
        </StatusCenterProvider>
    }
}

// ============================================================================
// Top Bar
// ============================================================================

/// Thin top bar with branding, command palette hint, and user menu
#[component]
pub fn TopBar() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();
    // Store auth_action for use in closures
    let auth_action_signal = StoredValue::new(auth_action);
    let (notifications_open, set_notifications_open) = signal(false);
    let (user_menu_open, set_user_menu_open) = signal(false);

    // Environment detection (dev/prod)
    let env_badge = {
        #[cfg(debug_assertions)]
        {
            "DEV"
        }
        #[cfg(not(debug_assertions))]
        {
            "PROD"
        }
    };

    let env_badge_variant = {
        #[cfg(debug_assertions)]
        {
            BadgeVariant::Warning
        }
        #[cfg(not(debug_assertions))]
        {
            BadgeVariant::Success
        }
    };

    view! {
        <header class="h-10 flex items-center justify-between border-b border-border/50 bg-background/95 backdrop-blur-sm px-4 shrink-0">
            // Left: Product name + environment badge
            <div class="flex items-center gap-3">
                <div class="flex items-center gap-2">
                    <span class="font-semibold text-sm tracking-tight">"AdapterOS"</span>
                    <Badge variant=env_badge_variant>{env_badge}</Badge>
                </div>
            </div>

            // Center: Global search box (opens Command Palette)
            <div class="flex-1 flex justify-center">
                <GlobalSearchBox/>
            </div>

            // Right: Glass toggle + Notifications + User menu
            <div class="flex items-center gap-2">
                // Glass theme toggle (PRD-UI-100)
                <GlassThemeToggle/>

                // Separator
                <div class="w-px h-4 bg-border/30"></div>

                // Notifications bell
                <div class="relative">
                    <button
                        class="p-1.5 rounded-md hover:bg-muted/50 text-muted-foreground hover:text-foreground transition-colors"
                        on:click=move |_| set_notifications_open.update(|v| *v = !*v)
                        aria-label="Notifications"
                        title="Notifications"
                    >
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/>
                        </svg>
                        // Notification badge (hidden for now)
                        // <span class="absolute top-0 right-0 w-2 h-2 bg-status-error rounded-full"></span>
                    </button>

                    // Notifications dropdown
                    <Show when=move || notifications_open.get()>
                        <div class="absolute right-0 top-full mt-1 w-72 bg-background border border-border rounded-lg shadow-lg z-50">
                            <div class="p-3 border-b border-border">
                                <h3 class="text-sm font-medium">"Notifications"</h3>
                            </div>
                            <div class="p-4 text-center text-sm text-muted-foreground">
                                "No new notifications"
                            </div>
                        </div>
                    </Show>
                </div>

                // User menu
                <div class="relative">
                    <button
                        class="flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50 transition-colors"
                        on:click=move |_| set_user_menu_open.update(|v| *v = !*v)
                    >
                        {move || {
                            if let Some(user) = auth_state.get().user() {
                                let initials = user.email.chars().next().unwrap_or('U').to_uppercase().to_string();
                                view! {
                                    <div class="w-6 h-6 rounded-full bg-primary/20 text-primary flex items-center justify-center text-xs font-medium">
                                        {initials}
                                    </div>
                                    <span class="text-xs text-muted-foreground hidden sm:block max-w-[100px] truncate">
                                        {user.email.clone()}
                                    </span>
                                    <svg class="w-3 h-3 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                                    </svg>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="w-6 h-6 rounded-full bg-muted flex items-center justify-center">
                                        <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
                                        </svg>
                                    </div>
                                }.into_any()
                            }
                        }}
                    </button>

                    // User dropdown
                    <Show when=move || user_menu_open.get()>
                        <div class="absolute right-0 top-full mt-1 w-48 bg-background border border-border rounded-lg shadow-lg z-50">
                            {move || {
                                let state = auth_state.get();
                                if let Some(user) = state.user() {
                                    let email = user.email.clone();
                                    view! {
                                        <div class="p-3 border-b border-border">
                                            <p class="text-sm font-medium truncate">{email}</p>
                                            <p class="text-xs text-muted-foreground">"Operator"</p>
                                        </div>
                                        <div class="p-1">
                                            <button
                                                class="w-full text-left px-3 py-2 text-sm rounded hover:bg-muted/50 transition-colors"
                                                on:click=move |_| {
                                                    set_user_menu_open.set(false);
                                                    auth_action_signal.with_value(|action| {
                                                        let action = action.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            action.logout().await;
                                                        });
                                                    });
                                                }
                                            >
                                                "Sign out"
                                            </button>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="p-3">
                                            <a
                                                href="/login"
                                                class="block text-sm text-center py-2 px-3 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                                            >
                                                "Sign in"
                                            </a>
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </Show>
                </div>
            </div>
        </header>
    }
}

// ============================================================================
// Chat Dock (moved to chat_dock.rs)
// ============================================================================

// The ChatDock implementation is now in components/chat_dock.rs
// This section is kept for documentation purposes.

// ============================================================================
// Bottom Taskbar
// ============================================================================

/// Navigation item for taskbar
struct NavItem {
    label: &'static str,
    href: &'static str,
    icon: &'static str,
}

impl NavItem {
    const fn new(label: &'static str, href: &'static str, icon: &'static str) -> Self {
        Self { label, href, icon }
    }
}

/// Pinned navigation items
const NAV_ITEMS: &[NavItem] = &[
    NavItem::new("Dashboard", "/", "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6"),
    NavItem::new("Adapters", "/adapters", "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"),
    NavItem::new("Chat", "/chat", "M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"),
    NavItem::new("Training", "/training", "M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"),
    NavItem::new("System", "/system", "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"),
    NavItem::new("Settings", "/settings", "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"),
];

/// Bottom taskbar with start button, pinned pages, and system tray
#[component]
pub fn Taskbar() -> impl IntoView {
    let (start_menu_open, set_start_menu_open) = signal(false);
    let location = use_location();
    let (chat_state, chat_action) = use_chat();

    view! {
        <nav class="h-12 flex items-center justify-between border-t border-border bg-background/95 backdrop-blur-sm px-2 shrink-0">
            // Left: Start button
            <div class="relative">
                <button
                    class=move || format!(
                        "flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
                        if start_menu_open.get() {
                            "bg-primary text-primary-foreground"
                        } else {
                            "hover:bg-muted/50 text-foreground"
                        }
                    )
                    on:click=move |_| set_start_menu_open.update(|v| *v = !*v)
                >
                    <svg class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
                        <rect x="3" y="3" width="8" height="8" rx="1"/>
                        <rect x="13" y="3" width="8" height="8" rx="1"/>
                        <rect x="3" y="13" width="8" height="8" rx="1"/>
                        <rect x="13" y="13" width="8" height="8" rx="1"/>
                    </svg>
                    <span class="text-sm font-medium hidden sm:block">"Start"</span>
                </button>

                // Start menu dropdown
                <Show when=move || start_menu_open.get()>
                    <StartMenu on_close=move || set_start_menu_open.set(false)/>
                </Show>
            </div>

            // Center: Pinned pages
            <div class="flex items-center gap-1">
                {NAV_ITEMS.iter().map(|item| {
                    let href = item.href;
                    let label = item.label;
                    let icon_path = item.icon;

                    view! {
                        <TaskbarButton
                            href=href
                            label=label
                            icon_path=icon_path
                            is_active=move || {
                                let path = location.pathname.get();
                                if href == "/" {
                                    path == "/" || path == "/dashboard"
                                } else {
                                    path.starts_with(href)
                                }
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
                                "relative flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors {}",
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
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                            </svg>

                            // Unread badge
                            {move || {
                                let unread = chat_state.get().unread_count;
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

/// Taskbar button for pinned pages
#[component]
fn TaskbarButton(
    href: &'static str,
    label: &'static str,
    icon_path: &'static str,
    is_active: impl Fn() -> bool + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <a
            href=href
            class=move || format!(
                "group flex items-center gap-2 px-3 py-1.5 rounded-md transition-colors relative {}",
                if is_active() {
                    "bg-muted text-foreground"
                } else {
                    "hover:bg-muted/50 text-muted-foreground hover:text-foreground"
                }
            )
            title=label
        >
            <svg
                class="w-4 h-4"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                stroke-width="2"
            >
                <path stroke-linecap="round" stroke-linejoin="round" d=icon_path/>
            </svg>
            <span class="text-sm hidden lg:block">{label}</span>

            // Active indicator
            {move || {
                if is_active() {
                    view! {
                        <span class="absolute bottom-0 left-1/2 -translate-x-1/2 w-4 h-0.5 bg-primary rounded-full"></span>
                    }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </a>
    }
}

// ============================================================================
// Start Menu
// ============================================================================

/// Module launcher menu with all application pages
#[component]
fn StartMenu(on_close: impl Fn() + Clone + 'static) -> impl IntoView {
    let on_close_clone = on_close.clone();

    // All module categories - comprehensive list of all pages
    let modules = vec![
        (
            "Core",
            vec![
                ("Dashboard", "/"),
                ("Chat", "/chat"),
                ("Adapters", "/adapters"),
            ],
        ),
        (
            "Resources",
            vec![
                ("Models", "/models"),
                ("Stacks", "/stacks"),
                ("Collections", "/collections"),
                ("Datasets", "/datasets"),
                ("Documents", "/documents"),
                ("Repositories", "/repositories"),
            ],
        ),
        (
            "Operations",
            vec![
                ("Training", "/training"),
                ("Workers", "/workers"),
                ("Routing", "/routing"),
                ("Runs", "/runs"),
            ],
        ),
        (
            "Monitoring",
            vec![
                ("System", "/system"),
                ("Monitoring", "/monitoring"),
                ("Audit", "/audit"),
                ("Errors", "/errors"),
            ],
        ),
        (
            "Administration",
            vec![
                ("Admin", "/admin"),
                ("Policies", "/policies"),
                ("Settings", "/settings"),
            ],
        ),
        (
            "Developer",
            vec![
                ("Diff", "/diff"),
                ("Style Audit", "/style-audit"),
            ],
        ),
    ];

    view! {
        <div
            class="absolute left-0 w-96 bg-background border border-border rounded-lg shadow-xl z-50"
            style="bottom: 100%; margin-bottom: 0.5rem;"
        >
            // Header
            <div class="p-4 border-b border-border">
                <h2 class="text-lg font-semibold">"AdapterOS"</h2>
                <p class="text-xs text-muted-foreground">"Module Launcher"</p>
            </div>

            // Module grid - two column layout for better organization
            <div class="p-3 max-h-[28rem] overflow-y-auto">
                <div class="grid grid-cols-2 gap-x-4">
                    {modules.into_iter().map(|(category, items)| {
                        let on_close = on_close_clone.clone();
                        view! {
                            <div class="mb-3">
                                <h3 class="text-xs font-medium text-muted-foreground uppercase tracking-wider px-2 mb-1">
                                    {category}
                                </h3>
                                <div class="space-y-0.5">
                                    {items.into_iter().map(|(label, href)| {
                                        let on_close = on_close.clone();
                                        view! {
                                            <a
                                                href=href
                                                class="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-muted/50 transition-colors"
                                                on:click=move |_| on_close()
                                            >
                                                <div class="w-6 h-6 rounded bg-primary/10 flex items-center justify-center shrink-0">
                                                    <span class="text-primary text-xs">{label.chars().next().unwrap_or('?')}</span>
                                                </div>
                                                <span class="text-sm font-medium truncate">{label}</span>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>

            // Footer
            <div class="p-3 border-t border-border">
                <div class="flex items-center justify-between text-xs text-muted-foreground">
                    <span>"v0.1.0"</span>
                    <a href="/settings" class="hover:text-foreground transition-colors" on:click=move |_| on_close()>
                        "Settings"
                    </a>
                </div>
            </div>
        </div>
    }
}

// ============================================================================
// System Tray
// ============================================================================

/// System tray with health indicator, connection status, and time
#[component]
fn SystemTray() -> impl IntoView {
    let status_center = use_status_center();
    let (health, _refetch_health) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.health().await });
    let (system_status, _refetch_status) =
        use_api_resource(|client: Arc<ApiClient>| async move { client.system_status().await });

    // Current time (updates every second)
    let (time, set_time) = signal(get_current_time());

    // Track whether we've created the interval to prevent duplicates
    let interval_created = StoredValue::new(false);

    // Update time every second - Effect runs once on mount
    // The interval is intentionally leaked (mem::forget) since this component
    // lives for the lifetime of the app and Interval doesn't implement Send+Sync
    Effect::new(move || {
        if !interval_created.get_value() {
            interval_created.set_value(true);
            let interval = gloo_timers::callback::Interval::new(1000, move || {
                set_time.set(get_current_time());
            });
            // Leak the interval - it lives for app lifetime anyway
            std::mem::forget(interval);
        }
    });

    view! {
        <div class="flex items-center gap-3">
            {move || {
                let (color, label, pulsing, title) = match system_status.get() {
                    LoadingState::Loaded(status) => {
                        let readiness = match status.readiness.overall {
                            adapteros_api_types::StatusIndicator::Ready => "Ready",
                            adapteros_api_types::StatusIndicator::NotReady => "Not Ready",
                            adapteros_api_types::StatusIndicator::Unknown => "Unknown",
                        };
                        let inference = match status.inference_ready {
                            adapteros_api_types::InferenceReadyState::True => "Ready",
                            adapteros_api_types::InferenceReadyState::False => "Not Ready",
                            adapteros_api_types::InferenceReadyState::Unknown => "Unknown",
                        };
                        let blockers = if status.inference_blockers.is_empty() {
                            "none".to_string()
                        } else {
                            format!("{} blockers", status.inference_blockers.len())
                        };
                        let title = format!(
                            "Readiness: {} | Inference: {} | Blockers: {}",
                            readiness, inference, blockers
                        );
                        match status.readiness.overall {
                            adapteros_api_types::StatusIndicator::Ready => {
                                (StatusColor::Green, "Healthy", true, title)
                            }
                            adapteros_api_types::StatusIndicator::NotReady => {
                                (StatusColor::Red, "Not Ready", false, title)
                            }
                            adapteros_api_types::StatusIndicator::Unknown => {
                                (StatusColor::Yellow, "Unknown", false, title)
                            }
                        }
                    }
                    LoadingState::Idle | LoadingState::Loading => (
                        StatusColor::Gray,
                        "Checking",
                        false,
                        "Loading system status".to_string(),
                    ),
                    LoadingState::Error(_) => (
                        StatusColor::Red,
                        "Unavailable",
                        false,
                        "System status unavailable".to_string(),
                    ),
                };
                let on_click = move |_| {
                    if let Some(ctx) = status_center {
                        ctx.open();
                    }
                };
                view! {
                    <button
                        class="flex items-center gap-1.5"
                        on:click=on_click
                        title=title
                        aria-label="System status"
                        type="button"
                    >
                        <StatusIndicator color=color pulsing=pulsing/>
                        <span class="text-xs text-muted-foreground hidden sm:block">{label}</span>
                    </button>
                }
            }}

            {move || {
                let (color, label, pulsing, title) = match health.get() {
                    LoadingState::Loaded(resp) => {
                        let status = resp.status.to_lowercase();
                        let (color, label) = if matches!(status.as_str(), "ok" | "healthy") {
                            (StatusColor::Green, "Connected")
                        } else if matches!(status.as_str(), "degraded" | "warning") {
                            (StatusColor::Yellow, "Degraded")
                        } else {
                            (StatusColor::Red, "Unhealthy")
                        };
                        let title = format!("Health: {} | Version: {}", resp.status, resp.version);
                        (color, label, false, title)
                    }
                    LoadingState::Idle | LoadingState::Loading => (
                        StatusColor::Yellow,
                        "Connecting",
                        true,
                        "Connecting to backend".to_string(),
                    ),
                    LoadingState::Error(_) => (
                        StatusColor::Red,
                        "Offline",
                        false,
                        "Backend offline".to_string(),
                    ),
                };
                view! {
                    <div class="flex items-center gap-1.5" title=title>
                        <StatusIndicator color=color pulsing=pulsing/>
                        <span class="text-xs text-muted-foreground hidden sm:block">{label}</span>
                    </div>
                }
            }}

            // Separator
            <div class="w-px h-4 bg-border/50"></div>

            // Time
            <span class="text-xs text-muted-foreground font-mono tabular-nums min-w-[4rem] text-right">
                {move || time.get()}
            </span>
        </div>
    }
}

/// Get current time formatted as HH:MM
fn get_current_time() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use js_sys::Date;
        let date = Date::new_0();
        let hours = date.get_hours();
        let minutes = date.get_minutes();
        format!("{:02}:{:02}", hours, minutes)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "00:00".to_string()
    }
}

// ============================================================================
// Legacy exports for compatibility
// ============================================================================

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
