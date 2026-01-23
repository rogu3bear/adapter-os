//! Reactive state management
//!
//! Global signals and state for the application.

pub mod auth;
pub mod chat;
pub mod modal;
pub mod notifications;
pub mod refetch;
pub mod search;
pub mod settings;

pub use auth::{provide_auth_context, use_auth, AuthAction, AuthContext, AuthState};
pub use chat::{
    provide_chat_context, use_chat, AdapterStateInfo, ChatAction, ChatContext, ChatMessage,
    ChatSessionMeta, ChatSessionsManager, ChatState, ChatTarget, ContextToggle, ContextToggles,
    DockState, PageContext, StoredChatSession, StoredMessage, SuggestedAdapter,
};
pub use modal::{
    provide_modal_context, use_is_modal_open, use_modal, use_modal_context, use_modal_state,
    ConfirmConfig, ModalAction, ModalContext, ModalId, ModalState,
};
pub use notifications::{
    provide_notifications_context, try_use_notifications, use_notification_context,
    use_notification_state, use_notifications, Notification, NotificationAction,
    NotificationContext, NotificationSeverity, NotificationState, Toast, ToastSeverity,
};
pub use refetch::{
    provide_refetch_context, use_refetch, use_refetch_context, use_refetch_signal,
    use_refetch_state, RefetchAction, RefetchContext, RefetchState, RefetchTopic,
};
pub use search::{provide_search_context, use_search, SearchContext};
pub use settings::{
    provide_settings_context, update_setting, use_settings, DefaultPage, SettingsContext, Theme,
    UserSettings,
};
