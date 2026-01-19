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
use crate::citations::collect_citations_for_adapters;
use crate::handlers::rag_common::{retrieve_rag_context, store_rag_evidence};
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{
    compute_policy_mask_digest, create_hook_context, enforce_at_hook,
};
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::run_envelope::set_policy_mask;
use crate::types::*;
use crate::uds_client::{UdsClient, WorkerStreamToken};
use adapteros_core::identity::IdentityEnvelope;
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
}

impl From<(&StreamingInferRequest, &Claims)> for InferenceRequestInternal {
    fn from((req, claims): (&StreamingInferRequest, &Claims)) -> Self {
        let is_admin = claims.role.eq_ignore_ascii_case("admin")
            || claims.roles.iter().any(|r| r.eq_ignore_ascii_case("admin"));
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            run_envelope: None,
            reasoning_mode: req.reasoning_mode,
            admin_override: is_admin,
            stream: true, // Always streaming for this endpoint
            require_step: true,
            require_determinism: false,
            allow_fallback: true,
            batch_item_id: None,
            rag_enabled: req.collection_id.is_some(), // Enable RAG if collection_id provided
            rag_collection_id: req.collection_id.clone(),
            dataset_version_id: None,
            adapter_stack: req.adapter_stack.clone(),
            adapters: req.adapters.clone(),
            stack_id: req.stack_id.clone(),
            stack_routing_determinism_mode: None,
            domain_hint: req.domain.clone(),
            stack_version: None,
            stack_determinism_mode: None,
            effective_adapter_ids: None, // Computed inside InferenceCore
            adapter_strength_overrides: req.adapter_strength_overrides.clone(),
            determinism_mode: None,
            routing_determinism_mode: req.routing_determinism_mode,
            seed_mode: None,
            request_seed: None,
            backend_profile: None,
            coreml_mode: req.coreml_mode,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            top_k: req.top_k,
            top_p: req.top_p,
            seed: req.seed,
            require_evidence: req.require_evidence,
            session_id: req.session_id.clone(),
            pinned_adapter_ids: None, // Looked up from session in route_and_infer if session_id is set
            chat_context_hash: None,  // Set later after build_chat_prompt
            claims: Some(claims.clone()),
            model: req.model.clone(),
            stop_policy: req.stop_policy.clone(),
            created_at: std::time::Instant::now(),
            router_seed: None, // Use default router behavior for streaming
            worker_auth_token: None,
            policy_mask_digest_b3: None, // Streaming requests don't use policy hooks
            utf8_healing: None,
            abstention_threshold: None, // AARA lifecycle
            citation_mode: None,        // AARA lifecycle
        }
    }
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

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
    /// First chunk with role
    Start,
    /// Token generated
    Token(String),
    /// Generation complete
    Done { finish_reason: String },
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
    Error { message: String, recoverable: bool },
}

/// Load phases for progress tracking
#[derive(Debug, Clone, Serialize)]
pub enum LoadPhase {
    Downloading,
    LoadingWeights,
    Warmup,
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
    Extension(_identity): Extension<IdentityEnvelope>,
    Json(req): Json<StreamingInferRequest>,
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
    let request_id = uuid::Uuid::new_v4().to_string();

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
    Extension(_identity): Extension<IdentityEnvelope>,
    Json(req): Json<StreamingInferRequest>,
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

    check_uma_backpressure(&state)?;

    // Generate request ID
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
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

    let (token_rx, done_rx) = spawn_streaming_inference(
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
    oneshot::Receiver<Result<InferenceResult, InferenceError>>,
) {
    // Bounded channel to apply backpressure when clients read slowly.
    let (token_tx, token_rx) = mpsc::channel(token_buffer_capacity);
    let (done_tx, done_rx) = oneshot::channel();

    tokio::spawn(async move {
        let core = InferenceCore::new(&state);
        let result = core
            .route_and_infer_stream(request, None, Some(cancellation_token), token_tx)
            .await;
        let _ = done_tx.send(result);
    });

    (token_rx, done_rx)
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
    /// User claims for policy enforcement
    claims: Option<crate::auth::Claims>,
    token_rx: Option<mpsc::Receiver<WorkerStreamToken>>,
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
            cancellation_token: CancellationToken::new(),
            phase: LoadingPhase::CheckingState,
            start_time: std::time::Instant::now(),
            token_count: 0,
            request_id,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            pending_evidence_ids: Vec::new(), // Initialized empty, populated during RAG retrieval
            token_rx: None,
            done_rx: None,
        }
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
                        });
                    }
                }

                let token = if let Some(rx) = self.token_rx.as_mut() {
                    rx.recv().await
                } else {
                    None
                };

                if let Some(token) = token {
                    self.token_count += 1;
                    return Some(InferenceEvent::Token {
                        text: token.text,
                        token_id: token.token_id,
                    });
                }

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
                        self.stop_reason_code = result.stop_reason_code;
                        self.stop_reason_token_index = result.stop_reason_token_index;
                        self.stop_policy_digest_b3 = result.stop_policy_digest_b3.clone();

                        // Collect citations (best-effort, non-blocking if empty)
                        let citations = collect_citations_for_adapters(
                            &self.state,
                            &self.tenant_id,
                            std::slice::from_ref(&self.adapter_id),
                            &self.request.prompt,
                            3,
                        )
                        .await;

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
                                jti: uuid::Uuid::new_v4().to_string(),
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
                            stop_reason_code: self.stop_reason_code,
                            stop_reason_token_index: self.stop_reason_token_index,
                            stop_policy_digest_b3: self.stop_policy_digest_b3.clone(),
                            pending_evidence_ids: self.pending_evidence_ids.clone(),
                        })
                    }
                    Some(Err(err)) => Some(InferenceEvent::Error {
                        message: format!("Inference failed: {}", err),
                        recoverable: false,
                    }),
                    None => Some(InferenceEvent::Error {
                        message: "Stream ended without completion".to_string(),
                        recoverable: false,
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
        let _handle = state
            .load_coordinator
            .load_or_wait(&adapter_id, move || {
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
            .map_err(|e| format!("Load coordination failed: {}", e))?;

        Ok(())
    }

    async fn wait_for_ready(&self) -> Result<u64, String> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(60);
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
            jti: uuid::Uuid::new_v4().to_string(),
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
        let (token_rx, done_rx) = spawn_streaming_inference(
            self.state.clone(),
            internal_request,
            self.cancellation_token.clone(),
            stream_config.inference_token_buffer_capacity,
        );

        self.token_rx = Some(token_rx);
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
    // Pending RAG evidence IDs for message binding
    pending_evidence_ids: Vec<String>,
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
    Start,
    StreamingTokens,
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
    ) -> Self {
        let canonical_request_id = run_envelope.run_id.clone();
        if request_id != canonical_request_id {
            warn!(
                request_id = %request_id,
                run_id = %canonical_request_id,
                "Streaming request_id mismatch; using run_envelope.run_id"
            );
        }

        Self {
            state,
            request_id: canonical_request_id,
            run_envelope,
            model_name,
            phase: StreamPhase::Start,
            tenant_id,
            user_id,
            after_hook_fired: false,
            token_rx,
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
            pending_evidence_ids,
        }
    }

    /// Check if stream has been idle for too long
    fn is_idle(&self) -> bool {
        self.last_token_at.elapsed() > self.idle_timeout
    }

    fn mark_token_activity(&mut self) {
        self.last_token_at = Instant::now();
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
                    token = self.token_rx.recv() => {
                        match token {
                            Some(token) => {
                                self.mark_token_activity();
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
                                        self.phase = StreamPhase::Done;
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
                                            )
                                        {
                                            info!(
                                                request_id = %self.request_id,
                                                error = %err,
                                                "Dev echo mode (stream): returning mock token"
                                            );
                                            self.phase = StreamPhase::Done;
                                            // Return echo text as a single token
                                            return Some(StreamEvent::Token(
                                                "[DEV ECHO] No inference worker available. Start a worker to enable real inference.".to_string()
                                            ));
                                        }
                                        self.phase = StreamPhase::Done;
                                        return Some(self.map_inference_error(err));
                                    }
                                    None => {
                                        self.phase = StreamPhase::Done;
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
                        jti: uuid::Uuid::new_v4().to_string(),
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
        let base_dir = PathBuf::from("var")
            .join("tmp")
            .join("streaming-infer-tests")
            .join(uuid::Uuid::new_v4().to_string());
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
            },
            chat_context: Default::default(),
            seed_mode: SeedMode::BestEffort,
            backend_profile: BackendKind::Auto,
            worker_id: 0,
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
        assert!((req.temperature - 0.7).abs() < 0.01);
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
        let (_done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let run_id = "chatcmpl-heartbeat";

        let mut stream = StreamState::new(
            state,
            run_id.to_string(),
            test_run_envelope(run_id, "tenant-1"),
            "test-model".to_string(),
            token_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_millis(10),
            Vec::new(), // No pending evidence IDs in test
        );

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

        drop(token_tx);
        let next = stream.next_event().await;
        assert!(
            !matches!(next, Some(StreamEvent::Heartbeat)),
            "expected stream to terminate or error after channel closure"
        );
    }

    #[tokio::test]
    async fn backpressure_keeps_stream_responsive() {
        let state = build_test_state().await;
        let (token_tx, token_rx) = mpsc::channel(1);
        let (done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();

        let mut stream = StreamState::new(
            state,
            "chatcmpl-backpressure".to_string(),
            test_run_envelope("chatcmpl-backpressure", "tenant-1"),
            "test-model".to_string(),
            token_rx,
            done_rx,
            None,
            None,
            "tenant-1".to_string(),
            "user-1".to_string(),
            None,
            cancellation,
            Duration::from_secs(2),
            Duration::from_millis(25),
            Vec::new(), // No pending evidence IDs in test
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
                    StreamEvent::Heartbeat | StreamEvent::Start => {}
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
        drop(token_tx);
        let (_done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let request_id = "chatcmpl-err-123";

        let stream = StreamState::new(
            state,
            request_id.to_string(),
            test_run_envelope(request_id, "tenant-err"),
            "test-model".to_string(),
            token_rx,
            done_rx,
            Some("session-1".to_string()),
            None,
            "tenant-err".to_string(),
            "user-err".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_secs(0),
            Vec::new(), // No pending evidence IDs in test
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
        drop(token_tx);
        let (_done_tx, done_rx) = oneshot::channel();
        let cancellation = CancellationToken::new();
        let request_id = "chatcmpl-error-once";

        let mut stream = StreamState::new(
            state,
            request_id.to_string(),
            test_run_envelope(request_id, "tenant-err"),
            "test-model".to_string(),
            token_rx,
            done_rx,
            None,
            None,
            "tenant-err".to_string(),
            "user-err".to_string(),
            None,
            cancellation,
            Duration::from_secs(5),
            Duration::from_secs(0),
            Vec::new(), // No pending evidence IDs in test
        );

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
    fn test_streaming_request_to_internal_no_collection() {
        use crate::auth::Claims;

        // Create a streaming request without collection_id
        let streaming_req = StreamingInferRequest {
            prompt: "Test".to_string(),
            model: None,
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
            jti: uuid::Uuid::new_v4().to_string(),
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
}
