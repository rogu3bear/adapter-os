//! SystemTray - System status indicators
//!
//! System tray with health indicator, connection status, and clock.

use crate::api::ApiClient;
use crate::components::status::{StatusColor, StatusIndicator};
use crate::components::status_center::use_status_center;
use crate::hooks::{use_api_resource, LoadingState};
use leptos::prelude::*;
use std::sync::Arc;

/// System tray with health indicator, connection status, and time
#[component]
pub fn SystemTray() -> impl IntoView {
    let ui_build_id_full = option_env!("AOS_BUILD_ID").unwrap_or("unknown");
    let ui_build_id_short = ui_build_id_full
        .split('-')
        .next()
        .unwrap_or(ui_build_id_full)
        .chars()
        .take(8)
        .collect::<String>();

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

            // Time + UI build id
            <div class="flex flex-col items-end leading-none">
                <span class="text-xs text-muted-foreground font-mono tabular-nums min-w-[4rem] text-right">
                    {move || time.get()}
                </span>
                <span
                    class="text-[10px] text-muted-foreground/70 font-mono tabular-nums"
                    title={format!("UI build id: {}", ui_build_id_full)}
                >
                    {ui_build_id_short}
                </span>
            </div>
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
