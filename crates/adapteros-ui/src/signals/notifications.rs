//! Notification and toast state management.

use leptos::prelude::*;
use uuid::Uuid;

const MAX_TOASTS: usize = 5;
#[cfg(target_arch = "wasm32")]
const DEFAULT_TOAST_DURATION_MS: u32 = 5000;

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

/// Notification item for long-lived views (not currently surfaced in UI).
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub message: String,
    pub severity: NotificationSeverity,
    pub read: bool,
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

    pub fn info(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Info, title, message);
    }

    pub fn success(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Success, title, message);
    }

    pub fn warning(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Warning, title, message);
    }

    pub fn error(&self, title: &str, message: &str) {
        self.push_simple(ToastSeverity::Error, title, message);
    }

    fn push_simple(&self, severity: ToastSeverity, title: &str, message: &str) {
        self.push_toast(
            Toast {
                id: Uuid::new_v4().to_string(),
                title: title.to_string(),
                message: message.to_string(),
                details: None,
                severity,
                dismissible: true,
            },
            None,
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
