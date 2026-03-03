//! TopBar - Top navigation bar
//!
//! Thin top bar with branding, command palette hint, and user menu.
//! Responsive: collapses to hamburger menu on mobile viewports.

use crate::components::global_search::GlobalSearchBox;
use crate::components::layout::nav_registry::build_mobile_nav_items;
use crate::components::responsive::use_is_mobile;
use crate::components::status::{Badge, BadgeVariant};
use crate::components::status_center::use_status_center;
use crate::components::IconX;
use crate::constants::ui_language;
use crate::constants::urls::docs_url;
use crate::hooks::{use_system_status, LoadingState};
use crate::signals::{
    update_setting, use_notification_state, use_search, use_settings, use_ui_profile,
    use_ui_profile_state, Density,
};
use adapteros_api_types::{
    InferenceBlocker, InferenceReadyState, StatusIndicator as ApiStatusIndicator,
    SystemStatusResponse,
};
use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Thin top bar with branding, command palette hint, and user menu.
/// Responsive: collapses to hamburger + key actions on mobile.
#[component]
pub fn TopBar() -> impl IntoView {
    let (mobile_menu_open, set_mobile_menu_open) = signal(false);
    let is_mobile = use_is_mobile();
    let settings = use_settings();
    let search = use_search();
    let (system_status, _refetch_system_status) = use_system_status();
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
    let mode_supports_lock = Memo::new(move |_| {
        system_status
            .try_get()
            .and_then(|state| {
                if let LoadingState::Loaded(status) = state {
                    let integrity_mode = status.integrity.mode.to_ascii_lowercase();
                    Some(
                        status.integrity.strict_mode
                            || integrity_mode.contains("strict")
                            || integrity_mode.contains("determin"),
                    )
                } else {
                    None
                }
            })
            .unwrap_or(false)
    });
    let mode_downgraded = Memo::new(move |_| mode_supports_lock.get() && !reproducible_ready.get());
    let density_mode = Memo::new(move |_| {
        settings
            .try_get()
            .map(|s| s.density)
            .unwrap_or(Density::Comfortable)
    });
    let toggle_density = move |_| {
        update_setting(settings, |s| {
            s.density = s.density.toggle();
        });
    };
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
                    <span class="topbar-brand-text font-semibold text-sm tracking-tight">"AdapterOS"</span>
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
                        class="fingerprint-badge__provenance inline-flex items-center justify-center p-1"
                        title=ui_language::CONFIG_FINGERPRINT_PROVENANCE
                    >
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                        </svg>
                    </a>
                </div>

                <div
                    class=move || {
                        if mode_downgraded.get() {
                            "mode-pill mode-pill--downgraded".to_string()
                        } else if mode_supports_lock.get() {
                            "mode-pill mode-pill--locked".to_string()
                        } else {
                            "mode-pill mode-pill--fast".to_string()
                        }
                    }
                    aria-label=move || {
                        if mode_downgraded.get() {
                            "Execution mode: Locked Output downgraded".to_string()
                        } else if mode_supports_lock.get() {
                            "Execution mode: Locked Output".to_string()
                        } else {
                            "Execution mode: Fast".to_string()
                        }
                    }
                    title=move || {
                        if mode_downgraded.get() {
                            "Locked Output requested but currently downgraded by system readiness.".to_string()
                        } else if mode_supports_lock.get() {
                            ui_language::REPRODUCIBLE_READY.to_string()
                        } else {
                            ui_language::REPRODUCIBLE_PENDING.to_string()
                        }
                    }
                >
                    <span class="mode-pill__icon" aria-hidden="true">
                        {move || {
                            if mode_supports_lock.get() {
                                view! {
                                    <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 11V7a4 4 0 00-8 0v4m16 0H4m16 0v8a2 2 0 01-2 2H6a2 2 0 01-2-2v-8"/>
                                    </svg>
                                }.into_any()
                            } else {
                                view! {
                                    <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z"/>
                                    </svg>
                                }.into_any()
                            }
                        }}
                    </span>
                    <span class="mode-pill__label">
                        {move || {
                            if mode_supports_lock.get() {
                                ui_language::LOCKED_OUTPUT.to_string()
                            } else {
                                "Fast".to_string()
                            }
                        }}
                    </span>
                    {move || {
                        if mode_downgraded.get() {
                            view! { <span class="mode-pill__downgrade">"Downgraded"</span> }.into_any()
                        } else {
                            view! {}.into_any()
                        }
                    }}
                </div>
            </div>

            // Center: Global search box (opens Command Palette) - hidden on mobile
            <div class="topbar-search flex-1 flex justify-center">
                <GlobalSearchBox/>
            </div>

            // Right: Glass toggle + User menu
            <div class="topbar-actions flex items-center gap-2">
                <button
                    class="topbar-action topbar-density-toggle"
                    on:click=toggle_density
                    aria-label=move || {
                        format!("Density: {}. Click to toggle.", density_mode.get().display())
                    }
                    title=move || {
                        format!(
                            "Density: {} (click to switch)",
                            density_mode.get().display()
                        )
                    }
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M4 7h16M4 12h16M4 17h16"/>
                    </svg>
                    <span class="topbar-density-toggle__label">
                        {move || format!("Density: {}", density_mode.get().display())}
                    </span>
                </button>
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
                // Status Center Bell
                <ErrorHistoryButton />
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
                    <span class="font-semibold text-sm">"AdapterOS"</span>
                    <button
                        class="mobile-menu-close"
                        on:click=move |_| on_close()
                        aria-label="Close menu"
                    >
                        <IconX class="w-5 h-5"/>
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
            // Clean SVG Notification Bell icon
            <svg class="w-5 h-5 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
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
