//! Reactive state management
//!
//! Global signals and state for the application.

pub mod auth;
pub mod chat;
pub mod notifications;
pub mod search;
pub mod settings;

pub use auth::{provide_auth_context, use_auth, AuthAction, AuthContext, AuthState};
pub use chat::{
    provide_chat_context, use_chat, ChatAction, ChatContext, ChatMessage, ChatState, ChatTarget,
    ContextToggle, ContextToggles, DockState, PageContext,
};
pub use notifications::{
    provide_notifications_context, try_use_notifications, use_notification_context,
    use_notification_state, use_notifications, Notification, NotificationAction,
    NotificationContext, NotificationSeverity, NotificationState, Toast, ToastSeverity,
};
pub use search::{provide_search_context, use_search, SearchContext};
pub use settings::{
    provide_settings_context, update_setting, use_settings, DefaultPage, SettingsContext, Theme,
    UserSettings,
};
