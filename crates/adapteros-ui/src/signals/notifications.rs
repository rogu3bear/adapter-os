//! Notification state management
//!
//! Provides a reactive notification system for displaying toasts/alerts.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use std::collections::VecDeque;
use uuid::Uuid;

/// Default notification duration in milliseconds
#[allow(dead_code)]
const DEFAULT_DURATION_MS: u32 = 5_000;

/// Maximum number of notifications to display at once
const MAX_VISIBLE_NOTIFICATIONS: usize = 5;

/// Deduplication window duration in seconds
const DEDUP_WINDOW_SECS: i64 = 60;

/// Toast severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToastSeverity {
    /// Informational message
    Info,
    /// Success message
    Success,
    /// Warning message
    Warning,
    /// Error message
    Error,
}

impl ToastSeverity {
    /// Get CSS class for this severity
    pub fn class(&self) -> &'static str {
        match self {
            Self::Info => "toast--info",
            Self::Success => "toast--success",
            Self::Warning => "toast--warning",
            Self::Error => "toast--error",
        }
    }

    /// Get the display name for the severity
    pub fn display(&self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }

    /// Get the icon character for the severity
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info => "i",
            Self::Success => "check",
            Self::Warning => "!",
            Self::Error => "x",
        }
    }

    /// Get icon path for this severity
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Info => "M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
            Self::Success => "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z",
            Self::Warning => "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
            Self::Error => "M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z",
        }
    }

    /// Get default auto-dismiss time based on severity
    pub fn default_dismiss_ms(&self) -> u32 {
        match self {
            Self::Info => 5000,
            Self::Success => 4000,
            Self::Warning => 6000,
            Self::Error => 0, // Errors require manual dismissal
        }
    }
}

/// Backward compatibility alias
pub type NotificationSeverity = ToastSeverity;

/// A single toast notification
#[derive(Debug, Clone)]
pub struct Toast {
    /// Unique identifier
    pub id: String,
    /// Severity level
    pub severity: ToastSeverity,
    /// Toast title
    pub title: String,
    /// Toast message
    pub message: String,
    /// Optional details (expandable)
    pub details: Option<String>,
    /// Timestamp when created
    pub timestamp: DateTime<Utc>,
    /// Auto-dismiss time in milliseconds (0 = no auto-dismiss)
    pub auto_dismiss_ms: u32,
    /// Whether the toast can be manually dismissed
    pub dismissible: bool,
}

impl Toast {
    /// Create a new toast with default settings
    pub fn new(
        severity: ToastSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            severity,
            title: title.into(),
            message: message.into(),
            details: None,
            timestamp: Utc::now(),
            auto_dismiss_ms: severity.default_dismiss_ms(),
            dismissible: true,
        }
    }

    /// Create a toast with details
    pub fn with_details(
        severity: ToastSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            severity,
            title: title.into(),
            message: message.into(),
            details: Some(details.into()),
            timestamp: Utc::now(),
            auto_dismiss_ms: severity.default_dismiss_ms(),
            dismissible: true,
        }
    }

    /// Generate a content hash for deduplication
    pub fn content_hash(&self) -> String {
        format!("{:?}:{}:{}", self.severity, self.title, self.message)
    }
}

/// Backward compatibility alias
pub type Notification = Toast;

/// Notification state
#[derive(Debug, Clone, Default)]
pub struct NotificationState {
    /// Active toasts
    pub toasts: Vec<Toast>,
    /// Recent toast hashes for deduplication (hash, timestamp)
    dedup_window: VecDeque<(String, DateTime<Utc>)>,
}

impl NotificationState {
    /// Create a new notification state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a toast with this content was shown recently
    fn is_duplicate(&mut self, hash: &str) -> bool {
        let now = Utc::now();

        // Remove expired entries from the dedup window
        while let Some((_, timestamp)) = self.dedup_window.front() {
            if now.signed_duration_since(*timestamp).num_seconds() > DEDUP_WINDOW_SECS {
                self.dedup_window.pop_front();
            } else {
                break;
            }
        }

        // Check if hash exists in window
        self.dedup_window.iter().any(|(h, _)| h == hash)
    }

    /// Record a toast hash in the dedup window
    fn record_hash(&mut self, hash: String) {
        self.dedup_window.push_back((hash, Utc::now()));
    }

    /// Add a toast, respecting deduplication
    pub fn add_toast(&mut self, toast: Toast) -> Option<String> {
        let hash = toast.content_hash();

        if self.is_duplicate(&hash) {
            return None;
        }

        self.record_hash(hash);
        let id = toast.id.clone();

        // Remove oldest toasts if we exceed max visible
        while self.toasts.len() >= MAX_VISIBLE_NOTIFICATIONS {
            self.toasts.remove(0);
        }

        self.toasts.push(toast);
        Some(id)
    }

    /// Remove a toast by ID
    pub fn remove(&mut self, id: &str) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Clear all toasts
    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    /// Get visible notifications (backward compatibility)
    pub fn notifications(&self) -> &[Toast] {
        &self.toasts
    }

    /// Get visible toasts
    pub fn visible(&self) -> impl Iterator<Item = &Toast> {
        self.toasts.iter()
    }
}

/// Notification actions
#[derive(Clone)]
pub struct NotificationAction {
    state: RwSignal<NotificationState>,
}

impl NotificationAction {
    /// Create new notification action
    pub fn new(state: RwSignal<NotificationState>) -> Self {
        Self { state }
    }

    /// Show a toast notification with title and message
    pub fn show(
        &self,
        severity: ToastSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Option<String> {
        let toast = Toast::new(severity, title, message);
        let auto_dismiss_ms = toast.auto_dismiss_ms;

        let id = self.state.try_update(|s| s.add_toast(toast))??;

        // Schedule auto-dismiss if enabled
        if auto_dismiss_ms > 0 {
            self.schedule_dismiss(id.clone(), auto_dismiss_ms);
        }

        Some(id)
    }

    /// Show a toast with details
    pub fn show_with_details(
        &self,
        severity: ToastSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Option<String> {
        let toast = Toast::with_details(severity, title, message, details);
        let auto_dismiss_ms = toast.auto_dismiss_ms;

        let id = self.state.try_update(|s| s.add_toast(toast))??;

        // Schedule auto-dismiss if enabled
        if auto_dismiss_ms > 0 {
            self.schedule_dismiss(id.clone(), auto_dismiss_ms);
        }

        Some(id)
    }

    /// Show an info notification
    pub fn info(&self, title: impl Into<String>, message: impl Into<String>) -> Option<String> {
        self.show(ToastSeverity::Info, title, message)
    }

    /// Show a success notification
    pub fn success(&self, title: impl Into<String>, message: impl Into<String>) -> Option<String> {
        self.show(ToastSeverity::Success, title, message)
    }

    /// Show a warning notification
    pub fn warning(&self, title: impl Into<String>, message: impl Into<String>) -> Option<String> {
        self.show(ToastSeverity::Warning, title, message)
    }

    /// Show an error notification
    pub fn error(&self, title: impl Into<String>, message: impl Into<String>) -> Option<String> {
        self.show(ToastSeverity::Error, title, message)
    }

    /// Show an error notification with details
    pub fn error_with_details(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Option<String> {
        self.show_with_details(ToastSeverity::Error, title, message, details)
    }

    /// Dismiss a notification by ID
    pub fn dismiss(&self, id: &str) {
        let id = id.to_string();
        self.state.update(|state| state.remove(&id));
    }

    /// Dismiss all notifications
    pub fn dismiss_all(&self) {
        self.state.update(|state| state.clear());
    }

    /// Clear all notifications (alias for dismiss_all)
    pub fn clear(&self) {
        self.dismiss_all();
    }

    /// Check if a toast should be shown (not duplicate)
    pub fn should_show_toast(&self, severity: ToastSeverity, title: &str, message: &str) -> bool {
        let hash = format!("{:?}:{}:{}", severity, title, message);
        self.state.with_untracked(|s| {
            let mut state = s.clone();
            !state.is_duplicate(&hash)
        })
    }

    /// Schedule auto-dismiss for a toast
    fn schedule_dismiss(&self, id: String, delay_ms: u32) {
        let state = self.state;

        #[cfg(target_arch = "wasm32")]
        {
            gloo_timers::callback::Timeout::new(delay_ms, move || {
                state.update(|s| s.remove(&id));
            })
            .forget();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (id, delay_ms, state);
        }
    }
}

/// Notification context type
pub type NotificationContext = (ReadSignal<NotificationState>, NotificationAction);

/// Provide notification context to the application
pub fn provide_notifications_context() {
    web_sys::console::log_1(&"[NotificationContext] Initializing...".into());
    let state = RwSignal::new(NotificationState::new());
    let action = NotificationAction::new(state);
    provide_context((state.read_only(), action));
    web_sys::console::log_1(&"[NotificationContext] Context provided".into());
}

/// Use notification context (returns action only for convenience)
pub fn use_notifications() -> NotificationAction {
    let (_state, action) = expect_context::<NotificationContext>();
    action
}

/// Use notification state (for rendering)
pub fn use_notification_state() -> ReadSignal<NotificationState> {
    let (state, _action) = expect_context::<NotificationContext>();
    state
}

/// Use full notification context (state and action)
pub fn use_notification_context() -> NotificationContext {
    expect_context::<NotificationContext>()
}

/// Try to use notification context (returns None if not provided)
pub fn try_use_notifications() -> Option<NotificationContext> {
    use_context::<NotificationContext>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_state_push_and_remove() {
        let mut state = NotificationState::new();
        assert!(state.toasts.is_empty());

        let toast = Toast::new(ToastSeverity::Info, "Test", "Test message");
        let id = toast.id.clone();

        let result = state.add_toast(toast);
        assert!(result.is_some());
        assert_eq!(state.toasts.len(), 1);

        state.remove(&id);
        assert!(state.toasts.is_empty());
    }

    #[test]
    fn test_notification_state_max_limit() {
        let mut state = NotificationState::new();

        // Add more than max notifications
        for i in 0..10 {
            // Each toast has unique content to avoid deduplication
            let toast = Toast::new(
                ToastSeverity::Info,
                format!("Title {}", i),
                format!("Message {}", i),
            );
            state.add_toast(toast);
        }

        assert_eq!(state.toasts.len(), MAX_VISIBLE_NOTIFICATIONS);
    }

    #[test]
    fn test_deduplication() {
        let mut state = NotificationState::new();

        // Add first toast
        let toast1 = Toast::new(ToastSeverity::Info, "Duplicate", "Same message");
        let result1 = state.add_toast(toast1);
        assert!(result1.is_some());

        // Try to add duplicate
        let toast2 = Toast::new(ToastSeverity::Info, "Duplicate", "Same message");
        let result2 = state.add_toast(toast2);
        assert!(result2.is_none());

        // Should only have one toast
        assert_eq!(state.toasts.len(), 1);
    }

    #[test]
    fn test_severity_classes() {
        assert_eq!(ToastSeverity::Info.class(), "toast--info");
        assert_eq!(ToastSeverity::Success.class(), "toast--success");
        assert_eq!(ToastSeverity::Warning.class(), "toast--warning");
        assert_eq!(ToastSeverity::Error.class(), "toast--error");
    }

    #[test]
    fn test_toast_with_details() {
        let toast = Toast::with_details(
            ToastSeverity::Error,
            "Error",
            "Something went wrong",
            "Stack trace here...",
        );
        assert!(toast.details.is_some());
        assert_eq!(toast.details.unwrap(), "Stack trace here...");
    }

    #[test]
    fn test_content_hash() {
        let toast1 = Toast::new(ToastSeverity::Info, "Title", "Message");
        let toast2 = Toast::new(ToastSeverity::Info, "Title", "Message");
        let toast3 = Toast::new(ToastSeverity::Error, "Title", "Message");

        assert_eq!(toast1.content_hash(), toast2.content_hash());
        assert_ne!(toast1.content_hash(), toast3.content_hash());
    }
}
