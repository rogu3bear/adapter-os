//! Chat state management
//!
//! Global chat state that persists across page navigation.

use crate::api::{api_base_url, ApiClient, ApiError, InferenceRequest};
use chrono::{DateTime, Utc};
use leptos::prelude::*;
use std::sync::Arc;

/// A single chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Unique message ID
    pub id: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Whether this message is still streaming
    pub is_streaming: bool,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content,
            timestamp: Utc::now(),
            is_streaming: false,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content,
            timestamp: Utc::now(),
            is_streaming: false,
        }
    }

    pub fn assistant_streaming() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: String::new(),
            timestamp: Utc::now(),
            is_streaming: true,
        }
    }
}

/// Target type for chat inference
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ChatTarget {
    /// Use the default model/stack
    #[default]
    Default,
    /// Target a specific model
    Model(String),
    /// Target a specific adapter stack
    Stack(String),
    /// Target a specific policy pack
    PolicyPack(String),
}

impl ChatTarget {
    pub fn display_name(&self) -> String {
        match self {
            Self::Default => "Default".to_string(),
            Self::Model(name) => format!("Model: {}", name),
            Self::Stack(name) => format!("Stack: {}", name),
            Self::PolicyPack(name) => format!("Policy: {}", name),
        }
    }
}

/// Context toggles for additional prompt metadata
#[derive(Debug, Clone, Default)]
pub struct ContextToggles {
    /// Include current page selection (adapter/job/worker)
    pub current_page: bool,
    /// Include recent logs (last 200 lines)
    pub recent_logs: bool,
    /// Include system snapshot (health + worker counts)
    pub system_snapshot: bool,
}

/// Dock visibility state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockState {
    /// Full panel on right
    #[default]
    Docked,
    /// Icon-only + unread badge
    Narrow,
    /// Hidden (navigate to full Chat page)
    Hidden,
}

/// Chat session state
#[derive(Debug, Clone)]
pub struct ChatState {
    /// Current dock visibility state
    pub dock_state: DockState,
    /// Messages in the current conversation
    pub messages: Vec<ChatMessage>,
    /// Current target for inference
    pub target: ChatTarget,
    /// Context toggles
    pub context: ContextToggles,
    /// Whether a request is in progress
    pub loading: bool,
    /// Last error message
    pub error: Option<String>,
    /// Unread message count (when dock is narrow)
    pub unread_count: usize,
    /// Current page context (for context toggle)
    pub page_context: Option<PageContext>,
}

/// Page context for context toggles
#[derive(Debug, Clone)]
pub struct PageContext {
    /// Current page path
    pub path: String,
    /// Selected entity type
    pub entity_type: Option<String>,
    /// Selected entity ID
    pub entity_id: Option<String>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            dock_state: DockState::Docked,
            messages: Vec::new(),
            target: ChatTarget::Default,
            context: ContextToggles::default(),
            loading: false,
            error: None,
            unread_count: 0,
            page_context: None,
        }
    }
}

/// Chat actions for modifying state
#[derive(Clone)]
pub struct ChatAction {
    client: Arc<ApiClient>,
    state: RwSignal<ChatState>,
}

impl ChatAction {
    pub fn new(client: Arc<ApiClient>, state: RwSignal<ChatState>) -> Self {
        Self { client, state }
    }

    /// Send a message and get a response
    pub async fn send_message(&self, content: String) -> Result<(), ApiError> {
        if content.trim().is_empty() {
            return Ok(());
        }
        if self.state.get_untracked().loading {
            return Ok(());
        }

        // Add user message
        self.state.update(|s| {
            s.messages.push(ChatMessage::user(content.clone()));
            s.loading = true;
            s.error = None;
        });

        // Build the prompt with context
        let prompt = self.build_prompt(&content);

        // Send inference request
        let request = InferenceRequest {
            prompt,
            system: Some("You are AdapterOS, an AI assistant for managing ML inference. Be helpful, concise, and technical when needed.".to_string()),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: None,
        };

        match self.client.infer(&request).await {
            Ok(response) => {
                self.state.update(|s| {
                    s.messages.push(ChatMessage::assistant(response.text));
                    s.loading = false;
                    // Increment unread count if dock is narrow
                    if s.dock_state == DockState::Narrow {
                        s.unread_count += 1;
                    }
                });
                Ok(())
            }
            Err(e) => {
                self.state.update(|s| {
                    s.loading = false;
                    s.error = Some(e.to_string());
                });
                Err(e)
            }
        }
    }

    /// Build prompt with context based on toggles
    fn build_prompt(&self, content: &str) -> String {
        let state = self.state.get();
        let mut parts = Vec::new();

        // Add context based on toggles
        if state.context.current_page {
            if let Some(ctx) = &state.page_context {
                parts.push(format!(
                    "[Context: Page={}, Entity={:?}, ID={:?}]",
                    ctx.path, ctx.entity_type, ctx.entity_id
                ));
            }
        }

        if state.context.system_snapshot {
            parts.push("[Context: System snapshot requested]".to_string());
        }

        if state.context.recent_logs {
            parts.push("[Context: Recent logs requested]".to_string());
        }

        // Build conversation history
        let history: Vec<String> = state
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect();

        if !history.is_empty() {
            parts.push(history.join("\n\n"));
        }

        parts.push(format!("user: {}", content));
        parts.join("\n\n")
    }

    /// Set dock state
    pub fn set_dock_state(&self, dock_state: DockState) {
        self.state.update(|s| {
            s.dock_state = dock_state;
            // Clear unread count when expanding
            if dock_state == DockState::Docked {
                s.unread_count = 0;
            }
        });
    }

    /// Toggle dock between docked and narrow
    pub fn toggle_dock(&self) {
        self.state.update(|s| {
            s.dock_state = match s.dock_state {
                DockState::Docked => DockState::Narrow,
                DockState::Narrow => DockState::Docked,
                DockState::Hidden => DockState::Docked,
            };
            if s.dock_state == DockState::Docked {
                s.unread_count = 0;
            }
        });
    }

    /// Set the chat target
    pub fn set_target(&self, target: ChatTarget) {
        self.state.update(|s| {
            s.target = target;
        });
    }

    /// Toggle a context option
    pub fn toggle_context(&self, toggle: ContextToggle) {
        self.state.update(|s| match toggle {
            ContextToggle::CurrentPage => s.context.current_page = !s.context.current_page,
            ContextToggle::RecentLogs => s.context.recent_logs = !s.context.recent_logs,
            ContextToggle::SystemSnapshot => s.context.system_snapshot = !s.context.system_snapshot,
        });
    }

    /// Update page context
    pub fn set_page_context(&self, context: PageContext) {
        self.state.update(|s| {
            s.page_context = Some(context);
        });
    }

    /// Clear all messages
    pub fn clear_messages(&self) {
        self.state.update(|s| {
            s.messages.clear();
            s.error = None;
            s.unread_count = 0;
        });
    }

    /// Clear error
    pub fn clear_error(&self) {
        self.state.update(|s| {
            s.error = None;
        });
    }
}

/// Context toggle types
#[derive(Debug, Clone, Copy)]
pub enum ContextToggle {
    CurrentPage,
    RecentLogs,
    SystemSnapshot,
}

/// Chat context type
pub type ChatContext = (ReadSignal<ChatState>, ChatAction);

/// Provide chat context to the application
pub fn provide_chat_context() {
    web_sys::console::log_1(&"[ChatContext] Initializing...".into());
    let base_url = format!("{}/api", api_base_url().trim_end_matches('/'));
    let client = Arc::new(ApiClient::with_base_url(base_url));
    web_sys::console::log_1(&"[ChatContext] Client created".into());
    let state = RwSignal::new(ChatState::default());
    web_sys::console::log_1(&"[ChatContext] State created".into());
    let action = ChatAction::new(Arc::clone(&client), state);

    provide_context((state.read_only(), action));
    web_sys::console::log_1(&"[ChatContext] Context provided".into());
}

/// Use chat context
pub fn use_chat() -> ChatContext {
    expect_context::<ChatContext>()
}
