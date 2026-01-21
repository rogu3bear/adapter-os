//! TopBar - Top navigation bar
//!
//! Thin top bar with branding, command palette hint, and user menu.

use crate::components::glass_toggle::GlassThemeToggle;
use crate::components::global_search::GlobalSearchBox;
use crate::components::status::{Badge, BadgeVariant};
use crate::signals::use_auth;
use leptos::prelude::*;

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
                    <span class="font-semibold text-sm tracking-tight">"adapterOS"</span>
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
