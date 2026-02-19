//! SystemTray - System status indicators
//!
//! System tray with health indicator, connection status, and clock.

use crate::api::ApiClient;
use crate::components::status::{StatusColor, StatusIndicator};
use crate::components::status_center::use_status_center;
use crate::constants::ui_language;
use crate::hooks::{
    use_api_resource, use_cached_api_resource, use_polling, use_startup_health, use_system_status,
    CacheTtl, LoadingState,
};
use adapteros_api_types::WorkerResponse;
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
    let (system_status, _refetch_status) = use_system_status();
    let (startup_health, _refetch_startup_health) = use_startup_health();
    let (workers, _refetch_workers) = use_cached_api_resource(
        "workers_tray",
        CacheTtl::LIST,
        |client: Arc<ApiClient>| async move { client.list_workers().await },
    );

    // Current time (updates every second)
    let (time, set_time) = signal(get_current_time());

    // Update time every second using cleanup-safe polling.
    let _ = use_polling(1_000, move || async move {
        let _ = set_time.try_set(get_current_time());
    });

    view! {
        <div class="system-tray flex items-center gap-3 shrink-0">
            {move || {
                let (class, label, title) = match startup_health.get() {
                    LoadingState::Loaded(boot) => {
                        let status = boot.status.to_ascii_lowercase();
                        if status == "ready" {
                            (
                                "system-tray-pill system-tray-pill--ready",
                                ui_language::BOOT_READY.to_string(),
                                ui_language::KERNEL_BOOT_SEQUENCE.to_string(),
                            )
                        } else if status == "degraded" {
                            (
                                "system-tray-pill system-tray-pill--warn",
                                format!("{} (degraded)", ui_language::SELF_HEALING_OS),
                                boot.next_action.clone(),
                            )
                        } else if status == "failed" {
                            let phase = boot
                                .failed_phase
                                .clone()
                                .unwrap_or_else(|| "unknown phase".to_string());
                            (
                                "system-tray-pill system-tray-pill--error",
                                "Boot needs attention".to_string(),
                                format!("{}: {}", phase, boot.next_action),
                            )
                        } else {
                            (
                                "system-tray-pill system-tray-pill--booting",
                                format!("{}…", ui_language::BOOTING),
                                boot.next_action.clone(),
                            )
                        }
                    }
                    LoadingState::Error(_) => (
                        "system-tray-pill system-tray-pill--error",
                        "Boot status unavailable".to_string(),
                        "Boot monitor is currently unavailable.".to_string(),
                    ),
                    LoadingState::Idle | LoadingState::Loading => (
                        "system-tray-pill system-tray-pill--booting",
                        format!("{}…", ui_language::BOOTING),
                        "Collecting startup state.".to_string(),
                    ),
                };
                view! {
                    <a
                        href="/system"
                        class=class
                        title=title
                        aria-label="Open kernel boot sequence details"
                    >
                        {label}
                    </a>
                }
            }}
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
                let (label, tone) = match workers.get() {
                    LoadingState::Loaded(list) => {
                        let active = list
                            .iter()
                            .filter(|worker| !is_terminal_worker_state(worker))
                            .count();
                        if active == 0 {
                            ("No engines online".to_string(), "system-tray-pill system-tray-pill--warn")
                        } else {
                            (format!("{} {} live", active, ui_language::INFERENCE_ENGINES), "system-tray-pill system-tray-pill--ready")
                        }
                    }
                    LoadingState::Error(_) => (
                        "Engine status unavailable".to_string(),
                        "system-tray-pill system-tray-pill--error",
                    ),
                    LoadingState::Idle | LoadingState::Loading => (
                        "Checking engines".to_string(),
                        "system-tray-pill system-tray-pill--booting",
                    ),
                };
                view! {
                    <a
                        href="/workers"
                        class=tone
                        title="Open inference engine activity"
                    >
                        {label}
                    </a>
                }
            }}

            {move || {
                let (label, tone) = match system_status.get() {
                    LoadingState::Loaded(status) => {
                        let model = status
                            .kernel
                            .as_ref()
                            .and_then(|kernel| kernel.model.as_ref())
                            .and_then(|model| model.model_id.clone())
                            .unwrap_or_else(|| "No base loaded".to_string());
                        let short = if model.len() > 24 {
                            format!("{}…", &model[..24])
                        } else {
                            model
                        };
                        (format!("Base: {}", short), "system-tray-pill system-tray-pill--neutral")
                    }
                    LoadingState::Error(_) => (
                        "Base model status unavailable".to_string(),
                        "system-tray-pill system-tray-pill--error",
                    ),
                    LoadingState::Idle | LoadingState::Loading => (
                        "Checking base model".to_string(),
                        "system-tray-pill system-tray-pill--booting",
                    ),
                };
                view! {
                    <a
                        href="/models"
                        class=tone
                        title=ui_language::BASE_MODEL_REGISTRY
                    >
                        {label}
                    </a>
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

fn is_terminal_worker_state(worker: &WorkerResponse) -> bool {
    let status = worker.status.to_ascii_lowercase();
    matches!(
        status.as_str(),
        "stopped" | "error" | "failed" | "terminated"
    )
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
