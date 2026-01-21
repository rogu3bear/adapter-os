//! Chat state management
//!
//! Global chat state that persists across page navigation.
//! Supports both non-streaming and SSE streaming inference.

use crate::api::{api_base_url, ApiClient, ApiError, InferenceRequest};
use chrono::{DateTime, Utc};
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, AbortSignal, Request, RequestInit, RequestMode, Response};

/// Maximum number of messages to retain in chat history.
/// Prevents unbounded memory growth in long sessions.
const MAX_MESSAGES: usize = 100;

/// Default maximum tokens for inference requests.
const DEFAULT_MAX_TOKENS: usize = 2048;

/// Default temperature for inference requests.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Maximum number of sessions to retain in localStorage.
const MAX_SESSIONS: usize = 20;

/// LocalStorage key for chat context toggles.
const CONTEXT_TOGGLES_KEY: &str = "adapteros_chat_context_toggles";

/// LocalStorage key for chat sessions.
const SESSIONS_STORAGE_KEY: &str = "adapteros_chat_sessions";

/// Evict old messages to maintain MAX_MESSAGES limit.
/// Uses drain() for O(n) single operation instead of O(n²) repeated remove(0).
#[inline]
fn evict_old_messages(messages: &mut Vec<ChatMessage>, max: usize) {
    if messages.len() > max {
        let to_remove = messages.len() - max;
        messages.drain(0..to_remove);
    }
}

// ============================================================================
// SSE Streaming Types (moved from pages/chat.rs)
// ============================================================================

/// Context request for inference with UI context toggles.
///
/// Mirrors `adapteros_api_types::inference::ContextRequest` for WASM builds.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ContextRequest {
    /// Include current page context (navigation path, selected entity)
    #[serde(default)]
    pub include_page_context: bool,
    /// Include recent system logs (last 200 lines)
    #[serde(default)]
    pub include_recent_logs: bool,
    /// Include system health snapshot (workers, memory, health)
    #[serde(default)]
    pub include_system_snapshot: bool,
    /// Current page path (e.g., "/adapters/my-adapter")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_path: Option<String>,
    /// Type of selected entity (e.g., "adapter", "job")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// ID of selected entity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}

/// Streaming inference request for POST /v1/infer/stream
#[derive(Debug, Clone, Serialize)]
pub struct StreamingInferRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Context toggles for additional prompt context (PRD-002 Phase 2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextRequest>,
}

/// SSE event types from the streaming inference endpoint
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
enum InferenceEvent {
    /// Inference token
    Token { text: String },
    /// Inference complete
    Done {
        #[serde(default)]
        total_tokens: usize,
        #[serde(default)]
        latency_ms: u64,
        #[serde(default)]
        trace_id: Option<String>,
        #[serde(default)]
        prompt_tokens: Option<u32>,
        #[serde(default)]
        completion_tokens: Option<u32>,
    },
    /// Error occurred
    Error { message: String },
    /// Adapter state update for visualization
    AdapterStateUpdate { adapters: Vec<AdapterStateInfo> },
    /// Catch-all for other events (Loading, Ready, etc.)
    #[serde(other)]
    Other,
}

/// Adapter state information from server
#[derive(Debug, Clone, Deserialize)]
pub struct AdapterStateInfo {
    pub adapter_id: String,
    pub uses_per_minute: u32,
    pub is_active: bool,
}

/// OpenAI-compatible streaming chunk (alternative format)
#[derive(Debug, Clone, Deserialize)]
struct StreamingChunk {
    #[serde(default)]
    pub choices: Vec<StreamingChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct StreamingChoice {
    #[serde(default)]
    pub delta: Delta,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Delta {
    #[serde(default)]
    pub content: Option<String>,
}

/// Parsed SSE event result
#[derive(Debug, Clone, Default)]
struct ParsedSseEvent {
    token: Option<String>,
    trace_id: Option<String>,
    latency_ms: Option<u64>,
    token_count: Option<u32>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    adapter_states: Option<Vec<AdapterStateInfo>>,
}

/// Trace info returned from stream_inference
#[derive(Debug, Clone, Default)]
pub struct StreamTraceInfo {
    pub trace_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub token_count: Option<u32>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

// ============================================================================
// Chat Message Types
// ============================================================================

/// A single chat message with optional trace information
#[derive(Debug, Clone, PartialEq)]
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
    /// Trace ID for this message (populated on stream completion)
    pub trace_id: Option<String>,
    /// Latency in milliseconds
    pub latency_ms: Option<u64>,
    /// Total tokens generated
    pub token_count: Option<u32>,
    /// Prompt tokens (input tokens)
    pub prompt_tokens: Option<u32>,
    /// Completion tokens (output tokens)
    pub completion_tokens: Option<u32>,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content,
            timestamp: Utc::now(),
            is_streaming: false,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content,
            timestamp: Utc::now(),
            is_streaming: false,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
        }
    }

    pub fn assistant_streaming() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: String::new(),
            timestamp: Utc::now(),
            is_streaming: true,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextToggles {
    /// Include current page selection (adapter/job/worker)
    pub current_page: bool,
    /// Include recent logs (last 200 lines)
    pub recent_logs: bool,
    /// Include system snapshot (health + worker counts)
    pub system_snapshot: bool,
}

/// Load context toggles from localStorage, falling back to defaults.
fn load_context_toggles() -> ContextToggles {
    let Some(window) = web_sys::window() else {
        return ContextToggles::default();
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return ContextToggles::default();
    };
    let Ok(Some(data)) = storage.get_item(CONTEXT_TOGGLES_KEY) else {
        return ContextToggles::default();
    };

    serde_json::from_str(&data).unwrap_or_default()
}

/// Save context toggles to localStorage.
fn save_context_toggles(toggles: &ContextToggles) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };

    if let Ok(json) = serde_json::to_string(toggles) {
        let _ = storage.set_item(CONTEXT_TOGGLES_KEY, &json);
    }
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

// ============================================================================
// Chat State
// ============================================================================

/// Wrapper type for AbortController that implements Send + Sync using SendWrapper
/// This is safe because WASM is single-threaded
type AbortControllerCell = SendWrapper<Rc<RefCell<Option<AbortController>>>>;

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
    /// Whether a request is in progress (loading = waiting for first token)
    pub loading: bool,
    /// Whether we're actively streaming tokens
    pub streaming: bool,
    /// Last error message
    pub error: Option<String>,
    /// ID of last message the user has "seen" (scrolled to or dock was open)
    pub last_read_message_id: Option<String>,
    /// Current page context (for context toggle)
    pub page_context: Option<PageContext>,
    /// Active adapters for visualization
    pub active_adapters: Vec<AdapterStateInfo>,
}

impl ChatState {
    /// Compute unread count as derived value from last_read_message_id.
    ///
    /// Returns the number of messages after the last read position.
    ///
    /// # Edge Cases
    /// - `None` bookmark + empty messages → 0 (nothing to read)
    /// - `None` bookmark + N messages → N (all unread, fresh session)
    /// - Valid bookmark → count of messages after that position
    /// - Stale bookmark (message was evicted) → 0 (user was caught up past eviction point)
    pub fn unread_count(&self) -> usize {
        match &self.last_read_message_id {
            None => self.messages.len(),
            Some(last_id) => self
                .messages
                .iter()
                .position(|m| m.id == *last_id)
                .map(|pos| self.messages.len().saturating_sub(pos + 1))
                // If bookmark not found, user had read past messages that were evicted.
                // They're still caught up relative to what existed then.
                .unwrap_or(0),
        }
    }

    /// Mark all current messages as read by setting last_read_message_id to the latest message.
    pub fn mark_as_read(&mut self) {
        self.last_read_message_id = self.messages.last().map(|m| m.id.clone());
    }
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
            context: load_context_toggles(),
            loading: false,
            streaming: false,
            error: None,
            last_read_message_id: None,
            page_context: None,
            active_adapters: Vec::new(),
        }
    }
}

// ============================================================================
// Chat Actions
// ============================================================================

/// Chat actions for modifying state
#[derive(Clone)]
pub struct ChatAction {
    client: Arc<ApiClient>,
    state: RwSignal<ChatState>,
    abort_controller: RwSignal<AbortControllerCell>,
}

impl ChatAction {
    pub fn new(client: Arc<ApiClient>, state: RwSignal<ChatState>) -> Self {
        Self {
            client,
            state,
            abort_controller: RwSignal::new(SendWrapper::new(Rc::new(RefCell::new(None)))),
        }
    }

    /// Send a message with SSE streaming (preferred method)
    pub fn send_message_streaming(&self, content: String) {
        // Normalize content - trim whitespace
        let content = content.trim().to_string();
        if content.is_empty() {
            return;
        }

        let current_state = self.state.get_untracked();
        if current_state.loading || current_state.streaming {
            return;
        }

        // Add user message with FIFO eviction (content already trimmed)
        self.state.update(|s| {
            s.messages.push(ChatMessage::user(content.clone()));
            evict_old_messages(&mut s.messages, MAX_MESSAGES);
            s.loading = true;
            s.streaming = true;
            s.error = None;
        });

        // Build the prompt with context and history
        let prompt = self.build_prompt(&content);

        // Add placeholder assistant message for streaming
        self.state.update(|s| {
            s.messages.push(ChatMessage::assistant_streaming());
        });

        // Create AbortController for this request
        let controller = AbortController::new().ok();
        let signal = controller.as_ref().map(|c| c.signal());

        // Store the controller
        let cell = self.abort_controller.get();
        *cell.borrow_mut() = controller;

        // Clone what we need for the async block
        let state = self.state;
        let abort_controller = self.abort_controller;

        // Build context request from current state (PRD-002 Phase 2)
        let context_request = {
            let current = self.state.get_untracked();
            ContextRequest {
                include_page_context: current.context.current_page,
                include_recent_logs: current.context.recent_logs,
                include_system_snapshot: current.context.system_snapshot,
                page_path: current.page_context.as_ref().map(|c| c.path.clone()),
                entity_type: current
                    .page_context
                    .as_ref()
                    .and_then(|c| c.entity_type.clone()),
                entity_id: current
                    .page_context
                    .as_ref()
                    .and_then(|c| c.entity_id.clone()),
            }
        };

        wasm_bindgen_futures::spawn_local(async move {
            let request = StreamingInferRequest {
                prompt,
                max_tokens: Some(DEFAULT_MAX_TOKENS),
                temperature: Some(DEFAULT_TEMPERATURE),
                adapters: None,
                context: Some(context_request),
            };

            match stream_inference_to_state(&request, state, signal.as_ref()).await {
                Ok(trace_info) => {
                    // Mark the last message as no longer streaming and add trace info
                    state.update(|s| {
                        if let Some(last) = s.messages.last_mut() {
                            if last.role == "assistant" {
                                last.is_streaming = false;
                                last.trace_id = trace_info.trace_id;
                                last.latency_ms = trace_info.latency_ms;
                                last.token_count = trace_info.token_count;
                                last.prompt_tokens = trace_info.prompt_tokens;
                                last.completion_tokens = trace_info.completion_tokens;
                            }
                        }
                        // When dock is open, mark new messages as read immediately
                        if s.dock_state == DockState::Docked {
                            s.mark_as_read();
                        }
                    });
                }
                Err(e) => {
                    if is_abort_error(&e) {
                        // Stream was cancelled by user - mark message as no longer streaming
                        state.update(|s| {
                            if let Some(last) = s.messages.last_mut() {
                                if last.role == "assistant" {
                                    last.is_streaming = false;
                                }
                            }
                        });
                    } else {
                        // Remove the empty assistant message on error
                        state.update(|s| {
                            if let Some(last) = s.messages.last() {
                                if last.role == "assistant" && last.content.is_empty() {
                                    s.messages.pop();
                                }
                            }
                            s.error = Some(e);
                        });
                    }
                }
            }

            state.update(|s| {
                s.loading = false;
                s.streaming = false;
            });

            // Clear the abort controller
            let cell = abort_controller.get();
            *cell.borrow_mut() = None;
        });
    }

    /// Cancel the current streaming request
    pub fn cancel_stream(&self) {
        let cell = self.abort_controller.get();
        if let Some(controller) = cell.borrow_mut().take() {
            controller.abort();
        }
        self.state.update(|s| {
            s.streaming = false;
            s.loading = false;
            // Mark the last message as no longer streaming
            if let Some(last) = s.messages.last_mut() {
                if last.role == "assistant" {
                    last.is_streaming = false;
                }
            }
        });
    }

    /// Send a message and get a response (non-streaming, legacy)
    ///
    /// **Deprecated**: Use `send_message_streaming()` instead for better UX.
    #[deprecated(since = "0.3.0", note = "Use send_message_streaming() instead")]
    #[allow(dead_code)]
    pub async fn send_message(&self, content: String) -> Result<(), ApiError> {
        if content.trim().is_empty() {
            return Ok(());
        }
        if self.state.get_untracked().loading {
            return Ok(());
        }

        // Add user message with FIFO eviction
        self.state.update(|s| {
            s.messages.push(ChatMessage::user(content.clone()));
            evict_old_messages(&mut s.messages, MAX_MESSAGES);
            s.loading = true;
            s.error = None;
        });

        // Build the prompt with context
        let prompt = self.build_prompt(&content);

        // Send inference request
        let request = InferenceRequest {
            prompt,
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: None,
        };

        match self.client.infer(&request).await {
            Ok(response) => {
                self.state.update(|s| {
                    s.messages.push(ChatMessage::assistant(response.text));
                    evict_old_messages(&mut s.messages, MAX_MESSAGES);
                    s.loading = false;
                    // When dock is open, mark new messages as read immediately
                    if s.dock_state == DockState::Docked {
                        s.mark_as_read();
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
        let state = self.state.get_untracked();
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
            // Mark messages as read when dock is opened
            if dock_state == DockState::Docked {
                s.mark_as_read();
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
            // Mark messages as read when dock is opened
            if s.dock_state == DockState::Docked {
                s.mark_as_read();
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
        // Persist toggled state to localStorage
        let toggles = self.state.get_untracked().context.clone();
        save_context_toggles(&toggles);
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
            s.last_read_message_id = None;
            s.active_adapters.clear();
        });
    }

    /// Clear error
    pub fn clear_error(&self) {
        self.state.update(|s| {
            s.error = None;
        });
    }

    /// Check if currently busy (loading or streaming)
    pub fn is_busy(&self) -> bool {
        let state = self.state.get_untracked();
        state.loading || state.streaming
    }

    /// Restore chat state from a stored session
    pub fn restore_session(&self, session: StoredChatSession) {
        use chrono::{DateTime, Utc};

        self.state.update(|s| {
            // Convert stored messages back to ChatMessages, including trace info
            s.messages = session
                .messages
                .into_iter()
                .map(|m| {
                    // Parse timestamp string back to DateTime, fallback to now
                    let timestamp = match DateTime::parse_from_rfc3339(&m.timestamp) {
                        Ok(dt) => dt.with_timezone(&Utc),
                        Err(e) => {
                            web_sys::console::warn_1(
                                &format!(
                                    "[Chat] Failed to parse timestamp '{}' for message {}: {}",
                                    m.timestamp, m.id, e
                                )
                                .into(),
                            );
                            Utc::now()
                        }
                    };

                    ChatMessage {
                        id: m.id,
                        role: m.role,
                        content: m.content,
                        timestamp,
                        is_streaming: false,
                        trace_id: m.trace_id,
                        latency_ms: m.latency_ms,
                        token_count: m.token_count,
                        prompt_tokens: m.prompt_tokens,
                        completion_tokens: m.completion_tokens,
                    }
                })
                .collect();
            s.error = None;
            // Mark all restored messages as read
            s.last_read_message_id = s.messages.last().map(|m| m.id.clone());
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
    let client = Arc::new(ApiClient::with_base_url(api_base_url()));
    let state = RwSignal::new(ChatState::default());
    let action = ChatAction::new(Arc::clone(&client), state);
    provide_context((state.read_only(), action));
}

/// Use chat context
pub fn use_chat() -> ChatContext {
    expect_context::<ChatContext>()
}

// ============================================================================
// SSE Streaming Implementation
// ============================================================================

/// Get CSRF token from cookies (csrf_token is not httpOnly)
fn get_csrf_token() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok())
        .and_then(|d| d.cookie().ok())
        .and_then(|cookies| {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("csrf_token=") {
                    return Some(token.to_string());
                }
            }
            None
        })
}

/// Check if a JsValue error is an AbortError (DOMException with name "AbortError").
///
/// This is the preferred method for abort detection as it uses proper type checking
/// via `dyn_ref` instead of fragile string matching. Different browsers may format
/// error messages differently, but the DOMException type and name are standardized.
///
/// # Example
/// ```ignore
/// let abort_controller = AbortController::new().unwrap();
/// abort_controller.abort();
/// // When a fetch is aborted, the error will be a DOMException with name "AbortError"
/// ```
fn is_abort_error_js(error: &JsValue) -> bool {
    if let Some(dom_exception) = error.dyn_ref::<web_sys::DomException>() {
        return dom_exception.name() == "AbortError";
    }
    false
}

/// Check if an error string indicates an AbortError (fallback for string errors).
///
/// This serves as a fallback for cases where:
/// 1. The error has already been converted to a string before reaching our check
/// 2. The error is not a DOMException but contains abort-related text
/// 3. We're receiving error messages from nested async operations
///
/// The string patterns cover common browser variations:
/// - "AbortError" - Standard DOMException name
/// - "aborted" - Generic abort indication
/// - "The operation was aborted" - Full Chrome/Firefox error message
#[cfg(not(test))]
fn is_abort_error(error: &str) -> bool {
    error.contains("AbortError")
        || error.contains("aborted")
        || error.contains("The operation was aborted")
}

/// Check if an error string indicates an AbortError (testable version).
#[cfg(test)]
pub fn is_abort_error(error: &str) -> bool {
    error.contains("AbortError")
        || error.contains("aborted")
        || error.contains("The operation was aborted")
}

/// Parse an SSE event and extract token content plus trace info
fn parse_sse_event_with_info(event_data: &str) -> ParsedSseEvent {
    let mut result = ParsedSseEvent::default();

    let mut data_line: Option<&str> = None;

    for line in event_data.lines() {
        if let Some(stripped) = line.strip_prefix("data: ") {
            data_line = Some(stripped);
        }
    }

    let data = match data_line {
        Some(d) => d,
        None => return result,
    };

    // Check for [DONE] marker
    if data == "[DONE]" {
        return result;
    }

    // Try parsing as InferenceEvent first (adapterOS format)
    if let Ok(event) = serde_json::from_str::<InferenceEvent>(data) {
        match event {
            InferenceEvent::Token { text } => {
                result.token = Some(text);
            }
            InferenceEvent::Done {
                total_tokens,
                latency_ms,
                trace_id,
                prompt_tokens,
                completion_tokens,
            } => {
                result.trace_id = trace_id;
                result.latency_ms = Some(latency_ms);
                result.token_count = Some(total_tokens as u32);
                result.prompt_tokens = prompt_tokens;
                result.completion_tokens = completion_tokens;
            }
            InferenceEvent::Error { message } => {
                web_sys::console::error_1(&JsValue::from_str(&format!(
                    "Stream error: {}",
                    message
                )));
            }
            InferenceEvent::AdapterStateUpdate { adapters } => {
                result.adapter_states = Some(adapters);
            }
            InferenceEvent::Other => {}
        }
        return result;
    }

    // Try parsing as OpenAI-compatible StreamingChunk
    if let Ok(chunk) = serde_json::from_str::<StreamingChunk>(data) {
        if let Some(choice) = chunk.choices.first() {
            if let Some(content) = &choice.delta.content {
                result.token = Some(content.clone());
            }
        }
    }

    result
}

/// Stream inference using POST SSE endpoint, updating state directly
async fn stream_inference_to_state(
    request: &StreamingInferRequest,
    state: RwSignal<ChatState>,
    abort_signal: Option<&AbortSignal>,
) -> Result<StreamTraceInfo, String> {
    let url = format!("{}/v1/infer/stream", api_base_url());

    let body = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;

    // Create fetch request with POST method
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&body));
    opts.set_credentials(web_sys::RequestCredentials::Include);

    if let Some(signal) = abort_signal {
        opts.set_signal(Some(signal));
    }

    let request_obj = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Failed to create request: {:?}", e))?;

    request_obj
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("Failed to set Content-Type header: {:?}", e))?;

    request_obj
        .headers()
        .set("Accept", "text/event-stream")
        .map_err(|e| format!("Failed to set Accept header: {:?}", e))?;

    if let Some(csrf_token) = get_csrf_token() {
        request_obj
            .headers()
            .set("X-CSRF-Token", &csrf_token)
            .map_err(|e| format!("Failed to set CSRF header: {:?}", e))?;
    }

    let window = web_sys::window().ok_or("No window object")?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request_obj))
        .await
        .map_err(|e| {
            // Prefer proper DomException type checking over fragile string matching
            if is_abort_error_js(&e) {
                return "AbortError: The operation was aborted".to_string();
            }
            // Fallback to string matching for edge cases (nested errors, already-stringified)
            let error_str = format!("{:?}", e);
            if is_abort_error(&error_str) {
                "AbortError: The operation was aborted".to_string()
            } else {
                format!("Fetch failed: {:?}", e)
            }
        })?
        .dyn_into()
        .map_err(|_| "Response is not a Response object")?;

    if !response.ok() {
        let status = response.status();
        let status_text = response.status_text();
        return Err(format!("HTTP error {}: {}", status, status_text));
    }

    let body_stream = response.body().ok_or("No response body")?;
    let reader = body_stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| "Failed to get reader")?;

    let mut buffer = String::new();
    let mut trace_info = StreamTraceInfo::default();

    loop {
        if let Some(signal) = abort_signal {
            if signal.aborted() {
                let _ = reader.cancel();
                return Err("AbortError: The operation was aborted".to_string());
            }
        }

        let result = JsFuture::from(reader.read()).await.map_err(|e| {
            // Prefer proper DomException type checking over fragile string matching
            if is_abort_error_js(&e) {
                return "AbortError: The operation was aborted".to_string();
            }
            // Fallback to string matching for edge cases (nested errors, already-stringified)
            let error_str = format!("{:?}", e);
            if is_abort_error(&error_str) {
                "AbortError: The operation was aborted".to_string()
            } else {
                format!("Read failed: {:?}", e)
            }
        })?;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .map_err(|_| "Failed to get done property")?
            .as_bool()
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|_| "Failed to get value property")?;

        if value.is_undefined() {
            continue;
        }

        let array = js_sys::Uint8Array::new(&value);
        let bytes: Vec<u8> = array.to_vec();
        let chunk = String::from_utf8_lossy(&bytes).to_string();

        buffer.push_str(&chunk);

        // Process complete SSE events from buffer
        // Use drain-style approach to avoid O(n²) string reallocation
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            // Use drain to remove processed bytes in-place (O(n) single operation)
            buffer.drain(..event_end + 2);

            let parsed = parse_sse_event_with_info(&event_data);

            if let Some(token_content) = parsed.token {
                // Append token to the last (assistant) message
                state.update(|s| {
                    if let Some(last) = s.messages.last_mut() {
                        if last.role == "assistant" {
                            last.content.push_str(&token_content);
                        }
                    }
                    // No longer loading once we have first token
                    s.loading = false;
                });
            }

            // Capture trace info from Done event
            if parsed.trace_id.is_some() {
                trace_info.trace_id = parsed.trace_id;
            }
            if parsed.latency_ms.is_some() {
                trace_info.latency_ms = parsed.latency_ms;
            }
            if parsed.token_count.is_some() {
                trace_info.token_count = parsed.token_count;
            }
            if parsed.prompt_tokens.is_some() {
                trace_info.prompt_tokens = parsed.prompt_tokens;
            }
            if parsed.completion_tokens.is_some() {
                trace_info.completion_tokens = parsed.completion_tokens;
            }

            // Update active adapters from adapter state info (merge by adapter_id)
            if let Some(adapter_states) = parsed.adapter_states {
                state.update(|s| {
                    // Merge new adapter states with existing ones
                    for new_adapter in adapter_states {
                        if let Some(existing) = s
                            .active_adapters
                            .iter_mut()
                            .find(|a| a.adapter_id == new_adapter.adapter_id)
                        {
                            // Update existing adapter state
                            existing.uses_per_minute = new_adapter.uses_per_minute;
                            existing.is_active = new_adapter.is_active;
                        } else {
                            // Add new adapter
                            s.active_adapters.push(new_adapter);
                        }
                    }
                });
            }
        }
    }

    // Warn if buffer has unprocessed data (indicates incomplete event)
    if !buffer.is_empty() {
        let trimmed = buffer.trim();
        if !trimmed.is_empty() {
            web_sys::console::warn_1(
                &format!(
                    "[SSE] Stream ended with unprocessed data ({} bytes): {}...",
                    buffer.len(),
                    &trimmed[..trimmed.len().min(50)]
                )
                .into(),
            );
        }
    }

    // Explicitly release the reader lock to clean up resources
    let _ = reader.release_lock();

    Ok(trace_info)
}

// ============================================================================
// Session Persistence (localStorage)
// ============================================================================

/// Chat session metadata for the landing page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMeta {
    pub id: String,
    pub title: String,
    pub target: String,
    pub message_count: usize,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Full chat session with messages (stored in localStorage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChatSession {
    pub id: String,
    pub title: String,
    pub target: String,
    pub messages: Vec<StoredMessage>,
    pub created_at: String,
    pub updated_at: String,
}

/// Stored message (simplified for localStorage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    /// Trace ID for this message (populated on stream completion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Total tokens generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
    /// Prompt tokens (input tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    /// Completion tokens (output tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
}

/// Manager for chat sessions in localStorage
pub struct ChatSessionsManager;

impl ChatSessionsManager {
    /// Load all session metadata from localStorage
    pub fn load_sessions() -> Vec<ChatSessionMeta> {
        let Some(window) = web_sys::window() else {
            return Vec::new();
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return Vec::new();
        };
        let Ok(Some(data)) = storage.get_item(SESSIONS_STORAGE_KEY) else {
            return Vec::new();
        };

        serde_json::from_str::<Vec<StoredChatSession>>(&data)
            .map(|sessions| {
                sessions
                    .into_iter()
                    .map(|s| ChatSessionMeta {
                        id: s.id,
                        title: s.title,
                        target: s.target,
                        message_count: s.messages.len(),
                        preview: s
                            .messages
                            .last()
                            .map(|m| {
                                let content = &m.content;
                                if content.len() > 100 {
                                    format!("{}...", &content[..100])
                                } else {
                                    content.clone()
                                }
                            })
                            .unwrap_or_default(),
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Load a specific session by ID
    pub fn load_session(id: &str) -> Option<StoredChatSession> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        let data = storage.get_item(SESSIONS_STORAGE_KEY).ok()??;

        let sessions: Vec<StoredChatSession> = serde_json::from_str(&data).ok()?;
        sessions.into_iter().find(|s| s.id == id)
    }

    /// Save or update a session
    pub fn save_session(session: &StoredChatSession) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return;
        };

        // Load existing sessions
        let mut sessions: Vec<StoredChatSession> = storage
            .get_item(SESSIONS_STORAGE_KEY)
            .ok()
            .flatten()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default();

        // Find and update or append
        if let Some(pos) = sessions.iter().position(|s| s.id == session.id) {
            sessions[pos] = session.clone();
        } else {
            sessions.push(session.clone());
        }

        // Sort by updated_at descending
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        // Trim to max sessions
        sessions.truncate(MAX_SESSIONS);

        // Save back
        if let Ok(json) = serde_json::to_string(&sessions) {
            let _ = storage.set_item(SESSIONS_STORAGE_KEY, &json);
        }
    }

    /// Delete a session by ID
    pub fn delete_session(id: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return;
        };

        let mut sessions: Vec<StoredChatSession> = storage
            .get_item(SESSIONS_STORAGE_KEY)
            .ok()
            .flatten()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default();

        sessions.retain(|s| s.id != id);

        if let Ok(json) = serde_json::to_string(&sessions) {
            let _ = storage.set_item(SESSIONS_STORAGE_KEY, &json);
        }
    }

    /// Create a session from current dock state
    pub fn session_from_state(id: &str, state: &ChatState) -> StoredChatSession {
        let now = chrono::Utc::now().to_rfc3339();
        let title = state
            .messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| {
                let content = &m.content;
                if content.len() > 50 {
                    format!("{}...", &content[..50])
                } else {
                    content.clone()
                }
            })
            .unwrap_or_else(|| "New Chat".to_string());

        StoredChatSession {
            id: id.to_string(),
            title,
            target: state.target.display_name(),
            messages: state
                .messages
                .iter()
                .map(|m| {
                    let timestamp_str = m.timestamp.to_rfc3339();
                    debug_assert!(
                        chrono::DateTime::parse_from_rfc3339(&timestamp_str).is_ok(),
                        "Timestamp should round-trip through RFC3339: {}",
                        timestamp_str
                    );
                    StoredMessage {
                        id: m.id.clone(),
                        role: m.role.clone(),
                        content: m.content.clone(),
                        timestamp: timestamp_str,
                        trace_id: m.trace_id.clone(),
                        latency_ms: m.latency_ms,
                        token_count: m.token_count,
                        prompt_tokens: m.prompt_tokens,
                        completion_tokens: m.completion_tokens,
                    }
                })
                .collect(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests for is_abort_error string-based detection
    mod abort_error_detection {
        use super::*;

        #[test]
        fn detects_standard_abort_error_name() {
            assert!(is_abort_error("AbortError"));
            assert!(is_abort_error("DOMException: AbortError"));
            assert!(is_abort_error("Error: AbortError - request cancelled"));
        }

        #[test]
        fn detects_aborted_keyword() {
            assert!(is_abort_error("The request was aborted"));
            assert!(is_abort_error("aborted by user"));
            assert!(is_abort_error("Request aborted"));
        }

        #[test]
        fn detects_standard_chrome_firefox_message() {
            assert!(is_abort_error("The operation was aborted"));
            assert!(is_abort_error("DOMException: The operation was aborted"));
        }

        #[test]
        fn rejects_non_abort_errors() {
            assert!(!is_abort_error("NetworkError"));
            assert!(!is_abort_error("TimeoutError"));
            assert!(!is_abort_error("Connection refused"));
            assert!(!is_abort_error("HTTP 500 Internal Server Error"));
            assert!(!is_abort_error(""));
        }

        #[test]
        fn handles_case_sensitivity() {
            // Current implementation is case-sensitive
            assert!(!is_abort_error("ABORTERROR"));
            assert!(!is_abort_error("ABORTED"));
            // Only lowercase variations are detected
            assert!(is_abort_error("aborted"));
        }
    }
}
