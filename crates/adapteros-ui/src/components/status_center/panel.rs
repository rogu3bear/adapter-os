//! Status Center Panel
//!
//! Main panel component with backdrop, sliding animation, and status sections.

use super::hooks::{use_escape_key, use_status_data, StatusLoadingState};
use super::items::{StatusItem, StatusItemMemory, StatusItemSeverity};
use super::sections::{StatusDivider, StatusSection, StatusSectionBadgeVariant};
use crate::components::Spinner;
use adapteros_api_types::{
    DataAvailability, InferenceBlocker, InferenceReadyState, MemoryPressureLevel,
    ServiceHealthStatus, StatusIndicator as ApiStatusIndicator,
};
use leptos::prelude::*;

/// Status Center Panel component
///
/// A sliding panel that displays comprehensive system status information.
/// Fetches data from /v1/system/status and /v1/system/state endpoints.
#[component]
pub fn StatusCenterPanel(
    /// Whether the panel is open
    open: RwSignal<bool>,
) -> impl IntoView {
    // Escape key closes the panel
    let escape_count = use_escape_key();
    Effect::new(move || {
        let _ = escape_count.get();
        if open.get() {
            open.set(false);
        }
    });

    // Fetch status data when panel opens
    let (status_state, refetch) = use_status_data();

    // Refetch when panel opens
    let refetch_clone = refetch.clone();
    Effect::new(move || {
        if open.get() {
            refetch_clone();
        }
    });

    let close = move |_| open.set(false);
    let refetch_for_button = refetch.clone();

    view! {
        // Backdrop
        <div
            class=move || {
                if open.get() {
                    "status-center-backdrop status-center-backdrop-visible"
                } else {
                    "status-center-backdrop status-center-backdrop-hidden"
                }
            }
            on:click=close
        />

        // Panel
        <div
            class=move || {
                if open.get() {
                    "status-center-panel status-center-panel-open"
                } else {
                    "status-center-panel status-center-panel-closed"
                }
            }
            role="dialog"
            aria-modal="true"
            aria-labelledby="status-center-title"
        >
            // Header
            <div class="status-center-header">
                <h2 id="status-center-title" class="status-center-title">
                    "Status Center"
                </h2>
                <div class="status-center-header-actions">
                    // Refresh button
                    <button
                        class="status-center-refresh-btn"
                        on:click=move |_| refetch_for_button()
                        title="Refresh status"
                        disabled=move || status_state.get().is_loading()
                    >
                        <svg
                            class=move || {
                                if status_state.get().is_loading() {
                                    "status-center-refresh-icon status-center-refresh-spinning"
                                } else {
                                    "status-center-refresh-icon"
                                }
                            }
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                        >
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>
                        </svg>
                    </button>

                    // Close button
                    <button
                        class="status-center-close-btn"
                        on:click=close
                        title="Close (Escape)"
                    >
                        <svg class="status-center-close-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>
            </div>

            // Content
            <div class="status-center-content">
                {move || {
                    match status_state.get() {
                        StatusLoadingState::Idle | StatusLoadingState::Loading => {
                            view! {
                                <div class="status-center-loading">
                                    <Spinner />
                                    <p class="status-center-loading-text">"Loading status..."</p>
                                </div>
                            }.into_any()
                        }
                        StatusLoadingState::Error(ref e) => {
                            let error_msg = e.to_string();
                            view! {
                                <div class="status-center-error">
                                    <svg class="status-center-error-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
                                    </svg>
                                    <p class="status-center-error-text">"Failed to load status"</p>
                                    <p class="status-center-error-detail">{error_msg}</p>
                                </div>
                            }.into_any()
                        }
                        StatusLoadingState::Loaded(ref data) => {
                            let status = data.status.clone();
                            let state = data.state.clone();

                            view! {
                                <StatusCenterSections status=status state=state />
                            }.into_any()
                        }
                    }
                }}
            </div>

            // Footer with keyboard shortcut hint
            <div class="status-center-footer">
                <span class="status-center-shortcut-hint">
                    <kbd class="status-center-kbd">"Ctrl"</kbd>
                    " + "
                    <kbd class="status-center-kbd">"Shift"</kbd>
                    " + "
                    <kbd class="status-center-kbd">"S"</kbd>
                    " to toggle"
                </span>
            </div>
        </div>
    }
}

/// Internal component to render all status sections
#[component]
fn StatusCenterSections(
    status: adapteros_api_types::SystemStatusResponse,
    state: adapteros_api_types::SystemStateResponse,
) -> impl IntoView {
    // Clone values needed in view! macro to avoid ownership issues
    // Each clone is for a separate closure in the view! macro
    let readiness_checks = status.readiness.checks.clone();
    let kernel_for_model = status.kernel.clone();
    let kernel_for_uma = status.kernel.clone();
    let kernel_for_ane = status.kernel.clone();
    let ready_count = [
        &readiness_checks.db,
        &readiness_checks.migrations,
        &readiness_checks.workers,
        &readiness_checks.models,
    ]
    .iter()
    .filter(|c| c.status == ApiStatusIndicator::Ready)
    .count();

    let total_checks = 4usize;

    let readiness_badge_variant = if ready_count == total_checks {
        StatusSectionBadgeVariant::Success
    } else if ready_count == 0 {
        StatusSectionBadgeVariant::Error
    } else {
        StatusSectionBadgeVariant::Warning
    };

    // Inference blockers count
    let blockers_count = status.inference_blockers.len();
    let inference_badge_variant = match status.inference_ready {
        InferenceReadyState::True => StatusSectionBadgeVariant::Success,
        InferenceReadyState::False => StatusSectionBadgeVariant::Error,
        InferenceReadyState::Unknown => StatusSectionBadgeVariant::Warning,
    };

    // Services health
    let healthy_services = state
        .node
        .services
        .iter()
        .filter(|s| s.status == ServiceHealthStatus::Healthy)
        .count();
    let total_services = state.node.services.len();
    let services_badge_variant = if healthy_services == total_services {
        StatusSectionBadgeVariant::Success
    } else if healthy_services == 0 {
        StatusSectionBadgeVariant::Error
    } else {
        StatusSectionBadgeVariant::Warning
    };

    view! {
        // Readiness Section
        <StatusSection
            title="Readiness"
            badge_count=ready_count
            badge_variant=readiness_badge_variant
            initially_expanded=true
        >
            <StatusItem
                label="Overall"
                value=format_status_indicator(status.readiness.overall)
                severity=status_indicator_to_severity(status.readiness.overall)
            />
            <StatusItem
                label="Database"
                value=format_component_check(&readiness_checks.db)
                severity=status_indicator_to_severity(readiness_checks.db.status)
                detail=readiness_checks.db.reason.clone().unwrap_or_default()
            />
            <StatusItem
                label="Migrations"
                value=format_component_check(&readiness_checks.migrations)
                severity=status_indicator_to_severity(readiness_checks.migrations.status)
                detail=readiness_checks.migrations.reason.clone().unwrap_or_default()
            />
            <StatusItem
                label="Workers"
                value=format_component_check(&readiness_checks.workers)
                severity=status_indicator_to_severity(readiness_checks.workers.status)
                detail=readiness_checks.workers.reason.clone().unwrap_or_default()
            />
            <StatusItem
                label="Models"
                value=format_component_check(&readiness_checks.models)
                severity=status_indicator_to_severity(readiness_checks.models.status)
                detail=readiness_checks.models.reason.clone().unwrap_or_default()
            />
        </StatusSection>

        <StatusDivider />

        // Inference Section
        <StatusSection
            title="Inference"
            badge_count={ if blockers_count > 0 { blockers_count } else { 1 } }
            badge_variant=inference_badge_variant
            initially_expanded=true
        >
            <StatusItem
                label="Inference Ready"
                value=format_inference_ready(status.inference_ready)
                severity=inference_ready_to_severity(status.inference_ready)
                pulsing=matches!(status.inference_ready, InferenceReadyState::True)
            />
            {if !status.inference_blockers.is_empty() {
                view! {
                    <div class="status-blockers">
                        <span class="status-blockers-label">"Blockers:"</span>
                        {status.inference_blockers.iter().map(|blocker| {
                            view! {
                                <StatusItem
                                    label=""
                                    value=format_blocker(blocker)
                                    severity=StatusItemSeverity::Error
                                />
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
            {kernel_for_model.as_ref().and_then(|k| k.model.as_ref()).map(|m| {
                view! {
                    <StatusItem
                        label="Active Model"
                        value=m.model_id.clone().unwrap_or_else(|| "None".to_string())
                        severity=if m.status == "loaded" { StatusItemSeverity::Success } else { StatusItemSeverity::Warning }
                        detail=format!("Status: {}", m.status)
                    />
                }
            })}
        </StatusSection>

        <StatusDivider />

        // Memory Section
        <StatusSection
            title="Memory"
            badge_variant=memory_pressure_to_badge(&state.memory.pressure_level)
            initially_expanded=false
        >
            <StatusItemMemory
                label="System Memory"
                used=Some(state.memory.used_mb)
                total=Some(state.memory.total_mb)
                available=true
            />
            <StatusItem
                label="Headroom"
                value=format!("{:.1}%", state.memory.headroom_percent)
                severity=headroom_to_severity(state.memory.headroom_percent)
            />
            <StatusItem
                label="Pressure Level"
                value=format_pressure_level(&state.memory.pressure_level)
                severity=pressure_to_severity(&state.memory.pressure_level)
            />

            // UMA Memory (if available in status)
            {kernel_for_uma.as_ref().and_then(|k| k.memory.as_ref()).and_then(|m| m.uma.as_ref()).map(|uma| {
                let available = uma.availability == DataAvailability::Available;
                view! {
                    <StatusItemMemory
                        label="UMA Memory"
                        used=uma.used_mb
                        total=uma.total_mb
                        available=available
                    />
                }
            })}

            // ANE Memory (if available in status)
            {kernel_for_ane.as_ref().and_then(|k| k.memory.as_ref()).and_then(|m| m.ane.as_ref()).map(|ane| {
                let available = ane.availability == DataAvailability::Available;
                view! {
                    <StatusItemMemory
                        label="ANE Memory"
                        used=ane.used_mb
                        total=ane.allocated_mb
                        available=available
                    />
                }
            })}

            // Top Adapters by Memory
            {if !state.memory.top_adapters.is_empty() {
                view! {
                    <div class="status-top-adapters">
                        <span class="status-top-adapters-label">"Top Adapters:"</span>
                        {state.memory.top_adapters.iter().take(5).map(|a| {
                            view! {
                                <StatusItem
                                    label=a.name.clone()
                                    value=format!("{:.1} MB", a.memory_mb)
                                    severity=StatusItemSeverity::Info
                                    detail=format!("State: {}", a.state)
                                />
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </StatusSection>

        <StatusDivider />

        // Node Section
        <StatusSection
            title="Node"
            badge_count=healthy_services
            badge_variant=services_badge_variant
            initially_expanded=false
        >
            <StatusItem
                label="Hostname"
                value=state.origin.hostname.clone()
                severity=StatusItemSeverity::Info
            />
            <StatusItem
                label="Federation Role"
                value=state.origin.federation_role.clone()
                severity=StatusItemSeverity::Info
            />
            <StatusItem
                label="Uptime"
                value=format_uptime(state.node.uptime_seconds)
                severity=StatusItemSeverity::Info
            />
            <StatusItem
                label="CPU Usage"
                value=format!("{:.1}%", state.node.cpu_usage_percent)
                severity=cpu_to_severity(state.node.cpu_usage_percent)
            />
            <StatusItem
                label="GPU Available"
                value=if state.node.gpu_available { "Yes" } else { "No" }.to_string()
                severity=if state.node.gpu_available { StatusItemSeverity::Success } else { StatusItemSeverity::Warning }
            />
            <StatusItem
                label="ANE Available"
                value=if state.node.ane_available { "Yes" } else { "No" }.to_string()
                severity=if state.node.ane_available { StatusItemSeverity::Success } else { StatusItemSeverity::Warning }
            />

            // Services
            {if !state.node.services.is_empty() {
                view! {
                    <div class="status-services">
                        <span class="status-services-label">"Services:"</span>
                        {state.node.services.iter().map(|s| {
                            view! {
                                <StatusItem
                                    label=s.name.clone()
                                    value=format_service_status(s.status)
                                    severity=service_status_to_severity(s.status)
                                />
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else {
                view! {}.into_any()
            }}
        </StatusSection>

        // Integrity Section
        <StatusDivider />
        <StatusSection
            title="Integrity"
            initially_expanded=false
        >
            <StatusItem
                label="Mode"
                value=status.integrity.mode.clone()
                severity=StatusItemSeverity::Info
            />
            <StatusItem
                label="Federated"
                value=if status.integrity.is_federated { "Yes" } else { "No" }.to_string()
                severity=StatusItemSeverity::Info
            />
            <StatusItem
                label="Strict Mode"
                value=if status.integrity.strict_mode { "Enabled" } else { "Disabled" }.to_string()
                severity=if status.integrity.strict_mode { StatusItemSeverity::Success } else { StatusItemSeverity::Info }
            />
            <StatusItem
                label="PF Deny OK"
                value=if status.integrity.pf_deny_ok { "Yes" } else { "No" }.to_string()
                severity=if status.integrity.pf_deny_ok { StatusItemSeverity::Success } else { StatusItemSeverity::Warning }
            />
            <StatusItem
                label="Drift Level"
                value=format!("{:?}", status.integrity.drift.level)
                severity=drift_level_to_severity(&status.integrity.drift.level)
                detail=status.integrity.drift.summary.clone().unwrap_or_default()
            />
        </StatusSection>
    }
}

// Helper functions for formatting and conversion

fn format_status_indicator(indicator: ApiStatusIndicator) -> String {
    match indicator {
        ApiStatusIndicator::Ready => "Ready".to_string(),
        ApiStatusIndicator::NotReady => "Not Ready".to_string(),
        ApiStatusIndicator::Unknown => "Unknown".to_string(),
    }
}

fn status_indicator_to_severity(indicator: ApiStatusIndicator) -> StatusItemSeverity {
    match indicator {
        ApiStatusIndicator::Ready => StatusItemSeverity::Success,
        ApiStatusIndicator::NotReady => StatusItemSeverity::Error,
        ApiStatusIndicator::Unknown => StatusItemSeverity::Warning,
    }
}

fn format_component_check(check: &adapteros_api_types::ComponentCheck) -> String {
    let status = format_status_indicator(check.status);
    if let Some(latency) = check.latency_ms {
        format!("{} ({}ms)", status, latency)
    } else {
        status
    }
}

fn format_inference_ready(state: InferenceReadyState) -> String {
    match state {
        InferenceReadyState::True => "Ready".to_string(),
        InferenceReadyState::False => "Not Ready".to_string(),
        InferenceReadyState::Unknown => "Unknown".to_string(),
    }
}

fn inference_ready_to_severity(state: InferenceReadyState) -> StatusItemSeverity {
    match state {
        InferenceReadyState::True => StatusItemSeverity::Success,
        InferenceReadyState::False => StatusItemSeverity::Error,
        InferenceReadyState::Unknown => StatusItemSeverity::Warning,
    }
}

fn format_blocker(blocker: &InferenceBlocker) -> String {
    match blocker {
        InferenceBlocker::DatabaseUnavailable => "Database Unavailable".to_string(),
        InferenceBlocker::WorkerMissing => "Worker Missing".to_string(),
        InferenceBlocker::NoModelLoaded => "No Model Loaded".to_string(),
        InferenceBlocker::ActiveModelMismatch => "Active Model Mismatch".to_string(),
        InferenceBlocker::TelemetryDegraded => "Telemetry Degraded".to_string(),
        InferenceBlocker::SystemBooting => "System Booting".to_string(),
        InferenceBlocker::BootFailed => "Boot Failed".to_string(),
    }
}

fn format_pressure_level(level: &MemoryPressureLevel) -> String {
    match level {
        MemoryPressureLevel::Low => "Low".to_string(),
        MemoryPressureLevel::Medium => "Medium".to_string(),
        MemoryPressureLevel::High => "High".to_string(),
        MemoryPressureLevel::Critical => "Critical".to_string(),
    }
}

fn pressure_to_severity(level: &MemoryPressureLevel) -> StatusItemSeverity {
    match level {
        MemoryPressureLevel::Low => StatusItemSeverity::Success,
        MemoryPressureLevel::Medium => StatusItemSeverity::Info,
        MemoryPressureLevel::High => StatusItemSeverity::Warning,
        MemoryPressureLevel::Critical => StatusItemSeverity::Error,
    }
}

fn memory_pressure_to_badge(level: &MemoryPressureLevel) -> StatusSectionBadgeVariant {
    match level {
        MemoryPressureLevel::Low => StatusSectionBadgeVariant::Success,
        MemoryPressureLevel::Medium => StatusSectionBadgeVariant::Info,
        MemoryPressureLevel::High => StatusSectionBadgeVariant::Warning,
        MemoryPressureLevel::Critical => StatusSectionBadgeVariant::Error,
    }
}

fn headroom_to_severity(headroom: f32) -> StatusItemSeverity {
    if headroom >= 20.0 {
        StatusItemSeverity::Success
    } else if headroom >= 15.0 {
        StatusItemSeverity::Info
    } else if headroom >= 10.0 {
        StatusItemSeverity::Warning
    } else {
        StatusItemSeverity::Error
    }
}

fn cpu_to_severity(cpu: f32) -> StatusItemSeverity {
    if cpu < 50.0 {
        StatusItemSeverity::Success
    } else if cpu < 75.0 {
        StatusItemSeverity::Info
    } else if cpu < 90.0 {
        StatusItemSeverity::Warning
    } else {
        StatusItemSeverity::Error
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn format_service_status(status: ServiceHealthStatus) -> String {
    match status {
        ServiceHealthStatus::Healthy => "Healthy".to_string(),
        ServiceHealthStatus::Degraded => "Degraded".to_string(),
        ServiceHealthStatus::Unhealthy => "Unhealthy".to_string(),
        ServiceHealthStatus::Unknown => "Unknown".to_string(),
    }
}

fn service_status_to_severity(status: ServiceHealthStatus) -> StatusItemSeverity {
    match status {
        ServiceHealthStatus::Healthy => StatusItemSeverity::Success,
        ServiceHealthStatus::Degraded => StatusItemSeverity::Warning,
        ServiceHealthStatus::Unhealthy => StatusItemSeverity::Error,
        ServiceHealthStatus::Unknown => StatusItemSeverity::Info,
    }
}

fn drift_level_to_severity(level: &adapteros_api_types::DriftLevel) -> StatusItemSeverity {
    match level {
        adapteros_api_types::DriftLevel::Ok => StatusItemSeverity::Success,
        adapteros_api_types::DriftLevel::Warn => StatusItemSeverity::Warning,
        adapteros_api_types::DriftLevel::Critical => StatusItemSeverity::Error,
    }
}
