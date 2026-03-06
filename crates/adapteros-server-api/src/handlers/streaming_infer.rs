//! Streaming inference endpoint handler
//!
//! Provides Server-Sent Events (SSE) streaming for token-by-token inference output.
//! Compatible with OpenAI's streaming API format for chat completions.
//!
//! # Endpoint
//! `POST /v1/infer/stream`
//!
//! # SSE Event Format
//! ```text
//! id: 42
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
//!
//! id: 43
//! data: [DONE]
//! ```
//!
//! # Reconnection Support
//! Reconnection replay is not supported for this endpoint; clients should retry the request.

use crate::api_error::ApiError;
use crate::auth::{is_dev_bypass_enabled, AuthMode, Claims, PrincipalType, JWT_ISSUER};
use crate::backpressure::check_uma_backpressure;
use crate::chat_context::build_chat_prompt;
use crate::handlers::rag_common::{retrieve_rag_context, store_rag_evidence};
use crate::inference_core::InferenceCore;
use crate::ip_extraction::ClientIp;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::security::check_tenant_access;
use crate::session_tokens::{
    ensure_no_adapter_overrides, resolve_session_token_lock, ResolvedSessionTokenLock,
    SessionTokenContext,
};
use crate::state::AppState;
use crate::types::run_envelope::set_policy_mask;
use crate::types::*;
use crate::uds_client::{UdsClient, WorkerStreamPaused, WorkerStreamToken};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_id::{IdPrefix, TypedId};
use adapteros_policy::hooks::PolicyHook;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;

/// Preload adapters in the worker before starting inference stream.
///
/// Sends `AdapterCommand::Preload` for each adapter ID via UDS. If any
/// preload fails, returns an error so the caller can fail fast before
/// starting the SSE stream.
async fn preload_adapters_for_inference(
    state: &AppState,
    tenant_id: &str,
    adapter_ids: &[String],
) -> Result<(), ApiError> {
    if adapter_ids.is_empty() {
        return Ok(());
    }

    let inference_core = InferenceCore::new(state);
    let worker_binding = inference_core
        .select_worker_for_tenant(tenant_id)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "WORKER_UNAVAILABLE",
                format!("No worker available for adapter preload: {}", e),
            )
        })?;

    let uds_path = std::path::PathBuf::from(&worker_binding.uds_path);
    let uds_client = UdsClient::new(Duration::from_secs(30));

    for adapter_id in adapter_ids {
        // Evicted adapters reload transparently: each inference request issues
        // an explicit preload before streaming starts.
        let preload_cmd = adapteros_lora_worker::AdapterCommand::Preload {
            adapter_id: adapter_id.clone(),
            hash: adapteros_core::B3Hash::default(),
        };
        match uds_client
            .send_adapter_command_json(&uds_path, &preload_cmd)
            .await
        {
            Ok(result) if result.success => {
                debug!(adapter_id = %adapter_id, "Adapter preloaded for inference");
            }
            Ok(result) => {
                // Non-success may mean already loaded (idempotent); log and continue
                debug!(
                    adapter_id = %adapter_id,
                    message = %result.message,
                    "Adapter preload returned non-success (may be already loaded)"
                );
            }
            Err(e) => {
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "ADAPTER_PRELOAD_FAILED",
                    format!("Failed to preload adapter {}: {}", adapter_id, e),
                ));
            }
        }
    }
    Ok(())
}

/// Streaming inference request
///
/// Accepts the same fields as the standard `/v1/infer` endpoint but returns
/// a stream of Server-Sent Events (SSE) with tokens as they are generated.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingInferRequest {
    /// The input prompt for inference
    pub prompt: String,
    /// Model identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Explicit backend preference (auto|coreml|mlx|metal|cpu)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<adapteros_core::BackendKind>,
    /// CoreML mode for backend selection (coreml_strict|coreml_preferred|backend_auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,
    /// Per-request override for router determinism (deterministic/adaptive)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Adapter stack identifier to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Optional domain hint to bias adapter/package selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Maximum number of tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Sampling temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Top-p nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Stop sequences to terminate generation
    #[serde(default)]
    pub stop: Vec<String>,
    /// Adapter stack to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Request strict bit-identical deterministic behavior.
    #[serde(default)]
    pub bit_identical: bool,
    /// Random seed for reproducibility.
    ///
    /// Required when the effective determinism mode is strict and tenant policy
    /// sets `determinism.require_seed=true`. Missing or invalid seeds are rejected
    /// as determinism violations (no partial result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Per-adapter strength overrides (session scoped)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,
    /// Enable reasoning-aware routing and mid-flight swaps
    #[serde(default)]
    pub reasoning_mode: bool,
    /// Collection ID for scoping RAG retrieval to specific document collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Session ID for linking inference to chat sessions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Effective adapter IDs (control-plane computed; ignored from clients)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Stop policy specification (PRD: Hard Deterministic Stop Controller)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,
    /// Context request for UI context injection (PRD-002 Phase 2)
    /// When flags are true, the server fetches and injects the corresponding
    /// context data into the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<adapteros_api_types::inference::ContextRequest>,
}

impl From<(&StreamingInferRequest, &Claims)> for InferenceRequestInternal {
    fn from((req, claims): (&StreamingInferRequest, &Claims)) -> Self {
        let is_admin = claims.role.eq_ignore_ascii_case("admin")
            || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
        Self {
            request_id: crate::id_generator::readable_request_id(),
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            reasoning_mode: req.reasoning_mode,
            admin_override: is_admin,
            stream: true, // Always streaming for this endpoint
            require_step: true,
            allow_fallback: if req.bit_identical {
                false
            } else {
                !matches!(
                    req.backend,
                    Some(backend) if backend != adapteros_core::BackendKind::Auto
                )
            },
            bit_identical: req.bit_identical,
            rag_enabled: req.collection_id.is_some(), // Enable RAG if collection_id provided
            rag_collection_id: req.collection_id.clone(),
            adapter_stack: req.adapter_stack.clone(),
            adapters: req.adapters.clone(),
            stack_id: req.stack_id.clone(),
            domain_hint: req.domain.clone(),
            adapter_strength_overrides: req.adapter_strength_overrides.clone(),
            routing_determinism_mode: req.routing_determinism_mode,
            backend_profile: req.backend,
            coreml_mode: req.coreml_mode,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_k: req.top_k,
            top_p: Some(req.top_p.unwrap_or(1.0)),
            fusion_interval: None,
            seed: req.seed,
            require_evidence: req.require_evidence,
            session_id: req.session_id.clone(),
            claims: Some(claims.clone()),
            model: req.model.clone(),
            stop_policy: req.stop_policy.clone(),
            created_at: std::time::Instant::now(),
            router_seed: None, // Use default router behavior for streaming
            ..Self::default()
        }
    }
}

use crate::types::{default_max_tokens, default_temperature};

/// OpenAI-compatible streaming chunk response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChunk {
    /// Unique identifier for the completion
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    pub object: String,
    /// Unix timestamp of creation
    pub created: u64,
    /// Model used for generation
    pub model: String,
    /// System fingerprint for determinism tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// Array of choices
    pub choices: Vec<StreamingChoice>,
}

/// Individual choice in a streaming response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChoice {
    /// Index of this choice
    pub index: usize,
    /// Delta containing new content
    pub delta: Delta,
    /// Finish reason (null until complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Stop reason code explaining why generation terminated (PRD: Hard Deterministic Stop Controller)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,
}

/// Delta containing new content
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Delta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Content delta (new tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Internal streaming event
#[derive(Debug, Clone)]
enum StreamEvent {
    /// Stream lifecycle started - sent as first event with stream metadata
    StreamStarted {
        stream_id: String,
        idempotency_key: Option<String>,
    },
    /// First chunk with role
    Start,
    /// Token generated
    Token(String),
    /// Inference paused for human-in-the-loop review
    Paused(WorkerStreamPaused),
    /// Generation complete
    Done { finish_reason: String },
    /// Stream lifecycle finished - sent as final event with summary
    StreamFinished {
        stream_id: String,
        total_tokens: usize,
        duration_ms: u64,
    },
    /// Heartbeat to keep SSE connection alive
    Heartbeat,
    /// Error occurred
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
struct StreamErrorPayload {
    code: String,
    message: String,
    retryable: bool,
    correlation_id: String,
}

/// Adapter state information for visualization
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateInfo {
    /// Adapter identifier
    pub adapter_id: String,
    /// Usage rate (uses per minute)
    pub uses_per_minute: u32,
    /// Currently active (in use for this inference)
    pub is_active: bool,
}

/// Inference event types for progress streaming
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event")]
pub enum InferenceEvent {
    /// Model is being loaded
    Loading {
        phase: LoadPhase,
        progress: u8,
        eta_seconds: Option<u64>,
    },
    /// Model is ready
    Ready { warmup_latency_ms: u64 },
    /// Inference token
    Token { text: String, token_id: Option<u32> },
    /// Inference paused for human review
    Paused {
        /// Unique pause ID for resume correlation.
        pause_id: String,
        /// Inference request ID.
        inference_id: String,
        /// Why the pause was triggered.
        trigger_kind: String,
        /// Context for the reviewer.
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        /// Generated text so far.
        #[serde(skip_serializing_if = "Option::is_none")]
        text_so_far: Option<String>,
        /// Token count at pause point.
        token_count: usize,
    },
    /// Inference complete
    Done {
        total_tokens: usize,
        latency_ms: u64,
        /// Pinned adapters that were unavailable
        #[serde(skip_serializing_if = "Option::is_none")]
        unavailable_pinned_adapters: Option<Vec<String>>,
        /// Routing fallback mode
        #[serde(skip_serializing_if = "Option::is_none")]
        pinned_routing_fallback: Option<String>,
        /// Citations attached to the response
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<adapteros_api_types::inference::Citation>>,
        /// Source documents attached to the response as clickable links.
        #[serde(skip_serializing_if = "Option::is_none")]
        document_links: Option<Vec<adapteros_api_types::inference::DocumentLink>>,
        /// Adapter attachments with reason + exact version metadata.
        #[serde(skip_serializing_if = "Option::is_none")]
        adapter_attachments: Option<Vec<adapteros_api_types::inference::AdapterAttachment>>,
        /// Explicit degraded/fallback notices.
        #[serde(skip_serializing_if = "Option::is_none")]
        degraded_notices: Option<Vec<adapteros_api_types::inference::DegradedNotice>>,
        /// Whether backend fallback occurred during execution.
        #[serde(default)]
        fallback_triggered: bool,
        /// Backend selected after fallback (if different from requested).
        #[serde(skip_serializing_if = "Option::is_none")]
        fallback_backend: Option<String>,
        /// Stop reason code (PRD: Hard Deterministic Stop Controller)
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
        /// Token index at which the stop decision was made
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason_token_index: Option<u32>,
        /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_policy_digest_b3: Option<String>,
        /// Pending RAG evidence IDs that need to be bound to a message_id.
        /// After creating the assistant message, call db.bind_evidence_to_message()
        /// with these IDs to complete the audit trail.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pending_evidence_ids: Vec<String>,
    },
    /// Error occurred
    Error {
        message: String,
        recoverable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    /// Adapter state update for visualization
    AdapterStateUpdate { adapters: Vec<AdapterStateInfo> },
}

/// Load phases for progress tracking
#[derive(Debug, Clone, Serialize)]
pub enum LoadPhase {
    Downloading,
    LoadingWeights,
    Warmup,
}

/// Build context prefix from ContextRequest flags.
///
/// Fetches system health snapshot and/or page context when the corresponding
/// flags are enabled, returning a formatted prefix to prepend to the prompt.
async fn build_context_prefix(
    state: &AppState,
    context: &adapteros_api_types::inference::ContextRequest,
    tenant_id: &str,
) -> String {
    let mut prefix_parts = Vec::new();

    // Include system health snapshot
    if context.include_system_snapshot {
        let health_summary = match collect_system_snapshot(state, tenant_id).await {
            Ok(snapshot) => snapshot,
            Err(e) => {
                warn!(error = %e, "Failed to collect system snapshot for context");
                "[System health data unavailable]".to_string()
            }
        };
        prefix_parts.push(format!("## System Status\n{}", health_summary));
    }

    // Include page context
    if context.include_page_context {
        let mut page_info = Vec::new();
        if let Some(path) = &context.page_path {
            page_info.push(format!("Current page: {}", path));
        }
        if let Some(entity_type) = &context.entity_type {
            if let Some(entity_id) = &context.entity_id {
                page_info.push(format!("Selected {}: {}", entity_type, entity_id));
            }
        }
        if !page_info.is_empty() {
            prefix_parts.push(format!("## Page Context\n{}", page_info.join("\n")));
        }
    }

    // Fetch recent audit logs (limited to 20 most recent)
    if context.include_recent_logs {
        let logs_summary = collect_recent_logs(state, tenant_id).await;
        prefix_parts.push(format!("## Recent Logs\n{}", logs_summary));
    }

    if prefix_parts.is_empty() {
        String::new()
    } else {
        format!(
            "Use the following system context to help answer:\n\n{}\n\n---\n\n",
            prefix_parts.join("\n\n")
        )
    }
}

/// Collect a compact system health snapshot for context injection.
async fn collect_system_snapshot(state: &AppState, tenant_id: &str) -> Result<String, String> {
    let mut lines = Vec::new();

    // Worker count (use list and count pattern)
    let worker_count = state
        .db
        .list_healthy_workers_by_tenant(tenant_id)
        .await
        .map(|workers| workers.len())
        .unwrap_or(0);
    lines.push(format!("- Workers: {} healthy", worker_count));

    // Adapter count (use list and count pattern)
    let adapter_count = state
        .db
        .list_adapters_for_tenant(tenant_id)
        .await
        .map(|adapters| adapters.len())
        .unwrap_or(0);
    lines.push(format!("- Adapters: {} registered", adapter_count));

    // System ready status
    lines.push("- Status: System online".to_string());

    Ok(lines.join("\n"))
}

/// Collect recent audit logs for context injection.
///
/// Fetches the 20 most recent audit log entries for the tenant,
/// formatted as a concise summary suitable for prompt injection.
async fn collect_recent_logs(state: &AppState, tenant_id: &str) -> String {
    const MAX_LOGS: i64 = 20;

    match state
        .db
        .query_audit_logs_for_tenant(
            tenant_id, None, // user_id
            None, // action
            None, // resource_type
            None, // start_date
            None, // end_date
            MAX_LOGS,
        )
        .await
    {
        Ok(logs) if !logs.is_empty() => {
            let log_lines: Vec<String> = logs
                .iter()
                .take(MAX_LOGS as usize)
                .map(|log| {
                    // Extract time portion from RFC3339 timestamp (e.g., "2024-01-20T15:30:45Z" -> "15:30:45")
                    let timestamp = log
                        .timestamp
                        .split('T')
                        .nth(1)
                        .and_then(|t| t.split('Z').next())
                        .and_then(|t| t.split('.').next())
                        .unwrap_or(&log.timestamp);
                    let status_icon = if log.status == "success" {
                        "✓"
                    } else {
                        "✗"
                    };
                    format!(
                        "[{}] {} {} {}",
                        timestamp, status_icon, log.action, log.resource_type
                    )
                })
                .collect();
            log_lines.join("\n")
        }
        Ok(_) => "[No recent activity]".to_string(),
        Err(e) => {
            warn!(error = %e, "Failed to fetch recent logs for context");
            "[Log retrieval failed]".to_string()
        }
    }
}

fn serialize_safe<T: Serialize>(value: &T, context: &str) -> String {
    match serde_json::to_string(value) {
        Ok(json) => json,
        Err(error) => {
            error!(
                context = %context,
                error = %error,
                "Failed to serialize streaming response payload"
            );
            serde_json::json!({
                "error": {
                    "message": "stream serialization failed",
                    "type": "serialization_error",
                    "code": "SERIALIZATION_ERROR",
                    "context": context
                }
            })
            .to_string()
        }
    }
}

/// Returns `true` if the trigger kind indicates a content-safety pause where
/// partial generated text must be redacted from the client-facing SSE event.
///
/// Safety triggers are those where the system flagged the generated content itself
/// as problematic. Leaking `text_so_far` for these defeats the purpose of the pause.
///
/// Non-safety triggers (human review, quality check, rate limit, etc.) are operational
/// pauses where the client seeing partial text is expected and useful.
///
/// The recognised values mirror `parse_trigger_kind` in `pause_tracker.rs`.
fn is_safety_trigger(trigger_kind: &str) -> bool {
    matches!(
        trigger_kind.to_lowercase().as_str(),
        "policy_violation"
            | "policy"
            | "policy_approval"
            | "safety_gate"
            | "threat"
            | "threat_escalation"
    )
}

fn run_envelope_event(envelope: &adapteros_api_types::RunEnvelope) -> Event {
    Event::default()
        .event("aos.run_envelope")
        .data(serialize_safe(envelope, "run_envelope"))
}

/// Streaming inference handler with loading progress
///
/// Accepts inference requests and returns a stream of Server-Sent Events (SSE)
/// with model loading progress, then inference tokens. This endpoint automatically
/// detects if the requested adapter needs to be loaded and streams progress events.
///
/// # SSE Event Format
/// ```text
/// data: {"event":"Loading","phase":"LoadingWeights","progress":0,"eta_seconds":30}
/// data: {"event":"Loading","phase":"Warmup","progress":50,"eta_seconds":10}
/// data: {"event":"Ready","warmup_latency_ms":1234}
/// data: {"event":"Token","text":"Hello","token_id":null}
/// data: {"event":"Done","total_tokens":10,"latency_ms":5678}
/// ```
///
/// # Example
/// ```bash
/// curl -X POST http://localhost:3000/v1/infer/stream/progress \
///   -H "Content-Type: application/json" \
///   -H "Authorization: Bearer <token>" \
///   -d '{"prompt": "Hello, world!", "max_tokens": 100, "adapters": ["my-adapter"]}'
/// ```
#[utoipa::path(
    post,
    path = "/v1/infer/stream/progress",
    tag = "inference",
    request_body = StreamingInferRequest,
    responses(
        (status = 200, description = "Server-Sent Events stream with loading progress and tokens"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 503, description = "Service under memory pressure", body = ErrorResponse),
    )
)]
pub async fn streaming_infer_with_progress(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Extension(_identity): Extension<IdentityEnvelope>,
    session_token: Option<Extension<SessionTokenContext>>,
    Json(mut req): Json<StreamingInferRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can execute inference
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request
    if req.prompt.is_empty() {
        return Err(ApiError::bad_request("Prompt cannot be empty").into());
    }
    if req.prompt.len() > MAX_REPLAY_TEXT_SIZE {
        return Err(ApiError::bad_request("Prompt too long for context window").into());
    }
    if req.max_tokens > MAX_TOKENS_LIMIT {
        return Err(ApiError::bad_request(format!(
            "max_tokens ({}) exceeds maximum allowed ({})",
            req.max_tokens, MAX_TOKENS_LIMIT
        ))
        .into());
    }

    let session_lock = if let Some(token) = session_token.as_ref() {
        ensure_no_adapter_overrides(&[
            ("adapters", req.adapters.is_some()),
            ("adapter_stack", req.adapter_stack.is_some()),
            ("stack_id", req.stack_id.is_some()),
            ("effective_adapter_ids", req.effective_adapter_ids.is_some()),
            (
                "adapter_strength_overrides",
                req.adapter_strength_overrides.is_some(),
            ),
        ])?;
        let resolved = resolve_session_token_lock(&state, &claims, &token.0.lock).await?;
        if let (Some(requested), Some(locked)) = (req.backend, resolved.backend_profile) {
            if requested != locked {
                return Err(ApiError::forbidden("session token backend mismatch")
                    .with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    ))
                    .into());
            }
        }
        if let (Some(requested), Some(locked)) = (req.coreml_mode, resolved.coreml_mode) {
            if requested != locked {
                return Err(ApiError::forbidden("session token coreml_mode mismatch")
                    .with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    ))
                    .into());
            }
        }
        Some(resolved)
    } else {
        None
    };

    if let Some(lock) = session_lock.as_ref() {
        req.adapters = Some(lock.adapter_ids.clone());
        req.adapter_stack = None;
        req.stack_id = lock.stack_id.clone();
        req.adapter_strength_overrides = None;
        req.effective_adapter_ids = None;
        if let Some(backend) = lock.backend_profile {
            req.backend = Some(backend);
        }
        if let Some(coreml_mode) = lock.coreml_mode {
            req.coreml_mode = Some(coreml_mode);
        }
    }

    // Extract adapter ID from request (for policy context)
    let adapter_id = if let Some(adapters) = &req.adapters {
        if adapters.is_empty() {
            return Err(
                ApiError::bad_request("Adapters list cannot be empty when provided").into(),
            );
        }
        adapters[0].clone()
    } else if let Some(stack) = &req.adapter_stack {
        if stack.is_empty() {
            return Err(
                ApiError::bad_request("Adapter stack list cannot be empty when provided").into(),
            );
        }
        stack[0].clone()
    } else if let Some(stack_id) = &req.stack_id {
        stack_id.clone()
    } else {
        return Err(
            ApiError::bad_request("Must specify adapters, adapter_stack, or stack_id").into(),
        );
    };

    check_uma_backpressure(&state)?;

    // Generate request ID for hook contexts
    let request_id = crate::id_generator::readable_request_id();

    // P2 HARDENING: Collect ALL policy decisions BEFORE creating envelope
    // This ensures policy_mask_digest is a true pre-flight commitment, not post-hoc proof
    let mut all_policy_decisions = Vec::new();

    // Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    let routing_decisions =
        enforce_at_hook(&state, &routing_hook_ctx)
            .await
            .map_err(|violation| {
                ApiError::new(
                    StatusCode::FORBIDDEN,
                    "POLICY_HOOK_VIOLATION",
                    "Policy hook violation (pre-routing)",
                )
                .with_details(violation.message)
            })?;
    all_policy_decisions.extend(routing_decisions);

    // Enforce policies at OnBeforeInference hook
    let hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnBeforeInference,
        "inference",
        Some(&adapter_id),
    );
    let inference_decisions = enforce_at_hook(&state, &hook_ctx)
        .await
        .map_err(|violation| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "POLICY_HOOK_VIOLATION",
                "Policy hook violation",
            )
            .with_details(violation.message)
        })?;
    all_policy_decisions.extend(inference_decisions);

    // P2 HARDENING: Compute policy digest BEFORE creating envelope
    // This makes the digest a cryptographic commitment sent to client before inference begins
    let policy_digest = compute_policy_mask_digest(&all_policy_decisions);

    // Create envelope WITH pre-computed policy digest
    let mut run_envelope =
        new_run_envelope(&state, &claims, request_id.clone(), req.reasoning_mode);
    set_policy_mask(&mut run_envelope, Some(&policy_digest));

    info!(
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens,
        adapter_id = %adapter_id,
        "Starting streaming inference with loading progress"
    );

    // Audit log: inference execution start
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    // Create the loading progress stream
    let stream = stream_with_loading_progress(
        &state,
        req,
        run_envelope.clone(),
        adapter_id,
        claims.tenant_id.clone(),
        claims.sub.clone(),
        Some(claims),
        session_lock.clone(),
    )
    .await;

    let stream = stream::once(async move { Ok(run_envelope_event(&run_envelope)) }).chain(stream);

    Ok(Sse::new(stream))
}

/// Streaming inference handler
///
/// Accepts inference requests and returns a stream of Server-Sent Events (SSE)
/// with tokens as they are generated. Compatible with OpenAI's streaming API.
///
/// # SSE Event Format
/// ```text
/// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
/// data: [DONE]
/// ```
///
/// # Collection-Scoped RAG
/// When `collection_id` is provided, the inference will be scoped to documents
/// within that collection, enabling targeted RAG retrieval for domain-specific queries.
///
/// # Example
/// ```bash
/// curl -X POST http://localhost:3000/v1/infer/stream \
///   -H "Content-Type: application/json" \
///   -H "Authorization: Bearer <token>" \
///   -d '{"prompt": "Hello, world!", "max_tokens": 100}'
/// ```
///
/// # Example with Collection-Scoped RAG
/// ```bash
/// curl -X POST http://localhost:3000/v1/infer/stream \
///   -H "Content-Type: application/json" \
///   -H "Authorization: Bearer <token>" \
///   -d '{"prompt": "What is the licensing model?", "max_tokens": 200, "collection_id": "marketing-docs"}'
/// ```
#[utoipa::path(
    post,
    path = "/v1/infer/stream",
    tag = "inference",
    request_body = StreamingInferRequest,
    responses(
        (status = 200, description = "Server-Sent Events stream of tokens"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 503, description = "Service under memory pressure", body = ErrorResponse),
    )
)]
pub async fn streaming_infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Extension(_identity): Extension<IdentityEnvelope>,
    headers: axum::http::HeaderMap,
    session_token: Option<Extension<SessionTokenContext>>,
    Json(mut req): Json<StreamingInferRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can execute inference
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request
    if req.prompt.is_empty() {
        return Err(ApiError::bad_request("Prompt cannot be empty").into());
    }
    if req.prompt.len() > MAX_REPLAY_TEXT_SIZE {
        return Err(ApiError::bad_request("Prompt too long for context window").into());
    }
    if req.max_tokens > MAX_TOKENS_LIMIT {
        return Err(ApiError::bad_request(format!(
            "max_tokens ({}) exceeds maximum allowed ({})",
            req.max_tokens, MAX_TOKENS_LIMIT
        ))
        .into());
    }

    let session_lock = if let Some(token) = session_token.as_ref() {
        ensure_no_adapter_overrides(&[
            ("adapters", req.adapters.is_some()),
            ("adapter_stack", req.adapter_stack.is_some()),
            ("stack_id", req.stack_id.is_some()),
            ("effective_adapter_ids", req.effective_adapter_ids.is_some()),
            (
                "adapter_strength_overrides",
                req.adapter_strength_overrides.is_some(),
            ),
        ])?;
        let resolved = resolve_session_token_lock(&state, &claims, &token.0.lock).await?;
        if let (Some(requested), Some(locked)) = (req.backend, resolved.backend_profile) {
            if requested != locked {
                return Err(ApiError::forbidden("session token backend mismatch")
                    .with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    ))
                    .into());
            }
        }
        if let (Some(requested), Some(locked)) = (req.coreml_mode, resolved.coreml_mode) {
            if requested != locked {
                return Err(ApiError::forbidden("session token coreml_mode mismatch")
                    .with_details(format!(
                        "requested {}, token {}",
                        requested.as_str(),
                        locked.as_str()
                    ))
                    .into());
            }
        }
        Some(resolved)
    } else {
        None
    };

    if let Some(lock) = session_lock.as_ref() {
        req.adapters = Some(lock.adapter_ids.clone());
        req.adapter_stack = None;
        req.stack_id = lock.stack_id.clone();
        req.adapter_strength_overrides = None;
        req.effective_adapter_ids = None;
        if let Some(backend) = lock.backend_profile {
            req.backend = Some(backend);
        }
        if let Some(coreml_mode) = lock.coreml_mode {
            req.coreml_mode = Some(coreml_mode);
        }
    }

    check_uma_backpressure(&state)?;

    // Extract idempotency key from headers for stream recovery
    let idempotency_key = headers
        .get("Idempotency-Key")
        .or_else(|| headers.get("idempotency-key"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Generate request ID
    let request_id = crate::id_generator::readable_openai_chatcmpl_dash_id();
    // NOTE: Envelope creation is deferred until AFTER policy enforcement (P2 hardening)
    let model_name = req.model.clone().unwrap_or_else(|| "adapteros".to_string());

    // CRITICAL: Validate session belongs to user's tenant if provided
    if let Some(session_id) = &req.session_id {
        match state.db.get_chat_session(session_id).await {
            Ok(Some(session)) => {
                if !check_tenant_access(&claims, &session.tenant_id) {
                    warn!(
                        request_id = %request_id,
                        session_id = %session_id,
                        user_tenant = %claims.tenant_id,
                        session_tenant = %session.tenant_id,
                        "Session access denied - tenant mismatch"
                    );
                    return Err(ApiError::forbidden("Access denied to session")
                        .with_details("Session does not belong to your tenant")
                        .into());
                }
                // Validate session lifecycle state
                if session.archived_at.is_some() {
                    return Err(ApiError::forbidden("Session has been archived").into());
                }
                if let Some(ref status) = session.status {
                    if status == "deleted" || status == "inactive" {
                        return Err(ApiError::forbidden(format!("Session is {}", status)).into());
                    }
                }
            }
            Ok(None) => {
                return Err(ApiError::not_found("Session").into());
            }
            Err(e) => {
                error!(
                    request_id = %request_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to validate session access"
                );
                return Err(ApiError::db_error(e).into());
            }
        }
    }

    // P2 HARDENING: Collect ALL policy decisions BEFORE creating envelope
    // This ensures policy_mask_digest is a true pre-flight commitment, not post-hoc proof
    let mut all_policy_decisions = Vec::new();

    // Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    let routing_decisions =
        enforce_at_hook(&state, &routing_hook_ctx)
            .await
            .map_err(|violation| {
                ApiError::new(
                    StatusCode::FORBIDDEN,
                    "POLICY_HOOK_VIOLATION",
                    "Policy hook violation (pre-routing)",
                )
                .with_details(violation.message)
            })?;
    all_policy_decisions.extend(routing_decisions);

    // Enforce policies at OnBeforeInference hook
    let adapter_id = req.adapters.as_ref().and_then(|a| a.first()).cloned();
    let adapter_ids_sorted: Option<Vec<String>> = req.adapters.as_ref().map(|ids| {
        let mut ids = ids.clone();
        ids.sort();
        ids
    });

    let hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnBeforeInference,
        "inference",
        adapter_id.as_deref(),
    )
    // P0 audit-correctness: preserve full adapter ID set deterministically.
    // This avoids weakening policy auditability when multiple adapters are requested.
    .with_metadata(
        "adapter_ids",
        serde_json::json!(adapter_ids_sorted.clone().unwrap_or_default()),
    );
    let inference_decisions = enforce_at_hook(&state, &hook_ctx)
        .await
        .map_err(|violation| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "POLICY_HOOK_VIOLATION",
                "Policy hook violation",
            )
            .with_details(violation.message)
        })?;
    all_policy_decisions.extend(inference_decisions);

    // P2 HARDENING: Compute policy digest BEFORE creating envelope
    // This makes the digest a cryptographic commitment sent to client before inference begins
    let policy_digest = compute_policy_mask_digest(&all_policy_decisions);

    // Create envelope WITH pre-computed policy digest
    let mut run_envelope =
        new_run_envelope(&state, &claims, request_id.clone(), req.reasoning_mode);
    set_policy_mask(&mut run_envelope, Some(&policy_digest));

    info!(
        request_id = %request_id,
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens,
        collection_id = ?req.collection_id,
        session_id = ?req.session_id,
        "Starting streaming inference"
    );

    // Build multi-turn prompt if session_id is provided
    // This loads chat history and formats it with role markers for context
    let (base_prompt, chat_context_hash) = if let Some(ref session_id) = req.session_id {
        // STABILITY: Use poison-safe lock access
        let chat_config = state
            .config
            .read()
            .unwrap_or_else(|e| {
                tracing::warn!("Config lock poisoned in streaming_infer, recovering");
                e.into_inner()
            })
            .chat_context
            .clone();
        match build_chat_prompt(&state.db, session_id, &req.prompt, &chat_config).await {
            Ok(result) => {
                info!(
                    request_id = %request_id,
                    session_id = %session_id,
                    message_count = result.message_count,
                    truncated = result.truncated,
                    context_hash = %result.context_hash,
                    "Built multi-turn prompt from session history"
                );
                (result.prompt_text, Some(result.context_hash))
            }
            Err(e) => {
                warn!(
                    request_id = %request_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to build multi-turn prompt, using single-turn"
                );
                (req.prompt.clone(), None)
            }
        }
    } else {
        // No session, use prompt directly (single-turn)
        (req.prompt.clone(), None)
    };

    // Collection-scoped RAG integration
    // When collection_id is provided, retrieve relevant context and augment the prompt
    // Also capture evidence IDs for later message binding
    let (augmented_prompt, pending_evidence_ids) = if let Some(collection_id) = &req.collection_id {
        // CRITICAL: Validate collection belongs to user's tenant
        match state
            .db
            .get_collection(&claims.tenant_id, collection_id)
            .await
        {
            Ok(Some(collection)) => {
                if collection.tenant_id != claims.tenant_id {
                    warn!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        user_tenant = %claims.tenant_id,
                        collection_tenant = %collection.tenant_id,
                        "Collection access denied - tenant mismatch"
                    );
                    return Err(ApiError::forbidden("Access denied to collection")
                        .with_details("Collection does not belong to your tenant")
                        .into());
                }
            }
            Ok(None) => {
                return Err(ApiError::not_found("Collection").into());
            }
            Err(e) => {
                error!(
                    request_id = %request_id,
                    collection_id = %collection_id,
                    error = %e,
                    "Failed to validate collection access"
                );
                return Err(ApiError::db_error(e).into());
            }
        }

        if let Some(ref embedding_model) = state.embedding_model {
            match retrieve_rag_context(
                &state,
                &claims.tenant_id,
                collection_id,
                &req.prompt,
                embedding_model.clone(),
                None,
            )
            .await
            {
                Ok(rag_result) if !rag_result.context.is_empty() => {
                    // Store evidence for this retrieval (Phase 1 of two-phase binding).
                    // NOTE: message_id is None because the message is created after inference.
                    // After message creation, call db.bind_evidence_to_message()
                    // with the returned evidence_ids to complete the audit trail.
                    let evidence_ids = store_rag_evidence(
                        &state,
                        &rag_result,
                        &request_id,
                        req.session_id.as_deref(),
                        None, // Phase 2: bind_evidence_to_message(evidence_ids, message_id)
                        None, // model_context not available in streaming flow
                    )
                    .await;

                    info!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        context_len = rag_result.context.len(),
                        doc_count = rag_result.doc_ids.len(),
                        evidence_count = evidence_ids.len(),
                        "Augmented prompt with RAG context"
                    );
                    // RAG context prepended to base_prompt (which may include chat history)
                    (
                        format!(
                            "Use the following context to answer the question.\n\n\
                         Context:\n{}\n\n\
                         {}",
                            rag_result.context, base_prompt
                        ),
                        evidence_ids,
                    )
                }
                Ok(_) => {
                    debug!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        "No relevant context found in collection"
                    );
                    (base_prompt.clone(), Vec::new())
                }
                Err(e) => {
                    warn!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        error = %e,
                        "RAG retrieval failed, proceeding without context"
                    );
                    (base_prompt.clone(), Vec::new())
                }
            }
        } else {
            warn!(
                request_id = %request_id,
                "Embedding model not configured, skipping RAG retrieval"
            );
            (base_prompt.clone(), Vec::new())
        }
    } else {
        // No collection_id, use base_prompt directly (may include chat history)
        (base_prompt, Vec::new())
    };

    // PRD-002 Phase 2: Inject UI context when toggles are enabled
    let augmented_prompt = if let Some(ref context) = req.context {
        if context.has_any_context() {
            let context_prefix = build_context_prefix(&state, context, &claims.tenant_id).await;
            if !context_prefix.is_empty() {
                info!(
                    request_id = %request_id,
                    page_context = context.include_page_context,
                    recent_logs = context.include_recent_logs,
                    system_snapshot = context.include_system_snapshot,
                    "Injecting UI context into prompt"
                );
                format!("{}{}", context_prefix, augmented_prompt)
            } else {
                augmented_prompt
            }
        } else {
            augmented_prompt
        }
    } else {
        augmented_prompt
    };

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));

    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    let core = InferenceCore::new(&state);
    let _worker = core
        .select_worker_for_tenant(&claims.tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let mut internal_request: InferenceRequestInternal = (&req, &claims).into();
    internal_request.request_id = request_id.clone();
    internal_request.run_envelope = Some(run_envelope.clone());
    internal_request.prompt = augmented_prompt;
    internal_request.chat_context_hash = chat_context_hash.clone();
    internal_request.session_id = req.session_id.clone();
    internal_request.adapters = req.adapters.clone();
    internal_request.adapter_strength_overrides = req.adapter_strength_overrides.clone();
    internal_request.model = req.model.clone();
    internal_request.stop_policy = req.stop_policy.clone();
    internal_request.created_at = std::time::Instant::now();
    if let Some(lock) = session_lock.as_ref() {
        internal_request.adapter_stack = None;
        internal_request.adapters = Some(lock.adapter_ids.clone());
        internal_request.effective_adapter_ids = Some(lock.adapter_ids.clone());
        internal_request.stack_id = lock.stack_id.clone();
        internal_request.pinned_adapter_ids = Some(lock.pinned_adapter_ids.clone());
        if let Some(backend) = lock.backend_profile {
            internal_request.backend_profile = Some(backend);
            internal_request.allow_fallback = backend == adapteros_core::BackendKind::Auto;
        }
        if let Some(coreml_mode) = lock.coreml_mode {
            internal_request.coreml_mode = Some(coreml_mode);
        }
    }

    // Preload adapters before starting stream (fail fast, not mid-stream)
    if let Some(ref adapter_ids) = internal_request.effective_adapter_ids {
        preload_adapters_for_inference(&state, &claims.tenant_id, adapter_ids)
            .await
            .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    }

    // Clone data for the stream
    let request_id_clone = request_id.clone();
    let model_name_clone = model_name.clone();
    let session_id = req.session_id.clone();
    let adapters = req.adapters.clone();
    // Pass hook context to stream state
    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();
    let stream_config = state
        .config
        .read()
        .unwrap_or_else(|e| {
            tracing::warn!("Config lock poisoned in streaming_infer, recovering");
            e.into_inner()
        })
        .streaming
        .clone();

    // Create cancellation token for client disconnect detection
    let cancellation_token = CancellationToken::new();
    let drop_guard = StreamDropGuard {
        cancellation_token: cancellation_token.clone(),
        request_id: request_id.clone(),
    };

    let (token_rx, pause_rx, done_rx) = spawn_streaming_inference(
        state.clone(),
        internal_request,
        cancellation_token.clone(),
        stream_config.inference_token_buffer_capacity,
    );

    // Create the SSE stream with cancellation support
    let stream = stream::unfold(
        (
            StreamState::new(
                state,
                request_id_clone,
                run_envelope.clone(),
                model_name_clone,
                token_rx,
                pause_rx,
                done_rx,
                session_id,
                adapters,
                tenant_id,
                user_id,
                Some(claims),
                cancellation_token,
                Duration::from_secs(stream_config.inference_idle_timeout_secs),
                Duration::from_secs(stream_config.inference_heartbeat_interval_secs),
                pending_evidence_ids, // Pass evidence IDs for message binding
                idempotency_key,      // Pass idempotency key for stream recovery
                Duration::from_secs(stream_config.max_pause_duration_secs.unwrap_or(1800)),
            ),
            Some(drop_guard), // Keep guard alive while stream is active
        ),
        |(mut stream_state, guard)| async move {
            match stream_state.next_event().await {
                Some(event) => {
                    let sse_event = stream_state.format_event(event);
                    Some((Ok(sse_event), (stream_state, guard)))
                }
                None => None,
            }
        },
    );

    // Chain: run envelope -> live stream
    let stream = stream::once(async move { Ok(run_envelope_event(&run_envelope)) }).chain(stream);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Stream loading progress then inference
///
/// This function monitors adapter loading progress via the lifecycle system,
/// emits Loading events as the model downloads/loads, then emits Ready when
/// the model passes health check, then streams inference tokens, and finally
/// emits Done event.
pub async fn stream_with_loading_progress(
    state: &AppState,
    request: StreamingInferRequest,
    run_envelope: adapteros_api_types::RunEnvelope,
    adapter_id: String,
    tenant_id: String,
    user_id: String,
    claims: Option<crate::auth::Claims>,
    session_lock: Option<ResolvedSessionTokenLock>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let state_clone = state.clone();
    let adapter_id_clone = adapter_id.clone();
    let tenant_id_clone = tenant_id.clone();
    let user_id_clone = user_id.clone();
    let claims_clone = claims.clone();

    stream::unfold(
        LoadingStreamState::new(
            state_clone,
            request,
            run_envelope,
            adapter_id_clone,
            tenant_id_clone,
            user_id_clone,
            claims_clone,
            session_lock,
        ),
        |mut loading_state| async move {
            match loading_state.next_loading_event().await {
                Some(event) => {
                    let sse_event = loading_state.format_loading_event(event);
                    Some((Ok(sse_event), loading_state))
                }
                None => None,
            }
        },
    )
}

fn spawn_streaming_inference(
    state: AppState,
    request: InferenceRequestInternal,
    cancellation_token: CancellationToken,
    token_buffer_capacity: usize,
) -> (
    mpsc::Receiver<WorkerStreamToken>,
    mpsc::Receiver<WorkerStreamPaused>,
    oneshot::Receiver<Result<InferenceResult, InferenceError>>,
) {
    // Bounded channel to apply backpressure when clients read slowly.
    let (token_tx, token_rx) = mpsc::channel(token_buffer_capacity);
    // Pause events are rare and should never create memory pressure.
    let (pause_tx, pause_rx) = mpsc::channel(8);
    let (done_tx, done_rx) = oneshot::channel();

    tokio::spawn(async move {
        let core = InferenceCore::new(&state);
        let result = core
            .route_and_infer_stream(
                request,
                None,
                Some(cancellation_token),
                token_tx,
                Some(pause_tx),
            )
            .await;
        let _ = done_tx.send(result);
    });

    (token_rx, pause_rx, done_rx)
}

/// State machine for loading progress streaming
struct LoadingStreamState {
    state: AppState,
    request: StreamingInferRequest,
    run_envelope: adapteros_api_types::RunEnvelope,
    adapter_id: String,
    tenant_id: String,
    user_id: String,
    cancellation_token: CancellationToken,
    phase: LoadingPhase,
    start_time: std::time::Instant,
    token_count: usize,
    request_id: String,
    /// Pinned adapters that were unavailable
    unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode
    pinned_routing_fallback: Option<String>,
    /// Whether backend fallback occurred during execution.
    fallback_triggered: bool,
    /// Backend selected after fallback (if different from requested).
    fallback_backend: Option<String>,
    /// Adapter attachments with reason + exact version metadata.
    adapter_attachments: Vec<adapteros_api_types::inference::AdapterAttachment>,
    /// Explicit degraded/fallback notices.
    degraded_notices: Vec<adapteros_api_types::inference::DegradedNotice>,
    /// User claims for policy enforcement
    claims: Option<crate::auth::Claims>,
    /// Session token adapter lock (if present)
    session_lock: Option<ResolvedSessionTokenLock>,
    token_rx: Option<mpsc::Receiver<WorkerStreamToken>>,
    /// Worker pause events (human-in-the-loop review)
    pause_rx: Option<mpsc::Receiver<WorkerStreamPaused>>,
    pause_rx_closed: bool,
    pause_active: bool,
    /// Pause ID of the currently active pause (if any), for server-side review verification
    active_pause_id: Option<String>,
    /// Whether the currently active pause is a safety-triggered pause requiring review
    /// before tokens can resume (policy_violation, threat_escalation, safety_gate, etc.)
    active_pause_is_safety: bool,
    done_rx: Option<oneshot::Receiver<Result<InferenceResult, InferenceError>>>,
    /// Stop reason code (PRD: Hard Deterministic Stop Controller)
    stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    stop_policy_digest_b3: Option<String>,
    /// Pending RAG evidence IDs for message binding
    pending_evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum LoadingPhase {
    CheckingState,
    LoadingAdapter,
    WaitingForReady,
    Inferring,
    Complete,
}

impl LoadingStreamState {
    fn new(
        state: AppState,
        request: StreamingInferRequest,
        run_envelope: adapteros_api_types::RunEnvelope,
        adapter_id: String,
        tenant_id: String,
        user_id: String,
        claims: Option<crate::auth::Claims>,
        session_lock: Option<ResolvedSessionTokenLock>,
    ) -> Self {
        let request_id = run_envelope.run_id.clone();
        Self {
            state,
            request,
            run_envelope,
            adapter_id,
            tenant_id,
            user_id,
            claims,
            session_lock,
            cancellation_token: CancellationToken::new(),
            phase: LoadingPhase::CheckingState,
            start_time: std::time::Instant::now(),
            token_count: 0,
            request_id,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            pending_evidence_ids: Vec::new(), // Initialized empty, populated during RAG retrieval
            token_rx: None,
            pause_rx: None,
            pause_rx_closed: false,
            pause_active: false,
            active_pause_id: None,
            active_pause_is_safety: false,
            done_rx: None,
        }
    }

    /// Check whether an active safety pause has been reviewed.
    ///
    /// Returns `true` if resume is allowed (pause was reviewed or no tracker configured).
    /// Returns `false` if the pause is still pending review — tokens must be dropped.
    fn is_safety_pause_reviewed(&self) -> bool {
        let pause_id = match self.active_pause_id.as_deref() {
            Some(id) => id,
            None => return true, // No active pause — allow
        };

        let tracker = match self.state.pause_tracker.as_ref() {
            Some(t) => t,
            None => return true, // No tracker configured — allow (non-production path)
        };

        // If the pause_id is still in the tracker, the review has NOT been submitted.
        // submit_review() removes the entry on success.
        tracker.get_state_by_pause_id(pause_id).is_none()
    }

    /// Clear pause tracking state when a pause ends (reviewed, done, or error).
    fn clear_pause_state(&mut self) {
        self.pause_active = false;
        self.active_pause_id = None;
        self.active_pause_is_safety = false;
    }

    async fn next_loading_event(&mut self) -> Option<InferenceEvent> {
        match self.phase {
            LoadingPhase::CheckingState => {
                // Check current adapter state
                match self.check_adapter_state().await {
                    Ok(is_ready) => {
                        if is_ready {
                            // Adapter is already loaded, skip to inference
                            self.phase = LoadingPhase::Inferring;
                            Some(InferenceEvent::Ready {
                                warmup_latency_ms: 0,
                            })
                        } else {
                            // Need to load adapter
                            self.phase = LoadingPhase::LoadingAdapter;
                            Some(InferenceEvent::Loading {
                                phase: LoadPhase::LoadingWeights,
                                progress: 0,
                                eta_seconds: Some(30),
                            })
                        }
                    }
                    Err(e) => {
                        self.phase = LoadingPhase::Complete;
                        Some(InferenceEvent::Error {
                            message: format!("Failed to check adapter state: {}", e),
                            recoverable: false,
                            code: None,
                        })
                    }
                }
            }
            LoadingPhase::LoadingAdapter => {
                // Simulate loading progress
                // In production, this would poll the lifecycle manager for actual progress
                match self.trigger_adapter_load().await {
                    Ok(_) => {
                        self.phase = LoadingPhase::WaitingForReady;
                        Some(InferenceEvent::Loading {
                            phase: LoadPhase::Warmup,
                            progress: 50,
                            eta_seconds: Some(10),
                        })
                    }
                    Err(e) => {
                        self.phase = LoadingPhase::Complete;
                        Some(InferenceEvent::Error {
                            message: format!("Failed to load adapter: {}", e),
                            recoverable: true,
                            code: None,
                        })
                    }
                }
            }
            LoadingPhase::WaitingForReady => {
                // Wait for adapter to be ready and perform warmup
                match self.wait_for_ready().await {
                    Ok(latency_ms) => {
                        self.phase = LoadingPhase::Inferring;
                        Some(InferenceEvent::Ready {
                            warmup_latency_ms: latency_ms,
                        })
                    }
                    Err(e) => {
                        self.phase = LoadingPhase::Complete;
                        Some(InferenceEvent::Error {
                            message: format!("Adapter failed to become ready: {}", e),
                            recoverable: true,
                            code: None,
                        })
                    }
                }
            }
            LoadingPhase::Inferring => {
                if self.token_rx.is_none() {
                    if let Err(e) = self.start_inference_stream().await {
                        self.phase = LoadingPhase::Complete;
                        return Some(InferenceEvent::Error {
                            message: format!("Inference failed: {}", e),
                            recoverable: false,
                            code: None,
                        });
                    }
                }

                // Select across both pause and token channels, matching StreamState's
                // pattern. A pause during loading/early-inference is safety-relevant
                // and must not be dropped.
                //
                // The loop handles the case where pause_rx closes (sender dropped):
                // we set pause_rx_closed and re-enter the select on token_rx only,
                // avoiding a recursive async call that would require boxing.
                loop {
                    let pause_closed = self.pause_rx_closed;

                    enum LoadingSelect {
                        Paused(Option<WorkerStreamPaused>),
                        Token(Option<WorkerStreamToken>),
                    }

                    let selected = if let Some(token_rx) = self.token_rx.as_mut() {
                        if let Some(pause_rx) = self.pause_rx.as_mut() {
                            tokio::select! {
                                paused = pause_rx.recv(), if !pause_closed => {
                                    LoadingSelect::Paused(paused)
                                }
                                token = token_rx.recv() => {
                                    LoadingSelect::Token(token)
                                }
                            }
                        } else {
                            LoadingSelect::Token(token_rx.recv().await)
                        }
                    } else {
                        LoadingSelect::Token(None)
                    };

                    match selected {
                        LoadingSelect::Paused(Some(paused)) => {
                            self.pause_active = true;
                            self.active_pause_id = Some(paused.pause_id.clone());
                            self.active_pause_is_safety = is_safety_trigger(&paused.trigger_kind);

                            // Redact partial text for safety-triggered pauses to avoid leaking
                            // the exact content that was flagged as unsafe to the client.
                            let text_so_far = if self.active_pause_is_safety {
                                None
                            } else {
                                paused.text_so_far
                            };

                            return Some(InferenceEvent::Paused {
                                pause_id: paused.pause_id,
                                inference_id: paused.inference_id,
                                trigger_kind: paused.trigger_kind,
                                context: paused.context,
                                text_so_far,
                                token_count: paused.token_count,
                            });
                        }
                        LoadingSelect::Paused(None) => {
                            // Sender dropped; stop selecting to avoid a tight loop.
                            self.pause_rx_closed = true;
                            // Re-enter to select on token_rx only.
                            continue;
                        }
                        LoadingSelect::Token(Some(token)) => {
                            // Server-side review gate: if a safety-triggered pause is active
                            // and the review has NOT been submitted yet, drop the token.
                            // This prevents a compromised or buggy worker from resuming
                            // inference without human approval for safety-critical pauses.
                            if self.pause_active
                                && self.active_pause_is_safety
                                && !self.is_safety_pause_reviewed()
                            {
                                warn!(
                                    request_id = %self.request_id,
                                    pause_id = ?self.active_pause_id,
                                    "Dropping token: safety pause active but review not yet submitted \
                                     (possible worker bug or compromise)"
                                );
                                continue;
                            }
                            if self.pause_active {
                                self.clear_pause_state();
                            }
                            self.token_count += 1;
                            return Some(InferenceEvent::Token {
                                text: token.text,
                                token_id: token.token_id,
                            });
                        }
                        LoadingSelect::Token(None) => {
                            // token_rx closed; fall through to done_rx handling below.
                            break;
                        }
                    }
                } // end loop

                let done_rx = self.done_rx.take();
                let result = if let Some(done_rx) = done_rx {
                    done_rx.await.ok()
                } else {
                    None
                };

                self.phase = LoadingPhase::Complete;
                let latency_ms = self.start_time.elapsed().as_millis() as u64;

                match result {
                    Some(Ok(result)) => {
                        self.unavailable_pinned_adapters = result.unavailable_pinned_adapters;
                        self.pinned_routing_fallback = result.pinned_routing_fallback;
                        self.fallback_triggered = result.fallback_triggered;
                        self.fallback_backend = result.fallback_backend.clone();
                        self.adapter_attachments = result.adapter_attachments.clone();
                        self.degraded_notices = result.degraded_notices.clone();
                        self.stop_reason_code = result.stop_reason_code;
                        self.stop_reason_token_index = result.stop_reason_token_index;
                        self.stop_policy_digest_b3 = result.stop_policy_digest_b3.clone();
                        let citations = result.citations;
                        let document_links = result.document_links;

                        // Fire OnAfterInference hook at stream completion
                        // NOTE: For streaming, this is fire-and-forget audit logging only.
                        // Tokens have already been sent to the client, so we cannot block
                        // the response based on Evidence/Refusal policy violations.
                        let state_clone = self.state.clone();
                        let tenant_id = self.tenant_id.clone();
                        let user_id = self.user_id.clone();
                        let request_id = self.request_id.clone();
                        let adapter_id = self.adapter_id.clone();
                        let claims_clone = self.claims.clone();

                        tokio::spawn(async move {
                            // Use real claims if available, fallback to basic user claims
                            let claims = claims_clone.unwrap_or_else(|| crate::auth::Claims {
                                sub: user_id.clone(),
                                email: String::new(),
                                role: "user".to_string(),
                                roles: Vec::new(),
                                tenant_id: tenant_id.clone(),
                                admin_tenants: Vec::new(),
                                device_id: None,
                                session_id: None,
                                mfa_level: None,
                                rot_id: None,
                                exp: 0,
                                iat: 0,
                                jti: TypedId::new(IdPrefix::Tok).to_string(),
                                nbf: 0,
                                iss: JWT_ISSUER.to_string(),
                                auth_mode: AuthMode::BearerToken,
                                principal_type: Some(PrincipalType::User),
                            });

                            let hook_ctx = create_hook_context(
                                &claims,
                                &request_id,
                                PolicyHook::OnAfterInference,
                                "streaming_inference",
                                Some(&adapter_id),
                            );

                            if let Err(violation) = enforce_at_hook(&state_clone, &hook_ctx).await {
                                warn!(
                                    tenant_id = %tenant_id,
                                    request_id = %request_id,
                                    violations = ?violation.violations.len(),
                                    "OnAfterInference policy violation in loading stream (audit only)"
                                );
                            }
                        });

                        Some(InferenceEvent::Done {
                            total_tokens: self.token_count,
                            latency_ms,
                            unavailable_pinned_adapters: self.unavailable_pinned_adapters.clone(),
                            pinned_routing_fallback: self.pinned_routing_fallback.clone(),
                            citations: if citations.is_empty() {
                                None
                            } else {
                                Some(citations)
                            },
                            document_links: if document_links.is_empty() {
                                None
                            } else {
                                Some(document_links)
                            },
                            adapter_attachments: if self.adapter_attachments.is_empty() {
                                None
                            } else {
                                Some(self.adapter_attachments.clone())
                            },
                            degraded_notices: if self.degraded_notices.is_empty() {
                                None
                            } else {
                                Some(self.degraded_notices.clone())
                            },
                            fallback_triggered: self.fallback_triggered,
                            fallback_backend: self.fallback_backend.clone(),
                            stop_reason_code: self.stop_reason_code,
                            stop_reason_token_index: self.stop_reason_token_index,
                            stop_policy_digest_b3: self.stop_policy_digest_b3.clone(),
                            pending_evidence_ids: self.pending_evidence_ids.clone(),
                        })
                    }
                    Some(Err(err)) => Some(InferenceEvent::Error {
                        message: format!("Inference failed: {}", err),
                        recoverable: false,
                        code: Some(err.error_code().to_string()),
                    }),
                    None => Some(InferenceEvent::Error {
                        message: "Stream ended without completion".to_string(),
                        recoverable: false,
                        code: None,
                    }),
                }
            }
            LoadingPhase::Complete => None,
        }
    }

    async fn check_adapter_state(&self) -> Result<bool, String> {
        // Query database to check if adapter is in warm, hot, or resident state
        match self
            .state
            .db
            .get_adapter_by_id(&self.tenant_id, &self.adapter_id)
            .await
        {
            Ok(Some(adapter)) => {
                // First check if adapter is archived or purged - reject immediately
                if adapter.archived_at.is_some() {
                    warn!(
                        adapter_id = %self.adapter_id,
                        tenant_id = %self.tenant_id,
                        archived_at = ?adapter.archived_at,
                        "Rejected inference request for archived adapter"
                    );
                    return Err(format!(
                        "Adapter '{}' is archived and cannot be used for inference",
                        self.adapter_id
                    ));
                }
                if adapter.purged_at.is_some() {
                    warn!(
                        adapter_id = %self.adapter_id,
                        tenant_id = %self.tenant_id,
                        purged_at = ?adapter.purged_at,
                        "Rejected inference request for purged adapter"
                    );
                    return Err(format!(
                        "Adapter '{}' has been purged and cannot be used for inference",
                        self.adapter_id
                    ));
                }

                // Check if adapter is in a ready state (runtime state, not lifecycle state)
                // current_state tracks runtime memory state: unloaded, cold, warm, hot, resident
                // lifecycle_state tracks metadata state: draft, active, deprecated, retired
                let is_ready =
                    matches!(adapter.current_state.as_str(), "warm" | "hot" | "resident");
                info!(
                    adapter_id = %self.adapter_id,
                    tenant_id = %self.tenant_id,
                    current_state = %adapter.current_state,
                    is_ready = is_ready,
                    "Checked adapter state"
                );
                Ok(is_ready)
            }
            Ok(None) => Err(format!("Adapter not found: {}", self.adapter_id)),
            Err(e) => Err(format!("Database error: {}", e)),
        }
    }

    async fn trigger_adapter_load(&self) -> Result<(), String> {
        // Use LoadCoordinator to prevent thundering herd
        let adapter_id = self.adapter_id.clone();
        let state = self.state.clone();
        let tenant_id = self.tenant_id.clone();
        let state_for_load = state.clone();
        let tenant_id_for_load = tenant_id.clone();
        let adapter_id_for_load = adapter_id.clone();

        // Wrap the actual load operation with LoadCoordinator
        // This ensures only one request actually triggers the load
        // Use timeout from config to prevent indefinite hangs
        let load_timeout = state
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .timeouts
            .adapter_load_timeout();
        let _handle = state
            .load_coordinator
            .load_or_wait_with_timeout(&adapter_id, load_timeout, move || {
                let state = state_for_load.clone();
                let tenant_id = tenant_id_for_load.clone();
                let adapter_id = adapter_id_for_load.clone();
                async move {
                    let core = InferenceCore::new(&state);
                    let worker = core
                        .select_worker_for_tenant(&tenant_id)
                        .await
                        .map_err(|e| adapteros_core::AosError::Worker(e.to_string()))?;

                    let uds_path = std::path::PathBuf::from(&worker.uds_path);
                    let uds_client = UdsClient::new(Duration::from_secs(30));

                    // Send load request via HTTP endpoint
                    let load_request = serde_json::json!({
                        "adapter_id": adapter_id,
                        "command": "load"
                    });

                    uds_client
                        .send_http_request(&uds_path, "POST", "/adapter/load", Some(load_request))
                        .await
                        .map_err(|e| {
                            adapteros_core::AosError::Lifecycle(format!(
                                "Failed to send load command: {}",
                                e
                            ))
                        })?;

                    info!(adapter_id = %adapter_id, "Triggered adapter load");

                    // Return a dummy AdapterHandle since we don't have the actual handle yet
                    // The lifecycle manager will track the actual state
                    Ok(adapteros_lora_lifecycle::loader::AdapterHandle {
                        adapter_id: 0, // Placeholder
                        path: uds_path,
                        memory_bytes: 0,
                        metadata: adapteros_lora_lifecycle::loader::AdapterMetadata {
                            num_parameters: 0,
                            rank: None,
                            target_modules: vec![],
                            ..Default::default()
                        },
                    })
                }
            })
            .await
            .map_err(|e| {
                // Check if this is a timeout error and provide a clear message
                let error_msg = e.to_string();
                if error_msg.contains("Timeout")
                    || matches!(e, adapteros_core::AosError::Timeout { .. })
                {
                    warn!(
                        adapter_id = %adapter_id,
                        timeout_secs = load_timeout.as_secs(),
                        "Adapter load timed out - consider increasing AOS_ADAPTER_LOAD_TIMEOUT_SECS"
                    );
                    format!(
                        "ADAPTER_LOAD_TIMEOUT: Adapter '{}' load timed out after {} seconds",
                        adapter_id,
                        load_timeout.as_secs()
                    )
                } else {
                    format!("Load coordination failed: {}", e)
                }
            })?;

        Ok(())
    }

    async fn wait_for_ready(&self) -> Result<u64, String> {
        let start = std::time::Instant::now();
        // Use configured timeout from state, falling back to default
        let timeout = self
            .state
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .timeouts
            .adapter_load_timeout();
        let poll_interval = Duration::from_millis(500);

        // Poll adapter state until it becomes ready or timeout
        loop {
            if start.elapsed() > timeout {
                return Err("Timeout waiting for adapter to become ready".to_string());
            }

            match self.check_adapter_state().await {
                Ok(true) => {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    info!(
                        adapter_id = %self.adapter_id,
                        latency_ms = latency_ms,
                        "Adapter is ready"
                    );
                    return Ok(latency_ms);
                }
                Ok(false) => {
                    tokio::time::sleep(poll_interval).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn start_inference_stream(&mut self) -> Result<(), String> {
        // Build Claims from stored tenant/user info, or use provided claims
        let claims = self.claims.clone().unwrap_or_else(|| Claims {
            sub: self.user_id.clone(),
            email: String::new(),
            role: "user".to_string(),
            roles: Vec::new(),
            tenant_id: self.tenant_id.clone(),
            admin_tenants: Vec::new(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: TypedId::new(IdPrefix::Tok).to_string(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        });

        // Convert StreamingInferRequest to InferenceRequestInternal
        let mut internal_request: InferenceRequestInternal = (&self.request, &claims).into();
        internal_request.request_id = self.request_id.clone();
        internal_request.run_envelope = Some(self.run_envelope.clone());
        internal_request.prompt = self.request.prompt.clone();
        internal_request.session_id = self.request.session_id.clone();
        internal_request.adapters = self.request.adapters.clone();
        internal_request.adapter_strength_overrides =
            self.request.adapter_strength_overrides.clone();
        internal_request.model = self.request.model.clone();
        internal_request.stop_policy = self.request.stop_policy.clone();
        internal_request.created_at = std::time::Instant::now();
        if let Some(lock) = self.session_lock.as_ref() {
            internal_request.adapter_stack = None;
            internal_request.adapters = Some(lock.adapter_ids.clone());
            internal_request.effective_adapter_ids = Some(lock.adapter_ids.clone());
            internal_request.stack_id = lock.stack_id.clone();
            internal_request.pinned_adapter_ids = Some(lock.pinned_adapter_ids.clone());
            if let Some(backend) = lock.backend_profile {
                internal_request.backend_profile = Some(backend);
                internal_request.allow_fallback = backend == adapteros_core::BackendKind::Auto;
            }
            if let Some(coreml_mode) = lock.coreml_mode {
                internal_request.coreml_mode = Some(coreml_mode);
            }
        }

        let stream_config = self
            .state
            .config
            .read()
            .unwrap_or_else(|e| {
                tracing::warn!("Config lock poisoned in loading stream, recovering");
                e.into_inner()
            })
            .streaming
            .clone();
        let (token_rx, pause_rx, done_rx) = spawn_streaming_inference(
            self.state.clone(),
            internal_request,
            self.cancellation_token.clone(),
            stream_config.inference_token_buffer_capacity,
        );

        self.token_rx = Some(token_rx);
        self.pause_rx = Some(pause_rx);
        self.done_rx = Some(done_rx);
        Ok(())
    }

    fn format_loading_event(&self, event: InferenceEvent) -> Event {
        // P2 HARDENING: Assert Run ID consistency between request and envelope
        // This is a release-mode assertion - ID mutation breaks determinism receipts
        assert_eq!(
            self.request_id, self.run_envelope.run_id,
            "Run ID mismatch in LoadingStreamState: request_id ({}) != envelope.run_id ({}). \
             This indicates envelope recreation occurred after streaming began.",
            self.request_id, self.run_envelope.run_id
        );

        let json = serialize_safe(&event, "loading_event");
        Event::default().data(json)
    }
}

/// Internal state for streaming generation
#[allow(dead_code)]
struct StreamState {
    /// Application state
    state: AppState,
    request_id: String,
    run_envelope: adapteros_api_types::RunEnvelope,
    model_name: String,
    // State machine
    phase: StreamPhase,
    // Worker token stream
    token_rx: mpsc::Receiver<WorkerStreamToken>,
    /// Worker pause events (human-in-the-loop review)
    pause_rx: mpsc::Receiver<WorkerStreamPaused>,
    pause_rx_closed: bool,
    pause_active: bool,
    /// When the current pause started (set on pause entry, cleared on resume).
    pause_started_at: Option<Instant>,
    /// Maximum duration a pause may hold the connection open before expiring.
    max_pause_duration: Duration,
    /// Pause ID of the currently active pause (if any), for server-side review verification
    active_pause_id: Option<String>,
    /// Whether the currently active pause is a safety-triggered pause requiring review
    /// before tokens can resume (policy_violation, threat_escalation, safety_gate, etc.)
    active_pause_is_safety: bool,
    done_rx: Option<oneshot::Receiver<Result<InferenceResult, InferenceError>>>,
    // Idle timeout tracking
    last_token_at: Instant,
    idle_timeout: Duration,
    heartbeat_interval: Duration,
    // Cancellation token for stream abort
    cancellation_token: CancellationToken,
    // Session tracking
    session_id: Option<String>,
    adapters: Option<Vec<String>>,
    // Hook context for OnAfterInference
    tenant_id: String,
    user_id: String,
    // Track if after-hook has been fired (one-shot)
    after_hook_fired: bool,
    // User claims for policy enforcement
    claims: Option<crate::auth::Claims>,
    // Stop controller metadata (PRD: Hard Deterministic Stop Controller)
    stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    stop_reason_token_index: Option<u32>,
    stop_policy_digest_b3: Option<String>,
    citations: Vec<adapteros_api_types::inference::Citation>,
    document_links: Vec<adapteros_api_types::inference::DocumentLink>,
    adapters_used: Vec<String>,
    unavailable_pinned_adapters: Option<Vec<String>>,
    pinned_routing_fallback: Option<String>,
    fallback_triggered: bool,
    fallback_backend: Option<String>,
    adapter_attachments: Vec<adapteros_api_types::inference::AdapterAttachment>,
    degraded_notices: Vec<adapteros_api_types::inference::DegradedNotice>,
    // Pending RAG evidence IDs for message binding
    pending_evidence_ids: Vec<String>,
    // Stream lifecycle tracking for reliability
    /// Unique stream ID for recovery (derived from request_id)
    stream_id: String,
    /// Optional idempotency key for stream recovery on reconnect
    idempotency_key: Option<String>,
    /// Token count for stream_finished event
    token_count: usize,
    /// Stream start time for duration calculation
    stream_start: Instant,
    /// Finish reason captured from Done event for stream_finished
    captured_finish_reason: Option<String>,
}

/// Guard that cancels the stream when dropped (client disconnect detection)
///
/// When the SSE response body is dropped (client disconnects), this guard
/// triggers the cancellation token, allowing in-flight operations to abort.
struct StreamDropGuard {
    cancellation_token: CancellationToken,
    request_id: String,
}

impl Drop for StreamDropGuard {
    fn drop(&mut self) {
        if !self.cancellation_token.is_cancelled() {
            info!(
                request_id = %self.request_id,
                "Stream dropped (client disconnect), cancelling"
            );
            self.cancellation_token.cancel();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum StreamPhase {
    /// Initial lifecycle event - stream_started
    StreamLifecycleStart,
    /// First chunk with role
    Start,
    /// Streaming tokens
    StreamingTokens,
    /// Sending final lifecycle event - stream_finished
    StreamLifecycleFinish,
    /// Stream complete
    Done,
}

impl StreamState {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: AppState,
        request_id: String,
        run_envelope: adapteros_api_types::RunEnvelope,
        model_name: String,
        token_rx: mpsc::Receiver<WorkerStreamToken>,
        pause_rx: mpsc::Receiver<WorkerStreamPaused>,
        done_rx: oneshot::Receiver<Result<InferenceResult, InferenceError>>,
        session_id: Option<String>,
        adapters: Option<Vec<String>>,
        tenant_id: String,
        user_id: String,
        claims: Option<crate::auth::Claims>,
        cancellation_token: CancellationToken,
        idle_timeout: Duration,
        heartbeat_interval: Duration,
        pending_evidence_ids: Vec<String>,
        idempotency_key: Option<String>,
        max_pause_duration: Duration,
    ) -> Self {
        let canonical_request_id = run_envelope.run_id.clone();
        if request_id != canonical_request_id {
            warn!(
                request_id = %request_id,
                run_id = %canonical_request_id,
                "Streaming request_id mismatch; using run_envelope.run_id"
            );
        }

        // Generate stream_id from request_id for recovery tracking
        let stream_id = format!("stream_{}", canonical_request_id);

        Self {
            state,
            request_id: canonical_request_id,
            run_envelope,
            model_name,
            phase: StreamPhase::StreamLifecycleStart,
            tenant_id,
            user_id,
            after_hook_fired: false,
            token_rx,
            pause_rx,
            pause_rx_closed: false,
            pause_active: false,
            pause_started_at: None,
            max_pause_duration,
            active_pause_id: None,
            active_pause_is_safety: false,
            done_rx: Some(done_rx),
            last_token_at: Instant::now(),
            idle_timeout,
            heartbeat_interval,
            cancellation_token,
            session_id,
            adapters,
            claims,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            citations: Vec::new(),
            document_links: Vec::new(),
            adapters_used: Vec::new(),
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            fallback_triggered: false,
            fallback_backend: None,
            adapter_attachments: Vec::new(),
            degraded_notices: Vec::new(),
            pending_evidence_ids,
            // Stream lifecycle fields
            stream_id,
            idempotency_key,
            token_count: 0,
            stream_start: Instant::now(),
            captured_finish_reason: None,
        }
    }

    /// Check if stream has been idle for too long.
    ///
    /// When the worker pauses inference for review, the stream remains open
    /// to allow resume on the same UDS stream -- but only up to
    /// `max_pause_duration`. After that the pause is considered expired and
    /// the normal idle-timeout check resumes.
    fn is_idle(&mut self) -> bool {
        if self.pause_active {
            if let Some(started) = self.pause_started_at {
                if started.elapsed() >= self.max_pause_duration {
                    warn!(
                        request_id = %self.request_id,
                        pause_secs = started.elapsed().as_secs(),
                        max_secs = self.max_pause_duration.as_secs(),
                        "Pause exceeded max_pause_duration, expiring"
                    );
                    self.clear_pause_state();
                    // Fall through to the normal idle check below.
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }
        self.last_token_at.elapsed() > self.idle_timeout
    }

    fn mark_token_activity(&mut self) {
        self.last_token_at = Instant::now();
    }

    /// Returns true if the given trigger kind represents a safety-critical pause
    /// that requires human review before tokens may resume.
    ///
    /// Delegates to the module-level `is_safety_trigger` function which is the
    /// canonical implementation shared with `format_event` for SSE redaction.
    fn is_safety_trigger(trigger_kind: &str) -> bool {
        is_safety_trigger(trigger_kind)
    }

    /// Check whether an active safety pause has been reviewed.
    ///
    /// Returns `true` if resume is allowed (pause was reviewed or no tracker configured).
    /// Returns `false` if the pause is still pending review — tokens must be dropped.
    ///
    /// This is a non-blocking read lock on the pause tracker; it will never deadlock
    /// the token receive path.
    fn is_safety_pause_reviewed(&self) -> bool {
        let pause_id = match self.active_pause_id.as_deref() {
            Some(id) => id,
            None => return true, // No active pause — allow
        };

        let tracker = match self.state.pause_tracker.as_ref() {
            Some(t) => t,
            None => return true, // No tracker configured — allow (non-production path)
        };

        // If the pause_id is still in the tracker, the review has NOT been submitted.
        // submit_review() removes the entry on success.
        tracker.get_state_by_pause_id(pause_id).is_none()
    }

    /// Clear pause tracking state when a pause ends (reviewed, done, or error).
    fn clear_pause_state(&mut self) {
        self.pause_active = false;
        self.pause_started_at = None;
        self.active_pause_id = None;
        self.active_pause_is_safety = false;
    }

    fn error_event(&self, code: &str, message: impl Into<String>, retryable: bool) -> StreamEvent {
        StreamEvent::Error {
            code: code.to_string(),
            message: message.into(),
            retryable,
        }
    }

    fn map_inference_error(&self, err: InferenceError) -> StreamEvent {
        let retryable = matches!(
            err,
            InferenceError::WorkerNotAvailable(_)
                | InferenceError::WorkerError(_)
                | InferenceError::Timeout(_)
                | InferenceError::BackpressureError(_)
                | InferenceError::CacheBudgetExceeded { .. }
                | InferenceError::NoCompatibleWorker { .. }
                | InferenceError::WorkerDegraded { .. }
        );
        self.error_event(err.error_code(), err.to_string(), retryable)
    }

    /// Check if stream has been cancelled
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    async fn next_event(&mut self) -> Option<StreamEvent> {
        // Check for cancellation (client disconnect)
        if self.is_cancelled() {
            warn!(request_id = %self.request_id, "Stream cancelled by client disconnect");
            self.phase = StreamPhase::Done;
            return Some(self.error_event("STREAM_CANCELLED", "Stream cancelled", false));
        }

        // Check for idle timeout
        if self.is_idle() {
            warn!(request_id = %self.request_id, "Stream idle timeout");
            self.phase = StreamPhase::Done;
            return Some(self.error_event("STREAM_IDLE_TIMEOUT", "Stream idle timeout", true));
        }

        match self.phase {
            StreamPhase::StreamLifecycleStart => {
                // Send stream_started lifecycle event first
                self.phase = StreamPhase::Start;
                info!(
                    request_id = %self.request_id,
                    stream_id = %self.stream_id,
                    idempotency_key = ?self.idempotency_key,
                    "Stream lifecycle started"
                );
                Some(StreamEvent::StreamStarted {
                    stream_id: self.stream_id.clone(),
                    idempotency_key: self.idempotency_key.clone(),
                })
            }
            StreamPhase::Start => {
                // Send initial role chunk
                self.phase = StreamPhase::StreamingTokens;
                Some(StreamEvent::Start)
            }
            StreamPhase::StreamingTokens => loop {
                if self.is_cancelled() {
                    warn!(request_id = %self.request_id, "Stream cancelled by client disconnect");
                    self.phase = StreamPhase::Done;
                    return Some(self.error_event("STREAM_CANCELLED", "Stream cancelled", false));
                }

                if self.is_idle() {
                    warn!(request_id = %self.request_id, "Stream idle timeout");
                    self.phase = StreamPhase::Done;
                    return Some(self.error_event(
                        "STREAM_IDLE_TIMEOUT",
                        "Stream idle timeout",
                        true,
                    ));
                }

                let heartbeat_interval = if self.heartbeat_interval.is_zero() {
                    Duration::from_secs(3600)
                } else {
                    self.heartbeat_interval
                };
                let heartbeat_in = heartbeat_interval.saturating_sub(self.last_token_at.elapsed());

                tokio::select! {
                    paused = self.pause_rx.recv(), if !self.pause_rx_closed => {
                        match paused {
                            Some(paused) => {
                                self.pause_active = true;
                                self.pause_started_at = Some(Instant::now());
                                self.active_pause_id = Some(paused.pause_id.clone());
                                self.active_pause_is_safety = Self::is_safety_trigger(&paused.trigger_kind);
                                if self.active_pause_is_safety {
                                    info!(
                                        request_id = %self.request_id,
                                        pause_id = %paused.pause_id,
                                        trigger_kind = %paused.trigger_kind,
                                        "Safety pause activated — tokens gated until review submitted"
                                    );
                                }
                                return Some(StreamEvent::Paused(paused));
                            }
                            None => {
                                // Sender dropped after completion; stop selecting on this receiver
                                // to avoid a tight loop.
                                self.pause_rx_closed = true;
                            }
                        }
                    }
                    token = self.token_rx.recv() => {
                        match token {
                            Some(token) => {
                                // Server-side review gate: if a safety-triggered pause is active
                                // and the review has NOT been submitted yet, drop the token.
                                // This prevents a compromised or buggy worker from resuming
                                // inference without human approval for safety-critical pauses.
                                if self.pause_active && self.active_pause_is_safety {
                                    if !self.is_safety_pause_reviewed() {
                                        warn!(
                                            request_id = %self.request_id,
                                            pause_id = ?self.active_pause_id,
                                            "Dropping token: safety pause active but review not yet submitted \
                                             (possible worker bug or compromise)"
                                        );
                                        // Do NOT clear pause_active, do NOT emit the token.
                                        // Continue the loop to wait for more events.
                                        continue;
                                    }
                                    // Review was submitted — allow resume and clear pause state.
                                    info!(
                                        request_id = %self.request_id,
                                        pause_id = ?self.active_pause_id,
                                        "Safety pause review verified — resuming token flow"
                                    );
                                }
                                self.mark_token_activity();
                                self.clear_pause_state();
                                self.token_count += 1;
                                return Some(StreamEvent::Token(token.text));
                            }
                            None => {
                                let done_rx = self.done_rx.take();
                                let result = if let Some(done_rx) = done_rx {
                                    done_rx.await.ok()
                                } else {
                                    None
                                };

                                match result {
                                    Some(Ok(result)) => {
                                        self.stop_reason_code = result.stop_reason_code;
                                        self.stop_reason_token_index = result.stop_reason_token_index;
                                        self.stop_policy_digest_b3 = result.stop_policy_digest_b3.clone();
                                        self.citations = result.citations.clone();
                                        self.document_links = result.document_links.clone();
                                        self.adapters_used = result.adapters_used.clone();
                                        self.unavailable_pinned_adapters =
                                            result.unavailable_pinned_adapters.clone();
                                        self.pinned_routing_fallback =
                                            result.pinned_routing_fallback.clone();
                                        self.fallback_triggered = result.fallback_triggered;
                                        self.fallback_backend = result.fallback_backend.clone();
                                        self.adapter_attachments = result.adapter_attachments.clone();
                                        self.degraded_notices = result.degraded_notices.clone();
                                        self.clear_pause_state();
                                        // Capture finish_reason for stream_finished event
                                        self.captured_finish_reason = Some(result.finish_reason.clone());
                                        // Transition to lifecycle finish phase instead of Done
                                        self.phase = StreamPhase::StreamLifecycleFinish;
                                        return Some(StreamEvent::Done {
                                            finish_reason: result.finish_reason,
                                        });
                                    }
                                    Some(Err(err)) => {
                                        // Dev echo mode: return echo token instead of error
                                        // when worker is unavailable in dev bypass mode
                                        if is_dev_bypass_enabled()
                                            && matches!(
                                                err,
                                                InferenceError::WorkerDegraded { .. }
                                                    | InferenceError::NoCompatibleWorker { .. }
                                                    | InferenceError::WorkerError(_)
                                                    | InferenceError::WorkerNotAvailable(_)
                                                    | InferenceError::ModelNotReady(_)
                                            )
                                        {
                                            info!(
                                                request_id = %self.request_id,
                                                error = %err,
                                                "Dev echo mode (stream): returning mock token"
                                            );
                                            self.token_count += 1;
                                            self.captured_finish_reason = Some("dev_echo".to_string());
                                            self.phase = StreamPhase::StreamLifecycleFinish;
                                            // Return echo text as a single token
                                            return Some(StreamEvent::Token(
                                                "[DEV ECHO] No inference worker available. Start a worker to enable real inference.".to_string()
                                            ));
                                        }
                                        self.phase = StreamPhase::Done;
                                        self.clear_pause_state();
                                        return Some(self.map_inference_error(err));
                                    }
                                    None => {
                                        self.phase = StreamPhase::Done;
                                        self.clear_pause_state();
                                        return Some(self.error_event(
                                            "STREAM_ENDED",
                                            "Stream ended without completion",
                                            true,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(heartbeat_in) => {
                        if !self.heartbeat_interval.is_zero()
                            && self.last_token_at.elapsed() >= self.heartbeat_interval
                        {
                            return Some(StreamEvent::Heartbeat);
                        }
                    }
                }
            },
            StreamPhase::StreamLifecycleFinish => {
                // Send stream_finished lifecycle event
                let duration_ms = self.stream_start.elapsed().as_millis() as u64;
                info!(
                    request_id = %self.request_id,
                    stream_id = %self.stream_id,
                    token_count = self.token_count,
                    duration_ms = duration_ms,
                    "Stream lifecycle finished"
                );
                self.phase = StreamPhase::Done;
                Some(StreamEvent::StreamFinished {
                    stream_id: self.stream_id.clone(),
                    total_tokens: self.token_count,
                    duration_ms,
                })
            }
            StreamPhase::Done => None,
        }
    }

    fn format_event(&self, event: StreamEvent) -> Event {
        // P2 HARDENING: Assert Run ID consistency between chunk and envelope
        // This is a release-mode assertion - ID mutation breaks determinism receipts
        assert_eq!(
            self.request_id, self.run_envelope.run_id,
            "Run ID mismatch in StreamState: chunk.id ({}) != envelope.run_id ({}). \
             This indicates envelope recreation occurred after streaming began.",
            self.request_id, self.run_envelope.run_id
        );

        match event {
            StreamEvent::StreamStarted {
                stream_id,
                idempotency_key,
            } => {
                // Lifecycle event: stream_started
                // Sent as first event to enable client recovery tracking
                let lifecycle_event = serde_json::json!({
                    "type": "stream_started",
                    "stream_id": stream_id,
                    "request_id": self.request_id,
                    "idempotency_key": idempotency_key,
                    "timestamp_ms": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                });
                Event::default()
                    .event("stream_started")
                    .data(serialize_safe(&lifecycle_event, "stream_started"))
            }
            StreamEvent::Start => {
                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: Some("assistant".to_string()),
                            content: None,
                        },
                        finish_reason: None,
                        stop_reason_code: None,
                        stop_reason_token_index: None,
                        stop_policy_digest_b3: None,
                    }],
                };
                Event::default().data(serialize_safe(&chunk, "stream_start"))
            }
            StreamEvent::Token(content) => {
                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: Some(content),
                        },
                        finish_reason: None,
                        stop_reason_code: None,
                        stop_reason_token_index: None,
                        stop_policy_digest_b3: None,
                    }],
                };
                Event::default().data(serialize_safe(&chunk, "stream_token"))
            }
            StreamEvent::Paused(paused) => {
                // Redact partial text for safety-triggered pauses to avoid leaking
                // the exact content that was flagged as unsafe to the client.
                let text_so_far = if is_safety_trigger(&paused.trigger_kind) {
                    None
                } else {
                    paused.text_so_far
                };
                let payload = InferenceEvent::Paused {
                    pause_id: paused.pause_id,
                    inference_id: paused.inference_id,
                    trigger_kind: paused.trigger_kind,
                    context: paused.context,
                    text_so_far,
                    token_count: paused.token_count,
                };
                Event::default()
                    .event("paused")
                    .data(serialize_safe(&payload, "paused_inference"))
            }
            StreamEvent::Heartbeat => Event::default().comment("heartbeat"),
            StreamEvent::Done { finish_reason } => {
                // Fire OnAfterInference hook at stream completion
                // NOTE: For streaming, this is fire-and-forget audit logging only.
                // Tokens have already been sent to the client, so we cannot block
                // the response based on Evidence/Refusal policy violations.
                // The hook logs to policy_audit_decisions for compliance tracking.
                let state_clone = self.state.clone();
                let tenant_id = self.tenant_id.clone();
                let user_id = self.user_id.clone();
                let request_id = self.request_id.clone();
                let adapter_id = self.adapters.as_ref().and_then(|a| a.first()).cloned();
                let adapter_ids_sorted: Vec<String> = self
                    .adapters
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .collect();
                let mut adapter_ids_sorted = adapter_ids_sorted;
                adapter_ids_sorted.sort();
                // P0 audit-correctness: preserve original claims for policy enforcement
                let claims_clone = self.claims.clone();

                tokio::spawn(async move {
                    // Use stored claims with fallback only for truly None cases
                    let claims_for_hook = claims_clone.unwrap_or_else(|| crate::auth::Claims {
                        sub: user_id.clone(),
                        email: String::new(),
                        role: "user".to_string(),
                        roles: Vec::new(),
                        tenant_id: tenant_id.clone(),
                        admin_tenants: Vec::new(),
                        device_id: None,
                        session_id: None,
                        mfa_level: None,
                        rot_id: None,
                        exp: 0,
                        iat: 0,
                        jti: TypedId::new(IdPrefix::Tok).to_string(),
                        nbf: 0,
                        iss: JWT_ISSUER.to_string(),
                        auth_mode: AuthMode::BearerToken,
                        principal_type: Some(PrincipalType::User),
                    });
                    let hook_ctx = create_hook_context(
                        &claims_for_hook,
                        &request_id,
                        PolicyHook::OnAfterInference,
                        "streaming_inference",
                        adapter_id.as_deref(),
                    )
                    // P0 audit-correctness: include the full adapter ID set deterministically.
                    .with_metadata("adapter_ids", serde_json::json!(adapter_ids_sorted));

                    if let Err(violation) = enforce_at_hook(&state_clone, &hook_ctx).await {
                        // Log but don't fail - tokens already sent to client
                        warn!(
                            tenant_id = %tenant_id,
                            request_id = %request_id,
                            violations = ?violation.violations.len(),
                            "OnAfterInference policy violation in streaming (audit only, response already sent)"
                        );
                    }
                });

                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: None,
                        },
                        finish_reason: Some(finish_reason),
                        stop_reason_code: self.stop_reason_code,
                        stop_reason_token_index: self.stop_reason_token_index,
                        stop_policy_digest_b3: self.stop_policy_digest_b3.clone(),
                    }],
                };
                let chunk_json = serialize_safe(&chunk, "stream_done");
                // Send final chunk followed by [DONE]
                Event::default().data(format!("{}\n\ndata: [DONE]", chunk_json))
            }
            StreamEvent::StreamFinished {
                stream_id,
                total_tokens,
                duration_ms,
            } => {
                // Lifecycle event: stream_finished
                // Sent as final event to confirm stream completion
                let lifecycle_event = serde_json::json!({
                    "type": "stream_finished",
                    "stream_id": stream_id,
                    "request_id": self.request_id,
                    "total_tokens": total_tokens,
                    "duration_ms": duration_ms,
                    "finish_reason": self.captured_finish_reason,
                    "citations": self.citations.clone(),
                    "document_links": self.document_links.clone(),
                    "adapters_used": self.adapters_used.clone(),
                    "unavailable_pinned_adapters": self.unavailable_pinned_adapters.clone(),
                    "pinned_routing_fallback": self.pinned_routing_fallback.clone(),
                    "fallback_triggered": self.fallback_triggered,
                    "fallback_backend": self.fallback_backend.clone(),
                    "adapter_attachments": self.adapter_attachments.clone(),
                    "degraded_notices": self.degraded_notices.clone(),
                    "timestamp_ms": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                });
                Event::default()
                    .event("stream_finished")
                    .data(serialize_safe(&lifecycle_event, "stream_finished"))
            }
            StreamEvent::Error {
                code,
                message,
                retryable,
            } => {
                let error_response = StreamErrorPayload {
                    code,
                    message,
                    retryable,
                    correlation_id: self.request_id.clone(),
                };
                Event::default()
                    .event("error")
                    .data(serialize_safe(&error_response, "stream_error"))
            }
        }
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::rag_common::parse_rag_doc_id;
    use crate::state::MetricsConfig;
    use crate::telemetry::MetricsRegistry;
    use crate::test_utils;
    use crate::{ApiConfig, PathsConfig};
    use adapteros_core::{BackendKind, SeedMode};
    use adapteros_metrics_exporter::MetricsExporter;
    use adapteros_telemetry::metrics::MetricsConfig as TelemetryMetricsConfig;
    use adapteros_telemetry::MetricsCollector;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use futures_util::stream;
    use std::path::PathBuf;
    use std::sync::{Arc, RwLock};

    async fn build_test_state() -> AppState {
        let db = adapteros_db::Db::new_in_memory().await.unwrap();
        let jwt_secret = b"streaming-test-secret-32-bytes!".to_vec();
        let base_tempdir = test_utils::tempdir_with_prefix("aos-test-streaming-infer-");
        let base_dir = base_tempdir.into_path();
        for dir in [
            "artifacts",
            "bundles",
            "adapters",
            "plan",
            "datasets",
            "documents",
        ] {
            let path = base_dir.join(dir);
            std::fs::create_dir_all(&path).unwrap();
        }
        let config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: "test".to_string(),
            },
            directory_analysis_timeout_secs: 1,
            use_session_stack_for_routing: false,
            capacity_limits: Default::default(),
            general: None,
            server: Default::default(),
            security: Default::default(),
            auth: Default::default(),
            self_hosting: Default::default(),
            performance: Default::default(),
            streaming: Default::default(),
            paths: PathsConfig {
                artifacts_root: base_dir.join("artifacts").to_string_lossy().to_string(),
                bundles_root: base_dir.join("bundles").to_string_lossy().to_string(),
                adapters_root: base_dir.join("adapters").to_string_lossy().to_string(),
                plan_dir: base_dir.join("plan").to_string_lossy().to_string(),
                datasets_root: base_dir.join("datasets").to_string_lossy().to_string(),
                documents_root: base_dir.join("documents").to_string_lossy().to_string(),
                synthesis_model_path: None,
                training_worker_bin: None,
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
            worker_id: 0,
            timeouts: Default::default(),
            rate_limit: None,
            inference_cache: Default::default(),
        }));
        let metrics_exporter =
            Arc::new(MetricsExporter::new(vec![0.1, 1.0]).expect("metrics exporter"));
        let metrics_collector = Arc::new(MetricsCollector::new(TelemetryMetricsConfig::default()));
        let metrics_registry = Arc::new(MetricsRegistry::new());
        let uma_monitor = Arc::new(adapteros_lora_worker::memory::UmaPressureMonitor::new(
            15, None,
        ));

        AppState::new(
            db,
            jwt_secret,
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        )
    }

    fn test_run_envelope(run_id: &str, tenant: &str) -> adapteros_api_types::RunEnvelope {
        adapteros_api_types::RunEnvelope {
            run_id: run_id.to_string(),
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            workspace_id: tenant.to_string(),
            actor: adapteros_api_types::RunActor {
                subject: "tester".to_string(),
                roles: vec!["admin".to_string()],
                principal_type: Some("user".to_string()),
                auth_mode: Some("bearer".to_string()),
            },
            manifest_hash_b3: None,
            plan_id: None,
            policy_mask_digest_b3: None,
            router_seed: None,
            tick: None,
            worker_id: None,
            reasoning_mode: false,
            determinism_version: "v1".to_string(),
            boot_trace_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_streaming_request_defaults() {
        let json = r#"{"prompt": "Hello"}"#;
        let req: StreamingInferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.max_tokens, 512);
        assert!((req.temperature - default_temperature()).abs() < 0.01);
    }

    #[test]
    fn test_streaming_chunk_serialization() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some("Hello".to_string()),
                },
                finish_reason: None,
                stop_reason_code: None,
                stop_reason_token_index: None,
                stop_policy_digest_b3: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("chat.completion.chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_done_chunk_format() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
                stop_reason_code: None,
                stop_reason_token_index: None,
                stop_policy_digest_b3: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("stop"));
    }

    #[test]
    fn serialize_safe_returns_error_payload_on_failure() {
        use serde::ser::{Serialize, Serializer};

        // Custom struct that always fails serialization
        struct FailingSerializer;
        impl Serialize for FailingSerializer {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                Err(serde::ser::Error::custom("intentional failure"))
            }
        }

        let serialized = serialize_safe(&FailingSerializer, "test_context");
        assert!(
            serialized.contains("serialization_error"),
            "Expected 'serialization_error' in output, got: {}",
            serialized
        );
        assert!(serialized.contains("test_context"));
    }

    #[tokio::test]
    async fn run_envelope_event_includes_payload_first() {
        let envelope = adapteros_api_types::RunEnvelope {
            run_id: "stream-run".to_string(),
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            workspace_id: "tenant-x".to_string(),
            actor: adapteros_api_types::RunActor {
                subject: "tester".to_string(),
                roles: vec!["role".to_string()],
                principal_type: Some("user".to_string()),
                auth_mode: Some("bearer".to_string()),
            },
            manifest_hash_b3: Some("b3hash".to_string()),
            plan_id: None,
            policy_mask_digest_b3: None,
            router_seed: None,
            tick: Some(42),
            worker_id: None,
            reasoning_mode: false,
            determinism_version: "v1".to_string(),
            boot_trace_id: None,
            created_at: chrono::Utc::now(),
        };

        let envelope_event = run_envelope_event(&envelope);
        let follow_on = Event::default().event("other").data("payload");
        let sse = Sse::new(stream::iter(vec![
            Ok::<_, Infallible>(envelope_event),
            Ok::<_, Infallible>(follow_on),
        ]));
        let response = sse.into_response();
        // 100MB limit to prevent unbounded memory allocation
        const MAX_RESPONSE_SIZE: usize = 100 * 1024 * 1024;
        let body = to_bytes(response.into_body(), MAX_RESPONSE_SIZE)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            body_str.starts_with("event: aos.run_envelope"),
            "expected run envelope event to lead the stream, got: {body_str}"
        );
        assert!(
            body_str.contains("\"run_id\":\"stream-run\""),
            "expected serialized envelope in payload"
        );
        let expected_schema = format!(
            "\"schema_version\":\"{}\"",
            adapteros_api_types::API_SCHEMA_VERSION
        );
        assert!(
            body_str.contains(&expected_schema),
            "expected schema_version in payload"
        );
        let first_idx = body_str
            .find("event: aos.run_envelope")
            .expect("envelope event present");
        let other_idx = body_str
            .find("event: other")
            .expect("follow-on event present");
        assert!(
            first_idx < other_idx,
            "envelope event must precede streaming payload"
        );
    }

    #[test]
    fn test_inference_event_serialization() {
        let event = InferenceEvent::Loading {
            phase: LoadPhase::LoadingWeights,
            progress: 50,
            eta_seconds: Some(30),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Loading"));
        assert!(json.contains("LoadingWeights"));
        assert!(json.contains("50"));
    }

    #[test]
    fn test_inference_event_ready() {
        let event = InferenceEvent::Ready {
            warmup_latency_ms: 1234,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Ready"));
        assert!(json.contains("1234"));
    }

    #[test]
    fn test_inference_event_token() {
        let event = InferenceEvent::Token {
            text: "Hello".to_string(),
            token_id: Some(42),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Token"));
        assert!(json.contains("Hello"));
        assert!(json.contains("42"));
    }

    #[test]
    fn test_inference_event_done() {
        let event = InferenceEvent::Done {
            total_tokens: 100,
            latency_ms: 5000,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            citations: None,
            document_links: None,
            adapter_attachments: None,
            degraded_notices: None,
            fallback_triggered: false,
            fallback_backend: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            pending_evidence_ids: Vec::new(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("100"));
        assert!(json.contains("5000"));
    }

    #[test]
    fn test_inference_event_done_with_pinned_fallback() {
        let event = InferenceEvent::Done {
            total_tokens: 50,
            latency_ms: 2500,
            unavailable_pinned_adapters: Some(vec!["pin-1".to_string(), "pin-2".to_string()]),
            pinned_routing_fallback: Some("stack_only".to_string()),
            citations: None,
            document_links: None,
            adapter_attachments: None,
            degraded_notices: None,
            fallback_triggered: false,
            fallback_backend: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            pending_evidence_ids: Vec::new(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("pin-1"));
        assert!(json.contains("stack_only"));
    }

    #[tokio::test]
    async fn stream_emits_heartbeat_then_resumes_with_tokens() {
        let state = build_test_state().await;
        let (token_tx, token_rx) = mpsc::channel(4);
        let (_pause_tx, pause_rx) = mpsc::channel(1);
        let (done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let run_id = "chatcmpl-heartbeat";

        let mut stream = StreamState::new(
            state,
            run_id.to_string(),
            test_run_envelope(run_id, "tenant-1"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_millis(10),
            Vec::new(),                // No pending evidence IDs in test
            None,                      // idempotency_key
            Duration::from_secs(1800), // max_pause_duration (default)
        );

        // First event is now StreamLifecycleStart -> StreamStarted
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::StreamStarted { .. })
        ));

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Start)
        ));

        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Heartbeat)
        ));

        token_tx
            .send(WorkerStreamToken {
                text: "hi".to_string(),
                token_id: Some(1),
            })
            .await
            .unwrap();
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Token(_))
        ));

        drop(done_tx);
        drop(token_tx);
        let next = stream.next_event().await;
        assert!(
            !matches!(next, Some(StreamEvent::Heartbeat)),
            "expected stream to terminate or error after channel closure"
        );
    }

    #[tokio::test]
    async fn stream_pause_disables_idle_timeout() {
        let state = build_test_state().await;
        let (_token_tx, token_rx) = mpsc::channel(1);
        let (pause_tx, pause_rx) = mpsc::channel(1);
        let (_done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let run_id = "chatcmpl-paused-idle";

        let mut stream = StreamState::new(
            state,
            run_id.to_string(),
            test_run_envelope(run_id, "tenant-1"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_millis(10), // very small idle timeout
            Duration::from_secs(0),    // disable StreamEvent heartbeats
            Vec::new(),
            None,
            Duration::from_secs(1800), // large max_pause_duration (won't expire in this test)
        );

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::StreamStarted { .. })
        ));
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Start)
        ));

        pause_tx
            .send(WorkerStreamPaused {
                pause_id: "pause-1".to_string(),
                inference_id: run_id.to_string(),
                trigger_kind: "uncertainty".to_string(),
                context: Some("needs review".to_string()),
                text_so_far: Some("partial".to_string()),
                token_count: 3,
            })
            .await
            .unwrap();

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Paused(_))
        ));

        // After a pause, the stream must not emit STREAM_IDLE_TIMEOUT regardless of idle_timeout.
        tokio::time::sleep(Duration::from_millis(25)).await;
        let res = tokio::time::timeout(Duration::from_millis(5), stream.next_event()).await;
        assert!(
            res.is_err(),
            "expected next_event() to block while paused (no idle timeout), got: {res:?}"
        );
    }

    #[tokio::test]
    async fn stream_pause_expires_after_max_duration() {
        let state = build_test_state().await;
        let (_token_tx, token_rx) = mpsc::channel(1);
        let (pause_tx, pause_rx) = mpsc::channel(1);
        let (_done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let run_id = "chatcmpl-paused-expire";

        let mut stream = StreamState::new(
            state,
            run_id.to_string(),
            test_run_envelope(run_id, "tenant-1"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_millis(10), // very small idle timeout
            Duration::from_secs(0),    // disable heartbeats
            Vec::new(),
            None,
            Duration::from_millis(30), // very short max_pause_duration for test
        );

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::StreamStarted { .. })
        ));
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Start)
        ));

        pause_tx
            .send(WorkerStreamPaused {
                pause_id: "pause-expire-1".to_string(),
                inference_id: run_id.to_string(),
                trigger_kind: "uncertainty".to_string(),
                context: Some("needs review".to_string()),
                text_so_far: Some("partial".to_string()),
                token_count: 3,
            })
            .await
            .unwrap();

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Paused(_))
        ));

        // Wait longer than the max_pause_duration (30ms) AND the idle_timeout (10ms).
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The pause should have expired, re-enabling idle-timeout detection.
        // next_event should now return STREAM_IDLE_TIMEOUT instead of blocking.
        let res = tokio::time::timeout(Duration::from_millis(100), stream.next_event()).await;
        assert!(
            res.is_ok(),
            "expected next_event() to return (pause expired + idle timeout), but it blocked"
        );
        let event = res.unwrap();
        assert!(
            matches!(&event, Some(StreamEvent::Error { code, .. }) if code == "STREAM_IDLE_TIMEOUT"),
            "expected STREAM_IDLE_TIMEOUT after pause expiry, got: {event:?}"
        );
    }

    #[tokio::test]
    async fn backpressure_keeps_stream_responsive() {
        let state = build_test_state().await;
        let (token_tx, token_rx) = mpsc::channel(1);
        let (_pause_tx, pause_rx) = mpsc::channel(1);
        let (done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();

        let mut stream = StreamState::new(
            state,
            "chatcmpl-backpressure".to_string(),
            test_run_envelope("chatcmpl-backpressure", "tenant-1"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_secs(2),
            Duration::from_millis(25),
            Vec::new(),                // No pending evidence IDs in test
            None,                      // idempotency_key
            Duration::from_secs(1800), // max_pause_duration (default)
        );

        let producer = tokio::spawn(async move {
            for i in 0..5u32 {
                token_tx
                    .send(WorkerStreamToken {
                        text: format!("t{i}"),
                        token_id: Some(i),
                    })
                    .await
                    .map_err(|e| e.to_string())?;
            }
            drop(done_tx);
            Ok::<_, String>(())
        });

        // First event is now StreamLifecycleStart -> StreamStarted
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::StreamStarted { .. })
        ));

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Start)
        ));
        let mut received = 0;
        while received < 5 {
            if let Some(event) = stream.next_event().await {
                match event {
                    StreamEvent::Token(text) => {
                        received += 1;
                        assert!(!text.is_empty());
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    StreamEvent::Done { .. } | StreamEvent::Error { .. } => break,
                    StreamEvent::Heartbeat
                    | StreamEvent::Start
                    | StreamEvent::Paused(_)
                    | StreamEvent::StreamStarted { .. }
                    | StreamEvent::StreamFinished { .. } => {}
                }
            } else {
                break;
            }
        }

        let producer_result = tokio::time::timeout(Duration::from_secs(1), producer)
            .await
            .unwrap();
        assert!(producer_result.is_ok(), "producer should not hang");
        assert!(
            received >= 1,
            "stream should stay responsive under backpressure"
        );
    }

    #[tokio::test]
    async fn stream_error_event_carries_structured_fields() {
        let state = build_test_state().await;
        let (token_tx, token_rx) = mpsc::channel(1);
        let (_pause_tx, pause_rx) = mpsc::channel(1);
        drop(token_tx);
        let (done_tx, done_rx) = oneshot::channel();
        drop(done_tx);
        let cancellation = CancellationToken::new();
        let request_id = "chatcmpl-err-123";

        let stream = StreamState::new(
            state,
            request_id.to_string(),
            test_run_envelope(request_id, "tenant-err"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            Some("session-1".to_string()),
            None,
            "tenant-err".to_string(),
            "user-err".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_secs(0),
            Vec::new(),                // No pending evidence IDs in test
            None,                      // idempotency_key
            Duration::from_secs(1800), // max_pause_duration (default)
        );

        let event = stream.format_event(StreamEvent::Error {
            code: "WORKER_DOWN".to_string(),
            message: "worker unavailable".to_string(),
            retryable: false,
        });
        let response = Sse::new(stream::iter(vec![Ok::<_, Infallible>(event)])).into_response();
        const MAX_RESPONSE_SIZE: usize = 1024 * 1024;
        let body = to_bytes(response.into_body(), MAX_RESPONSE_SIZE)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(
            body_str.starts_with("event: error"),
            "expected SSE error event, got {body_str}"
        );

        let json_payload = body_str
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("data line present");
        let payload: serde_json::Value = serde_json::from_str(json_payload).unwrap();
        assert_eq!(payload["code"], "WORKER_DOWN");
        assert_eq!(payload["message"], "worker unavailable");
        assert_eq!(payload["retryable"], false);
        assert_eq!(payload["correlation_id"], request_id);
    }

    #[tokio::test]
    async fn stream_error_emitted_once_then_closes() {
        let state = build_test_state().await;
        let (token_tx, token_rx) = mpsc::channel(1);
        let (_pause_tx, pause_rx) = mpsc::channel(1);
        drop(token_tx);
        let (done_tx, done_rx) = oneshot::channel();
        drop(done_tx);
        let cancellation = CancellationToken::new();
        let request_id = "chatcmpl-error-once";

        let mut stream = StreamState::new(
            state,
            request_id.to_string(),
            test_run_envelope(request_id, "tenant-err"),
            "test-model".to_string(),
            token_rx,
            pause_rx,
            done_rx,
            None,
            None,
            "tenant-err".to_string(),
            "user-err".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_secs(0),
            Vec::new(),                // No pending evidence IDs in test
            None,                      // idempotency_key
            Duration::from_secs(1800), // max_pause_duration (default)
        );

        // First event is now StreamLifecycleStart -> StreamStarted
        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::StreamStarted { .. })
        ));

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Start)
        ));

        assert!(matches!(
            stream.next_event().await,
            Some(StreamEvent::Error { .. })
        ));

        assert!(stream.next_event().await.is_none());
    }

    #[test]
    fn test_inference_event_error() {
        let event = InferenceEvent::Error {
            message: "Test error".to_string(),
            recoverable: true,
            code: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("Test error"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_parse_rag_doc_id_valid() {
        use crate::handlers::rag_common::ParsedRagDocId;

        // Standard format: document_id__chunk_index
        let result = parse_rag_doc_id("doc-123__chunk_0");
        assert_eq!(
            result,
            Some(ParsedRagDocId {
                document_id: "doc-123".to_string(),
                chunk_index: 0
            })
        );

        let result = parse_rag_doc_id("my-document-uuid__chunk_42");
        assert_eq!(
            result,
            Some(ParsedRagDocId {
                document_id: "my-document-uuid".to_string(),
                chunk_index: 42
            })
        );

        // Document ID with underscores (edge case - uses rfind)
        let result = parse_rag_doc_id("doc_with_underscores__chunk_5");
        assert_eq!(
            result,
            Some(ParsedRagDocId {
                document_id: "doc_with_underscores".to_string(),
                chunk_index: 5
            })
        );
    }

    #[test]
    fn test_parse_rag_doc_id_invalid() {
        use crate::handlers::rag_common::ParsedRagDocId;

        // Missing __chunk_ separator
        assert_eq!(parse_rag_doc_id("doc-123"), None);
        assert_eq!(parse_rag_doc_id("doc-123_chunk_0"), None);

        // Invalid chunk index (not a number)
        assert_eq!(parse_rag_doc_id("doc-123__chunk_abc"), None);

        // Empty document ID
        assert_eq!(
            parse_rag_doc_id("__chunk_0"),
            Some(ParsedRagDocId {
                document_id: "".to_string(),
                chunk_index: 0
            })
        );

        // Negative chunk index (should still parse as i32)
        assert_eq!(
            parse_rag_doc_id("doc__chunk_-1"),
            Some(ParsedRagDocId {
                document_id: "doc".to_string(),
                chunk_index: -1
            })
        );
    }

    #[test]
    fn test_parse_rag_doc_id_special_cases() {
        use crate::handlers::rag_common::ParsedRagDocId;

        // UUID-style document ID
        let result = parse_rag_doc_id("550e8400-e29b-41d4-a716-446655440000__chunk_99");
        assert_eq!(
            result,
            Some(ParsedRagDocId {
                document_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                chunk_index: 99
            })
        );

        // Document ID with path-like format (sanitized)
        let result = parse_rag_doc_id("documents_report_2024_pdf__chunk_0");
        assert_eq!(
            result,
            Some(ParsedRagDocId {
                document_id: "documents_report_2024_pdf".to_string(),
                chunk_index: 0
            })
        );
    }

    #[test]
    fn test_streaming_request_to_internal_conversion() {
        use crate::auth::Claims;

        // Create a streaming request with all fields populated
        let streaming_req = StreamingInferRequest {
            prompt: "Test prompt".to_string(),
            model: Some("test-model".to_string()),
            backend: None,
            coreml_mode: None,
            stack_id: None,
            max_tokens: 100,
            temperature: 0.8,
            top_p: Some(0.9),
            top_k: Some(50),
            stop: vec!["STOP".to_string()],
            adapter_stack: Some(vec!["adapter1".to_string(), "adapter2".to_string()]),
            adapters: Some(vec!["adapter3".to_string()]),
            seed: Some(12345),
            adapter_strength_overrides: None,
            require_evidence: true,
            collection_id: Some("test-collection".to_string()),
            domain: None,
            routing_determinism_mode: None,
            session_id: Some("test-session".to_string()),
            effective_adapter_ids: None,
            reasoning_mode: false,
            stop_policy: None,
            context: None,
            bit_identical: false,
        };

        // Create mock claims
        let claims = Claims {
            sub: "user123".to_string(),
            email: "test@example.com".to_string(),
            role: "operator".to_string(),
            roles: Vec::new(),
            tenant_id: "tenant456".to_string(),
            admin_tenants: Vec::new(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: "test-jti".to_string(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        // Convert using From implementation
        let internal: InferenceRequestInternal = (&streaming_req, &claims).into();

        // Verify all fields are correctly mapped
        assert_eq!(internal.cpid, "tenant456");
        assert_eq!(internal.prompt, "Test prompt");
        assert!(internal.stream); // Always true for streaming endpoint
        assert!(internal.batch_item_id.is_none());
        assert!(internal.rag_enabled); // Should be true because collection_id is Some
        assert_eq!(
            internal.rag_collection_id,
            Some("test-collection".to_string())
        );
        assert_eq!(
            internal.adapter_stack,
            Some(vec!["adapter1".to_string(), "adapter2".to_string()])
        );
        assert_eq!(internal.adapters, Some(vec!["adapter3".to_string()]));
        assert_eq!(internal.max_tokens, 100);
        assert!((internal.temperature - 0.8).abs() < 0.01);
        assert_eq!(internal.top_p, Some(0.9));
        assert_eq!(internal.top_k, Some(50));
        assert_eq!(internal.seed, Some(12345));
        assert!(internal.require_evidence);
        assert_eq!(internal.session_id, Some("test-session".to_string()));
        assert_eq!(internal.model, Some("test-model".to_string()));
    }

    #[test]
    fn test_streaming_request_to_internal_with_session_and_reasoning() {
        use crate::auth::Claims;

        let streaming_req = StreamingInferRequest {
            prompt: "Reason it out".to_string(),
            model: None,
            backend: None,
            coreml_mode: None,
            stack_id: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: None,
            top_k: None,
            stop: vec![],
            adapter_stack: None,
            adapters: None,
            seed: None,
            adapter_strength_overrides: None,
            require_evidence: false,
            collection_id: None,
            domain: None,
            routing_determinism_mode: None,
            session_id: Some("sess-1".to_string()),
            effective_adapter_ids: None,
            reasoning_mode: true,
            stop_policy: None,
            context: None,
            bit_identical: false,
        };

        let claims = Claims {
            sub: "user".to_string(),
            email: String::new(),
            tenant_id: "tenant".to_string(),
            role: "operator".to_string(),
            roles: Vec::new(),
            admin_tenants: Vec::new(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: TypedId::new(IdPrefix::Tok).to_string(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let internal: InferenceRequestInternal = (&streaming_req, &claims).into();

        assert_eq!(internal.session_id, Some("sess-1".to_string()));
        assert!(internal.reasoning_mode);
    }

    #[test]
    fn test_streaming_request_to_internal_no_collection() {
        use crate::auth::Claims;

        // Create a streaming request without collection_id
        let streaming_req = StreamingInferRequest {
            prompt: "Test".to_string(),
            model: None,
            backend: None,
            coreml_mode: None,
            stack_id: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: None,
            top_k: None,
            stop: vec![],
            adapter_stack: None,
            adapters: None,
            seed: None,
            adapter_strength_overrides: None,
            require_evidence: false,
            collection_id: None, // No collection
            domain: None,
            routing_determinism_mode: None,
            session_id: None,
            effective_adapter_ids: None,
            reasoning_mode: false,
            stop_policy: None,
            context: None,
            bit_identical: false,
        };

        let claims = Claims {
            sub: "user".to_string(),
            email: String::new(),
            tenant_id: "tenant".to_string(),
            role: "operator".to_string(),
            roles: Vec::new(),
            admin_tenants: Vec::new(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: TypedId::new(IdPrefix::Tok).to_string(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let internal: InferenceRequestInternal = (&streaming_req, &claims).into();

        // RAG should be disabled when no collection_id
        assert!(!internal.rag_enabled);
        assert!(internal.rag_collection_id.is_none());
    }

    #[test]
    fn test_streaming_request_bit_identical_sets_internal_strict_flags() {
        use crate::auth::Claims;

        let streaming_req = StreamingInferRequest {
            prompt: "Pinned run".to_string(),
            model: None,
            backend: None,
            coreml_mode: None,
            stack_id: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: None,
            top_k: None,
            stop: vec![],
            adapter_stack: None,
            adapters: Some(vec!["repo-a@ver-1".to_string()]),
            seed: Some(42),
            adapter_strength_overrides: None,
            require_evidence: false,
            collection_id: None,
            domain: None,
            routing_determinism_mode: None,
            session_id: None,
            effective_adapter_ids: None,
            reasoning_mode: false,
            stop_policy: None,
            context: None,
            bit_identical: true,
        };

        let claims = Claims {
            sub: "user".to_string(),
            email: String::new(),
            tenant_id: "tenant".to_string(),
            role: "operator".to_string(),
            roles: Vec::new(),
            admin_tenants: Vec::new(),
            device_id: None,
            session_id: None,
            mfa_level: None,
            rot_id: None,
            exp: 0,
            iat: 0,
            jti: TypedId::new(IdPrefix::Tok).to_string(),
            nbf: 0,
            iss: JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        };

        let internal: InferenceRequestInternal = (&streaming_req, &claims).into();
        assert!(internal.bit_identical);
        assert!(!internal.allow_fallback);
    }

    #[test]
    fn is_safety_trigger_classifies_correctly() {
        // Safety triggers: text_so_far must be redacted
        assert!(is_safety_trigger("policy_violation"));
        assert!(is_safety_trigger("policy"));
        assert!(is_safety_trigger("policy_approval"));
        assert!(is_safety_trigger("safety_gate"));
        assert!(is_safety_trigger("threat"));
        assert!(is_safety_trigger("threat_escalation"));

        // Case-insensitive
        assert!(is_safety_trigger("Policy_Violation"));
        assert!(is_safety_trigger("THREAT_ESCALATION"));
        assert!(is_safety_trigger("Safety_Gate"));

        // Non-safety triggers: text_so_far is preserved
        assert!(!is_safety_trigger("uncertainty"));
        assert!(!is_safety_trigger("ExplicitTag"));
        assert!(!is_safety_trigger("explicit_tag"));
        assert!(!is_safety_trigger("review"));
        assert!(!is_safety_trigger("manual"));
        assert!(!is_safety_trigger("user_requested"));
        assert!(!is_safety_trigger("resource"));
        assert!(!is_safety_trigger("resource_wait"));
        assert!(!is_safety_trigger("complexity_threshold"));
        assert!(!is_safety_trigger("unknown"));
    }

    #[test]
    fn paused_event_redacts_text_for_safety_trigger() {
        let paused = WorkerStreamPaused {
            pause_id: "pause-safety-001".to_string(),
            inference_id: "inf-safety-001".to_string(),
            trigger_kind: "policy_violation".to_string(),
            context: Some("Content flagged by safety policy".to_string()),
            text_so_far: Some("bad content that should not leak".to_string()),
            token_count: 7,
        };

        // Simulate what format_event does for Paused
        let text_so_far = if is_safety_trigger(&paused.trigger_kind) {
            None
        } else {
            paused.text_so_far.clone()
        };

        let payload = InferenceEvent::Paused {
            pause_id: paused.pause_id,
            inference_id: paused.inference_id,
            trigger_kind: paused.trigger_kind,
            context: paused.context,
            text_so_far,
            token_count: paused.token_count,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(
            parsed.get("text_so_far").is_none(),
            "text_so_far must be absent (skip_serializing_if = None) for policy_violation, got: {json}"
        );
        assert_eq!(parsed["trigger_kind"], "policy_violation");
        assert_eq!(parsed["pause_id"], "pause-safety-001");
    }

    #[test]
    fn paused_event_redacts_text_for_threat_escalation() {
        let paused = WorkerStreamPaused {
            pause_id: "pause-threat-001".to_string(),
            inference_id: "inf-threat-001".to_string(),
            trigger_kind: "threat_escalation".to_string(),
            context: Some("Threat detected".to_string()),
            text_so_far: Some("dangerous content".to_string()),
            token_count: 3,
        };

        let text_so_far = if is_safety_trigger(&paused.trigger_kind) {
            None
        } else {
            paused.text_so_far.clone()
        };

        let payload = InferenceEvent::Paused {
            pause_id: paused.pause_id,
            inference_id: paused.inference_id,
            trigger_kind: paused.trigger_kind,
            context: paused.context,
            text_so_far,
            token_count: paused.token_count,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(
            parsed.get("text_so_far").is_none(),
            "text_so_far must be absent for threat_escalation, got: {json}"
        );
    }

    #[test]
    fn paused_event_redacts_text_for_safety_gate() {
        let paused = WorkerStreamPaused {
            pause_id: "pause-gate-001".to_string(),
            inference_id: "inf-gate-001".to_string(),
            trigger_kind: "safety_gate".to_string(),
            context: None,
            text_so_far: Some("unsafe output".to_string()),
            token_count: 2,
        };

        let text_so_far = if is_safety_trigger(&paused.trigger_kind) {
            None
        } else {
            paused.text_so_far.clone()
        };

        let payload = InferenceEvent::Paused {
            pause_id: paused.pause_id,
            inference_id: paused.inference_id,
            trigger_kind: paused.trigger_kind,
            context: paused.context,
            text_so_far,
            token_count: paused.token_count,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(
            parsed.get("text_so_far").is_none(),
            "text_so_far must be absent for safety_gate, got: {json}"
        );
    }

    #[test]
    fn paused_event_preserves_text_for_non_safety_trigger() {
        let paused = WorkerStreamPaused {
            pause_id: "pause-review-001".to_string(),
            inference_id: "inf-review-001".to_string(),
            trigger_kind: "uncertainty".to_string(),
            context: Some("needs human review".to_string()),
            text_so_far: Some("partial output that is fine".to_string()),
            token_count: 5,
        };

        let text_so_far = if is_safety_trigger(&paused.trigger_kind) {
            None
        } else {
            paused.text_so_far.clone()
        };

        let payload = InferenceEvent::Paused {
            pause_id: paused.pause_id,
            inference_id: paused.inference_id,
            trigger_kind: paused.trigger_kind,
            context: paused.context,
            text_so_far,
            token_count: paused.token_count,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed["text_so_far"], "partial output that is fine",
            "text_so_far must be preserved for non-safety triggers, got: {json}"
        );
    }

    #[tokio::test]
    async fn loading_stream_redacts_and_gates_safety_pause() {
        let state = build_test_state().await;
        let tracker = Arc::new(crate::pause_tracker::ServerPauseTracker::new());
        let state = state.with_pause_tracker(tracker.clone());

        let (token_tx, token_rx) = mpsc::channel(8);
        let (pause_tx, pause_rx) = mpsc::channel(8);
        let (_done_tx, done_rx) = oneshot::channel();

        let run_id = "chatcmpl-loading-pause";
        let pause_id = "pause-loading-001";

        // Register the pause in the tracker so it starts "unreviewed".
        tracker.register_server_pause(
            "tenant-1".to_string(),
            pause_id.to_string(),
            run_id.to_string(),
            "policy_violation",
            Some("needs review".to_string()),
            None,
        );

        let request = StreamingInferRequest {
            prompt: "hello".to_string(),
            model: None,
            backend: None,
            coreml_mode: None,
            routing_determinism_mode: None,
            stack_id: None,
            domain: None,
            max_tokens: 16,
            temperature: 0.7,
            top_p: None,
            top_k: None,
            stop: Vec::new(),
            adapter_stack: None,
            adapters: None,
            seed: None,
            adapter_strength_overrides: None,
            require_evidence: false,
            reasoning_mode: false,
            collection_id: None,
            session_id: None,
            effective_adapter_ids: None,
            stop_policy: None,
            context: None,
            bit_identical: false,
        };

        let mut stream = LoadingStreamState::new(
            state,
            request,
            test_run_envelope(run_id, "tenant-1"),
            "adapter-1".to_string(),
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            None,
        );
        stream.phase = LoadingPhase::Inferring;
        stream.token_rx = Some(token_rx);
        stream.pause_rx = Some(pause_rx);
        stream.done_rx = Some(done_rx);

        pause_tx
            .send(WorkerStreamPaused {
                pause_id: pause_id.to_string(),
                inference_id: run_id.to_string(),
                trigger_kind: "policy_violation".to_string(),
                context: Some("needs review".to_string()),
                text_so_far: Some("unsafe partial that must not leak".to_string()),
                token_count: 3,
            })
            .await
            .unwrap();

        let paused_event = stream.next_loading_event().await.expect("paused event");
        match &paused_event {
            InferenceEvent::Paused {
                pause_id: got_pause_id,
                trigger_kind,
                text_so_far,
                ..
            } => {
                assert_eq!(got_pause_id, pause_id);
                assert_eq!(trigger_kind, "policy_violation");
                assert!(text_so_far.is_none(), "text_so_far must be redacted");
            }
            other => panic!("expected paused event, got: {other:?}"),
        }

        let json = serde_json::to_string(&paused_event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed.get("text_so_far").is_none(),
            "text_so_far must be absent (skip_serializing_if) for policy_violation, got: {json}"
        );

        // While the safety pause is unreviewed, tokens must be dropped.
        token_tx
            .send(WorkerStreamToken {
                text: "should_drop".to_string(),
                token_id: Some(1),
            })
            .await
            .unwrap();

        let dropped =
            tokio::time::timeout(Duration::from_millis(50), stream.next_loading_event()).await;
        assert!(
            dropped.is_err(),
            "expected token to be dropped (no event returned), but got: {dropped:?}"
        );

        // Simulate review submission by removing the pause from the tracker.
        tracker.remove(pause_id);

        token_tx
            .send(WorkerStreamToken {
                text: "allowed".to_string(),
                token_id: Some(2),
            })
            .await
            .unwrap();

        let resumed =
            tokio::time::timeout(Duration::from_millis(100), stream.next_loading_event()).await;
        let event = resumed
            .expect("expected token after review removal")
            .expect("event");
        assert!(
            matches!(event, InferenceEvent::Token { ref text, .. } if text == "allowed"),
            "expected allowed token after review removal, got: {event:?}"
        );
    }
}
