//! TopBar - Top navigation bar
//!
//! Thin top bar with branding, command palette hint, and user menu.
//! Responsive: collapses to hamburger menu on mobile viewports.

use crate::api::report_error_with_toast;
use crate::components::glass_toggle::GlassThemeToggle;
use crate::components::global_search::GlobalSearchBox;
use crate::components::layout::nav_registry::build_mobile_nav_items;
use crate::components::responsive::use_is_mobile;
use crate::components::status::{Badge, BadgeVariant};
use crate::components::status_center::use_status_center;
use crate::constants::ui_language;
use crate::constants::urls::docs_url;
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{
    use_auth, use_notification_state, use_notifications, use_refetch, use_search, use_ui_profile,
    use_ui_profile_state,
};
use adapteros_api_types::{
    InferenceBlocker, InferenceReadyState, StatusIndicator as ApiStatusIndicator,
    SystemStatusResponse,
};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

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
    let ui_profile_state = use_ui_profile_state();
    let (system_status, _refetch_system_status) = use_system_status();
    let docs_url_value = Signal::derive(move || {
        ui_profile_state
            .try_get()
            .and_then(|s| s.runtime_docs_url)
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(docs_url)
    });
    let fingerprint = Memo::new(move |_| match system_status.try_get() {
        Some(LoadingState::Loaded(status)) => Some(configuration_fingerprint(&status)),
        _ => None,
    });
    let reproducible_ready = Memo::new(move |_| {
        system_status
            .try_get()
            .and_then(|state| {
                if let LoadingState::Loaded(status) = state {
                    Some(is_reproducible_mode_ready(&status))
                } else {
                    None
                }
            })
            .unwrap_or(false)
    });
    let fingerprint_changed = RwSignal::new(false);
    let last_fingerprint = RwSignal::new(None::<String>);
    Effect::new(move || {
        let Some(next_fingerprint) = fingerprint.try_get().flatten() else {
            return;
        };
        let previous = last_fingerprint.get_untracked();
        if previous.as_deref() != Some(next_fingerprint.as_str()) {
            if previous.is_some() {
                let changed_signal = fingerprint_changed;
                changed_signal.set(true);
                set_timeout_simple(move || changed_signal.set(false), 1500);
            }
            last_fingerprint.set(Some(next_fingerprint));
        }
    });

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

        // Use try_get/try_set to avoid panic when signals are disposed
        // during SPA navigation (these closures are leaked via .forget())
        let click_closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            if !user_menu_open.try_get_untracked().unwrap_or(false) {
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
            let _ = set_user_menu_open.try_set(false);
        }) as Box<dyn FnMut(_)>);

        let key_closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if !user_menu_open.try_get_untracked().unwrap_or(false) {
                return;
            }
            if event.key() == "Escape" {
                let _ = set_user_menu_open.try_set(false);
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
        <header class="topbar os-topbar h-12 flex items-center justify-between border-b border-border/50 shrink-0">
            // Left: Hamburger (mobile) + product identity + trust badges
            <div class="topbar-left flex items-center gap-3 min-w-0">
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
                    <span class="topbar-brand-text font-semibold text-sm tracking-tight">"AdapterOS Kernel"</span>
                    <Badge variant=env_badge_variant>{env_badge}</Badge>
                </div>

                // Always-visible runtime identity: Current Configuration Fingerprint.
                <div class=move || {
                    let changed = fingerprint_changed.try_get().unwrap_or(false);
                    format!(
                        "fingerprint-badge {}",
                        if changed {
                            "fingerprint-badge--changed"
                        } else {
                            ""
                        }
                    )
                }
                    aria-label="Current Configuration Fingerprint"
                >
                    <span class="fingerprint-badge__label">{ui_language::CONFIG_FINGERPRINT_LABEL}</span>
                    <span
                        class="fingerprint-badge__value"
                        title=ui_language::CONFIG_FINGERPRINT_HELP
                    >
                        {move || {
                            fingerprint
                                .try_get()
                                .flatten()
                                .map(|value| short_fingerprint(&value))
                                .unwrap_or_else(|| ui_language::CONFIG_FINGERPRINT_LOADING.to_string())
                        }}
                    </span>
                    <button
                        class="fingerprint-badge__copy"
                        title=ui_language::CONFIG_FINGERPRINT_COPY
                        aria-label=ui_language::CONFIG_FINGERPRINT_COPY
                        on:click=move |_| {
                            let value = fingerprint
                                .get_untracked()
                                .unwrap_or_else(|| ui_language::CONFIG_FINGERPRINT_EMPTY.to_string());
                            wasm_bindgen_futures::spawn_local(async move {
                                let _ = copy_text_to_clipboard(&value).await;
                            });
                        }
                    >
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M8 16h8M8 12h8m-8-4h8m5 10a2 2 0 01-2 2H7a2 2 0 01-2-2V6a2 2 0 012-2h8l4 4v10z"/>
                        </svg>
                    </button>
                    <a
                        href="/runs"
                        class="fingerprint-badge__provenance"
                        title=ui_language::CONFIG_FINGERPRINT_PROVENANCE
                    >
                        {ui_language::CONFIG_FINGERPRINT_PROVENANCE}
                    </a>
                </div>

                <div
                    class=move || {
                        if reproducible_ready.get() {
                            "trust-badge trust-badge--locked".to_string()
                        } else {
                            "trust-badge".to_string()
                        }
                    }
                    aria-label=move || {
                        if reproducible_ready.get() {
                            "Locked Output active".to_string()
                        } else {
                            "Locked Output pending".to_string()
                        }
                    }
                    title=move || {
                        if reproducible_ready.get() {
                            ui_language::REPRODUCIBLE_READY.to_string()
                        } else {
                            ui_language::REPRODUCIBLE_PENDING.to_string()
                        }
                    }
                >
                    <span class="trust-badge__icon" aria-hidden="true">
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M12 11V7a4 4 0 00-8 0v4m16 0H4m16 0v8a2 2 0 01-2 2H6a2 2 0 01-2-2v-8"/>
                        </svg>
                    </span>
                    <span class="trust-badge__text">{ui_language::REPRODUCIBLE_MODE}</span>
                    <span class="trust-badge__state">{ui_language::LOCKED_OUTPUT}</span>
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
                {move || {
                    let href = docs_url_value.get();
                    (!href.is_empty()).then(|| view! {
                        <a
                            class="topbar-action flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50 transition-colors"
                            href=href
                            target="_blank"
                            rel="noopener noreferrer"
                        >
                            <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M12 18h.01M10 8h4m-4 4h2m7 4a2 2 0 01-2 2H7a2 2 0 01-2-2V6a2 2 0 012-2h6l4 4v8z"/>
                            </svg>
                            <span class="text-xs text-muted-foreground">"Manual"</span>
                        </a>
                    })
                }}
                // Error history button with unread badge
                <ErrorHistoryButton />

                // Tenant picker (multi-tenant users only)
                <TenantPicker />

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
                        id="user-menu-button"
                        node_ref=user_menu_button_ref
                        on:click=move |_| set_user_menu_open.update(|v| *v = !*v)
                        aria-expanded=move || user_menu_open.get().to_string()
                        aria-controls="user-menu"
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
                            class="absolute right-0 top-full mt-1 w-48 rounded-lg shadow-lg z-50"
                            id="user-menu"
                            aria-labelledby="user-menu-button"
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
                                                "Workspace Profile"
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
                                                "Personal Preferences"
                                            </a>
                                        </div>
                                        // Session actions
                                        <div class="p-1">
                                            <button
                                                class="flex items-center gap-2 w-full text-left px-3 py-2 text-sm rounded hover:bg-muted/50 transition-colors text-destructive"
                                                on:click=move |_| {
                                                    set_user_menu_open.set(false);
                                                    auth_action_signal.try_with_value(|action| {
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
    let ui_profile_state = use_ui_profile_state();
    let docs_url_value = Signal::derive(move || {
        ui_profile_state
            .try_get()
            .and_then(|s| s.runtime_docs_url)
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(docs_url)
    });
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
                    <span class="font-semibold text-sm">"AdapterOS Kernel"</span>
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
                        {move || {
                            let href = docs_url_value.get();
                            (!href.is_empty()).then(|| view! {
                                <a
                                    href=href
                                    class="mobile-menu-link"
                                    target="_blank"
                                    rel="noopener noreferrer"
                                >
                                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 18h.01M10 8h4m-4 4h2m7 4a2 2 0 01-2 2H7a2 2 0 01-2-2V6a2 2 0 012-2h6l4 4v8z"/>
                                    </svg>
                                    <span>"Operator Manual"</span>
                                </a>
                            })
                        }}
                    </div>
                </div>
            </nav>
        </div>
    }
}

/// Tenant picker for users with access to multiple tenants.
///
/// Shows a dropdown select when `admin_tenants.len() > 1`. On selection change,
/// POSTs to the tenant switch endpoint, refreshes auth state, and triggers
/// a global refetch so all tenant-scoped data reloads.
#[component]
fn TenantPicker() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();
    let notifications = use_notifications();
    let refetch = use_refetch();
    let auth_action_stored = StoredValue::new(auth_action);
    let notifications_stored = StoredValue::new(notifications);
    let refetch_stored = StoredValue::new(refetch);
    let (switching, set_switching) = signal(false);

    // Extract tenant info reactively
    let tenants = Signal::derive(move || {
        auth_state
            .get()
            .user()
            .map(|u| (u.tenant_id.clone(), u.admin_tenants.clone()))
    });

    let on_change = move |ev: web_sys::Event| {
        let target = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok());
        let selected = match target {
            Some(el) => el.value(),
            None => return,
        };

        // Skip if already on this tenant
        if tenants
            .get()
            .map(|(current, _)| current == selected)
            .unwrap_or(true)
        {
            return;
        }

        set_switching.set(true);
        let selected_id = selected.clone();

        auth_action_stored.with_value(|action| {
            let action = action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match action.switch_tenant(&selected_id).await {
                    Ok(()) => {
                        notifications_stored.with_value(|n| {
                            n.success(
                                "Workspace switched",
                                &format!("Now using workspace {}", selected_id),
                            );
                        });
                        refetch_stored.with_value(|r| r.all());
                    }
                    Err(e) => {
                        report_error_with_toast(&e, "Failed to switch tenant", None, true);
                    }
                }
                set_switching.set(false);
            });
        });
    };

    view! {
        {move || {
            let info = tenants.get();
            match info {
                Some((current, admin_tenants)) if admin_tenants.len() > 1 => {
                    let options = admin_tenants
                        .iter()
                        .map(|t| {
                            let selected = t == &current;
                            let val = t.clone();
                            let label = t.clone();
                            view! {
                                <option value=val selected=selected>{label}</option>
                            }
                        })
                        .collect::<Vec<_>>();

                    Some(view! {
                        <div class="topbar-action-secondary flex items-center gap-2">
                            <span
                                class="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground/80"
                                title="Tenants are workspaces, not user accounts"
                            >
                                "Workspace"
                            </span>
                            <select
                                class="tenant-picker text-xs bg-transparent border border-border/50 rounded px-2 py-1 text-foreground cursor-pointer hover:bg-muted/50 transition-colors focus:outline-none focus:ring-1 focus:ring-primary/50"
                                on:change=on_change
                                disabled=move || switching.get()
                                aria-label="Switch workspace tenant"
                                title="Switch workspace (tenant context, not user account)"
                            >
                                {options}
                            </select>
                        </div>
                    })
                }
                _ => None,
            }
        }}
    }
}

/// Error history button with unread count badge
#[component]
fn ErrorHistoryButton() -> impl IntoView {
    let notification_state = use_notification_state();
    let status_center = use_status_center();

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
        if let Some(ctx) = status_center {
            ctx.toggle();
        }
    };
    let has_unread = move || unread_count() > 0;

    view! {
        <button
            class="topbar-action relative flex items-center justify-center w-8 h-8 rounded-md hover:bg-muted/50 transition-colors"
            on:click=on_click
            title="Event Viewer (Ctrl+Shift+S)"
            aria-label="Event Viewer (Ctrl+Shift+S)"
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

pub(crate) fn fingerprint_seed(status: &SystemStatusResponse) -> String {
    let model_id = status
        .kernel
        .as_ref()
        .and_then(|kernel| kernel.model.as_ref())
        .and_then(|model| model.model_id.clone())
        .unwrap_or_else(|| "none".to_string());
    let plan_id = status
        .kernel
        .as_ref()
        .and_then(|kernel| kernel.plan.as_ref())
        .map(|plan| plan.plan_id.clone())
        .unwrap_or_else(|| "none".to_string());
    let inference_ready = match status.inference_ready {
        InferenceReadyState::True => "ready",
        InferenceReadyState::False => "blocked",
        InferenceReadyState::Unknown => "unknown",
    };
    let readiness = match status.readiness.overall {
        ApiStatusIndicator::Ready => "ready",
        ApiStatusIndicator::NotReady => "not_ready",
        ApiStatusIndicator::Unknown => "unknown",
    };
    let mut blockers = status
        .inference_blockers
        .iter()
        .map(fingerprint_blocker_key)
        .collect::<Vec<_>>();
    blockers.sort_unstable();
    let blockers = if blockers.is_empty() {
        "none".to_string()
    } else {
        blockers.join(",")
    };
    let boot_phase = status
        .boot
        .as_ref()
        .map(|boot| boot.phase.clone())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        status.integrity.mode,
        status.integrity.strict_mode,
        model_id,
        plan_id,
        inference_ready,
        readiness,
        blockers,
        boot_phase
    )
}

pub(crate) fn configuration_fingerprint(status: &SystemStatusResponse) -> String {
    // Deterministic FNV-1a digest for a stable, copyable UI fingerprint.
    let seed = fingerprint_seed(status);
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("CFG-{:016x}", hash)
}

pub(crate) fn short_fingerprint(value: &str) -> String {
    if value.len() <= 18 {
        value.to_string()
    } else {
        format!(
            "{}…{}",
            &value[..10],
            &value[value.len().saturating_sub(6)..]
        )
    }
}

pub(crate) fn is_reproducible_mode_ready(status: &SystemStatusResponse) -> bool {
    let integrity_mode = status.integrity.mode.to_ascii_lowercase();
    let mode_supports_lock = status.integrity.strict_mode
        || integrity_mode.contains("strict")
        || integrity_mode.contains("determin");
    let readiness_ready = matches!(status.readiness.overall, ApiStatusIndicator::Ready);
    let has_critical_blockers = status.inference_blockers.iter().any(|blocker| {
        matches!(
            blocker,
            InferenceBlocker::BootFailed
                | InferenceBlocker::SystemBooting
                | InferenceBlocker::DatabaseUnavailable
                | InferenceBlocker::WorkerMissing
                | InferenceBlocker::NoModelLoaded
                | InferenceBlocker::ActiveModelMismatch
        )
    });
    mode_supports_lock && readiness_ready && !has_critical_blockers
}

fn fingerprint_blocker_key(blocker: &InferenceBlocker) -> &'static str {
    match blocker {
        InferenceBlocker::DatabaseUnavailable => "db_unavailable",
        InferenceBlocker::WorkerMissing => "engine_missing",
        InferenceBlocker::NoModelLoaded => "base_missing",
        InferenceBlocker::ActiveModelMismatch => "base_mismatch",
        InferenceBlocker::TelemetryDegraded => "telemetry_degraded",
        InferenceBlocker::SystemBooting => "booting",
        InferenceBlocker::BootFailed => "boot_failed",
    }
}

async fn copy_text_to_clipboard(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    let clipboard =
        js_sys::Reflect::get(&navigator, &wasm_bindgen::JsValue::from_str("clipboard")).ok();
    let Some(clipboard) = clipboard else {
        return false;
    };
    let write_text =
        js_sys::Reflect::get(&clipboard, &wasm_bindgen::JsValue::from_str("writeText")).ok();
    let Some(write_text) = write_text else {
        return false;
    };
    let Ok(write_text) = write_text.dyn_into::<js_sys::Function>() else {
        return false;
    };
    let promise = match write_text.call1(&clipboard, &wasm_bindgen::JsValue::from_str(text)) {
        Ok(promise) => promise,
        Err(_) => return false,
    };
    JsFuture::from(js_sys::Promise::resolve(&promise))
        .await
        .is_ok()
}

#[cfg(target_arch = "wasm32")]
fn set_timeout_simple<F>(f: F, ms: i32)
where
    F: FnOnce() + 'static,
{
    let closure = Closure::once_into_js(f);
    if let Some(window) = web_sys::window() {
        let _ = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}
