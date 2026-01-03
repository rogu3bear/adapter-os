//! Reactive state management
//!
//! Global signals and state for the application.

pub mod auth;
pub mod chat;
pub mod settings;

pub use auth::{provide_auth_context, use_auth, AuthAction, AuthContext, AuthState};
pub use chat::{
    provide_chat_context, use_chat, ChatAction, ChatContext, ChatMessage, ChatState, ChatTarget,
    ContextToggle, ContextToggles, DockState, PageContext,
};
pub use settings::{
    provide_settings_context, update_setting, use_settings, DefaultPage, SettingsContext, Theme,
    UserSettings,
};
