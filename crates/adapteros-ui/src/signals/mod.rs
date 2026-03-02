//! Reactive state management
//!
//! Global signals and state for the application.

pub mod auth;
pub mod chat;
pub mod notifications;
pub mod page_context;
pub mod progress_rail;
pub mod refetch;
pub mod search;
pub mod settings;
pub mod ui_profile;

pub use auth::{provide_auth_context, use_auth, AuthAction, AuthContext, AuthError, AuthState};
pub use chat::{
    provide_chat_context, use_chat, AdapterStateInfo, ChatAction, ChatContext, ChatMessage,
    ChatSessionMeta, ChatSessionsManager, ChatState, ChatTarget, ContextToggle, ContextToggles,
    DockState, MessageStatus, PageContext, PendingPhase, StoredChatSession, StoredMessage,
    StreamNotice, StreamNoticeTone, SuggestedAdapter,
};
pub use notifications::{
    provide_notifications_context, try_use_notifications, use_notification_context,
    use_notification_state, use_notifications, Notification, NotificationAction,
    NotificationContext, NotificationSeverity, NotificationState, Toast, ToastSeverity,
};
pub use page_context::{
    provide_route_context, try_use_route_context, use_route_context, RouteContext, SelectedEntity,
};
pub use progress_rail::{
    provide_progress_rail_context, use_progress_rail, use_progress_rail_writer, ProgressRailState,
};
pub use refetch::{
    provide_refetch_context, use_refetch, use_refetch_context, use_refetch_signal,
    use_refetch_state, RefetchAction, RefetchContext, RefetchState, RefetchTopic,
};
pub use search::{provide_search_context, use_search, SearchContext};
pub use settings::{
    perf_logging_enabled, provide_settings_context, update_setting, use_settings, DefaultPage,
    Density, SettingsContext, Theme, TrustDisplay, UserSettings,
};
pub use ui_profile::{
    provide_ui_profile_context, use_ui_profile, use_ui_profile_state, UiProfileState,
};
