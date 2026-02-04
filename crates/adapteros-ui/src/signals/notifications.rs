//! Notification and toast state management.

use leptos::prelude::*;

const MAX_TOASTS: usize = 5;
#[cfg(target_arch = "wasm32")]
const DEFAULT_TOAST_DURATION_MS: u32 = 8000; // info/success
#[cfg(target_arch = "wasm32")]
const WARNING_TOAST_DURATION_MS: u32 = 15000;
#[cfg(target_arch = "wasm32")]
const ERROR_TOAST_DURATION_MS: u32 = 20000;

fn readable_id(prefix: &str, slug_source: &str) -> String {
    let slug = slugify(slug_source);
    let suffix = random_suffix(6);
    format!("{}.{}.{}", prefix, slug, suffix)
}

fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

fn random_suffix(len: usize) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let idx = (js_sys::Math::random() * 32.0).floor() as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Severity levels for notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }
}

/// Severity levels for toasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastSeverity {
    pub fn class(&self) -> &'static str {
        match self {
            Self::Info => "toast-info",
            Self::Success => "toast-success",
            Self::Warning => "toast-warning",
            Self::Error => "toast-error",
        }
    }

    pub fn icon_class(&self) -> &'static str {
        match self {
            Self::Info => "toast-icon toast-icon-info",
            Self::Success => "toast-icon toast-icon-success",
            Self::Warning => "toast-icon toast-icon-warning",
            Self::Error => "toast-icon toast-icon-error",
        }
    }

    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Info => "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
            Self::Success => "M5 13l4 4L19 7",
            Self::Warning => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
            Self::Error => "M12 9v4m0 4h.01M10.29 3.86l-7.29 12.6A1 1 0 003.86 18h16.28a1 1 0 00.86-1.5l-7.29-12.6a1 1 0 00-1.72 0z",
        }
    }
}

/// Notification item for long-lived views (surfaced in error history panel).
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub details: Option<String>,
    pub severity: NotificationSeverity,
    pub read: bool,
    pub timestamp: f64, // JS timestamp (ms since epoch)
}

const MAX_NOTIFICATION_HISTORY: usize = 50;

/// Action link for actionable toasts.
#[derive(Debug, Clone)]
pub struct ToastAction {
    pub label: String,
    pub href: String,
}

/// Toast data for transient notifications.
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: String,
    pub title: String,
    pub message: String,
    pub details: Option<String>,
    pub severity: ToastSeverity,
    pub dismissible: bool,
    /// Optional action link (label, href)
    pub action: Option<ToastAction>,
}

/// Notification state.
#[derive(Debug, Clone, Default)]
pub struct NotificationState {
    pub toasts: Vec<Toast>,
    pub notifications: Vec<Notification>,
}

/// Notification action helpers.
#[derive(Clone)]
pub struct NotificationAction {
    state: RwSignal<NotificationState>,
}

impl NotificationAction {
    pub fn new(state: RwSignal<NotificationState>) -> Self {
        Self { state }
    }

    pub fn push_toast(&self, toast: Toast, duration_ms: Option<u32>) {
        self.state.update(|state| {
            // Drop duplicate messages (same title+message+severity) arriving back-to-back
            if let Some(last) = state.toasts.last() {
                if last.title == toast.title
                    && last.message == toast.message
                    && last.severity == toast.severity
                {
                    return;
                }
            }
            state.toasts.push(toast);
            if state.toasts.len() > MAX_TOASTS {
                state.toasts.remove(0);
            }
        });

        #[cfg(target_arch = "wasm32")]
        {
            let id = self
                .state
                .get_untracked()
                .toasts
                .last()
                .map(|toast| toast.id.clone());
            let duration = duration_ms.unwrap_or(DEFAULT_TOAST_DURATION_MS);
            let action = self.clone();
            if let Some(id) = id {
                let handle = gloo_timers::callback::Timeout::new(duration, move || {
                    action.dismiss(&id);
                });
                handle.forget();
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        let _ = duration_ms;
    }

    pub fn dismiss(&self, id: &str) {
        self.state.update(|state| {
            state.toasts.retain(|toast| toast.id != id);
        });
    }

    /// Push a notification to persistent history.
    /// Called automatically for errors and warnings.
    fn push_notification(
        &self,
        severity: NotificationSeverity,
        title: &str,
        message: &str,
        details: Option<&str>,
    ) {
        self.state.update(|state| {
            let notification = Notification {
                id: readable_id("notif", "hist"),
                title: title.to_string(),
                message: message.to_string(),
                details: details.map(|d| d.to_string()),
                severity,
                read: false,
                timestamp: js_sys::Date::now(),
            };
            state.notifications.push(notification);
            // Keep history bounded
            if state.notifications.len() > MAX_NOTIFICATION_HISTORY {
                state.notifications.remove(0);
            }
        });
    }

    /// Mark a notification as read.
    pub fn mark_read(&self, id: &str) {
        self.state.update(|state| {
            if let Some(n) = state.notifications.iter_mut().find(|n| n.id == id) {
                n.read = true;
            }
        });
    }

    /// Mark all notifications as read.
    pub fn mark_all_read(&self) {
        self.state.update(|state| {
            for n in &mut state.notifications {
                n.read = true;
            }
        });
    }

    /// Clear all notifications from history.
    pub fn clear_notifications(&self) {
        self.state.update(|state| {
            state.notifications.clear();
        });
    }

    /// Get unread notification count.
    pub fn unread_count(&self) -> usize {
        self.state
            .get_untracked()
            .notifications
            .iter()
            .filter(|n| !n.read)
            .count()
    }

    pub fn info(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Info, title, message);
    }

    pub fn success(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Success, title, message);
    }

    pub fn warning(&self, title: &str, message: &str) {
        self.push_notification(NotificationSeverity::Warning, title, message, None);
        self.push_simple(ToastSeverity::Warning, title, message);
    }

    pub fn error(&self, title: &str, message: &str) {
        self.push_notification(NotificationSeverity::Error, title, message, None);
        self.push_simple(ToastSeverity::Error, title, message);
    }

    /// Show an error toast with expandable details.
    ///
    /// Use this for surfacing errors where users may want to copy diagnostic info
    /// (e.g., API errors, streaming failures, timeout details).
    pub fn error_with_details(&self, title: &str, message: &str, details: &str) {
        self.push_notification(NotificationSeverity::Error, title, message, Some(details));

        #[cfg(target_arch = "wasm32")]
        let duration = ERROR_TOAST_DURATION_MS;
        #[cfg(not(target_arch = "wasm32"))]
        let duration = 20000;

        self.push_toast(
            Toast {
                id: readable_id("notif", "toast"),
                title: title.to_string(),
                message: message.to_string(),
                details: Some(details.to_string()),
                severity: ToastSeverity::Error,
                dismissible: true,
                action: None,
            },
            Some(duration),
        );
    }

    /// Show a warning toast with expandable details.
    ///
    /// Use this for surfacing warnings where users may want to copy diagnostic info
    /// (e.g., in-flight adapter conflicts, rate limiting).
    pub fn warning_with_details(&self, title: &str, message: &str, details: &str) {
        self.push_notification(NotificationSeverity::Warning, title, message, Some(details));

        #[cfg(target_arch = "wasm32")]
        let duration = WARNING_TOAST_DURATION_MS;
        #[cfg(not(target_arch = "wasm32"))]
        let duration = 15000;

        self.push_toast(
            Toast {
                id: readable_id("notif", "toast"),
                title: title.to_string(),
                message: message.to_string(),
                details: Some(details.to_string()),
                severity: ToastSeverity::Warning,
                dismissible: true,
                action: None,
            },
            Some(duration),
        );
    }

    fn push_simple(&self, severity: ToastSeverity, title: &str, message: &str) {
        #[cfg(target_arch = "wasm32")]
        let duration = match severity {
            ToastSeverity::Error => Some(ERROR_TOAST_DURATION_MS),
            ToastSeverity::Warning => Some(WARNING_TOAST_DURATION_MS),
            ToastSeverity::Info | ToastSeverity::Success => None, // uses DEFAULT_TOAST_DURATION_MS
        };
        #[cfg(not(target_arch = "wasm32"))]
        let duration = None;

        self.push_toast(
            Toast {
                id: readable_id("notif", "toast"),
                title: title.to_string(),
                message: message.to_string(),
                details: None,
                severity,
                dismissible: true,
                action: None,
            },
            duration,
        );
    }

    /// Show a success toast with an action link.
    ///
    /// Use for notifications where the user may want to navigate to a related page
    /// (e.g., "Try in Chat" after training completes).
    pub fn success_with_action(
        &self,
        title: &str,
        message: &str,
        action_label: &str,
        action_href: &str,
    ) {
        self.push_toast(
            Toast {
                id: readable_id("notif", "toast"),
                title: title.to_string(),
                message: message.to_string(),
                details: None,
                severity: ToastSeverity::Success,
                dismissible: true,
                action: Some(ToastAction {
                    label: action_label.to_string(),
                    href: action_href.to_string(),
                }),
            },
            Some(10_000), // 10s for actionable toasts
        );
    }
}

/// Notification context type.
pub type NotificationContext = (ReadSignal<NotificationState>, NotificationAction);

/// Provide notifications context.
pub fn provide_notifications_context() {
    let state = RwSignal::new(NotificationState::default());
    let action = NotificationAction::new(state);
    provide_context((state.read_only(), action));
}

/// Use notifications context (panics if missing).
pub fn use_notification_context() -> NotificationContext {
    expect_context::<NotificationContext>()
}

/// Try to use notifications action.
pub fn try_use_notifications() -> Option<NotificationAction> {
    use_context::<NotificationContext>().map(|(_, action)| action)
}

/// Use notifications action (panics if missing).
pub fn use_notifications() -> NotificationAction {
    use_notification_context().1
}

/// Use notification state only.
pub fn use_notification_state() -> ReadSignal<NotificationState> {
    use_notification_context().0
}
