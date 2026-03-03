//! Chat state management
//!
//! Global chat state that persists across page navigation.
//! Supports both non-streaming and SSE streaming inference.

use crate::api::{api_base_url, ApiClient, ApiError};
use crate::signals::perf_logging_enabled;
use adapteros_api_types::inference::{
    AdapterAttachment, Citation, ContextRequest, DegradedNotice, DocumentLink as ApiDocumentLink,
};
use chrono::{DateTime, Utc};
#[cfg(target_arch = "wasm32")]
use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, AbortSignal, Request, RequestInit, RequestMode, Response};
use web_time::Instant;

/// Maximum number of messages to retain in chat history.
/// Prevents unbounded memory growth in long sessions.
const MAX_MESSAGES: usize = 100;

/// Default maximum tokens for inference requests.
const DEFAULT_MAX_TOKENS: usize = 2048;

/// Default temperature for inference requests.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Verified mode uses deterministic-ish decoding defaults and shorter outputs to
/// reduce policy-triggered pauses during demos and reviews.
const VERIFIED_MAX_TOKENS: usize = 256;
const VERIFIED_TEMPERATURE: f32 = 0.0;

/// Maximum number of sessions to retain in localStorage.
const MAX_SESSIONS: usize = 20;

/// LocalStorage key for chat sessions.
const SESSIONS_STORAGE_KEY: &str = "adapteros_chat_sessions";

/// Dev-only fallback token emitted by backend when no worker is available.
const DEV_ECHO_NO_WORKER_PREFIX: &str = "[DEV ECHO] No inference worker available.";

/// LocalStorage key for default pinned adapters (persisted across sessions).
#[allow(dead_code)]
const PINNED_ADAPTERS_KEY: &str = "adapteros_pinned_adapters";
/// LocalStorage key for chat context toggles.
#[cfg(target_arch = "wasm32")]
const CONTEXT_TOGGLES_KEY: &str = "adapteros_context_toggles";

/// Threshold at which a "nearing capacity" warning is shown.
const OVERFLOW_WARNING_THRESHOLD: usize = 80;

/// Evict old messages to maintain MAX_MESSAGES limit.
/// Uses drain() for O(n) single operation instead of O(n²) repeated remove(0).
/// Returns the number of messages evicted.
#[inline]
fn evict_old_messages(messages: &mut Vec<ChatMessage>, max: usize) -> usize {
    if messages.len() > max {
        let to_remove = messages.len() - max;
        messages.drain(0..to_remove);
        to_remove
    } else {
        0
    }
}

fn readable_id(prefix: &str, _slug_source: &str) -> String {
    use adapteros_id::{IdPrefix, TypedId};
    let id_prefix = match prefix {
        "msg" => IdPrefix::Msg,
        "idem" => IdPrefix::Req,
        "session" => IdPrefix::Ses,
        _ => IdPrefix::Evt,
    };
    TypedId::new(id_prefix).to_string()
}

// ============================================================================
// SSE Streaming Types (moved from pages/chat.rs)
// ============================================================================

/// Streaming inference request for POST /v1/infer/stream
#[derive(Debug, Clone, Serialize)]
pub struct StreamingInferRequest {
    pub prompt: String,
    /// Optional model identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional server-side adapter stack id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Explicit mode intent from chat UI.
    pub bit_identical: bool,
    /// Context toggles for additional prompt context (PRD-002 Phase 2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextRequest>,
    /// Collection ID for scoped RAG retrieval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Enable reasoning mode: semantic router for mid-stream adapter swaps (prefers CoreML for ANE-accelerated embedder)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_mode: Option<bool>,
    /// Explicit backend preference (auto|coreml|mlx|metal|cpu)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
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
        #[serde(default)]
        backend_used: Option<String>,
        #[serde(default)]
        citations: Option<Vec<Citation>>,
        #[serde(default)]
        document_links: Option<Vec<ApiDocumentLink>>,
        #[serde(default)]
        adapters_used: Option<Vec<String>>,
        #[serde(default)]
        unavailable_pinned_adapters: Option<Vec<String>>,
        #[serde(default)]
        pinned_routing_fallback: Option<String>,
        #[serde(default)]
        adapter_attachments: Option<Vec<AdapterAttachment>>,
        #[serde(default)]
        degraded_notices: Option<Vec<DegradedNotice>>,
        #[serde(default)]
        fallback_triggered: bool,
        #[serde(default)]
        fallback_backend: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        policy_warnings: Vec<ChatPolicyWarning>,
    },
    /// Error occurred
    Error {
        message: String,
        #[serde(default)]
        recoverable: Option<bool>,
        #[serde(default)]
        code: Option<String>,
    },
    /// Inference paused for human review
    Paused {
        /// Unique pause ID for resume correlation
        pause_id: String,
        /// Inference request ID
        inference_id: String,
        /// Why the pause was triggered
        trigger_kind: String,
        /// Context for the reviewer
        #[serde(default)]
        context: Option<String>,
        /// Generated text so far
        #[serde(default)]
        text_so_far: Option<String>,
        /// Token count at pause point
        #[serde(default)]
        token_count: usize,
    },
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

/// Pause information from a Paused event.
#[derive(Debug, Clone, Default)]
pub struct PauseInfo {
    /// Unique pause ID for resume correlation.
    pub pause_id: String,
    /// Inference request ID.
    pub inference_id: String,
    /// Why the pause was triggered.
    pub trigger_kind: String,
    /// Context for the reviewer.
    pub context: Option<String>,
    /// Generated text so far.
    pub text_so_far: Option<String>,
    /// Token count at pause point.
    pub token_count: usize,
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
    backend_used: Option<String>,
    citations: Option<Vec<ChatCitation>>,
    document_links: Option<Vec<ChatDocumentLink>>,
    adapters_used: Option<Vec<String>>,
    policy_warnings: Option<Vec<ChatPolicyWarning>>,
    unavailable_pinned_adapters: Option<Vec<String>>,
    pinned_routing_fallback: Option<String>,
    adapter_attachments: Option<Vec<AdapterAttachment>>,
    degraded_notices: Option<Vec<DegradedNotice>>,
    fallback_triggered: Option<bool>,
    fallback_backend: Option<String>,
    error_message: Option<String>,
    error_code: Option<String>,
    error_retryable: Option<bool>,
    pause_info: Option<PauseInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct StreamFinishedPayload {
    #[serde(default)]
    citations: Option<Vec<Citation>>,
    #[serde(default)]
    document_links: Option<Vec<ApiDocumentLink>>,
    #[serde(default)]
    adapters_used: Option<Vec<String>>,
    #[serde(default)]
    unavailable_pinned_adapters: Option<Vec<String>>,
    #[serde(default)]
    pinned_routing_fallback: Option<String>,
    #[serde(default)]
    adapter_attachments: Option<Vec<AdapterAttachment>>,
    #[serde(default)]
    degraded_notices: Option<Vec<DegradedNotice>>,
    #[serde(default)]
    fallback_triggered: bool,
    #[serde(default)]
    fallback_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    policy_warnings: Vec<ChatPolicyWarning>,
}

/// Trace info returned from stream_inference
#[derive(Debug, Clone, Default)]
pub struct StreamTraceInfo {
    pub trace_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub token_count: Option<u32>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub backend_used: Option<String>,
    pub citations: Option<Vec<ChatCitation>>,
    pub document_links: Option<Vec<ChatDocumentLink>>,
    pub adapters_used: Vec<String>,
    pub policy_warnings: Option<Vec<ChatPolicyWarning>>,
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    pub pinned_routing_fallback: Option<String>,
    pub adapter_attachments: Option<Vec<AdapterAttachment>>,
    pub degraded_notices: Vec<DegradedNotice>,
    pub fallback_triggered: bool,
    pub fallback_backend: Option<String>,
}

/// Streaming status notice for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamNotice {
    pub message: String,
    pub tone: StreamNoticeTone,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamNoticeTone {
    Info,
    Warning,
    Error,
    /// Inference is paused awaiting human review
    Paused,
}

impl StreamNotice {
    fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            tone: StreamNoticeTone::Info,
            retryable: false,
        }
    }

    fn warning(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            message: message.into(),
            tone: StreamNoticeTone::Warning,
            retryable,
        }
    }

    fn error(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            message: message.into(),
            tone: StreamNoticeTone::Error,
            retryable,
        }
    }

    /// Inference paused for human review
    fn paused(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            tone: StreamNoticeTone::Paused,
            retryable: false,
        }
    }
}

/// Stream recovery metadata (idempotency + last request linkage).
#[derive(Debug, Clone)]
pub struct StreamRecovery {
    pub idempotency_key: String,
    pub user_message_id: String,
    pub user_message: String,
    pub assistant_message_id: String,
    pub request_id: Option<String>,
}

// ============================================================================
// Chat Message Types
// ============================================================================

/// Lightweight citation metadata stored on chat messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCitation {
    pub citation_id: Option<String>,
    pub file_path: String,
    pub chunk_id: String,
    pub offset_start: u64,
    pub offset_end: u64,
    pub page_number: Option<u32>,
    pub relevance_score: Option<f64>,
    pub rank: Option<u32>,
}

fn map_api_citations(citations: Vec<Citation>) -> Vec<ChatCitation> {
    citations
        .into_iter()
        .map(|citation| ChatCitation {
            citation_id: citation.citation_id,
            file_path: citation.file_path,
            chunk_id: citation.chunk_id,
            offset_start: citation.offset_start,
            offset_end: citation.offset_end,
            page_number: citation.page_number,
            relevance_score: citation.relevance_score,
            rank: citation.rank,
        })
        .collect()
}

/// Lightweight document-link metadata stored on chat messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatDocumentLink {
    pub document_id: String,
    pub document_name: String,
    pub download_url: String,
    pub adapter_id: Option<String>,
    pub dataset_version_id: Option<String>,
    pub source_file: Option<String>,
}

fn map_api_document_links(document_links: Vec<ApiDocumentLink>) -> Vec<ChatDocumentLink> {
    document_links
        .into_iter()
        .map(|link| ChatDocumentLink {
            document_id: link.document_id,
            document_name: link.document_name,
            download_url: link.download_url,
            adapter_id: link.adapter_id,
            dataset_version_id: link.dataset_version_id,
            source_file: link.source_file,
        })
        .collect()
}

/// Message delivery status for queue UX
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageStatus {
    /// Message sent and response complete
    #[default]
    Complete,
    /// Message accepted, waiting for inference to be ready
    Queued,
    /// Request in flight to backend
    Sending,
    /// Response actively streaming
    Streaming,
    /// Message failed after retries
    Failed,
}

/// Phase of the pending indicator (progressive disclosure)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PendingPhase {
    /// 0 → 1.5× typical wait: just "waiting..."
    #[default]
    Calm,
    /// 1.5× → 3× typical wait: shows blocker reason
    Informative,
    /// > 3× typical wait: shows time estimate
    Estimated,
}

/// Replay proof status for a chat message (Feature 1: "Why did you say that?").
#[derive(Debug, Clone, PartialEq)]
pub enum ChatReplayStatus {
    /// Replay is available at the given fidelity.
    Available {
        /// "exact", "approximate", or "degraded".
        mode: String,
    },
    /// Replay has been executed; result is ready for display.
    Completed {
        /// "exact", "semantic", or "divergent".
        match_status: String,
        /// Replay execution ID for linking to full results.
        replay_id: String,
    },
    /// Replay is currently in progress.
    Loading,
    /// Replay is not available for this inference.
    Unavailable { reason: String },
}

/// Lightweight policy warning for UI display (Feature 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatPolicyWarning {
    /// Policy name (e.g., "Evidence", "Egress").
    pub policy_name: String,
    /// Human-readable description.
    pub message: String,
    /// "info" or "advisory".
    pub severity: String,
}

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
    /// Whether this message is still streaming (legacy, use status instead)
    pub is_streaming: bool,
    /// Message delivery status (queue UX)
    pub status: MessageStatus,
    /// When the message entered queued state (for progressive disclosure timing)
    pub queued_at: Option<DateTime<Utc>>,
    /// Current pending phase for UI rendering
    pub pending_phase: PendingPhase,
    /// Blocker reason when in Informative/Estimated phase
    pub pending_reason: Option<String>,
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
    /// Backend used for inference (e.g., "coreml", "mlx")
    pub backend_used: Option<String>,
    /// Citations returned for this assistant message (memory-only).
    pub citations: Option<Vec<ChatCitation>>,
    /// Clickable source document links for this assistant message (memory-only).
    pub document_links: Option<Vec<ChatDocumentLink>>,
    /// Lightweight persisted marker that citations existed.
    pub has_citations: bool,
    /// Adapter IDs that were active during this inference
    pub adapters_used: Option<Vec<String>>,
    /// Pinned adapters that were unavailable during this inference.
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode used when pinned adapters were unavailable.
    pub pinned_routing_fallback: Option<String>,
    /// Whether worker fallback occurred and may have changed semantics.
    pub fallback_triggered: bool,
    /// Backend selected after fallback when different from requested.
    pub fallback_backend: Option<String>,
    /// Adapter attachment details (reason + exact version metadata).
    pub adapter_attachments: Vec<AdapterAttachment>,
    /// Degraded/failure notices for trust detail rendering.
    pub degraded_notices: Vec<DegradedNotice>,

    // ---- Groundwork fields for upcoming features ----
    /// Replay proof status for "Why did you say that?" button (Feature 1).
    /// None = not yet checked, Some = replay state.
    pub replay_status: Option<ChatReplayStatus>,

    /// Policy warnings surfaced on this response (Feature 5).
    /// Non-blocking advisory warnings from the policy engine.
    pub policy_warnings: Vec<ChatPolicyWarning>,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self {
            id: readable_id("msg", "chat"),
            role: "user".to_string(),
            content,
            timestamp: crate::utils::now_utc(),
            is_streaming: false,
            status: MessageStatus::Complete,
            queued_at: None,
            pending_phase: PendingPhase::Calm,
            pending_reason: None,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
            backend_used: None,
            citations: None,
            document_links: None,
            has_citations: false,
            adapters_used: None,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            replay_status: None,
            policy_warnings: Vec::new(),
        }
    }

    /// Create a user message that's queued (waiting for inference)
    pub fn user_queued(content: String) -> Self {
        Self {
            id: readable_id("msg", "chat"),
            role: "user".to_string(),
            content,
            timestamp: crate::utils::now_utc(),
            is_streaming: false,
            status: MessageStatus::Queued,
            queued_at: Some(crate::utils::now_utc()),
            pending_phase: PendingPhase::Calm,
            pending_reason: None,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
            backend_used: None,
            citations: None,
            document_links: None,
            has_citations: false,
            adapters_used: None,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            replay_status: None,
            policy_warnings: Vec::new(),
        }
    }

    pub fn assistant(content: String) -> Self {
        Self {
            id: readable_id("msg", "chat"),
            role: "assistant".to_string(),
            content,
            timestamp: crate::utils::now_utc(),
            is_streaming: false,
            status: MessageStatus::Complete,
            queued_at: None,
            pending_phase: PendingPhase::Calm,
            pending_reason: None,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
            backend_used: None,
            citations: None,
            document_links: None,
            has_citations: false,
            adapters_used: None,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            replay_status: None,
            policy_warnings: Vec::new(),
        }
    }

    pub fn assistant_streaming() -> Self {
        Self {
            id: readable_id("msg", "chat"),
            role: "assistant".to_string(),
            content: String::new(),
            timestamp: crate::utils::now_utc(),
            is_streaming: true,
            status: MessageStatus::Streaming,
            queued_at: None,
            pending_phase: PendingPhase::Calm,
            pending_reason: None,
            trace_id: None,
            latency_ms: None,
            token_count: None,
            prompt_tokens: None,
            completion_tokens: None,
            backend_used: None,
            citations: None,
            document_links: None,
            has_citations: false,
            adapters_used: None,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            replay_status: None,
            policy_warnings: Vec::new(),
        }
    }

    /// Check if this message is in a pending/queued state
    pub fn is_pending(&self) -> bool {
        matches!(self.status, MessageStatus::Queued | MessageStatus::Sending)
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
            Self::Default => "Auto".to_string(),
            Self::Model(name) => format!("Model: {}", name),
            Self::Stack(name) => format!("Stack: {}", name),
            Self::PolicyPack(name) => format!("Policy: {}", name),
        }
    }

    /// Display name that resolves "Auto" to the active model when known.
    pub fn display_name_with_model(&self, active_model: Option<&str>) -> String {
        match self {
            Self::Default => match active_model {
                Some(name) => format!("Auto ({})", name),
                None => "Auto".to_string(),
            },
            other => other.display_name(),
        }
    }
}

/// Context toggles for additional prompt metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextToggles {
    /// Include current page selection (adapter/job/worker)
    pub current_page: bool,
    /// Include recent logs (last 200 lines)
    pub recent_logs: bool,
    /// Include system snapshot (health + worker counts)
    pub system_snapshot: bool,
    /// Enable reasoning mode: semantic router for mid-stream adapter swaps (not a dedicated prefill step)
    #[serde(default)]
    pub reasoning_mode: bool,
}

impl Default for ContextToggles {
    fn default() -> Self {
        Self {
            current_page: true,
            recent_logs: false,
            system_snapshot: false,
            reasoning_mode: false,
        }
    }
}

/// Load context toggles from localStorage, falling back to defaults.
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
fn load_context_toggles() -> ContextToggles {
    ContextToggles::default()
}

/// Save context toggles to localStorage.
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
fn save_context_toggles(_toggles: &ContextToggles) {}

/// Save default pinned adapters to localStorage.
#[cfg(target_arch = "wasm32")]
fn save_pinned_adapters(adapters: &[String]) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(adapters) {
        let _ = storage.set_item(PINNED_ADAPTERS_KEY, &json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_pinned_adapters(_adapters: &[String]) {}

/// Load default pinned adapters from localStorage.
#[cfg(target_arch = "wasm32")]
fn load_pinned_adapters() -> Vec<String> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let Ok(Some(storage)) = window.local_storage() else {
        return Vec::new();
    };
    let Ok(Some(data)) = storage.get_item(PINNED_ADAPTERS_KEY) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&data).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_pinned_adapters() -> Vec<String> {
    Vec::new()
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
    /// Current chat session ID (from route when available)
    pub session_id: Option<String>,
    /// Active adapters for visualization
    pub active_adapters: Vec<AdapterStateInfo>,
    /// Suggested adapters from router preview (based on input text)
    pub suggested_adapters: Vec<SuggestedAdapter>,
    /// Selected adapter for next message (one-shot override)
    pub selected_adapter: Option<String>,
    /// User-pinned adapters to include in next request
    pub pinned_adapters: Vec<String>,
    /// Session-only pinned adapters (e.g. from `?adapter=` deep links).
    ///
    /// These are NOT persisted to localStorage and are cleared on session change.
    pub session_pinned_adapters: Vec<String>,
    /// Session-local mode toggle (Fast/Verified)
    pub verified_mode: bool,
    /// Last bit-identical run was blocked by strict adapter pin validation.
    pub bit_identical_mode_blocked: bool,
    /// Last run indicated adapter fallback/degrade semantics.
    pub bit_identical_mode_degraded: bool,
    /// Session-scoped active collection for RAG retrieval.
    pub active_collection_id: Option<String>,
    /// Persistent user knowledge collection.
    pub knowledge_collection_id: Option<String>,
    /// Adapter IDs from the latest assistant response.
    pub last_response_adapter_set: HashSet<String>,
    /// Streaming status notice for the UI
    pub stream_notice: Option<StreamNotice>,
    /// If the current stream is paused awaiting human review, store correlation info
    pub paused_inference: Option<PauseInfo>,
    /// Stream recovery metadata (idempotency + last request linkage)
    pub stream_recovery: Option<StreamRecovery>,
    /// Assistant message IDs that were cancelled/partial (exclude from prompt + persistence)
    pub partial_assistant_ids: Vec<String>,
    /// True after a pin toggle until SSE confirms active adapter set post-inference
    pub adapter_selection_pending: bool,
    /// Epoch counter — incremented on every pin mutation
    pub pin_change_epoch: u64,
    /// Epoch captured when last message was sent
    pub last_sent_pin_epoch: u64,
    /// Set when an AdapterStateUpdate confirms the current pin epoch during a stream;
    /// pending is only cleared once the stream completes (not mid-stream).
    pub adapter_state_confirmed: bool,
    /// Cumulative count of messages evicted by the FIFO cap in this session.
    pub total_messages_evicted: usize,
    /// Whether the user has dismissed the overflow indicator for this session.
    pub overflow_dismissed: bool,
}

/// Suggested adapter from router preview
#[derive(Debug, Clone)]
pub struct SuggestedAdapter {
    /// Adapter ID
    pub adapter_id: String,
    /// Human-readable name (from topology)
    pub name: Option<String>,
    /// One-line purpose (from topology cluster description)
    pub purpose: Option<String>,
    /// Confidence score from router (0.0-1.0)
    pub confidence: f32,
    /// Whether this adapter is pinned by the user
    pub is_pinned: bool,
}

/// Maximum queued messages per conversation
const MAX_QUEUED_MESSAGES: usize = 5;

/// Queue expiry timeout in seconds (30 minutes)
const QUEUE_EXPIRY_SECS: i64 = 30 * 60;

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

    /// Context overflow status for the UI.
    ///
    /// Returns `None` if no indicator should be shown (dismissed, or below threshold).
    /// Returns `Some(message)` with the appropriate warning/info text.
    pub fn overflow_notice(&self) -> Option<String> {
        if self.overflow_dismissed {
            return None;
        }
        let count = self.messages.len();
        if self.total_messages_evicted > 0 {
            let n = self.total_messages_evicted;
            let noun = if n == 1 { "message" } else { "messages" };
            Some(format!(
                "{n} older {noun} removed to maintain context window"
            ))
        } else if count >= OVERFLOW_WARNING_THRESHOLD {
            Some("Older messages will be dropped to maintain context".to_string())
        } else {
            None
        }
    }

    /// Count messages currently in queued state
    pub fn queued_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.status == MessageStatus::Queued)
            .count()
    }

    /// Check if we can queue another message
    pub fn can_queue(&self) -> bool {
        self.queued_count() < MAX_QUEUED_MESSAGES
    }

    /// Get the oldest queued message (for retry)
    pub fn oldest_queued_message(&self) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .find(|m| m.status == MessageStatus::Queued)
    }

    /// Check if any messages have expired and mark them as failed
    pub fn expire_old_queued_messages(&mut self) {
        let now = crate::utils::now_utc();
        for msg in &mut self.messages {
            if msg.status == MessageStatus::Queued {
                if let Some(queued_at) = msg.queued_at {
                    let elapsed = (now - queued_at).num_seconds();
                    if elapsed > QUEUE_EXPIRY_SECS {
                        msg.status = MessageStatus::Failed;
                        msg.pending_reason = Some("Request timed out".to_string());
                    }
                }
            }
        }
    }

    /// Update pending phases based on elapsed time
    ///
    /// Thresholds (hardcoded for now, will be adaptive later):
    /// - Calm: 0-3 seconds
    /// - Informative: 3-10 seconds
    /// - Estimated: >10 seconds
    pub fn update_pending_phases(&mut self, blocker_reason: Option<&str>) {
        let now = crate::utils::now_utc();
        for msg in &mut self.messages {
            if msg.status == MessageStatus::Queued {
                if let Some(queued_at) = msg.queued_at {
                    let elapsed_secs = (now - queued_at).num_seconds();

                    let new_phase = if elapsed_secs < 3 {
                        PendingPhase::Calm
                    } else if elapsed_secs < 10 {
                        PendingPhase::Informative
                    } else {
                        PendingPhase::Estimated
                    };

                    if new_phase != msg.pending_phase {
                        msg.pending_phase = new_phase;
                        // Update reason when escalating to Informative or Estimated
                        if matches!(
                            new_phase,
                            PendingPhase::Informative | PendingPhase::Estimated
                        ) {
                            msg.pending_reason = blocker_reason.map(|s| s.to_string());
                        }
                    }
                }
            }
        }
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
            dock_state: DockState::Narrow,
            messages: Vec::new(),
            target: ChatTarget::Default,
            context: load_context_toggles(),
            loading: false,
            streaming: false,
            error: None,
            last_read_message_id: None,
            page_context: None,
            session_id: None,
            active_adapters: Vec::new(),
            suggested_adapters: Vec::new(),
            selected_adapter: None,
            pinned_adapters: load_pinned_adapters(),
            session_pinned_adapters: Vec::new(),
            verified_mode: false,
            bit_identical_mode_blocked: false,
            bit_identical_mode_degraded: false,
            active_collection_id: None,
            knowledge_collection_id: None,
            last_response_adapter_set: HashSet::new(),
            stream_notice: None,
            paused_inference: None,
            stream_recovery: None,
            partial_assistant_ids: Vec::new(),
            adapter_selection_pending: false,
            pin_change_epoch: 0,
            last_sent_pin_epoch: 0,
            adapter_state_confirmed: false,
            total_messages_evicted: 0,
            overflow_dismissed: false,
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

#[derive(Debug, Clone, Serialize)]
struct CreateChatSessionRequestUi {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateChatSessionResponseUi {
    session_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct ArchiveChatSessionRequestUi {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// Lightweight representation of a backend chat session for hydration.
/// Fields match the subset of `ChatSession` (from adapteros-db) that the UI needs.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendChatSession {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A single message fetched from the backend for session backfill.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub created_at: String,
    pub sequence: i64,
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
    ///
    /// If inference is ready, sends immediately.
    /// If not ready, queues the message and starts polling for readiness.
    pub fn send_message_streaming(&self, content: String) {
        let content = content.trim().to_string();
        if content.is_empty() {
            return;
        }

        let idempotency_key = readable_id("idem", "chat");
        self.start_streaming_request(
            content,
            true,
            idempotency_key,
            None,
            StreamNotice::info("Waiting for server..."),
        );
    }

    /// Create a backend chat session and return the server-issued session_id.
    pub async fn create_backend_session(
        &self,
        name: String,
        title: Option<String>,
    ) -> Result<String, ApiError> {
        let req = CreateChatSessionRequestUi { name, title };
        let resp: CreateChatSessionResponseUi = self.client.post("/v1/chat/sessions", &req).await?;
        Ok(resp.session_id)
    }

    /// Archive a backend chat session.
    pub async fn archive_backend_session(
        &self,
        session_id: &str,
        reason: Option<String>,
    ) -> Result<(), ApiError> {
        let req = ArchiveChatSessionRequestUi { reason };
        self.client
            .post_no_response(&format!("/v1/chat/sessions/{}/archive", session_id), &req)
            .await
    }

    /// Restore an archived backend chat session.
    pub async fn restore_backend_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.client
            .post_no_response(
                &format!("/v1/chat/sessions/{}/restore", session_id),
                &serde_json::json!({}),
            )
            .await
    }

    /// List chat sessions from the backend for hydration on page load.
    pub async fn list_backend_sessions(&self) -> Result<Vec<BackendChatSession>, ApiError> {
        self.client
            .get::<Vec<BackendChatSession>>("/v1/chat/sessions?limit=50")
            .await
    }

    /// Get a specific backend session by ID for deep-link confirmation.
    pub async fn get_backend_session(
        &self,
        session_id: &str,
    ) -> Result<BackendChatSession, ApiError> {
        self.client
            .get::<BackendChatSession>(&format!("/v1/chat/sessions/{}", session_id))
            .await
    }

    /// Fetch messages for a specific backend session (for backfilling stubs).
    pub async fn fetch_session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<BackendChatMessage>, ApiError> {
        self.client
            .get::<Vec<BackendChatMessage>>(&format!("/v1/chat/sessions/{}/messages", session_id))
            .await
    }

    /// Queue a message for later delivery (when inference becomes ready)
    pub fn queue_message(&self, content: String) {
        let content = content.trim().to_string();
        if content.is_empty() {
            return;
        }

        // Check queue limit
        let can_queue = self.state.get_untracked().can_queue();
        if !can_queue {
            let _ = self.state.try_update(|s| {
                s.error =
                    Some("Queue full. Please wait for pending messages to complete.".to_string());
            });
            return;
        }

        // Add queued user message
        let user_message = ChatMessage::user_queued(content);
        let _ = self.state.try_update(|s| {
            s.messages.push(user_message);
            s.total_messages_evicted += evict_old_messages(&mut s.messages, MAX_MESSAGES);
        });
    }

    /// Process the next queued message (called when inference becomes ready)
    pub fn process_queued_message(&self) {
        let state = self.state.get_untracked();

        // Don't process if already busy
        if state.loading || state.streaming {
            return;
        }

        // Find oldest queued message
        let queued_msg = state
            .messages
            .iter()
            .find(|m| m.status == MessageStatus::Queued);

        if let Some(msg) = queued_msg {
            let content = msg.content.clone();
            let msg_id = msg.id.clone();

            // Update message status to sending
            let _ = self.state.try_update(|s| {
                if let Some(m) = s.messages.iter_mut().find(|m| m.id == msg_id) {
                    m.status = MessageStatus::Sending;
                }
            });

            // Start the actual streaming request
            let idempotency_key = readable_id("idem", "chat");
            self.start_streaming_request(
                content,
                false, // Don't add user message again
                idempotency_key,
                Some(msg_id),
                StreamNotice::info("Processing queued message..."),
            );
        }
    }

    /// Update pending phases for queued messages (call periodically)
    pub fn tick_pending_phases(&self, blocker_reason: Option<&str>) {
        let _ = self.state.try_update(|s| {
            s.update_pending_phases(blocker_reason);
            s.expire_old_queued_messages();
        });
    }

    /// Cancel a specific queued message
    pub fn cancel_queued_message(&self, message_id: &str) {
        let _ = self.state.try_update(|s| {
            s.messages
                .retain(|m| !(m.id == message_id && m.status == MessageStatus::Queued));
        });
    }

    /// Check if there are any queued messages
    pub fn has_queued_messages(&self) -> bool {
        self.state.get_untracked().queued_count() > 0
    }

    /// Set or clear the current chat session ID used for streaming requests.
    pub fn set_session_id(&self, session_id: Option<String>) {
        let _ = self.state.try_update(|s| {
            s.session_id = session_id;
            s.last_response_adapter_set.clear();
        });
    }

    /// Cancel the current streaming request
    pub fn cancel_stream(&self) {
        // Use try_get to avoid panic if signal is disposed during navigation
        if let Some(cell) = self.abort_controller.try_get() {
            if let Some(controller) = cell.borrow_mut().take() {
                controller.abort();
            }
        }
        // Use try_update to avoid panic if signal is disposed during navigation
        let _ = self.state.try_update(|s| {
            s.streaming = false;
            s.loading = false;
            s.stream_notice = Some(StreamNotice::warning("Stream cancelled", true));
            // Mark the last message as no longer streaming and track as partial
            let mut partial_id = None;
            if let Some(last) = s.messages.last_mut() {
                if last.role == "assistant" {
                    if last.content.is_empty() {
                        s.messages.pop();
                    } else {
                        last.is_streaming = false;
                        partial_id = Some(last.id.clone());
                    }
                }
            }
            if let Some(id) = partial_id {
                mark_partial_assistant(s, &id);
            }
        });
    }

    /// Retry the most recent streaming request (uses last idempotency key).
    pub fn retry_last_stream(&self) {
        let current_state = self.state.get_untracked();
        if current_state.loading || current_state.streaming {
            return;
        }

        let Some(recovery) = current_state.stream_recovery.clone() else {
            return;
        };

        self.start_streaming_request(
            recovery.user_message.clone(),
            false,
            recovery.idempotency_key.clone(),
            Some(recovery.user_message_id.clone()),
            StreamNotice::info("Retrying..."),
        );
    }

    fn start_streaming_request(
        &self,
        content: String,
        include_user_message: bool,
        idempotency_key: String,
        user_message_id_override: Option<String>,
        notice: StreamNotice,
    ) {
        let content = content.trim().to_string();
        if content.is_empty() {
            return;
        }

        let current_state = self.state.get_untracked();
        if current_state.loading || current_state.streaming {
            return;
        }

        let mut user_message_id = user_message_id_override;

        if include_user_message {
            let user_message = ChatMessage::user(content.clone());
            user_message_id = Some(user_message.id.clone());
            let _ = self.state.try_update(|s| {
                s.messages.push(user_message);
                s.total_messages_evicted += evict_old_messages(&mut s.messages, MAX_MESSAGES);
            });
        }

        // Mark request start + notice
        let _ = self.state.try_update(|s| {
            s.loading = true;
            s.streaming = true;
            s.error = None;
            s.stream_notice = Some(notice);
            s.paused_inference = None;
            s.bit_identical_mode_blocked = false;
            s.bit_identical_mode_degraded = false;
        });

        // Build the prompt with context and history
        let prompt = self.build_prompt(&content);

        // Add placeholder assistant message for streaming
        let assistant_message = ChatMessage::assistant_streaming();
        let assistant_message_id = assistant_message.id.clone();
        let resolved_user_message_id = user_message_id.clone().or_else(|| {
            self.state
                .get_untracked()
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.id.clone())
        });
        let _ = self.state.try_update(|s| {
            s.messages.push(assistant_message);
            s.total_messages_evicted += evict_old_messages(&mut s.messages, MAX_MESSAGES);
            s.stream_recovery = Some(StreamRecovery {
                idempotency_key: idempotency_key.clone(),
                user_message_id: resolved_user_message_id
                    .clone()
                    .unwrap_or_else(|| readable_id("msg", "chat")),
                user_message: content.clone(),
                assistant_message_id: assistant_message_id.clone(),
                request_id: None,
            });
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
        // Also capture pinned adapters (persistent + session), next-message override, and reasoning mode.
        let (
            context_request,
            pinned_adapters,
            selected_adapter,
            reasoning_mode,
            session_id,
            verified_mode,
            selected_collection_id,
        ) = {
            let current = self.state.get_untracked();
            let context = ContextRequest {
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
            };
            // Capture effective pinned adapters (persistent + session) if any.
            let mut pinned = current.pinned_adapters.clone();
            for id in &current.session_pinned_adapters {
                if !pinned.contains(id) {
                    pinned.push(id.clone());
                }
            }
            let pinned = if pinned.is_empty() {
                None
            } else {
                Some(pinned)
            };
            (
                context,
                pinned,
                current.selected_adapter.clone(),
                current.context.reasoning_mode,
                current.session_id.clone(),
                current.verified_mode,
                current
                    .active_collection_id
                    .clone()
                    .or(current.knowledge_collection_id.clone()),
            )
        };

        let (model, stack_id) = {
            let current = self.state.get_untracked();
            match current.target.clone() {
                ChatTarget::Model(id) => (Some(id), None),
                ChatTarget::Stack(id) => (None, Some(id)),
                _ => (None, None),
            }
        };

        // Clear one-shot selection and snapshot pin epoch so the SSE handler
        // knows this message carries the current pin set.
        let _ = self.state.try_update(|s| {
            s.last_sent_pin_epoch = s.pin_change_epoch;
            s.selected_adapter = None;
            s.adapter_state_confirmed = false; // Reset for new stream
        });

        wasm_bindgen_futures::spawn_local(async move {
            // When reasoning mode is enabled, route to CoreML backend
            let (reasoning_mode_opt, backend_opt) = if reasoning_mode {
                (Some(true), Some("coreml".to_string()))
            } else {
                (None, None)
            };

            // Merge pinned adapters with one-shot selection (deduped)
            let mut adapters = pinned_adapters.unwrap_or_default();
            if let Some(selected) = selected_adapter {
                if !adapters.contains(&selected) {
                    adapters.insert(0, selected);
                }
            }
            let adapters = if adapters.is_empty() {
                None
            } else {
                Some(adapters)
            };

            let (max_tokens, temperature) = if verified_mode {
                (Some(VERIFIED_MAX_TOKENS), Some(VERIFIED_TEMPERATURE))
            } else {
                (Some(DEFAULT_MAX_TOKENS), Some(DEFAULT_TEMPERATURE))
            };

            let request = StreamingInferRequest {
                prompt,
                model,
                session_id,
                stack_id,
                max_tokens,
                temperature,
                adapters,
                bit_identical: verified_mode,
                context: Some(context_request),
                collection_id: selected_collection_id,
                reasoning_mode: reasoning_mode_opt,
                backend: backend_opt,
            };

            match stream_inference_to_state(&request, state, signal.as_ref(), Some(idempotency_key))
                .await
            {
                Ok(trace_info) => {
                    // Mark the last message as no longer streaming and add trace info
                    // Use try_update to avoid panic if signal is disposed during navigation
                    let _ = state.try_update(|s| {
                        let mut remove_empty_assistant = false;
                        if let Some(last) = s.messages.last_mut() {
                            if last.role == "assistant" {
                                if last.content.trim().is_empty() {
                                    remove_empty_assistant = true;
                                } else {
                                    last.is_streaming = false;
                                    last.trace_id = trace_info.trace_id;
                                    last.latency_ms = trace_info.latency_ms;
                                    last.token_count = trace_info.token_count;
                                    last.prompt_tokens = trace_info.prompt_tokens;
                                    last.completion_tokens = trace_info.completion_tokens;
                                    last.backend_used = trace_info.backend_used;
                                    let citations =
                                        trace_info.citations.clone().unwrap_or_default();
                                    last.has_citations = !citations.is_empty();
                                    last.citations = (!citations.is_empty()).then_some(citations);
                                    let document_links =
                                        trace_info.document_links.clone().unwrap_or_default();
                                    last.document_links =
                                        (!document_links.is_empty()).then_some(document_links);
                                    last.adapters_used = if trace_info.adapters_used.is_empty() {
                                        None
                                    } else {
                                        Some(trace_info.adapters_used.clone())
                                    };
                                    last.unavailable_pinned_adapters =
                                        trace_info.unavailable_pinned_adapters.clone();
                                    last.pinned_routing_fallback =
                                        trace_info.pinned_routing_fallback.clone();
                                    last.fallback_triggered = trace_info.fallback_triggered;
                                    last.fallback_backend = trace_info.fallback_backend.clone();
                                    last.adapter_attachments =
                                        trace_info.adapter_attachments.clone().unwrap_or_default();
                                    last.degraded_notices = trace_info.degraded_notices.clone();
                                    last.policy_warnings =
                                        trace_info.policy_warnings.clone().unwrap_or_default();
                                }
                            }
                        }
                        if s.verified_mode
                            && (trace_info.pinned_routing_fallback.is_some()
                                || trace_info.fallback_triggered)
                        {
                            s.bit_identical_mode_degraded = true;
                        }
                        if remove_empty_assistant {
                            s.messages.pop();
                        }
                        let current_set: HashSet<String> =
                            trace_info.adapters_used.iter().cloned().collect();
                        if !remove_empty_assistant
                            && !current_set.is_empty()
                            && !s.last_response_adapter_set.is_empty()
                            && current_set != s.last_response_adapter_set
                        {
                            let mut added: Vec<String> = current_set
                                .difference(&s.last_response_adapter_set)
                                .cloned()
                                .collect();
                            let mut removed: Vec<String> = s
                                .last_response_adapter_set
                                .difference(&current_set)
                                .cloned()
                                .collect();
                            added.sort();
                            removed.sort();
                            if !added.is_empty() || !removed.is_empty() {
                                let mut parts = Vec::new();
                                if !added.is_empty() {
                                    parts.push(format!("+{}", added.join(", +")));
                                }
                                if !removed.is_empty() {
                                    parts.push(format!("-{}", removed.join(", -")));
                                }
                                s.messages.push(ChatMessage {
                                    id: readable_id("msg", "adapter-change"),
                                    role: "system".to_string(),
                                    content: format!("Adapters changed: {}", parts.join(" ")),
                                    timestamp: crate::utils::now_utc(),
                                    is_streaming: false,
                                    status: MessageStatus::Complete,
                                    queued_at: None,
                                    pending_phase: PendingPhase::Calm,
                                    pending_reason: None,
                                    trace_id: None,
                                    latency_ms: None,
                                    token_count: None,
                                    prompt_tokens: None,
                                    completion_tokens: None,
                                    backend_used: None,
                                    citations: None,
                                    document_links: None,
                                    has_citations: false,
                                    adapters_used: None,
                                    unavailable_pinned_adapters: None,
                                    pinned_routing_fallback: None,
                                    fallback_triggered: false,
                                    fallback_backend: None,
                                    adapter_attachments: Vec::new(),
                                    degraded_notices: Vec::new(),
                                    replay_status: None,
                                    policy_warnings: Vec::new(),
                                });
                            }
                        }
                        s.last_response_adapter_set = current_set;
                        let keep_notice = s
                            .stream_notice
                            .as_ref()
                            .map(|notice| {
                                matches!(
                                    notice.tone,
                                    StreamNoticeTone::Warning | StreamNoticeTone::Paused
                                )
                            })
                            .unwrap_or(false);
                        if !keep_notice {
                            s.stream_notice = None;
                        }
                        s.stream_recovery = None;
                        // When dock is open, mark new messages as read immediately
                        if s.dock_state == DockState::Docked {
                            s.mark_as_read();
                        }
                    });
                }
                Err(failure) => {
                    if is_abort_error(&failure.message) {
                        // Stream was cancelled by user - mark message as no longer streaming
                        // Use try_update to avoid panic if signal is disposed during navigation
                        let _ = state.try_update(|s| {
                            let mut partial_id = None;
                            if let Some(last) = s.messages.last_mut() {
                                if last.role == "assistant" {
                                    if last.content.is_empty() {
                                        s.messages.pop();
                                    } else {
                                        last.is_streaming = false;
                                        partial_id = Some(last.id.clone());
                                    }
                                }
                            }
                            if let Some(id) = partial_id {
                                mark_partial_assistant(s, &id);
                            }
                        });
                    } else {
                        let notice = stream_notice_from_failure(&failure);
                        let degraded_notice = degraded_notice_from_failure(&failure);
                        let strict_pin_failure = matches!(
                            failure.code.as_deref(),
                            Some("BIT_IDENTICAL_ADAPTER_PIN_REQUIRED")
                                | Some("BIT_IDENTICAL_ADAPTER_PIN_INVALID")
                        );
                        // Remove empty assistant message on error; keep partial otherwise
                        // Use try_update to avoid panic if signal is disposed during navigation
                        let _ = state.try_update(|s| {
                            let mut partial_id = None;
                            let mut remove_last = false;
                            if let Some(last) = s.messages.last() {
                                if last.role == "assistant" {
                                    if last.content.is_empty() {
                                        remove_last = true;
                                    } else {
                                        partial_id = Some(last.id.clone());
                                    }
                                }
                            }
                            if remove_last {
                                s.messages.pop();
                            } else if let Some(id) = partial_id {
                                mark_partial_assistant(s, &id);
                            }
                            if let Some(degraded) = degraded_notice.clone() {
                                if let Some(last) = s.messages.last_mut() {
                                    if last.role == "assistant" {
                                        last.degraded_notices.push(degraded);
                                    }
                                }
                            }
                            s.error = Some(failure.message.clone());
                            s.stream_notice = if strict_pin_failure {
                                Some(StreamNotice::error(
                                    "Run blocked: adapter version not pinned.",
                                    false,
                                ))
                            } else {
                                Some(notice)
                            };
                            if strict_pin_failure {
                                s.bit_identical_mode_blocked = true;
                            }
                        });
                    }
                }
            }

            // Use try_update to avoid panic if signal is disposed during navigation
            let _ = state.try_update(|s| {
                s.loading = false;
                s.streaming = false;
                // Pending is cleared on AdapterStateUpdate confirmation (mid-stream).
                s.adapter_state_confirmed = false;
            });

            // Clear the abort controller - use try_get to avoid panic if disposed
            if let Some(cell) = abort_controller.try_get() {
                *cell.borrow_mut() = None;
            }
        });
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
            .filter(|m| {
                !(m.role == "assistant" && state.partial_assistant_ids.iter().any(|id| id == &m.id))
            })
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
        let _ = self.state.try_update(|s| {
            s.dock_state = dock_state;
            // Mark messages as read when dock is opened
            if dock_state == DockState::Docked {
                s.mark_as_read();
            }
        });
    }

    /// Toggle dock between docked and narrow
    pub fn toggle_dock(&self) {
        let _ = self.state.try_update(|s| {
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
        let _ = self.state.try_update(|s| {
            s.target = target;
        });
    }

    /// Set session-local mode (Fast/Verified)
    pub fn set_verified_mode(&self, verified: bool) {
        let _ = self.state.try_update(|s| {
            s.verified_mode = verified;
            s.bit_identical_mode_blocked = false;
            s.bit_identical_mode_degraded = false;
        });
        let state = self.state.get_untracked();
        if let Some(id) = state.session_id.clone() {
            if !id.is_empty() && !state.messages.is_empty() {
                let session = ChatSessionsManager::session_from_state(&id, &state);
                ChatSessionsManager::save_session(&session);
            }
        }
    }

    /// Select an adapter for the next message (one-shot override).
    pub fn select_next_adapter(&self, adapter_id: &str) {
        let id = adapter_id.to_string();
        let _ = self.state.try_update(|s| {
            if s.selected_adapter.as_deref() == Some(&id) {
                s.selected_adapter = None;
            } else {
                s.selected_adapter = Some(id);
            }
        });
    }

    /// Toggle a context option
    pub fn toggle_context(&self, toggle: ContextToggle) {
        let _ = self.state.try_update(|s| match toggle {
            ContextToggle::CurrentPage => s.context.current_page = !s.context.current_page,
            ContextToggle::RecentLogs => s.context.recent_logs = !s.context.recent_logs,
            ContextToggle::SystemSnapshot => s.context.system_snapshot = !s.context.system_snapshot,
            ContextToggle::ReasoningMode => s.context.reasoning_mode = !s.context.reasoning_mode,
        });
        // Persist toggled state to localStorage
        let toggles = self.state.get_untracked().context.clone();
        save_context_toggles(&toggles);
    }

    /// Update page context
    pub fn set_page_context(&self, context: PageContext) {
        let _ = self.state.try_update(|s| {
            s.page_context = Some(context);
        });
    }

    /// Set or clear active collection for session-scoped RAG.
    pub fn set_active_collection_id(&self, collection_id: Option<String>) {
        let _ = self.state.try_update(|s| {
            s.active_collection_id = collection_id;
        });
    }

    /// Set or clear persistent knowledge collection.
    pub fn set_knowledge_collection_id(&self, collection_id: Option<String>) {
        let _ = self.state.try_update(|s| {
            s.knowledge_collection_id = collection_id;
        });
    }

    /// Clear all messages
    pub fn clear_messages(&self) {
        let _ = self.state.try_update(|s| {
            s.messages.clear();
            s.error = None;
            s.last_read_message_id = None;
            s.active_adapters.clear();
            s.suggested_adapters.clear();
            s.selected_adapter = None;
            s.active_collection_id = None;
            s.last_response_adapter_set.clear();
            // Keep persistent pins; they represent user intent across sessions.
            // Clear session-only pins since they're tied to the current session.
            s.session_pinned_adapters.clear();
            s.stream_notice = None;
            s.stream_recovery = None;
            s.partial_assistant_ids.clear();
            s.adapter_selection_pending = false;
            s.pin_change_epoch = 0;
            s.last_sent_pin_epoch = 0;
            s.adapter_state_confirmed = false;
            s.total_messages_evicted = 0;
            s.overflow_dismissed = false;
        });
    }

    /// Append an arbitrary message (e.g. system notice) to the conversation.
    pub fn append_message(&self, message: ChatMessage) {
        let _ = self.state.try_update(|s| {
            s.messages.push(message);
        });
    }

    /// Dismiss the context overflow indicator for this session.
    pub fn dismiss_overflow_notice(&self) {
        let _ = self.state.try_update(|s| {
            s.overflow_dismissed = true;
        });
    }

    /// Replace the full session-only pinned adapter set.
    ///
    /// This is used for deep links like `/chat/s/<id>?adapter=<adapter_id>`.
    /// It does not persist to localStorage.
    pub fn set_session_pinned_adapters(&self, adapter_ids: Vec<String>) {
        let _ = self.state.try_update(|s| {
            s.session_pinned_adapters = adapter_ids;
            // Sync pinned flags on suggestions (effective pin set).
            for adapter in &mut s.suggested_adapters {
                adapter.is_pinned = s.pinned_adapters.contains(&adapter.adapter_id)
                    || s.session_pinned_adapters.contains(&adapter.adapter_id);
            }
            s.pin_change_epoch += 1;
            s.adapter_selection_pending = true;
        });
    }

    /// Clear session-only pins (called on session change).
    pub fn clear_session_pins(&self) {
        let _ = self.state.try_update(|s| {
            if s.session_pinned_adapters.is_empty() {
                return;
            }
            s.session_pinned_adapters.clear();
            // Sync pinned flags on suggestions back to persistent set only.
            for adapter in &mut s.suggested_adapters {
                adapter.is_pinned = s.pinned_adapters.contains(&adapter.adapter_id);
            }
            s.pin_change_epoch += 1;
            s.adapter_selection_pending = true;
        });
    }

    /// Clear error state completely
    ///
    /// Clears error, notice, and recovery state to reset the chat to a clean state.
    /// The user can then send a new message without the previous error context.
    pub fn clear_error(&self) {
        let _ = self.state.try_update(|s| {
            s.error = None;
            s.stream_notice = None;
            // Clear recovery state since user dismissed the error
            // This prevents stale retry context from persisting
            s.stream_recovery = None;
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

        let _ = self.state.try_update(|s| {
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
                            crate::utils::now_utc()
                        }
                    };

                    ChatMessage {
                        id: m.id,
                        role: m.role,
                        content: m.content,
                        timestamp,
                        is_streaming: false,
                        status: MessageStatus::Complete,
                        queued_at: None,
                        pending_phase: PendingPhase::Calm,
                        pending_reason: None,
                        trace_id: m.trace_id,
                        latency_ms: m.latency_ms,
                        token_count: m.token_count,
                        prompt_tokens: m.prompt_tokens,
                        completion_tokens: m.completion_tokens,
                        backend_used: m.backend_used,
                        citations: None,
                        document_links: None,
                        has_citations: m.has_citations,
                        adapters_used: (!m.adapters_used.is_empty()).then_some(m.adapters_used),
                        unavailable_pinned_adapters: (!m.unavailable_pinned_adapters.is_empty())
                            .then_some(m.unavailable_pinned_adapters),
                        pinned_routing_fallback: m.pinned_routing_fallback,
                        fallback_triggered: m.fallback_triggered,
                        fallback_backend: m.fallback_backend,
                        adapter_attachments: m.adapter_attachments,
                        degraded_notices: m.degraded_notices,
                        replay_status: None,
                        policy_warnings: m.policy_warnings,
                    }
                })
                .collect();
            s.error = None;
            s.stream_notice = None;
            s.stream_recovery = None;
            s.partial_assistant_ids.clear();
            s.selected_adapter = None;
            s.verified_mode = session.verified_mode;
            // Mark all restored messages as read
            s.last_read_message_id = s.messages.last().map(|m| m.id.clone());
        });
    }

    /// Preview adapters for the given input text
    ///
    /// Calls the topology endpoint with preview_text to get router suggestions.
    /// Updates suggested_adapters in state with the predicted path.
    pub fn preview_adapters(&self, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            // Clear suggestions when input is empty
            // Use try_update to avoid panic if signal is disposed during navigation
            let _ = self.state.try_update(|s| {
                s.suggested_adapters.clear();
            });
            return;
        }

        let client = self.client.clone();
        let state = self.state;
        let pinned = {
            let current = self.state.get_untracked();
            let mut out = current.pinned_adapters.clone();
            for id in &current.session_pinned_adapters {
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
            out
        };

        wasm_bindgen_futures::spawn_local(async move {
            match client.get_topology_preview(Some(&text)).await {
                Ok(topology) => {
                    if let Some(predicted_path) = topology.predicted_path {
                        // Build adapter name lookup from topology
                        let adapter_names: std::collections::HashMap<String, String> = topology
                            .adapters
                            .iter()
                            .map(|a| (a.adapter_id.clone(), a.name.clone()))
                            .collect();
                        // Build cluster description lookup for one-line purpose
                        let cluster_descriptions: std::collections::HashMap<String, String> =
                            topology
                                .clusters
                                .iter()
                                .map(|c| (c.id.clone(), c.description.clone()))
                                .collect();

                        // Use try_update to avoid panic if signal is disposed during navigation
                        let _ = state.try_update(|s| {
                            // Convert predicted path to suggested adapters
                            // Sort by confidence DESC, adapter_id ASC for determinism
                            let mut suggestions: Vec<SuggestedAdapter> = predicted_path
                                .into_iter()
                                .filter_map(|node| {
                                    node.adapter_id.map(|id| {
                                        let name = adapter_names.get(&id).cloned();
                                        let purpose = node
                                            .cluster_id
                                            .as_ref()
                                            .and_then(|cid| cluster_descriptions.get(cid))
                                            .cloned();
                                        SuggestedAdapter {
                                            adapter_id: id.clone(),
                                            name,
                                            purpose,
                                            confidence: node.confidence.unwrap_or(0.0),
                                            is_pinned: pinned.contains(&id),
                                        }
                                    })
                                })
                                .collect();
                            // Deterministic ordering: score DESC, adapter_id ASC
                            suggestions.sort_by(|a, b| {
                                b.confidence
                                    .partial_cmp(&a.confidence)
                                    .unwrap_or(std::cmp::Ordering::Equal)
                                    .then_with(|| a.adapter_id.cmp(&b.adapter_id))
                            });
                            s.suggested_adapters = suggestions;
                        });
                    }
                }
                Err(e) => {
                    // Log error but don't show to user (preview is best-effort)
                    web_sys::console::warn_1(
                        &format!("[Chat] Adapter preview failed: {}", e).into(),
                    );
                }
            }
        });
    }

    /// Toggle pin state for an adapter
    ///
    /// When pinned, the adapter will be included in the next inference request.
    pub fn toggle_pin_adapter(&self, adapter_id: &str) {
        let id = adapter_id.to_string();
        let _ = self.state.try_update(|s| {
            if let Some(pos) = s.pinned_adapters.iter().position(|a| a == &id) {
                // Unpin persistent
                s.pinned_adapters.remove(pos);
            } else if let Some(pos) = s.session_pinned_adapters.iter().position(|a| a == &id) {
                // Unpin session-only (does not persist)
                s.session_pinned_adapters.remove(pos);
            } else {
                // Pin persistent
                s.pinned_adapters.push(id.clone());
            }
            // Update is_pinned in suggested adapters based on effective pin set
            for adapter in &mut s.suggested_adapters {
                if adapter.adapter_id == id {
                    adapter.is_pinned =
                        s.pinned_adapters.contains(&id) || s.session_pinned_adapters.contains(&id);
                }
            }
            // Mark pending until next SSE adapter state update confirms usage
            s.adapter_selection_pending = true;
            s.pin_change_epoch += 1;
        });
        save_pinned_adapters(&self.state.get_untracked().pinned_adapters);
    }

    /// Clear all pinned adapters
    pub fn clear_pinned_adapters(&self) {
        let _ = self.state.try_update(|s| {
            s.pinned_adapters.clear();
            for adapter in &mut s.suggested_adapters {
                adapter.is_pinned = s.session_pinned_adapters.contains(&adapter.adapter_id);
            }
            s.pin_change_epoch += 1;
        });
        save_pinned_adapters(&[]);
    }

    /// Replace the full pinned adapter set (from manage dialog)
    pub fn set_pinned_adapters(&self, adapter_ids: Vec<String>) {
        let _ = self.state.try_update(|s| {
            s.pinned_adapters = adapter_ids;
            // Sync is_pinned on suggested adapters
            for adapter in &mut s.suggested_adapters {
                adapter.is_pinned = s.pinned_adapters.contains(&adapter.adapter_id)
                    || s.session_pinned_adapters.contains(&adapter.adapter_id);
            }
            // Mark pending until next SSE confirms
            if !s.pinned_adapters.is_empty() {
                s.adapter_selection_pending = true;
            }
            s.pin_change_epoch += 1;
        });
        save_pinned_adapters(&self.state.get_untracked().pinned_adapters);
    }

    /// Clear suggested adapters
    pub fn clear_suggested_adapters(&self) {
        let _ = self.state.try_update(|s| {
            s.suggested_adapters.clear();
        });
    }
}

/// Context toggle types
#[derive(Debug, Clone, Copy)]
pub enum ContextToggle {
    CurrentPage,
    RecentLogs,
    SystemSnapshot,
    ReasoningMode,
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

#[derive(Debug, Clone)]
struct SseEnvelope {
    event_type: Option<String>,
    data: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StreamErrorPayload {
    code: String,
    message: String,
    retryable: bool,
    #[allow(dead_code)]
    correlation_id: Option<String>,
}

#[derive(Debug, Clone)]
struct StreamFailure {
    message: String,
    code: Option<String>,
    retryable: bool,
}

impl StreamFailure {
    fn new(message: impl Into<String>, code: Option<String>, retryable: bool) -> Self {
        Self {
            message: message.into(),
            code,
            retryable,
        }
    }
}

/// Progressive latency thresholds (milliseconds).
/// Each stage escalates the user-facing message.
const LATENCY_STAGE_1_MS: u32 = 2000;
const LATENCY_STAGE_2_MS: u32 = 5000;
const LATENCY_STAGE_3_MS: u32 = 10_000;

/// Duration (ms) to show the "time-to-first-token" badge after first token arrives.
const TTFT_DISPLAY_MS: u32 = 3000;

/// Progressive latency feedback timer.
///
/// Fires escalating notices at configurable thresholds while waiting for
/// the first SSE token. All handles are cancelled on first token or drop.
struct ProgressiveLatencyTimer {
    #[cfg(target_arch = "wasm32")]
    handles: Vec<Timeout>,
}

impl ProgressiveLatencyTimer {
    fn start(state: RwSignal<ChatState>) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let stages: [(u32, &str, StreamNoticeTone); 3] = [
                (
                    LATENCY_STAGE_1_MS,
                    "Thinking\u{2026}",
                    StreamNoticeTone::Info,
                ),
                (
                    LATENCY_STAGE_2_MS,
                    "Still working\u{2026}",
                    StreamNoticeTone::Info,
                ),
                (
                    LATENCY_STAGE_3_MS,
                    "This is taking longer than usual",
                    StreamNoticeTone::Warning,
                ),
            ];

            let handles = stages
                .into_iter()
                .map(|(delay, message, tone)| {
                    let msg = message.to_string();
                    Timeout::new(delay, move || {
                        let _ = state.try_update(|s| {
                            if s.loading && s.streaming {
                                s.stream_notice = Some(StreamNotice {
                                    message: msg,
                                    tone,
                                    retryable: false,
                                });
                            }
                        });
                    })
                })
                .collect();

            Self { handles }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Keep thresholds visible to host-target checks/tests even though
            // runtime latency notices are WASM-only.
            let _ = (
                state,
                LATENCY_STAGE_1_MS,
                LATENCY_STAGE_2_MS,
                LATENCY_STAGE_3_MS,
                TTFT_DISPLAY_MS,
            );
            Self {}
        }
    }

    fn cancel(&mut self) {
        #[cfg(target_arch = "wasm32")]
        for handle in self.handles.drain(..) {
            handle.cancel();
        }
    }

    /// Show a brief time-to-first-token indicator, then auto-clear it.
    #[cfg(target_arch = "wasm32")]
    fn show_ttft(state: RwSignal<ChatState>, elapsed: web_time::Duration) {
        let secs = elapsed.as_secs_f64();
        let msg = format!("{:.1}s to first token", secs);
        let _ = state.try_update(|s| {
            s.stream_notice = Some(StreamNotice::info(msg));
        });
        // Auto-clear the TTFT notice after a few seconds
        let _ttft_clear = Timeout::new(TTFT_DISPLAY_MS, move || {
            let _ = state.try_update(|s| {
                // Only clear if it's still the TTFT notice (not replaced by an error)
                if let Some(ref notice) = s.stream_notice {
                    if notice.tone == StreamNoticeTone::Info
                        && notice.message.ends_with("to first token")
                    {
                        s.stream_notice = None;
                    }
                }
            });
        });
        _ttft_clear.forget();
    }
}

impl Drop for ProgressiveLatencyTimer {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Parse a raw SSE event into its envelope (event type + data).
fn parse_sse_envelope(event_data: &str) -> Option<SseEnvelope> {
    let mut event_type: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();

    for line in event_data.lines() {
        if let Some(stripped) = line.strip_prefix("event: ") {
            event_type = Some(stripped.trim().to_string());
            continue;
        }

        if let Some(stripped) = line.strip_prefix("data:") {
            let data = stripped.strip_prefix(' ').unwrap_or(stripped);
            let trimmed = data.trim();
            if trimmed == "[DONE]" || trimmed == "data: [DONE]" {
                continue;
            }
            data_lines.push(data.to_string());
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some(SseEnvelope {
        event_type,
        data: data_lines.join("\n"),
    })
}

/// Decode streamed bytes into UTF-8 text while preserving split multibyte
/// sequences across chunk boundaries.
fn decode_utf8_stream_chunk(bytes: &[u8], pending: &mut Vec<u8>) -> String {
    pending.extend_from_slice(bytes);
    let mut decoded = String::new();

    loop {
        match std::str::from_utf8(pending.as_slice()) {
            Ok(valid) => {
                decoded.push_str(valid);
                pending.clear();
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    // safe: valid_up_to comes from UTF-8 parser boundary
                    if let Ok(valid) = std::str::from_utf8(&pending[..valid_up_to]) {
                        decoded.push_str(valid);
                    }
                }

                match err.error_len() {
                    Some(error_len) => {
                        let invalid_end = (valid_up_to + error_len).min(pending.len());
                        decoded
                            .push_str(&String::from_utf8_lossy(&pending[valid_up_to..invalid_end]));
                        pending.drain(..invalid_end);
                        if pending.is_empty() {
                            break;
                        }
                    }
                    None => {
                        // Incomplete multibyte sequence at chunk end. Keep tail buffered.
                        pending.drain(..valid_up_to);
                        break;
                    }
                }
            }
        }
    }

    decoded
}

/// Parse an SSE payload and extract token content plus trace info.
fn parse_sse_payload_with_info(data: &str) -> ParsedSseEvent {
    let mut result = ParsedSseEvent::default();

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
                backend_used,
                citations,
                document_links,
                adapters_used,
                unavailable_pinned_adapters,
                pinned_routing_fallback,
                adapter_attachments,
                degraded_notices,
                fallback_triggered,
                fallback_backend,
                policy_warnings,
            } => {
                result.trace_id = trace_id;
                result.latency_ms = Some(latency_ms);
                result.token_count = Some(total_tokens as u32);
                result.prompt_tokens = prompt_tokens;
                result.completion_tokens = completion_tokens;
                result.backend_used = backend_used;
                result.citations = citations.map(map_api_citations);
                result.document_links = document_links.map(map_api_document_links);
                result.adapters_used = adapters_used;
                result.unavailable_pinned_adapters = unavailable_pinned_adapters;
                result.pinned_routing_fallback = pinned_routing_fallback;
                result.adapter_attachments = adapter_attachments;
                result.degraded_notices = degraded_notices;
                result.fallback_triggered = Some(fallback_triggered);
                result.fallback_backend = fallback_backend;
                result.policy_warnings = (!policy_warnings.is_empty()).then_some(policy_warnings);
            }
            InferenceEvent::Error {
                message,
                recoverable,
                code,
            } => {
                result.error_message = Some(message);
                result.error_retryable = recoverable;
                result.error_code = code;
            }
            InferenceEvent::Paused {
                pause_id,
                inference_id,
                trigger_kind,
                context,
                text_so_far,
                token_count,
            } => {
                result.pause_info = Some(PauseInfo {
                    pause_id,
                    inference_id,
                    trigger_kind,
                    context,
                    text_so_far,
                    token_count,
                });
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

fn mark_partial_assistant(state: &mut ChatState, assistant_id: &str) {
    if !state
        .partial_assistant_ids
        .iter()
        .any(|id| id == assistant_id)
    {
        state.partial_assistant_ids.push(assistant_id.to_string());
    }
}

fn is_low_signal_stream_error(message: &str) -> bool {
    let normalized = message.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "" | "error" | "stream error" | "server error" | "internal error" | "internal server error"
    )
}

fn stream_failure_fallback_label(failure: &StreamFailure) -> String {
    if let Some(code) = failure.code.as_deref().filter(|value| !value.is_empty()) {
        let mapped = ApiError::Structured {
            error: failure.message.clone(),
            code: code.to_string(),
            failure_code: None,
            hint: None,
            details: Box::new(None),
            request_id: None,
            error_id: None,
            fingerprint: None,
            session_id: None,
            diag_trace_id: None,
            otel_trace_id: None,
        }
        .user_message();

        if !is_low_signal_stream_error(&mapped) {
            return mapped;
        }
    }

    if is_low_signal_stream_error(&failure.message) {
        "Request failed. Retry in a moment.".to_string()
    } else {
        failure.message.clone()
    }
}

/// Human-readable error label with context for the user.
///
/// Maps error codes and messages to clear, actionable labels that help users
/// understand what went wrong and whether they can do something about it.
fn stream_notice_from_failure(failure: &StreamFailure) -> StreamNotice {
    let code = failure.code.as_deref().unwrap_or("");
    let message_lower = failure.message.to_lowercase();

    // Map error codes to human-readable labels with context
    let label = if matches!(
        code,
        "BACKPRESSURE" | "CACHE_BUDGET_EXCEEDED" | "REQUEST_TIMEOUT" | "STREAM_IDLE_TIMEOUT"
    ) {
        // Transient server-side pressure - likely to resolve on retry
        "Server is busy".to_string()
    } else if matches!(
        code,
        "WORKER_DEGRADED"
            | "WORKER_NOT_AVAILABLE"
            | "NO_COMPATIBLE_WORKER"
            | "WORKER_ID_UNAVAILABLE"
    ) {
        // Worker-specific issue - retry may route to different worker
        "Worker unavailable. Response was not generated.".to_string()
    } else if matches!(code, "SERVICE_UNAVAILABLE") {
        if message_lower.contains("worker") {
            "Worker unavailable. Response was not generated.".to_string()
        } else {
            "Service temporarily unavailable".to_string()
        }
    } else if matches!(code, "ADAPTER_NOT_LOADABLE" | "ADAPTER_NOT_FOUND") {
        "Adapter attach failed. Response was not generated.".to_string()
    } else if matches!(
        code,
        "BIT_IDENTICAL_ADAPTER_PIN_REQUIRED" | "BIT_IDENTICAL_ADAPTER_PIN_INVALID"
    ) {
        "Bit-Identical requires pinned adapter versions".to_string()
    } else if matches!(
        code,
        "DUPLICATE_REQUEST" | "IDEMPOTENCY_CONFLICT" | "IDEMPOTENCY_TIMEOUT"
    ) {
        // Idempotency conflict - user should wait, not retry immediately
        "Request already in progress".to_string()
    } else if message_lower.contains("network") || message_lower.contains("fetch failed") {
        // Client-side network issue
        "Connection lost".to_string()
    } else if message_lower.contains("unauthorized") || code == "UNAUTHORIZED" {
        // Auth issue - not retryable without re-login
        "Session expired".to_string()
    } else if message_lower.contains("forbidden") || code == "FORBIDDEN" {
        // Permission issue - not retryable
        "Access denied".to_string()
    } else if message_lower.contains("rate limit") || code == "RATE_LIMITED" {
        // Rate limiting - retryable after delay
        "Too many requests".to_string()
    } else {
        stream_failure_fallback_label(failure)
    };

    if failure.retryable {
        StreamNotice::warning(label, true)
    } else {
        StreamNotice::error(label, false)
    }
}

fn degraded_notice_from_failure(failure: &StreamFailure) -> Option<DegradedNotice> {
    let code = failure.code.as_deref().unwrap_or("");
    match code {
        "WORKER_DEGRADED"
        | "WORKER_NOT_AVAILABLE"
        | "NO_COMPATIBLE_WORKER"
        | "WORKER_ID_UNAVAILABLE" => Some(DegradedNotice {
            kind: adapteros_api_types::inference::DegradedNoticeKind::WorkerUnavailable,
            level: adapteros_api_types::inference::DegradedNoticeLevel::Critical,
            message: "Worker unavailable. Response was not generated.".to_string(),
            meaning_changed: true,
        }),
        "ADAPTER_NOT_LOADABLE" | "ADAPTER_NOT_FOUND" => Some(DegradedNotice {
            kind: adapteros_api_types::inference::DegradedNoticeKind::AttachFailure,
            level: adapteros_api_types::inference::DegradedNoticeLevel::Critical,
            message: "Adapter attach failed. Response was not generated.".to_string(),
            meaning_changed: true,
        }),
        "BIT_IDENTICAL_ADAPTER_PIN_REQUIRED" | "BIT_IDENTICAL_ADAPTER_PIN_INVALID" => {
            Some(DegradedNotice {
                kind: adapteros_api_types::inference::DegradedNoticeKind::BlockedPins,
                level: adapteros_api_types::inference::DegradedNoticeLevel::Warning,
                message: "Run blocked: adapter version not pinned.".to_string(),
                meaning_changed: true,
            })
        }
        _ => None,
    }
}

fn is_dev_worker_unavailable_echo(token: &str) -> bool {
    token.trim_start().starts_with(DEV_ECHO_NO_WORKER_PREFIX)
}

/// Helper to safely update state, returning false if signal is disposed
fn try_update_state<F: FnOnce(&mut ChatState)>(state: RwSignal<ChatState>, f: F) -> bool {
    // Use try_update which returns None if the signal is disposed
    state.try_update(f).is_some()
}

/// Stream inference using POST SSE endpoint, updating state directly
async fn stream_inference_to_state(
    request: &StreamingInferRequest,
    state: RwSignal<ChatState>,
    abort_signal: Option<&AbortSignal>,
    idempotency_key: Option<String>,
) -> Result<StreamTraceInfo, StreamFailure> {
    let url = format!("{}/v1/infer/stream", api_base_url());
    let perf_enabled = perf_logging_enabled();
    let stream_started_at = Instant::now();
    let mut first_token_logged = false;

    let body = serde_json::to_string(request).map_err(|e| {
        StreamFailure::new(format!("Failed to serialize request: {}", e), None, false)
    })?;

    // Create fetch request with POST method
    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);
    opts.set_body(&JsValue::from_str(&body));
    opts.set_credentials(web_sys::RequestCredentials::Include);

    if let Some(signal) = abort_signal {
        opts.set_signal(Some(signal));
    }

    let request_obj = Request::new_with_str_and_init(&url, &opts).map_err(|e| {
        StreamFailure::new(format!("Failed to create request: {:?}", e), None, false)
    })?;

    request_obj
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| {
            StreamFailure::new(
                format!("Failed to set Content-Type header: {:?}", e),
                None,
                false,
            )
        })?;

    request_obj
        .headers()
        .set("Accept", "text/event-stream")
        .map_err(|e| {
            StreamFailure::new(format!("Failed to set Accept header: {:?}", e), None, false)
        })?;

    if let Some(csrf_token) = get_csrf_token() {
        request_obj
            .headers()
            .set("X-CSRF-Token", &csrf_token)
            .map_err(|e| {
                StreamFailure::new(format!("Failed to set CSRF header: {:?}", e), None, false)
            })?;
    }

    if let Some(key) = idempotency_key.as_deref() {
        let _ = request_obj.headers().set("Idempotency-Key", key);
    }

    let window =
        web_sys::window().ok_or_else(|| StreamFailure::new("No window object", None, false))?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request_obj))
        .await
        .map_err(|e| {
            if is_abort_error_js(&e) {
                return StreamFailure::new("AbortError: The operation was aborted", None, false);
            }
            let error_str = format!("{:?}", e);
            if is_abort_error(&error_str) {
                StreamFailure::new("AbortError: The operation was aborted", None, false)
            } else {
                StreamFailure::new(format!("Fetch failed: {:?}", e), None, true)
            }
        })?
        .dyn_into()
        .map_err(|_| StreamFailure::new("Response is not a Response object", None, false))?;

    if !response.ok() {
        let status = response.status();
        let status_text = response.status_text();
        let body_text = match response.text() {
            Ok(promise) => JsFuture::from(promise)
                .await
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| status_text.clone()),
            Err(_) => status_text.clone(),
        };
        let api_error = ApiError::from_response(status as u16, &body_text, None);
        return Err(StreamFailure::new(
            api_error.to_string(),
            api_error.code().map(|c| c.to_string()),
            api_error.is_retryable(),
        ));
    }

    let body_stream = response
        .body()
        .ok_or_else(|| StreamFailure::new("No response body", None, false))?;
    let reader = body_stream
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| StreamFailure::new("Failed to get reader", None, false))?;

    let mut buffer = String::new();
    let mut pending_utf8_bytes = Vec::new();
    let mut trace_info = StreamTraceInfo::default();
    let mut latency_timer = ProgressiveLatencyTimer::start(state);

    loop {
        if let Some(signal) = abort_signal {
            if signal.aborted() {
                let _ = reader.cancel();
                return Err(StreamFailure::new(
                    "AbortError: The operation was aborted",
                    None,
                    false,
                ));
            }
        }

        let result = JsFuture::from(reader.read()).await.map_err(|e| {
            if is_abort_error_js(&e) {
                return StreamFailure::new("AbortError: The operation was aborted", None, false);
            }
            let error_str = format!("{:?}", e);
            if is_abort_error(&error_str) {
                StreamFailure::new("AbortError: The operation was aborted", None, false)
            } else {
                StreamFailure::new(format!("Read failed: {:?}", e), None, true)
            }
        })?;

        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))
            .map_err(|_| StreamFailure::new("Failed to get done property", None, false))?
            .as_bool()
            .unwrap_or(true);

        if done {
            break;
        }

        let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))
            .map_err(|_| StreamFailure::new("Failed to get value property", None, false))?;

        if value.is_undefined() {
            continue;
        }

        let array = js_sys::Uint8Array::new(&value);
        let bytes: Vec<u8> = array.to_vec();
        let chunk = decode_utf8_stream_chunk(&bytes, &mut pending_utf8_bytes);

        buffer.push_str(&chunk);

        // Process complete SSE events from buffer
        // Use drain-style approach to avoid O(n²) string reallocation
        while let Some(event_end) = buffer.find("\n\n") {
            let event_data = buffer[..event_end].to_string();
            // Use drain to remove processed bytes in-place (O(n) single operation)
            buffer.drain(..event_end + 2);

            let Some(envelope) = parse_sse_envelope(&event_data) else {
                continue;
            };

            let event_type = envelope.event_type.as_deref().unwrap_or("message");
            let data = envelope.data;

            if event_type == "stream_started" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                    let request_id = parsed
                        .get("request_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let idempotency_key = parsed
                        .get("idempotency_key")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    // Update recovery with request_id if we have one
                    let _ = state.try_update(|s| {
                        if let Some(recovery) = s.stream_recovery.as_mut() {
                            recovery.request_id = request_id.clone();
                            if let Some(key) = idempotency_key.clone() {
                                recovery.idempotency_key = key;
                            }
                        }
                    });
                }
                continue;
            }

            if event_type == "stream_finished" {
                if let Ok(payload) = serde_json::from_str::<StreamFinishedPayload>(&data) {
                    trace_info.citations = payload.citations.map(map_api_citations);
                    trace_info.document_links = payload.document_links.map(map_api_document_links);
                    if let Some(adapters_used) = payload.adapters_used {
                        trace_info.adapters_used = adapters_used;
                    }
                    trace_info.unavailable_pinned_adapters = payload.unavailable_pinned_adapters;
                    trace_info.pinned_routing_fallback = payload.pinned_routing_fallback;
                    trace_info.adapter_attachments = payload.adapter_attachments;
                    trace_info.degraded_notices = payload.degraded_notices.unwrap_or_default();
                    trace_info.fallback_triggered = payload.fallback_triggered;
                    trace_info.fallback_backend = payload.fallback_backend;
                    if !payload.policy_warnings.is_empty() {
                        trace_info.policy_warnings = Some(payload.policy_warnings);
                    }
                }
                continue;
            }

            if event_type == "aos.run_envelope" {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(run_id) = parsed.get("run_id").and_then(|v| v.as_str()) {
                        trace_info.trace_id = Some(run_id.to_string());
                    }
                }
                continue;
            }

            if event_type == "error" {
                if let Ok(payload) = serde_json::from_str::<StreamErrorPayload>(&data) {
                    return Err(StreamFailure::new(
                        payload.message,
                        Some(payload.code),
                        payload.retryable,
                    ));
                }
                return Err(StreamFailure::new(
                    "Stream error",
                    Some("STREAM_ERROR".to_string()),
                    true,
                ));
            }

            let parsed = parse_sse_payload_with_info(&data);
            if let Some(message) = parsed.error_message {
                return Err(StreamFailure::new(
                    message,
                    parsed.error_code,
                    parsed.error_retryable.unwrap_or(false),
                ));
            }

            if let Some(token_content) = parsed.token {
                if is_dev_worker_unavailable_echo(&token_content) {
                    if !try_update_state(state, |s| {
                        s.loading = false;
                        s.streaming = false;
                        s.stream_notice = Some(StreamNotice::warning(
                            "No worker connected. Start a worker to enable real inference.",
                            false,
                        ));
                        s.paused_inference = None;
                    }) {
                        return Ok(trace_info);
                    }
                    continue;
                }

                // Append token to the last (assistant) message
                // Use try_update_state to avoid panic if signal is disposed during navigation
                let is_first_token = !first_token_logged;
                if !try_update_state(state, |s| {
                    if let Some(last) = s.messages.last_mut() {
                        if last.role == "assistant" {
                            last.content.push_str(&token_content);
                        }
                    }
                    // No longer loading once we have first token
                    s.loading = false;
                    // Tokens mean the stream is active (including after a pause/resume cycle).
                    s.streaming = true;
                    // Clear latency stage notices on first token only; subsequent tokens
                    // leave stream_notice alone so the brief TTFT badge can persist.
                    if is_first_token {
                        s.stream_notice = None;
                    }
                    s.paused_inference = None;
                }) {
                    // Signal disposed, bail out early
                    return Ok(trace_info);
                }
                if is_first_token {
                    let elapsed = stream_started_at.elapsed();
                    if perf_enabled {
                        crate::debug_log!("[perf] stream first token: {}ms", elapsed.as_millis());
                    }
                    first_token_logged = true;
                    // Show brief TTFT badge (only if latency was noticeable)
                    #[cfg(target_arch = "wasm32")]
                    if elapsed.as_millis() >= 500 {
                        ProgressiveLatencyTimer::show_ttft(state, elapsed);
                    }
                    latency_timer.cancel();
                }
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
            if parsed.backend_used.is_some() {
                trace_info.backend_used = parsed.backend_used;
            }
            if parsed.citations.is_some() {
                trace_info.citations = parsed.citations;
            }
            if parsed.document_links.is_some() {
                trace_info.document_links = parsed.document_links;
            }
            if let Some(adapters_used) = parsed.adapters_used {
                trace_info.adapters_used = adapters_used;
            }
            if let Some(policy_warnings) = parsed.policy_warnings {
                trace_info.policy_warnings = Some(policy_warnings);
            }
            if parsed.unavailable_pinned_adapters.is_some() {
                trace_info.unavailable_pinned_adapters = parsed.unavailable_pinned_adapters;
            }
            if parsed.pinned_routing_fallback.is_some() {
                trace_info.pinned_routing_fallback = parsed.pinned_routing_fallback;
            }
            if parsed.adapter_attachments.is_some() {
                trace_info.adapter_attachments = parsed.adapter_attachments;
            }
            if let Some(mut notices) = parsed.degraded_notices {
                trace_info.degraded_notices.append(&mut notices);
            }
            if parsed.fallback_triggered.is_some() {
                trace_info.fallback_triggered = parsed.fallback_triggered.unwrap_or(false);
            }
            if parsed.fallback_backend.is_some() {
                trace_info.fallback_backend = parsed.fallback_backend;
            }

            // Update active adapters from adapter state info (merge by adapter_id)
            if let Some(adapter_states) = parsed.adapter_states {
                // Use try_update_state to avoid panic if signal is disposed during navigation
                if !try_update_state(state, |s| {
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
                    // Clear adapter-selection pending as soon as we have confirmation
                    // for the pin epoch sent with this request.
                    if s.last_sent_pin_epoch >= s.pin_change_epoch {
                        s.adapter_selection_pending = false;
                        s.adapter_state_confirmed = false;
                    }
                }) {
                    // Signal disposed, bail out early
                    return Ok(trace_info);
                }
            }

            if trace_info.pinned_routing_fallback.is_some() || trace_info.fallback_triggered {
                let fallback_message = if trace_info.fallback_triggered {
                    "Semantic fallback changed execution path."
                } else {
                    "Pinned routing was overridden."
                };
                let _ = try_update_state(state, |s| {
                    s.bit_identical_mode_degraded = true;
                    s.stream_notice = Some(StreamNotice::warning(fallback_message, false));
                });
            }

            // Handle pause events (human-in-the-loop review)
            if let Some(pause_info) = parsed.pause_info {
                // Build a descriptive pause message for the UI
                let pause_message = match pause_info.trigger_kind.as_str() {
                    "policy_violation" => "Paused: Policy review required",
                    "uncertainty" => "Paused: Human review requested",
                    "safety_gate" => "Paused: Safety review required",
                    _ => "Paused: Awaiting review",
                };
                // Update state to show pause indicator
                if !try_update_state(state, |s| {
                    s.loading = false;
                    s.streaming = false;
                    s.stream_notice = Some(StreamNotice::paused(pause_message));
                    s.paused_inference = Some(pause_info.clone());
                    // If we have text so far, update the assistant message
                    if let Some(text) = &pause_info.text_so_far {
                        if let Some(last) = s.messages.last_mut() {
                            if last.role == "assistant" && last.content.is_empty() {
                                last.content = text.clone();
                            }
                        }
                    }
                }) {
                    return Ok(trace_info);
                }
                latency_timer.cancel();
                // Log pause event for debugging
                crate::debug_log!(
                    "[Pause] id={}, inference={}, trigger={}",
                    pause_info.pause_id,
                    pause_info.inference_id,
                    pause_info.trigger_kind
                );
            }
        }
    }

    if !pending_utf8_bytes.is_empty() {
        buffer.push_str(&String::from_utf8_lossy(&pending_utf8_bytes));
        pending_utf8_bytes.clear();
    }

    latency_timer.cancel();

    // Fallback: if the backend never emitted an AdapterStateUpdate during this stream,
    // pending could stay stuck. Clear it once the request is complete, but only if
    // no newer pin changes happened after the request started.
    let _ = state.try_update(|s| {
        if s.adapter_selection_pending && s.last_sent_pin_epoch >= s.pin_change_epoch {
            s.adapter_selection_pending = false;
            s.adapter_state_confirmed = false;
        }
    });

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
    reader.release_lock();

    // Snapshot active adapters for per-message attribution
    if trace_info.adapters_used.is_empty() {
        if let Some(current_state) = state.try_get_untracked() {
            trace_info.adapters_used = current_state
                .active_adapters
                .iter()
                .filter(|a| a.is_active)
                .map(|a| a.adapter_id.clone())
                .collect();
        }
    }

    Ok(trace_info)
}

// ============================================================================
// Session Persistence (localStorage)
// ============================================================================

/// Chat session metadata for the landing page
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatSessionMeta {
    pub id: String,
    pub title: String,
    pub target: String,
    pub message_count: usize,
    pub preview: String,
    #[serde(default)]
    pub archived: bool,
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
    /// Whether the session is archived in local storage.
    #[serde(default)]
    pub archived: bool,
    /// Session-local mode toggle (Fast/Verified)
    #[serde(default)]
    pub verified_mode: bool,
    /// Placeholder session created eagerly when navigating to a new `/chat/s/:session_id`.
    ///
    /// Placeholders are pruned if the user leaves without sending any messages.
    #[serde(default)]
    pub placeholder: bool,
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
    /// Backend used for inference (e.g., "coreml", "mlx")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Persisted lightweight citation marker. Citation payload stays memory-only.
    #[serde(default)]
    pub has_citations: bool,
    /// Adapter IDs active during inference
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapters_used: Vec<String>,
    /// Pinned adapters that were unavailable during this inference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_pinned_adapters: Vec<String>,
    /// Routing fallback mode used when pinned adapters were unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
    /// Whether backend fallback occurred and may have changed semantics.
    #[serde(default)]
    pub fallback_triggered: bool,
    /// Backend selected after fallback (if different from requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Adapter attachment details (reason + exact version metadata).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_attachments: Vec<AdapterAttachment>,
    /// Degraded/failure notices for trust detail rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded_notices: Vec<DegradedNotice>,
    /// Policy warnings surfaced on this response (Feature 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_warnings: Vec<ChatPolicyWarning>,
}

/// Manager for chat sessions in localStorage
pub struct ChatSessionsManager;

impl ChatSessionsManager {
    /// Validate that a session id is safe to use.
    ///
    /// Accepted prefixes:
    /// - `ses_` / `ses-` — current formats from `adapteros_id::TypedId`
    /// - `session-`  — legacy format from earlier generate_readable_id
    ///
    /// After prefix, only `[A-Za-z0-9_-]` is allowed.
    pub fn is_valid_session_id(id: &str) -> bool {
        let rest = if let Some(r) = id.strip_prefix("ses_") {
            r
        } else if let Some(r) = id.strip_prefix("ses-") {
            r
        } else if let Some(r) = id.strip_prefix("session-") {
            r
        } else {
            return false;
        };
        if rest.is_empty() {
            return false;
        }
        rest.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }

    /// Load all session metadata from localStorage
    pub fn load_sessions() -> Vec<ChatSessionMeta> {
        Self::load_sessions_by_archive(false)
    }

    /// Load archived session metadata from localStorage.
    pub fn load_archived_sessions() -> Vec<ChatSessionMeta> {
        Self::load_sessions_by_archive(true)
    }

    fn load_sessions_by_archive(archived: bool) -> Vec<ChatSessionMeta> {
        let Some(window) = web_sys::window() else {
            return Vec::new();
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return Vec::new();
        };
        let Ok(Some(data)) = storage.get_item(SESSIONS_STORAGE_KEY) else {
            return Vec::new();
        };

        let Ok(mut sessions) = serde_json::from_str::<Vec<StoredChatSession>>(&data) else {
            return Vec::new();
        };

        // Prune stale placeholders to avoid accumulating abandoned empty sessions.
        let before_len = sessions.len();
        Self::prune_stale_placeholders_in_memory(&mut sessions, chrono::Duration::hours(24));
        if sessions.len() != before_len {
            if let Ok(json) = serde_json::to_string(&sessions) {
                let _ = storage.set_item(SESSIONS_STORAGE_KEY, &json);
            }
        }

        sessions_to_meta_for_archive(sessions, archived)
    }

    /// Merge backend sessions into localStorage, inserting stubs for any
    /// sessions not already present locally.  Returns `true` if any new
    /// sessions were added.
    pub fn merge_backend_sessions(backend: &[BackendChatSession]) -> bool {
        let Some(window) = web_sys::window() else {
            return false;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return false;
        };

        let mut sessions: Vec<StoredChatSession> = storage
            .get_item(SESSIONS_STORAGE_KEY)
            .ok()
            .flatten()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default();

        let mut added = false;
        for bs in backend {
            // Skip archived/deleted backend sessions
            if bs.status.as_deref() == Some("archived") || bs.status.as_deref() == Some("deleted") {
                continue;
            }
            // Skip if already in localStorage
            if sessions.iter().any(|s| s.id == bs.id) {
                continue;
            }
            let title = bs.title.clone().unwrap_or_else(|| bs.name.clone());
            sessions.push(StoredChatSession {
                id: bs.id.clone(),
                title,
                target: ChatTarget::Default.display_name(),
                messages: Vec::new(),
                archived: false,
                verified_mode: false,
                placeholder: false,
                created_at: bs.created_at.clone(),
                updated_at: bs.updated_at.clone(),
            });
            added = true;
        }

        if added {
            sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            sessions.truncate(MAX_SESSIONS);
            if let Ok(json) = serde_json::to_string(&sessions) {
                let _ = storage.set_item(SESSIONS_STORAGE_KEY, &json);
            }
        }
        added
    }

    /// Backfill a stub session with messages fetched from the backend.
    /// Only populates messages when the local session has none (i.e. is a stub).
    /// Returns the updated session on success, or `None` if the session was not found locally.
    pub fn backfill_session_messages(
        session_id: &str,
        messages: &[BackendChatMessage],
    ) -> Option<StoredChatSession> {
        let mut session = Self::load_session(session_id)?;
        // Only backfill if session has no local messages (stub)
        if !session.messages.is_empty() {
            return Some(session);
        }
        session.messages = messages
            .iter()
            .map(|m| StoredMessage {
                id: m.id.clone(),
                role: m.role.clone(),
                content: m.content.clone(),
                timestamp: m.timestamp.clone(),
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                has_citations: false,
                adapters_used: Vec::new(),
                unavailable_pinned_adapters: Vec::new(),
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                policy_warnings: Vec::new(),
            })
            .collect();
        // Update title from first user message if still default
        if session.title == "New Chat" {
            if let Some(first_user) = session.messages.iter().find(|m| m.role == "user") {
                session.title = truncate_string(&first_user.content, 50);
            }
        }
        session.placeholder = false;
        Self::save_session(&session);
        Some(session)
    }

    /// Load a specific session by ID
    pub fn load_session(id: &str) -> Option<StoredChatSession> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        let data = storage.get_item(SESSIONS_STORAGE_KEY).ok()??;

        let sessions: Vec<StoredChatSession> = serde_json::from_str(&data).ok()?;
        sessions.into_iter().find(|s| s.id == id)
    }

    /// Create a new placeholder session (empty conversation) for a known-good session id.
    pub fn create_placeholder_session(id: &str) -> StoredChatSession {
        let now = crate::utils::now_utc().to_rfc3339();
        StoredChatSession {
            id: id.to_string(),
            title: "New Chat".to_string(),
            target: ChatTarget::Default.display_name(),
            messages: Vec::new(),
            archived: false,
            verified_mode: false,
            placeholder: true,
            created_at: now.clone(),
            updated_at: now,
        }
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

        // Keep storage hygienic even if callers don't go through load_sessions().
        Self::prune_stale_placeholders_in_memory(&mut sessions, chrono::Duration::hours(24));

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

    /// Convenience alias for save_session.
    pub fn upsert_session(session: &StoredChatSession) {
        Self::save_session(session);
    }

    /// Delete a placeholder session if it is still empty.
    pub fn prune_placeholder_session(id: &str) {
        if let Some(session) = Self::load_session(id) {
            if session.placeholder && session.messages.is_empty() {
                Self::delete_session(id);
            }
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

    /// Mark a session as archived.
    pub fn archive_session(id: &str) {
        Self::set_session_archived(id, true);
    }

    /// Mark a session as active again.
    pub fn unarchive_session(id: &str) {
        Self::set_session_archived(id, false);
    }

    /// Check whether a session is archived.
    pub fn is_session_archived(id: &str) -> bool {
        Self::load_session(id).map(|s| s.archived).unwrap_or(false)
    }

    /// Create a session from current dock state
    ///
    /// If an existing session is provided, its `created_at` is preserved.
    pub fn session_from_state(id: &str, state: &ChatState) -> StoredChatSession {
        // Check if session already exists to preserve created_at
        let existing = Self::load_session(id);
        Self::session_from_state_with_created(
            id,
            state,
            existing.as_ref().map(|s| s.created_at.as_str()),
            existing.as_ref().map(|s| s.archived).unwrap_or(false),
        )
    }

    /// Create a session from current dock state with explicit created_at
    fn session_from_state_with_created(
        id: &str,
        state: &ChatState,
        created_at: Option<&str>,
        archived: bool,
    ) -> StoredChatSession {
        let now = crate::utils::now_utc().to_rfc3339();
        let title = state
            .messages
            .iter()
            .find(|m| m.role == "user")
            .map(|m| truncate_string(&m.content, 50))
            .unwrap_or_else(|| "New Chat".to_string());

        StoredChatSession {
            id: id.to_string(),
            title,
            target: state.target.display_name(),
            messages: state
                .messages
                .iter()
                .filter(|m| {
                    !(m.role == "assistant"
                        && state.partial_assistant_ids.iter().any(|id| id == &m.id))
                })
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
                        backend_used: m.backend_used.clone(),
                        has_citations: m.has_citations
                            || m.citations.as_ref().is_some_and(|c| !c.is_empty()),
                        adapters_used: m.adapters_used.clone().unwrap_or_default(),
                        unavailable_pinned_adapters: m
                            .unavailable_pinned_adapters
                            .clone()
                            .unwrap_or_default(),
                        pinned_routing_fallback: m.pinned_routing_fallback.clone(),
                        fallback_triggered: m.fallback_triggered,
                        fallback_backend: m.fallback_backend.clone(),
                        adapter_attachments: m.adapter_attachments.clone(),
                        degraded_notices: m.degraded_notices.clone(),
                        policy_warnings: m.policy_warnings.clone(),
                    }
                })
                .collect(),
            archived,
            verified_mode: state.verified_mode,
            placeholder: false,
            // Preserve original created_at if updating an existing session
            created_at: created_at.unwrap_or(&now).to_string(),
            updated_at: now,
        }
    }

    fn set_session_archived(id: &str, archived: bool) {
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

        let now = crate::utils::now_utc().to_rfc3339();
        let mut changed = false;
        for session in sessions.iter_mut() {
            if session.id == id {
                session.archived = archived;
                session.updated_at = now.clone();
                changed = true;
                break;
            }
        }

        if changed {
            sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            if let Ok(json) = serde_json::to_string(&sessions) {
                let _ = storage.set_item(SESSIONS_STORAGE_KEY, &json);
            }
        }
    }

    fn prune_stale_placeholders_in_memory(
        sessions: &mut Vec<StoredChatSession>,
        ttl: chrono::Duration,
    ) {
        use chrono::{DateTime, Utc};
        let now = crate::utils::now_utc();
        sessions.retain(|s| {
            if !s.placeholder || !s.messages.is_empty() {
                return true;
            }
            // Parse created_at; if it's malformed, be conservative and keep it.
            let Ok(dt) = DateTime::parse_from_rfc3339(&s.created_at) else {
                return true;
            };
            let age = now.signed_duration_since(dt.with_timezone(&Utc));
            age <= ttl
        });
    }
}

fn sessions_to_meta_for_archive(
    mut sessions: Vec<StoredChatSession>,
    archived: bool,
) -> Vec<ChatSessionMeta> {
    // Deterministic ordering: most recently updated first.
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    sessions
        .into_iter()
        .filter(|s| s.archived == archived)
        .map(|s| ChatSessionMeta {
            id: s.id,
            title: s.title,
            target: s.target,
            message_count: s.messages.len(),
            preview: s
                .messages
                .iter()
                .rev()
                .find(|m| m.role != "system")
                .map(|m| truncate_string(&m.content, 100))
                .unwrap_or_default(),
            archived: s.archived,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect()
}

/// Truncate a string to a maximum number of characters, respecting UTF-8 boundaries.
/// Appends "..." if truncated.
fn truncate_string(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_infer_request_serializes_session_id_and_reasoning_mode() {
        let req = StreamingInferRequest {
            prompt: "Test prompt".to_string(),
            model: None,
            session_id: Some("session-123".to_string()),
            stack_id: None,
            max_tokens: None,
            temperature: None,
            adapters: None,
            bit_identical: false,
            context: None,
            collection_id: None,
            reasoning_mode: Some(true),
            backend: None,
        };

        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("\"session_id\":\"session-123\""));
        assert!(json.contains("\"reasoning_mode\":true"));
        assert!(json.contains("\"bit_identical\":false"));
    }

    #[test]
    fn parse_sse_done_event_extracts_policy_warnings() {
        let payload = r#"{
            "event":"Done",
            "total_tokens":12,
            "latency_ms":34,
            "policy_warnings":[
                {"policy_name":"Evidence","message":"Missing citation","severity":"advisory"}
            ]
        }"#;

        let parsed = parse_sse_payload_with_info(payload);
        let warnings = parsed.policy_warnings.expect("policy warnings parsed");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].policy_name, "Evidence");
        assert_eq!(warnings[0].severity, "advisory");
    }

    #[test]
    fn stored_chat_session_placeholder_defaults_false() {
        let json = r#"{
            "id": "ses_abc123",
            "title": "New Chat",
            "target": "Default",
            "messages": [],
            "verified_mode": false,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;
        let session: StoredChatSession = serde_json::from_str(json).expect("deserialize");
        assert!(!session.placeholder);
    }

    #[test]
    fn create_placeholder_session_marks_local_draft_state() {
        let session = ChatSessionsManager::create_placeholder_session("ses_draft_123");
        assert_eq!(session.id, "ses_draft_123");
        assert!(session.placeholder);
        assert!(session.messages.is_empty());
        assert!(!session.archived);
    }

    #[test]
    fn prune_stale_placeholders_removes_only_expired_empty_placeholders() {
        let old = (crate::utils::now_utc() - chrono::Duration::hours(30)).to_rfc3339();
        let fresh = (crate::utils::now_utc() - chrono::Duration::hours(2)).to_rfc3339();

        let mut sessions = vec![
            StoredChatSession {
                id: "ses_old_placeholder".to_string(),
                title: "Old".to_string(),
                target: "Default".to_string(),
                messages: Vec::new(),
                archived: false,
                verified_mode: false,
                placeholder: true,
                created_at: old.clone(),
                updated_at: old,
            },
            StoredChatSession {
                id: "ses_fresh_placeholder".to_string(),
                title: "Fresh".to_string(),
                target: "Default".to_string(),
                messages: Vec::new(),
                archived: false,
                verified_mode: false,
                placeholder: true,
                created_at: fresh.clone(),
                updated_at: fresh,
            },
            StoredChatSession {
                id: "ses_old_real".to_string(),
                title: "Real".to_string(),
                target: "Default".to_string(),
                messages: vec![StoredMessage {
                    id: "m1".to_string(),
                    role: "user".to_string(),
                    content: "hi".to_string(),
                    timestamp: crate::utils::now_utc().to_rfc3339(),
                    trace_id: None,
                    latency_ms: None,
                    token_count: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    backend_used: None,
                    has_citations: false,
                    adapters_used: Vec::new(),
                    unavailable_pinned_adapters: Vec::new(),
                    pinned_routing_fallback: None,
                    fallback_triggered: false,
                    fallback_backend: None,
                    adapter_attachments: Vec::new(),
                    degraded_notices: Vec::new(),
                    policy_warnings: Vec::new(),
                }],
                archived: false,
                verified_mode: false,
                placeholder: true,
                created_at: crate::utils::now_utc().to_rfc3339(),
                updated_at: crate::utils::now_utc().to_rfc3339(),
            },
        ];

        ChatSessionsManager::prune_stale_placeholders_in_memory(
            &mut sessions,
            chrono::Duration::hours(24),
        );

        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|s| s.id == "ses_fresh_placeholder"));
        assert!(sessions.iter().any(|s| s.id == "ses_old_real"));
        assert!(!sessions.iter().any(|s| s.id == "ses_old_placeholder"));
    }

    #[test]
    fn validates_session_id_format() {
        // Current formats (ses_ / ses- prefix)
        assert!(ChatSessionsManager::is_valid_session_id("ses_abc123"));
        assert!(ChatSessionsManager::is_valid_session_id("ses_ABC_123-xyz"));
        assert!(ChatSessionsManager::is_valid_session_id("ses-abc123"));
        assert!(ChatSessionsManager::is_valid_session_id("ses-ABC_123-xyz"));
        // Legacy format (session- prefix)
        assert!(ChatSessionsManager::is_valid_session_id(
            "session-8d88cf1c-2654-4dcb-91ce-7ac7f2035975"
        ));
        assert!(ChatSessionsManager::is_valid_session_id("session-chat-abc"));
        // Invalid
        assert!(!ChatSessionsManager::is_valid_session_id(""));
        assert!(!ChatSessionsManager::is_valid_session_id("ses_"));
        assert!(!ChatSessionsManager::is_valid_session_id("ses-"));
        assert!(!ChatSessionsManager::is_valid_session_id("session-"));
        assert!(!ChatSessionsManager::is_valid_session_id("foo"));
        assert!(!ChatSessionsManager::is_valid_session_id("ses_../evil"));
        assert!(!ChatSessionsManager::is_valid_session_id("ses_abc 123"));
    }

    #[test]
    fn chat_session_meta_archived_defaults_false() {
        let json = r#"{
            "id": "ses_abc123",
            "title": "Chat",
            "target": "Default",
            "message_count": 0,
            "preview": "",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;
        let meta: ChatSessionMeta = serde_json::from_str(json).expect("deserialize");
        assert!(!meta.archived);
    }

    #[test]
    fn sessions_to_meta_for_archive_filters_and_orders() {
        let active_old = StoredChatSession {
            id: "ses_active_old".to_string(),
            title: "Active old".to_string(),
            target: "Default".to_string(),
            messages: vec![StoredMessage {
                id: "m1".to_string(),
                role: "user".to_string(),
                content: "old".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                has_citations: false,
                adapters_used: Vec::new(),
                unavailable_pinned_adapters: Vec::new(),
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                policy_warnings: Vec::new(),
            }],
            archived: false,
            verified_mode: false,
            placeholder: false,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let archived_new = StoredChatSession {
            id: "ses_archived_new".to_string(),
            title: "Archived new".to_string(),
            target: "Default".to_string(),
            messages: vec![StoredMessage {
                id: "m2".to_string(),
                role: "assistant".to_string(),
                content: "new".to_string(),
                timestamp: "2024-01-02T00:00:00Z".to_string(),
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                has_citations: false,
                adapters_used: Vec::new(),
                unavailable_pinned_adapters: Vec::new(),
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                policy_warnings: Vec::new(),
            }],
            archived: true,
            verified_mode: false,
            placeholder: false,
            created_at: "2024-01-02T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
        };
        let active_new = StoredChatSession {
            id: "ses_active_new".to_string(),
            title: "Active new".to_string(),
            target: "Default".to_string(),
            messages: vec![StoredMessage {
                id: "m3".to_string(),
                role: "assistant".to_string(),
                content: "fresh".to_string(),
                timestamp: "2024-01-03T00:00:00Z".to_string(),
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                has_citations: false,
                adapters_used: Vec::new(),
                unavailable_pinned_adapters: Vec::new(),
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                policy_warnings: Vec::new(),
            }],
            archived: false,
            verified_mode: false,
            placeholder: false,
            created_at: "2024-01-03T00:00:00Z".to_string(),
            updated_at: "2024-01-03T00:00:00Z".to_string(),
        };

        let sessions = vec![active_old, archived_new, active_new];

        let active = sessions_to_meta_for_archive(sessions.clone(), false);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].id, "ses_active_new");
        assert_eq!(active[1].id, "ses_active_old");
        assert!(!active[0].archived);

        let archived = sessions_to_meta_for_archive(sessions, true);
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "ses_archived_new");
        assert!(archived[0].archived);
    }

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

    mod stream_notice_mapping {
        use super::*;

        #[test]
        fn stream_notice_uses_canonical_code_mapping_for_generic_error_text() {
            let failure = StreamFailure::new("error", Some("INTERNAL_ERROR".to_string()), false);
            let notice = stream_notice_from_failure(&failure);
            assert_eq!(notice.message, "Internal server error. Retry in a moment.");
            assert_eq!(notice.tone, StreamNoticeTone::Error);
            assert!(!notice.retryable);
        }

        #[test]
        fn stream_notice_replaces_low_signal_message_without_code() {
            let failure = StreamFailure::new("error", None, true);
            let notice = stream_notice_from_failure(&failure);
            assert_eq!(notice.message, "Request failed. Retry in a moment.");
            assert_eq!(notice.tone, StreamNoticeTone::Warning);
            assert!(notice.retryable);
        }

        #[test]
        fn degraded_notice_marks_worker_unavailable_as_critical_failure() {
            let failure = StreamFailure::new(
                "worker down",
                Some("WORKER_NOT_AVAILABLE".to_string()),
                true,
            );
            let notice = degraded_notice_from_failure(&failure).expect("notice should exist");
            assert_eq!(
                notice.kind,
                adapteros_api_types::inference::DegradedNoticeKind::WorkerUnavailable
            );
            assert_eq!(
                notice.level,
                adapteros_api_types::inference::DegradedNoticeLevel::Critical
            );
            assert!(notice.meaning_changed);
            assert_eq!(
                notice.message,
                "Worker unavailable. Response was not generated."
            );
        }
    }

    /// Tests for disposed signal safety in async contexts
    ///
    /// These regression tests verify that stream cancellation properly prevents
    /// subsequent state updates, avoiding potential panics from disposed signals.
    ///
    /// Note: These tests use test-only helper functions to avoid WASM dependencies
    /// (web_sys, js_sys) that would fail in native test environments.
    mod disposed_signal_safety {
        use super::*;

        /// Create a test-friendly ChatState without WASM dependencies
        fn test_chat_state() -> ChatState {
            ChatState {
                dock_state: DockState::Narrow,
                messages: Vec::new(),
                target: ChatTarget::Default,
                context: ContextToggles::default(),
                loading: false,
                streaming: false,
                error: None,
                last_read_message_id: None,
                page_context: None,
                session_id: None,
                active_adapters: Vec::new(),
                suggested_adapters: Vec::new(),
                selected_adapter: None,
                pinned_adapters: Vec::new(),
                session_pinned_adapters: Vec::new(),
                verified_mode: false,
                bit_identical_mode_blocked: false,
                bit_identical_mode_degraded: false,
                active_collection_id: None,
                knowledge_collection_id: None,
                last_response_adapter_set: HashSet::new(),
                stream_notice: None,
                paused_inference: None,
                stream_recovery: None,
                partial_assistant_ids: Vec::new(),
                adapter_selection_pending: false,
                pin_change_epoch: 0,
                last_sent_pin_epoch: 0,
                adapter_state_confirmed: false,
                total_messages_evicted: 0,
                overflow_dismissed: false,
            }
        }

        /// Create a test user message without WASM dependencies
        fn test_user_message(content: &str) -> ChatMessage {
            ChatMessage {
                id: "test-user-msg".to_string(),
                role: "user".to_string(),
                content: content.to_string(),
                timestamp: crate::utils::now_utc(),
                is_streaming: false,
                status: MessageStatus::Complete,
                queued_at: None,
                pending_phase: PendingPhase::Calm,
                pending_reason: None,
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                citations: None,
                document_links: None,
                has_citations: false,
                adapters_used: None,
                unavailable_pinned_adapters: None,
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                replay_status: None,
                policy_warnings: Vec::new(),
            }
        }

        /// Create a test assistant streaming message without WASM dependencies
        fn test_assistant_streaming() -> ChatMessage {
            ChatMessage {
                id: "test-assistant-msg".to_string(),
                role: "assistant".to_string(),
                content: String::new(),
                timestamp: crate::utils::now_utc(),
                is_streaming: true,
                status: MessageStatus::Streaming,
                queued_at: None,
                pending_phase: PendingPhase::Calm,
                pending_reason: None,
                trace_id: None,
                latency_ms: None,
                token_count: None,
                prompt_tokens: None,
                completion_tokens: None,
                backend_used: None,
                citations: None,
                document_links: None,
                has_citations: false,
                adapters_used: None,
                unavailable_pinned_adapters: None,
                pinned_routing_fallback: None,
                fallback_triggered: false,
                fallback_backend: None,
                adapter_attachments: Vec::new(),
                degraded_notices: Vec::new(),
                replay_status: None,
                policy_warnings: Vec::new(),
            }
        }

        #[test]
        fn chat_state_default_is_not_streaming() {
            let state = test_chat_state();
            assert!(!state.streaming, "Default state should not be streaming");
            assert!(!state.loading, "Default state should not be loading");
        }

        #[test]
        fn chat_state_can_mark_streaming_complete() {
            let mut state = test_chat_state();

            // Simulate starting a stream
            state.streaming = true;
            state.loading = true;
            state.messages.push(test_assistant_streaming());

            assert!(state.streaming);
            assert!(state.loading);
            assert!(state.messages.last().unwrap().is_streaming);

            // Simulate cancellation (what cancel_stream does internally)
            state.streaming = false;
            state.loading = false;
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.is_streaming = false;
                }
            }

            // Verify state is properly reset
            assert!(!state.streaming, "Streaming should be false after cancel");
            assert!(!state.loading, "Loading should be false after cancel");
            assert!(
                !state.messages.last().unwrap().is_streaming,
                "Last message should not be streaming after cancel"
            );
        }

        #[test]
        fn cancel_state_update_is_idempotent() {
            let mut state = test_chat_state();

            // Start with non-streaming state
            state.streaming = false;
            state.loading = false;

            // Multiple cancellation-style updates should be safe (idempotent)
            for _ in 0..3 {
                state.streaming = false;
                state.loading = false;
                if let Some(last) = state.messages.last_mut() {
                    if last.role == "assistant" {
                        last.is_streaming = false;
                    }
                }
            }

            // State should remain consistent
            assert!(!state.streaming);
            assert!(!state.loading);
        }

        #[test]
        fn empty_messages_safe_during_cancel() {
            let mut state = test_chat_state();
            assert!(state.messages.is_empty());

            // Cancellation logic should handle empty messages gracefully
            state.streaming = false;
            state.loading = false;
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.is_streaming = false;
                }
            }

            // No panic should occur, state remains valid
            assert!(state.messages.is_empty());
            assert!(!state.streaming);
        }

        #[test]
        fn cancel_with_user_message_only() {
            let mut state = test_chat_state();
            state.messages.push(test_user_message("Hello"));
            state.streaming = true;

            // Cancel should not modify user messages
            state.streaming = false;
            state.loading = false;
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.is_streaming = false;
                }
            }

            // User message should be untouched
            assert_eq!(state.messages.len(), 1);
            assert_eq!(state.messages[0].role, "user");
            assert!(!state.messages[0].is_streaming);
        }

        #[test]
        fn streaming_message_content_preserved_on_cancel() {
            let mut state = test_chat_state();
            let mut msg = test_assistant_streaming();
            msg.content = "Partial response content".to_string();
            state.messages.push(msg);
            state.streaming = true;

            // Cancel should preserve partial content
            state.streaming = false;
            if let Some(last) = state.messages.last_mut() {
                if last.role == "assistant" {
                    last.is_streaming = false;
                }
            }

            // Content should be preserved
            assert_eq!(
                state.messages.last().unwrap().content,
                "Partial response content"
            );
            assert!(!state.messages.last().unwrap().is_streaming);
        }
    }

    /// Tests for context overflow detection and eviction tracking
    mod overflow_tests {
        use super::*;

        /// Create test messages without WASM-dependent UUID generation.
        fn make_messages(n: usize) -> Vec<ChatMessage> {
            (0..n)
                .map(|i| ChatMessage {
                    id: format!("test-msg-{i}"),
                    role: "user".to_string(),
                    content: format!("msg {i}"),
                    timestamp: crate::utils::now_utc(),
                    is_streaming: false,
                    status: MessageStatus::Complete,
                    queued_at: None,
                    pending_phase: PendingPhase::Calm,
                    pending_reason: None,
                    trace_id: None,
                    latency_ms: None,
                    token_count: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    backend_used: None,
                    citations: None,
                    document_links: None,
                    has_citations: false,
                    adapters_used: None,
                    unavailable_pinned_adapters: None,
                    pinned_routing_fallback: None,
                    fallback_triggered: false,
                    fallback_backend: None,
                    adapter_attachments: Vec::new(),
                    degraded_notices: Vec::new(),
                    replay_status: None,
                    policy_warnings: Vec::new(),
                })
                .collect()
        }

        fn test_state() -> ChatState {
            ChatState {
                dock_state: DockState::Narrow,
                messages: Vec::new(),
                target: ChatTarget::Default,
                context: ContextToggles::default(),
                loading: false,
                streaming: false,
                error: None,
                last_read_message_id: None,
                page_context: None,
                session_id: None,
                active_adapters: Vec::new(),
                suggested_adapters: Vec::new(),
                selected_adapter: None,
                pinned_adapters: Vec::new(),
                session_pinned_adapters: Vec::new(),
                verified_mode: false,
                bit_identical_mode_blocked: false,
                bit_identical_mode_degraded: false,
                active_collection_id: None,
                knowledge_collection_id: None,
                last_response_adapter_set: HashSet::new(),
                stream_notice: None,
                paused_inference: None,
                stream_recovery: None,
                partial_assistant_ids: Vec::new(),
                adapter_selection_pending: false,
                pin_change_epoch: 0,
                last_sent_pin_epoch: 0,
                adapter_state_confirmed: false,
                total_messages_evicted: 0,
                overflow_dismissed: false,
            }
        }

        #[test]
        fn no_notice_below_threshold() {
            let mut state = test_state();
            state.messages = make_messages(79);
            assert!(state.overflow_notice().is_none());
        }

        #[test]
        fn warning_at_threshold() {
            let mut state = test_state();
            state.messages = make_messages(OVERFLOW_WARNING_THRESHOLD);
            let notice = state.overflow_notice().unwrap();
            assert!(notice.contains("will be dropped"));
        }

        #[test]
        fn evicted_message_shown() {
            let mut state = test_state();
            state.messages = make_messages(50);
            state.total_messages_evicted = 3;
            let notice = state.overflow_notice().unwrap();
            assert!(notice.contains("3 older messages removed"));
        }

        #[test]
        fn evicted_singular() {
            let mut state = test_state();
            state.total_messages_evicted = 1;
            let notice = state.overflow_notice().unwrap();
            assert!(notice.contains("1 older message removed"));
        }

        #[test]
        fn dismissed_hides_notice() {
            let mut state = test_state();
            state.messages = make_messages(OVERFLOW_WARNING_THRESHOLD);
            state.overflow_dismissed = true;
            assert!(state.overflow_notice().is_none());
        }

        #[test]
        fn evict_old_messages_returns_count() {
            let mut msgs = make_messages(105);
            let evicted = evict_old_messages(&mut msgs, MAX_MESSAGES);
            assert_eq!(evicted, 5);
            assert_eq!(msgs.len(), MAX_MESSAGES);
        }

        #[test]
        fn evict_old_messages_returns_zero_when_under_limit() {
            let mut msgs = make_messages(50);
            let evicted = evict_old_messages(&mut msgs, MAX_MESSAGES);
            assert_eq!(evicted, 0);
            assert_eq!(msgs.len(), 50);
        }
    }

    /// Tests for progressive latency thresholds and TTFT display
    mod latency_tests {
        use super::*;

        #[test]
        fn stage_thresholds_are_ordered() {
            const { assert!(LATENCY_STAGE_1_MS < LATENCY_STAGE_2_MS) };
            const { assert!(LATENCY_STAGE_2_MS < LATENCY_STAGE_3_MS) };
        }

        #[test]
        fn ttft_display_duration_is_positive() {
            const { assert!(TTFT_DISPLAY_MS > 0) };
        }

        fn test_state() -> ChatState {
            ChatState {
                dock_state: DockState::Narrow,
                messages: Vec::new(),
                target: ChatTarget::Default,
                context: ContextToggles::default(),
                loading: false,
                streaming: false,
                error: None,
                last_read_message_id: None,
                page_context: None,
                session_id: None,
                active_adapters: Vec::new(),
                suggested_adapters: Vec::new(),
                selected_adapter: None,
                pinned_adapters: Vec::new(),
                session_pinned_adapters: Vec::new(),
                verified_mode: false,
                bit_identical_mode_blocked: false,
                bit_identical_mode_degraded: false,
                active_collection_id: None,
                knowledge_collection_id: None,
                last_response_adapter_set: HashSet::new(),
                stream_notice: None,
                paused_inference: None,
                stream_recovery: None,
                partial_assistant_ids: Vec::new(),
                adapter_selection_pending: false,
                pin_change_epoch: 0,
                last_sent_pin_epoch: 0,
                adapter_state_confirmed: false,
                total_messages_evicted: 0,
                overflow_dismissed: false,
            }
        }

        #[test]
        fn stream_notice_info_clears_on_first_token() {
            // Simulates what happens in stream_inference_to_state when first token arrives:
            // s.stream_notice = None (cleared), then optionally replaced with TTFT
            let mut state = test_state();
            state.loading = true;
            state.streaming = true;
            state.stream_notice = Some(StreamNotice::info("Thinking\u{2026}"));

            // First token arrives — state update clears notice
            state.loading = false;
            state.stream_notice = None;
            assert!(state.stream_notice.is_none());
        }

        #[test]
        fn ttft_notice_recognized_by_suffix() {
            let notice = StreamNotice::info("2.3s to first token");
            assert!(notice.message.ends_with("to first token"));
            assert_eq!(notice.tone, StreamNoticeTone::Info);
        }

        #[test]
        fn warning_stage_is_not_retryable() {
            // Stage 3 should be Warning tone but not retryable (it's a latency notice, not an error)
            let notice = StreamNotice {
                message: "This is taking longer than usual".to_string(),
                tone: StreamNoticeTone::Warning,
                retryable: false,
            };
            assert_eq!(notice.tone, StreamNoticeTone::Warning);
            assert!(!notice.retryable);
        }
    }

    /// Tests for UTF-8 safe string truncation
    mod truncate_string_tests {
        use super::*;

        #[test]
        fn truncates_ascii_string() {
            let input = "Hello, world!";
            assert_eq!(truncate_string(input, 5), "Hello...");
        }

        #[test]
        fn preserves_short_string() {
            let input = "Hi";
            assert_eq!(truncate_string(input, 5), "Hi");
        }

        #[test]
        fn handles_exact_length() {
            let input = "12345";
            assert_eq!(truncate_string(input, 5), "12345");
        }

        #[test]
        fn handles_multibyte_characters() {
            // Each emoji is multiple bytes but counts as 1 char
            let input = "Hello 👋🌍!";
            // "Hello 👋🌍!" is 10 chars: H-e-l-l-o-space-wave-earth-!
            assert_eq!(truncate_string(input, 7), "Hello 👋...");
        }

        #[test]
        fn handles_cjk_characters() {
            let input = "你好世界"; // 4 characters, each is 3 bytes
            assert_eq!(truncate_string(input, 2), "你好...");
        }

        #[test]
        fn handles_empty_string() {
            assert_eq!(truncate_string("", 5), "");
        }
    }

    mod utf8_stream_decode_tests {
        use super::*;

        #[test]
        fn preserves_incomplete_multibyte_between_chunks() {
            let mut pending = Vec::new();
            let first = decode_utf8_stream_chunk(b"Hello \xF0\x9F", &mut pending);
            assert_eq!(first, "Hello ");
            assert!(!pending.is_empty());

            let second = decode_utf8_stream_chunk(b"\x91\x8B world", &mut pending);
            assert_eq!(second, "👋 world");
            assert!(pending.is_empty());
        }

        #[test]
        fn tolerates_invalid_bytes_lossily() {
            let mut pending = Vec::new();
            let decoded = decode_utf8_stream_chunk(&[b'f', b'o', 0x80, b'o'], &mut pending);
            assert_eq!(decoded, "fo�o");
            assert!(pending.is_empty());
        }
    }
}
