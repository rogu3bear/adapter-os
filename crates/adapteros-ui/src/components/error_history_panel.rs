//! Error History Panel
//!
//! A sliding panel that displays the history of error and warning notifications.
//! Accessible via error badge in topbar or Ctrl+Shift+E shortcut.

use crate::signals::{use_notification_context, Notification, NotificationSeverity};
use leptos::prelude::*;

/// Error History Panel component
///
/// A sliding panel that displays all persisted error and warning notifications.
#[component]
pub fn ErrorHistoryPanel(
    /// Whether the panel is open
    open: RwSignal<bool>,
) -> impl IntoView {
    let (state, action) = use_notification_context();
    let action_for_clear = action.clone();
    let action_for_mark = action.clone();

    let close = move |_| open.set(false);

    view! {
        // Backdrop
        <div
            class=move || {
                if open.get() {
                    "error-history-backdrop error-history-backdrop-visible"
                } else {
                    "error-history-backdrop error-history-backdrop-hidden"
                }
            }
            on:click=close
        />

        // Panel
        <div
            class=move || {
                if open.get() {
                    "error-history-panel error-history-panel-open"
                } else {
                    "error-history-panel error-history-panel-closed"
                }
            }
            role="dialog"
            aria-modal="true"
            aria-labelledby="error-history-title"
        >
            // Header
            <div class="error-history-header">
                <h2 id="error-history-title" class="error-history-title">
                    "Error History"
                </h2>
                <div class="error-history-header-actions">
                    // Clear all button
                    <button
                        class="error-history-clear-btn"
                        on:click=move |_| action_for_clear.clear_notifications()
                        title="Clear all"
                    >
                        <svg class="error-history-clear-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                        </svg>
                    </button>

                    // Mark all read button
                    <button
                        class="error-history-mark-btn"
                        on:click=move |_| action_for_mark.mark_all_read()
                        title="Mark all read"
                    >
                        <svg class="error-history-mark-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
                        </svg>
                    </button>

                    // Close button
                    <button
                        class="error-history-close-btn"
                        on:click=close
                        title="Close (Escape)"
                    >
                        <svg class="error-history-close-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                        </svg>
                    </button>
                </div>
            </div>

            // Content
            <div class="error-history-content">
                {move || {
                    let notifications = state.get().notifications.clone();
                    if notifications.is_empty() {
                        view! {
                            <div class="error-history-empty">
                                <svg class="error-history-empty-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                </svg>
                                <p class="error-history-empty-text">"No errors or warnings"</p>
                            </div>
                        }.into_any()
                    } else {
                        // Reverse to show newest first
                        let items: Vec<_> = notifications.into_iter().rev().collect();
                        view! {
                            <div class="error-history-list">
                                {items.into_iter().map(|n| {
                                    view! { <ErrorHistoryItem notification=n /> }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Footer with shortcut hint
            <div class="error-history-footer">
                <span class="error-history-shortcut-hint">
                    <kbd class="error-history-kbd">"Ctrl"</kbd>
                    " + "
                    <kbd class="error-history-kbd">"Shift"</kbd>
                    " + "
                    <kbd class="error-history-kbd">"E"</kbd>
                    " to toggle"
                </span>
            </div>
        </div>
    }
}

/// Individual error history item
#[component]
fn ErrorHistoryItem(notification: Notification) -> impl IntoView {
    let severity_class = match notification.severity {
        NotificationSeverity::Error => "error-history-item-error",
        NotificationSeverity::Warning => "error-history-item-warning",
        NotificationSeverity::Info => "error-history-item-info",
        NotificationSeverity::Success => "error-history-item-success",
    };

    let severity_icon = match notification.severity {
        NotificationSeverity::Error => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
        NotificationSeverity::Warning => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
        NotificationSeverity::Info => "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
        NotificationSeverity::Success => "M5 13l4 4L19 7",
    };

    let time_str = format_timestamp(notification.timestamp);
    let unread_class = if notification.read {
        ""
    } else {
        "error-history-item-unread"
    };
    let details = notification.details.clone();

    view! {
        <div class=format!("error-history-item {} {}", severity_class, unread_class)>
            <div class="error-history-item-icon">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d=severity_icon/>
                </svg>
            </div>
            <div class="error-history-item-content">
                <div class="error-history-item-header">
                    <span class="error-history-item-title">{notification.title}</span>
                    <span class="error-history-item-time">{time_str}</span>
                </div>
                <p class="error-history-item-message">{notification.message}</p>
                {details.map(|d| view! {
                    <details class="error-history-item-details">
                        <summary>"Details"</summary>
                        <pre class="error-history-item-details-content">{d}</pre>
                    </details>
                })}
            </div>
        </div>
    }
}

/// Format timestamp to relative time string
fn format_timestamp(timestamp: f64) -> String {
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

/// Hook for detecting Ctrl+Shift+E shortcut
pub fn use_error_history_shortcut() -> ReadSignal<u32> {
    use crate::components::status_center::use_keyboard_shortcut;
    use_keyboard_shortcut("e", true, true)
}

/// Error History provider component
///
/// This component should be placed at the app root level.
/// It listens for Ctrl+Shift+E keyboard shortcut to toggle the panel.
#[component]
pub fn ErrorHistory() -> impl IntoView {
    let open = RwSignal::new(false);

    // Listen for keyboard shortcut
    let shortcut_count = use_error_history_shortcut();

    // Toggle on shortcut
    Effect::new(move || {
        let Some(count) = shortcut_count.try_get() else {
            return;
        };
        if count > 0 {
            let _ = open.try_update(|o| *o = !*o);
        }
    });

    // Provide context for external control
    provide_context(ErrorHistoryContext { open });

    view! {
        <ErrorHistoryPanel open=open />
    }
}

/// Context for controlling Error History from child components
#[derive(Clone, Copy)]
pub struct ErrorHistoryContext {
    /// Signal to control panel open state
    pub open: RwSignal<bool>,
}

impl ErrorHistoryContext {
    /// Open the error history panel
    pub fn open(&self) {
        self.open.set(true);
    }

    /// Close the error history panel
    pub fn close(&self) {
        self.open.set(false);
    }

    /// Toggle the error history panel
    pub fn toggle(&self) {
        self.open.update(|o| *o = !*o);
    }
}

/// Hook to access Error History context
pub fn use_error_history() -> Option<ErrorHistoryContext> {
    use_context::<ErrorHistoryContext>()
}
