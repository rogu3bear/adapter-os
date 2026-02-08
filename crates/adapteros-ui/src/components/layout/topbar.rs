//! TopBar - Top navigation bar
//!
//! Thin top bar with branding, command palette hint, and user menu.
//! Responsive: collapses to hamburger menu on mobile viewports.

use crate::components::error_history_panel::use_error_history;
use crate::components::glass_toggle::GlassThemeToggle;
use crate::components::global_search::GlobalSearchBox;
use crate::components::layout::nav_registry::build_mobile_nav_items;
use crate::components::responsive::use_is_mobile;
use crate::components::status::{Badge, BadgeVariant};
use crate::constants::urls::docs_url;
use crate::signals::{use_auth, use_notification_state, use_search, use_ui_profile};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Thin top bar with branding, command palette hint, and user menu.
/// Responsive: collapses to hamburger + key actions on mobile.
#[component]
pub fn TopBar() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();
    // Store auth_action for use in closures
    let auth_action_signal = StoredValue::new(auth_action);
    let (user_menu_open, set_user_menu_open) = signal(false);
    let user_menu_ref = NodeRef::<leptos::html::Div>::new();
    let user_menu_button_ref = NodeRef::<leptos::html::Button>::new();
    let (mobile_menu_open, set_mobile_menu_open) = signal(false);
    let is_mobile = use_is_mobile();
    let search = use_search();
    let docs_url_value = docs_url();

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

    // Close user menu on outside click or Escape
    let user_menu_listeners_set = StoredValue::new(false);
    Effect::new(move || {
        if user_menu_listeners_set.get_value() {
            return;
        }
        user_menu_listeners_set.set_value(true);

        let user_menu_open = user_menu_open;
        let set_user_menu_open = set_user_menu_open;
        let user_menu_ref = user_menu_ref;
        let user_menu_button_ref = user_menu_button_ref;

        let click_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            if !user_menu_open.get_untracked() {
                return;
            }
            let target = match event.target() {
                Some(target) => target,
                None => return,
            };
            let target_node = match target.dyn_into::<web_sys::Node>() {
                Ok(node) => node,
                Err(_) => return,
            };
            if let Some(menu) = user_menu_ref.get() {
                if menu.contains(Some(&target_node)) {
                    return;
                }
            }
            if let Some(button) = user_menu_button_ref.get() {
                if button.contains(Some(&target_node)) {
                    return;
                }
            }
            set_user_menu_open.set(false);
        }) as Box<dyn FnMut(_)>);

        let key_closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if !user_menu_open.get_untracked() {
                return;
            }
            if event.key() == "Escape" {
                set_user_menu_open.set(false);
            }
        }) as Box<dyn FnMut(_)>);

        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            let _ = document.add_event_listener_with_callback(
                "mousedown",
                click_closure.as_ref().unchecked_ref(),
            );
            let _ = document.add_event_listener_with_callback(
                "touchstart",
                click_closure.as_ref().unchecked_ref(),
            );
            let _ = document
                .add_event_listener_with_callback("keydown", key_closure.as_ref().unchecked_ref());
        }
        click_closure.forget();
        key_closure.forget();
    });

    view! {
        <header class="topbar h-10 flex items-center justify-between border-b border-border/50 bg-background/95 backdrop-blur-sm shrink-0">
            // Left: Hamburger (mobile) + Product name + environment badge
            <div class="flex items-center gap-3">
                // Hamburger menu button (mobile only)
                <button
                    class="topbar-hamburger topbar-action"
                    on:click=move |_| set_mobile_menu_open.update(|v| *v = !*v)
                    aria-label="Open menu"
                    aria-expanded=move || mobile_menu_open.get().to_string()
                    aria-controls="mobile-menu"
                >
                    <div class=move || format!("hamburger-icon {}", if mobile_menu_open.get() { "open" } else { "" })>
                        <span></span>
                        <span></span>
                        <span></span>
                    </div>
                </button>

                <div class="flex items-center gap-2">
                    <span class="topbar-brand-text font-semibold text-sm tracking-tight">"adapterOS"</span>
                    <Badge variant=env_badge_variant>{env_badge}</Badge>
                </div>
            </div>

            // Center: Global search box (opens Command Palette) - hidden on mobile
            <div class="topbar-search flex-1 flex justify-center">
                <GlobalSearchBox/>
            </div>

            // Right: Glass toggle + User menu
            <div class="topbar-actions flex items-center gap-2">
                // Mobile-only command palette trigger
                <button
                    class="topbar-action flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted/50 transition-colors sm:hidden"
                    on:click=move |_| search.open()
                    aria-label="Open command palette"
                    title="Open command palette"
                >
                    <svg
                        class="w-4 h-4 text-muted-foreground"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <circle cx="11" cy="11" r="8" />
                        <line x1="21" y1="21" x2="16.65" y2="16.65" />
                    </svg>
                </button>
                {(!docs_url_value.is_empty()).then(|| view! {
                    <a
                        class="topbar-action flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50 transition-colors"
                        href=docs_url_value
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 18h.01M10 8h4m-4 4h2m7 4a2 2 0 01-2 2H7a2 2 0 01-2-2V6a2 2 0 012-2h6l4 4v8z"/>
                        </svg>
                        <span class="text-xs text-muted-foreground">"Docs"</span>
                    </a>
                })}
                // Error history button with unread badge
                <ErrorHistoryButton />

                // Glass theme toggle (PRD-UI-100) - secondary action, hidden on mobile
                <div class="topbar-action-secondary">
                    <GlassThemeToggle/>
                </div>

                // Separator - hidden on mobile
                <div class="topbar-action-secondary w-px h-4 bg-border/30"></div>

                // User menu
                <div class="relative">
                    <button
                        class="topbar-action flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50 transition-colors"
                        node_ref=user_menu_button_ref
                        on:click=move |_| set_user_menu_open.update(|v| *v = !*v)
                        aria-expanded=move || user_menu_open.get().to_string()
                        aria-controls="user-menu"
                        aria-haspopup="menu"
                    >
                        {move || {
                            if let Some(user) = auth_state.get().user() {
                                let identity = if user.display_name.is_empty() {
                                    user.email.clone()
                                } else {
                                    user.display_name.clone()
                                };
                                let initials = identity
                                    .chars()
                                    .next()
                                    .unwrap_or('U')
                                    .to_uppercase()
                                    .to_string();
                                view! {
                                    <div class="w-6 h-6 rounded-full bg-primary/20 text-primary flex items-center justify-center text-xs font-medium">
                                        {initials}
                                    </div>
                                    <span class="topbar-user-email text-xs text-muted-foreground max-w-[100px] truncate">
                                        {user.email.clone()}
                                    </span>
                                    <svg class="w-3 h-3 text-muted-foreground hidden sm:block" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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

                    // User dropdown - includes personal account links (Profile, Preferences)
                    // This is the primary way to access personal settings, separate from Org admin
                    <Show when=move || user_menu_open.get()>
                        <div
                            class="absolute right-0 top-full mt-1 w-48 bg-background border border-border rounded-lg shadow-lg z-50"
                            id="user-menu"
                            role="menu"
                            node_ref=user_menu_ref
                        >
                            {move || {
                                let state = auth_state.get();
                                if let Some(user) = state.user() {
                                    let email = user.email.clone();
                                    let identity = if user.display_name.is_empty() {
                                        email.clone()
                                    } else {
                                        user.display_name.clone()
                                    };
                                    view! {
                                        <div class="p-3 border-b border-border">
                                            <p class="text-sm font-medium truncate">{email}</p>
                                            <p class="text-xs text-muted-foreground">
                                                {format!("Logged in as {}", identity)}
                                            </p>
                                        </div>
                                        // Account section - personal settings
                                        <div class="p-1 border-b border-border">
                                            <a
                                                href="/user"
                                                class="flex items-center gap-2 w-full text-left px-3 py-2 text-sm rounded hover:bg-muted/50 transition-colors"
                                                on:click=move |_| set_user_menu_open.set(false)
                                            >
                                                <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
                                                </svg>
                                                "Profile"
                                            </a>
                                            <a
                                                href="/settings"
                                                class="flex items-center gap-2 w-full text-left px-3 py-2 text-sm rounded hover:bg-muted/50 transition-colors"
                                                on:click=move |_| set_user_menu_open.set(false)
                                            >
                                                <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                                                </svg>
                                                "Preferences"
                                            </a>
                                        </div>
                                        // Session actions
                                        <div class="p-1">
                                            <button
                                                class="flex items-center gap-2 w-full text-left px-3 py-2 text-sm rounded hover:bg-muted/50 transition-colors text-destructive"
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
                                                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"/>
                                                </svg>
                                                "Log out"
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
                                                "Log in"
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

        // Mobile menu overlay
        <Show when=move || is_mobile.get() && mobile_menu_open.get()>
            <MobileMenu
                on_close=move || set_mobile_menu_open.set(false)
            />
        </Show>
    }
}

/// Mobile navigation menu overlay
#[component]
fn MobileMenu(
    /// Callback to close the menu
    on_close: impl Fn() + Copy + Send + 'static,
) -> impl IntoView {
    let ui_profile = use_ui_profile();
    let docs_url_value = docs_url();
    view! {
        // Backdrop - close on click
        <div
            class="mobile-menu-overlay open"
            on:click=move |_| on_close()
        >
            // Menu panel - stop propagation so clicks inside don't close
            <nav
                class="mobile-menu"
                id="mobile-menu"
                role="navigation"
                aria-label="Mobile navigation"
                on:click=|e| e.stop_propagation()
            >
                <div class="mobile-menu-header">
                    <span class="font-semibold text-sm">"adapterOS"</span>
                    <button
                        class="mobile-menu-close"
                        on:click=move |_| on_close()
                        aria-label="Close menu"
                    >
                        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>

                <div class="mobile-menu-content">
                    <div class="mobile-menu-nav">
                        {move || {
                            build_mobile_nav_items(ui_profile.get())
                                .into_iter()
                                .map(|item| {
                                    let href = item.href;
                                    let label = item.label;
                                    let icon_path = item.icon;
                                    view! {
                                        <a
                                            href=href
                                            class="mobile-menu-link"
                                            on:click=move |_| on_close()
                                        >
                                            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                                <path stroke-linecap="round" stroke-linejoin="round" d=icon_path/>
                                            </svg>
                                            <span>{label}</span>
                                        </a>
                                    }
                                })
                                .collect::<Vec<_>>()
                        }}
                        {(!docs_url_value.is_empty()).then(|| view! {
                            <a
                                href=docs_url_value
                                class="mobile-menu-link"
                                target="_blank"
                                rel="noopener noreferrer"
                            >
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 18h.01M10 8h4m-4 4h2m7 4a2 2 0 01-2 2H7a2 2 0 01-2-2V6a2 2 0 012-2h6l4 4v8z"/>
                                </svg>
                                <span>"Documentation"</span>
                            </a>
                        })}
                    </div>
                </div>
            </nav>
        </div>
    }
}

/// Error history button with unread count badge
#[component]
fn ErrorHistoryButton() -> impl IntoView {
    let notification_state = use_notification_state();
    let error_history = use_error_history();

    // Count unread errors/warnings
    let unread_count = move || {
        notification_state
            .get()
            .notifications
            .iter()
            .filter(|n| !n.read)
            .count()
    };

    let on_click = move |_| {
        if let Some(ctx) = error_history {
            ctx.toggle();
        }
    };
    let has_unread = move || unread_count() > 0;

    view! {
        <button
            class="topbar-action relative flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted/50 transition-colors"
            on:click=on_click
            title="Error History (Ctrl+Shift+E)"
            aria-label="Open error history"
        >
            // Bell/notification icon
            <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"/>
            </svg>

            // Unread badge
            <Show when=has_unread>
                <span class="absolute -top-1 -right-1 flex items-center justify-center min-w-[18px] h-[18px] px-1 text-xs font-medium text-white bg-destructive rounded-full">
                    {move || {
                        let count = unread_count();
                        if count > 99 { "99+".to_string() } else { count.to_string() }
                    }}
                </span>
            </Show>
        </button>
    }
}
