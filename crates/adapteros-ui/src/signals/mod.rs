//! Reactive state management
//!
//! Global signals and state for the application.

pub mod auth;
pub mod chat;
pub mod settings;

pub use auth::{use_auth, provide_auth_context, AuthState, AuthAction, AuthContext};
pub use chat::{
    use_chat, provide_chat_context, ChatState, ChatAction, ChatContext,
    ChatMessage, ChatTarget, ContextToggles, ContextToggle, DockState, PageContext,
};
pub use settings::{
    use_settings, provide_settings_context, update_setting,
    UserSettings, SettingsContext, Theme, DefaultPage,
};
