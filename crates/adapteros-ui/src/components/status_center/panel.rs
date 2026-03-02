//! Status Center Panel
//!
//! Main panel component with backdrop, sliding animation, and status sections.

use super::hooks::{use_escape_key, use_status_data, StatusLoadingState};
use super::items::{StatusItem, StatusItemSeverity};
use super::sections::{StatusDivider, StatusSection, StatusSectionBadgeVariant};
use crate::api::report_error_with_toast;
use crate::components::glass_toggle::GlassThemeToggle;
use crate::components::{IconX, Spinner};
use crate::signals::{
    use_auth, use_notification_context, use_notifications, use_refetch, Notification,
    NotificationSeverity,
};
use crate::utils::status_display_label;
use adapteros_api_types::{
    InferenceBlocker, InferenceReadyState, ServiceHealthStatus,
    StatusIndicator as ApiStatusIndicator,
};
use leptos::prelude::*;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::JsCast;

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
        let _ = escape_count.try_get();
        if open.get_untracked() {
            open.set(false);
        }
    });

    // Fetch status data when panel opens
    let (status_state, refetch) = use_status_data();

    // Refs for focus trap
    let panel_ref = NodeRef::<leptos::html::Div>::new();
    let sentinel_start_ref = NodeRef::<leptos::html::Div>::new();
    let sentinel_end_ref = NodeRef::<leptos::html::Div>::new();

    // Focus restoration: store the element that had focus when panel opened
    let trigger_element: Rc<RefCell<Option<web_sys::Element>>> = Rc::new(RefCell::new(None));
    let trigger_element_for_effect = Rc::clone(&trigger_element);

    // When panel opens, capture trigger and focus the panel; on close, restore focus
    Effect::new(move || {
        let is_open = open.try_get().unwrap_or(false);
        if is_open {
            // Capture the currently focused element for later restoration
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                *trigger_element_for_effect.borrow_mut() = doc.active_element();
            }
            // Focus the panel container after DOM settles
            let panel = panel_ref;
            set_timeout_simple(
                move || {
                    if let Some(el) = panel.get() {
                        let _ = el.focus();
                    }
                },
                50,
            );
        } else {
            // Restore focus to the element that opened the panel
            if let Some(el) = trigger_element_for_effect.borrow_mut().take() {
                if let Some(html_el) = el.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = html_el.focus();
                }
            }
        }
    });

    // Refetch when panel opens
    let refetch_clone = refetch.clone();
    Effect::new(move || {
        if open.try_get().unwrap_or(false) {
            refetch_clone();
        }
    });

    let close = move |_| open.set(false);
    let refetch_for_button = refetch.clone();

    // Focus trap: redirect focus from sentinels back into the panel
    let on_sentinel_start_focus = move |_| {
        // When start sentinel gets focus (shift-tab from first element), wrap to end
        if let Some(el) = panel_ref.get() {
            if let Ok(focusables) = el.query_selector_all(
                "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex=\"-1\"])",
            ) {
                // Find the last real focusable (skip sentinels)
                for i in (0..focusables.length()).rev() {
                    if let Some(node) = focusables.item(i) {
                        if let Some(html_el) = node.dyn_ref::<web_sys::HtmlElement>() {
                            if html_el.class_list().contains("sr-only") {
                                continue;
                            }
                            let _ = html_el.focus();
                            return;
                        }
                    }
                }
            }
        }
    };

    let on_sentinel_end_focus = move |_| {
        // When end sentinel gets focus (tab from last element), wrap to start
        if let Some(el) = panel_ref.get() {
            if let Ok(focusables) = el.query_selector_all(
                "button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex=\"-1\"])",
            ) {
                // Find the first real focusable (skip sentinels)
                for i in 0..focusables.length() {
                    if let Some(node) = focusables.item(i) {
                        if let Some(html_el) = node.dyn_ref::<web_sys::HtmlElement>() {
                            if html_el.class_list().contains("sr-only") {
                                continue;
                            }
                            let _ = html_el.focus();
                            return;
                        }
                    }
                }
            }
        }
    };

    // Escape keydown on panel container (defense in depth alongside global hook)
    let on_panel_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            ev.prevent_default();
            open.set(false);
        }
    };

    view! {
        // Backdrop
        <div
            class=move || {
                if open.try_get().unwrap_or(false) {
                    "status-center-backdrop status-center-backdrop-visible"
                } else {
                    "status-center-backdrop status-center-backdrop-hidden"
                }
            }
            hidden=move || !open.try_get().unwrap_or(false)
            aria-hidden=move || (!open.try_get().unwrap_or(false)).to_string()
            on:click=close
        />

        // Panel
        <div
            node_ref=panel_ref
            class=move || {
                if open.try_get().unwrap_or(false) {
                    "status-center-panel status-center-panel-open"
                } else {
                    "status-center-panel status-center-panel-closed"
                }
            }
            hidden=move || !open.try_get().unwrap_or(false)
            aria-hidden=move || (!open.try_get().unwrap_or(false)).to_string()
            role="dialog"
            aria-modal="true"
            aria-labelledby="status-center-title"
            tabindex="-1"
            on:keydown=on_panel_keydown
        >
            // Focus trap: start sentinel
            <div
                node_ref=sentinel_start_ref
                class="sr-only"
                tabindex="0"
                on:focus=on_sentinel_start_focus
                aria-hidden="true"
            />

            // Header
            <div class="status-center-header">
                <h2 id="status-center-title" class="status-center-title">
                    "Status Center"
                </h2>
                <div class="status-center-header-actions">
                    // Refresh button
                    <button
                        class="btn btn-ghost status-center-refresh-btn"
                        on:click=move |_| refetch_for_button()
                        title="Refresh status"
                        aria-label="Refresh status"
                        disabled=move || status_state.try_get().map(|s| s.is_loading()).unwrap_or(false)
                    >
                        <svg
                            class=move || {
                                if status_state.try_get().map(|s| s.is_loading()).unwrap_or(false) {
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
                        class="btn btn-ghost status-center-close-btn"
                        on:click=close
                        title="Close (Escape)"
                        aria-label="Close"
                    >
                        <IconX class="status-center-close-icon"/>
                    </button>
                </div>
            </div>

            // Content
            <div class="status-center-content">
                // Session & Preferences
                <SessionPreferencesSection />
                <StatusDivider />

                // Notifications section (always visible, independent of status loading)
                <NotificationsSection />
                <StatusDivider />

                // Wrap dynamic status sections with aria-live for screen reader updates
                <div aria-live="polite">
                    {move || {
                        match status_state.try_get().unwrap_or(StatusLoadingState::Loading) {
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
                            StatusLoadingState::LoadedWithError(ref data, ref e) => {
                                let status = data.status.clone();
                                let state = data.state.clone();
                                let error_msg = e.to_string();

                                view! {
                                    <div class="status-center-warning">
                                        <p class="status-center-error-text">"Using cached status. Refreshing..."</p>
                                        <p class="status-center-error-detail">{error_msg}</p>
                                        <div class="status-center-divider status-center-divider--spaced"></div>
                                    </div>
                                    <StatusCenterSections status=status state=state />
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

            // Focus trap: end sentinel
            <div
                node_ref=sentinel_end_ref
                class="sr-only"
                tabindex="0"
                on:focus=on_sentinel_end_focus
                aria-hidden="true"
            />
        </div>
    }
}

/// Internal component to render status sections: Readiness, Inference, Services.
#[component]
fn StatusCenterSections(
    status: adapteros_api_types::SystemStatusResponse,
    state: adapteros_api_types::SystemStateResponse,
) -> impl IntoView {
    let readiness_checks = status.readiness.checks.clone();
    let kernel_for_model = status.kernel.clone();
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
                let model_status = m.status.clone();
                let model_status_token = model_status.trim().to_ascii_lowercase().replace('-', "_");
                let model_status_label = status_display_label(&model_status);
                view! {
                    <StatusItem
                        label="Active Model"
                        value=m.model_id.clone().unwrap_or_else(|| "None".to_string())
                        severity=if model_status_token == "ready" || model_status_token == "loaded" {
                            StatusItemSeverity::Success
                        } else {
                            StatusItemSeverity::Warning
                        }
                        detail=format!("Status: {} ({})", model_status_label, model_status)
                    />
                }
            })}
        </StatusSection>

        <StatusDivider />

        // Services Section (simple list with status indicator)
        <StatusSection
            title="Services"
            badge_count=healthy_services
            badge_variant=services_badge_variant
            initially_expanded=false
        >
            {if !state.node.services.is_empty() {
                view! {
                    <div class="status-services">
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
                view! {
                    <StatusItem
                        label="Services"
                        value="No services registered"
                        severity=StatusItemSeverity::Warning
                    />
                }.into_any()
            }}
        </StatusSection>
    }
}

/// Notifications section showing error/warning history
#[component]
fn NotificationsSection() -> impl IntoView {
    let (state, action) = use_notification_context();
    let action_for_clear = action.clone();
    let action_for_mark = action.clone();

    let unread_count = move || {
        state
            .get_untracked()
            .notifications
            .iter()
            .filter(|n| !n.read)
            .count()
    };

    // Derive announcement text for notification count changes
    let notification_announcement = move || {
        let count = unread_count();
        if count == 0 {
            "No unread notifications".to_string()
        } else if count == 1 {
            "1 unread notification".to_string()
        } else {
            format!("{} unread notifications", count)
        }
    };

    let has_notifications = move || !state.get_untracked().notifications.is_empty();

    // Determine badge variant from most severe unread notification
    let badge_variant = move || {
        let notifications = state.get_untracked().notifications.clone();
        let worst = notifications
            .iter()
            .filter(|n| !n.read)
            .map(|n| &n.severity)
            .fold(None, |acc: Option<&NotificationSeverity>, s| {
                Some(match (acc, s) {
                    (Some(NotificationSeverity::Error), _) | (_, NotificationSeverity::Error) => {
                        &NotificationSeverity::Error
                    }
                    (Some(NotificationSeverity::Warning), _)
                    | (_, NotificationSeverity::Warning) => &NotificationSeverity::Warning,
                    _ => s,
                })
            });
        match worst {
            Some(NotificationSeverity::Error) => StatusSectionBadgeVariant::Error,
            Some(NotificationSeverity::Warning) => StatusSectionBadgeVariant::Warning,
            _ => StatusSectionBadgeVariant::Info,
        }
    };

    view! {
        <StatusSection
            title="Notifications"
            badge_count=unread_count()
            badge_variant=badge_variant()
            initially_expanded=true
        >
            // Header actions (mark all read + clear)
            {move || {
                let mark_action = action_for_mark.clone();
                let clear_action = action_for_clear.clone();
                has_notifications().then(|| view! {
                    <div class="status-notifications-actions" style="margin-bottom: 0.5rem;">
                        <button
                            class="btn btn-ghost status-notifications-action-btn"
                            on:click=move |_| mark_action.mark_all_read()
                            title="Mark all read"
                            aria-label="Mark all read"
                        >
                            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
                            </svg>
                        </button>
                        <button
                            class="btn btn-ghost status-notifications-action-btn"
                            on:click=move |_| clear_action.clear_notifications()
                            title="Clear all"
                            aria-label="Clear all"
                        >
                            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                            </svg>
                        </button>
                    </div>
                })
            }}

            {move || {
                let notifications = state
                    .try_get()
                    .map(|s| s.notifications.clone())
                    .unwrap_or_default();
                if notifications.is_empty() {
                    view! {
                        <div class="status-notifications-empty">
                            <svg class="status-notifications-empty-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                            </svg>
                            <span style="font-size: 0.75rem;">"No notifications"</span>
                        </div>
                    }.into_any()
                } else {
                    let items: Vec<_> = notifications.into_iter().rev().collect();
                    view! {
                        <div>
                            {items.into_iter().map(|n| {
                                view! { <NotificationItem notification=n /> }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }
            }}

            // Hidden announcement for notification count changes
            <span class="sr-only" aria-live="polite">
                {notification_announcement}
            </span>
        </StatusSection>
    }
}

/// Individual notification item for the Status Center
#[component]
fn NotificationItem(notification: Notification) -> impl IntoView {
    let severity_class = match notification.severity {
        NotificationSeverity::Error => "status-notification-item-error",
        NotificationSeverity::Warning => "status-notification-item-warning",
        NotificationSeverity::Info => "status-notification-item-info",
        NotificationSeverity::Success => "status-notification-item-success",
    };

    let severity_icon = match notification.severity {
        NotificationSeverity::Error => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
        NotificationSeverity::Warning => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
        NotificationSeverity::Info => "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
        NotificationSeverity::Success => "M5 13l4 4L19 7",
    };

    let time_str = format_notification_timestamp(notification.timestamp);
    let unread_class = if notification.read {
        ""
    } else {
        "status-notification-item-unread"
    };
    let details = notification.details.clone();

    view! {
        <div class=format!("status-notification-item {} {}", severity_class, unread_class)>
            <div class="status-notification-item-icon">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d=severity_icon/>
                </svg>
            </div>
            <div class="status-notification-item-content">
                <div class="status-notification-item-header">
                    <span class="status-notification-item-title">{notification.title}</span>
                    <span class="status-notification-item-time">{time_str}</span>
                </div>
                <p class="status-notification-item-message">{notification.message}</p>
                {details.map(|d| view! {
                    <details class="status-notification-item-details">
                        <summary>"Details"</summary>
                        <pre class="status-notification-item-details-content">{d}</pre>
                    </details>
                })}
            </div>
        </div>
    }
}

/// Format timestamp to relative time string
fn format_notification_timestamp(timestamp: f64) -> String {
    let now = js_sys::Date::now();
    let diff_ms = now - timestamp;
    let diff_secs = (diff_ms / 1000.0) as u64;

    if diff_secs < 60 {
        "just now".to_string()
    } else if diff_secs < 3600 {
        let mins = diff_secs / 60;
        format!("{}m ago", mins)
    } else if diff_secs < 86400 {
        let hours = diff_secs / 3600;
        format!("{}h ago", hours)
    } else {
        let days = diff_secs / 86400;
        format!("{}d ago", days)
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

#[cfg(target_arch = "wasm32")]
fn set_timeout_simple<F: FnOnce() + 'static>(f: F, ms: i32) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let closure = Closure::once_into_js(f);
    let Some(window) = web_sys::window() else {
        tracing::error!("set_timeout_simple: no window object available");
        return;
    };
    let _ =
        window.set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), ms);
}

#[cfg(not(target_arch = "wasm32"))]
fn set_timeout_simple<F: FnOnce() + 'static>(_f: F, _ms: i32) {}

/// Session & Preferences section for the Status Center
#[component]
fn SessionPreferencesSection() -> impl IntoView {
    let (auth_state, auth_action) = use_auth();
    let auth_action_signal = StoredValue::new(auth_action);

    // Tenant Picker logic
    let notifications = use_notifications();
    let refetch = use_refetch();
    let auth_action_stored = StoredValue::new(auth_action_signal.get_value());
    let notifications_stored = StoredValue::new(notifications);
    let refetch_stored = StoredValue::new(refetch);
    let (switching, set_switching) = signal(false);

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
        <StatusSection
            title="Session & Preferences"
            badge_count=0
            badge_variant=StatusSectionBadgeVariant::Info
            initially_expanded=true
        >
            <div class="p-2 flex flex-col gap-4">
                // Profile
                <div class="flex items-center justify-between">
                    <div class="flex items-center gap-3">
                        {move || {
                            if let Some(user) = auth_state.get().user() {
                                let identity = if user.display_name.is_empty() {
                                    user.email.clone()
                                } else {
                                    user.display_name.clone()
                                };
                                let initials = identity.chars().next().unwrap_or('U').to_uppercase().to_string();
                                view! {
                                    <div class="w-8 h-8 rounded-full bg-primary/20 text-primary flex items-center justify-center text-sm font-medium">
                                        {initials}
                                    </div>
                                    <div class="flex flex-col">
                                        <span class="text-sm font-medium">{user.email.clone()}</span>
                                        <span class="text-xs text-muted-foreground">{format!("Logged in as {}", identity)}</span>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="w-8 h-8 rounded-full bg-muted flex items-center justify-center">
                                        <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"/>
                                        </svg>
                                    </div>
                                    <span class="text-sm font-medium text-muted-foreground">"Not logged in"</span>
                                }.into_any()
                            }
                        }}
                    </div>
                    {move || {
                        if auth_state.get().user().is_some() {
                            view! {
                                <button
                                    class="text-xs text-destructive hover:underline px-2 py-1"
                                    on:click=move |_| {
                                        auth_action_signal.try_with_value(|action| {
                                            let action = action.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                action.logout().await;
                                            });
                                        });
                                    }
                                >
                                    "Sign out"
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <a href="/login" class="text-xs text-primary hover:underline px-2 py-1">"Log in"</a>
                            }.into_any()
                        }
                    }}
                </div>

                // Tenant Picker
                {move || {
                    let info = tenants.get();
                    match info {
                        Some((current, admin_tenants)) if admin_tenants.len() > 1 => {
                            let options = admin_tenants.iter().map(|t| {
                                let selected = t == &current;
                                let val = t.clone();
                                let label = t.clone();
                                view! { <option value=val selected=selected>{label}</option> }
                            }).collect::<Vec<_>>();

                            Some(view! {
                                <div class="flex items-center justify-between py-1 border-t border-border/50 mt-1">
                                    <span class="text-sm text-foreground">"Workspace"</span>
                                    <select
                                        class="tenant-picker text-xs bg-muted border border-border/50 rounded px-2 py-1 text-foreground cursor-pointer hover:bg-muted/80 transition-colors focus:outline-none"
                                        on:change=on_change
                                        disabled=move || switching.get()
                                        aria-label="Switch workspace tenant"
                                    >
                                        {options}
                                    </select>
                                </div>
                            })
                        }
                        _ => None,
                    }
                }}

                // Display Theme
                <div class="flex items-center justify-between py-1 border-t border-border/50 mt-1">
                    <span class="text-sm text-foreground">"Theme / Appearance"</span>
                    <GlassThemeToggle />
                </div>
            </div>
        </StatusSection>
    }
}
