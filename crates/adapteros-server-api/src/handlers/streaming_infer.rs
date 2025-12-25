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
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
//! data: [DONE]
//! ```

use crate::api_error::ApiError;
use crate::auth::{AuthMode, Claims, PrincipalType, JWT_ISSUER};
use crate::backpressure::check_uma_backpressure;
use crate::chat_context::build_chat_prompt;
use crate::citations::collect_citations_for_adapters;
use crate::handlers::rag_common::{retrieve_rag_context, store_rag_evidence};
use crate::inference_core::InferenceCore;
use crate::middleware::policy_enforcement::{create_hook_context, enforce_at_hook};
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::*;
use crate::uds_client::UdsClient;
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
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;
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
    /// Random seed for reproducibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Per-adapter strength overrides (session scoped)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,
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
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            stream: true, // Always streaming for this endpoint
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
            model: req.model.clone(),
            stop_policy: req.stop_policy.clone(),
            created_at: std::time::Instant::now(),
            router_seed: None, // Use default router behavior for streaming
            worker_auth_token: None,
            policy_mask_digest: None, // Streaming requests don't use policy hooks
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
    /// Error occurred
    Error(String),
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

    // Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    if let Err(violation) = enforce_at_hook(&state, &routing_hook_ctx).await {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy hook violation (pre-routing)",
        )
        .with_details(violation.message)
        .into());
    }

    // Enforce policies at OnBeforeInference hook
    let hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnBeforeInference,
        "inference",
        Some(&adapter_id),
    );
    if let Err(violation) = enforce_at_hook(&state, &hook_ctx).await {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy hook violation",
        )
        .with_details(violation.message)
        .into());
    }

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
        adapter_id,
        claims.tenant_id.clone(),
        claims.sub.clone(),
    )
    .await;

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
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
///   -d '{"prompt": "What is the pricing model?", "max_tokens": 200, "collection_id": "marketing-docs"}'
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

    check_uma_backpressure(&state)?;

    // Generate request ID
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
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

    // Enforce policies at OnRequestBeforeRouting hook (before adapter selection)
    let routing_hook_ctx = create_hook_context(
        &claims,
        &request_id,
        PolicyHook::OnRequestBeforeRouting,
        "inference",
        None, // No adapter selected yet
    );
    if let Err(violation) = enforce_at_hook(&state, &routing_hook_ctx).await {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy hook violation (pre-routing)",
        )
        .with_details(violation.message)
        .into());
    }

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
    if let Err(violation) = enforce_at_hook(&state, &hook_ctx).await {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_HOOK_VIOLATION",
            "Policy hook violation",
        )
        .with_details(violation.message)
        .into());
    }

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
    let augmented_prompt = if let Some(collection_id) = &req.collection_id {
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
                    // Store evidence for this retrieval
                    store_rag_evidence(&state, &rag_result, &request_id, req.session_id.as_deref())
                        .await;

                    info!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        context_len = rag_result.context.len(),
                        doc_count = rag_result.doc_ids.len(),
                        "Augmented prompt with RAG context"
                    );
                    // RAG context prepended to base_prompt (which may include chat history)
                    format!(
                        "Use the following context to answer the question.\n\n\
                         Context:\n{}\n\n\
                         {}",
                        rag_result.context, base_prompt
                    )
                }
                Ok(_) => {
                    debug!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        "No relevant context found in collection"
                    );
                    base_prompt.clone()
                }
                Err(e) => {
                    warn!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        error = %e,
                        "RAG retrieval failed, proceeding without context"
                    );
                    base_prompt.clone()
                }
            }
        } else {
            warn!(
                request_id = %request_id,
                "Embedding model not configured, skipping RAG retrieval"
            );
            base_prompt.clone()
        }
    } else {
        // No collection_id, use base_prompt directly (may include chat history)
        base_prompt
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
    let worker = core
        .select_worker_for_tenant(&claims.tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let uds_path_buf = std::path::PathBuf::from(&worker.uds_path);

    // Clone data for the stream
    let request_id_clone = request_id.clone();
    let model_name_clone = model_name.clone();
    let prompt = augmented_prompt; // Use RAG-augmented prompt if available
    let max_tokens = req.max_tokens;
    let temperature = req.temperature;
    let session_id = req.session_id.clone();
    let adapters = req.adapters.clone();
    // Pass hook context to stream state
    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();

    // Create cancellation token for client disconnect detection
    let cancellation_token = CancellationToken::new();
    let drop_guard = StreamDropGuard {
        cancellation_token: cancellation_token.clone(),
        request_id: request_id.clone(),
    };

    // Create the SSE stream with cancellation support
    let stream = stream::unfold(
        (
            StreamState::new(
                state,
                uds_path_buf,
                prompt,
                max_tokens,
                temperature,
                request_id_clone,
                model_name_clone,
                session_id,
                adapters,
                tenant_id,
                user_id,
                chat_context_hash,
                cancellation_token,
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
    adapter_id: String,
    tenant_id: String,
    user_id: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let state_clone = state.clone();
    let adapter_id_clone = adapter_id.clone();
    let tenant_id_clone = tenant_id.clone();
    let user_id_clone = user_id.clone();

    stream::unfold(
        LoadingStreamState::new(
            state_clone,
            request,
            adapter_id_clone,
            tenant_id_clone,
            user_id_clone,
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

/// State machine for loading progress streaming
struct LoadingStreamState {
    state: AppState,
    request: StreamingInferRequest,
    adapter_id: String,
    tenant_id: String,
    user_id: String,
    phase: LoadingPhase,
    start_time: std::time::Instant,
    token_count: usize,
    request_id: String,
    /// Pinned adapters that were unavailable
    unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode
    pinned_routing_fallback: Option<String>,
    /// Stop reason code (PRD: Hard Deterministic Stop Controller)
    stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    stop_policy_digest_b3: Option<String>,
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
        adapter_id: String,
        tenant_id: String,
        user_id: String,
    ) -> Self {
        let request_id = uuid::Uuid::new_v4().to_string();
        Self {
            state,
            request,
            adapter_id,
            tenant_id,
            user_id,
            phase: LoadingPhase::CheckingState,
            start_time: std::time::Instant::now(),
            token_count: 0,
            request_id,
            unavailable_pinned_adapters: None,
            pinned_routing_fallback: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
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
                // Run inference and stream tokens
                match self.run_inference().await {
                    Ok(Some(token)) => {
                        self.token_count += 1;
                        Some(InferenceEvent::Token {
                            text: token,
                            token_id: None,
                        })
                    }
                    Ok(None) => {
                        // Inference complete
                        self.phase = LoadingPhase::Complete;
                        let latency_ms = self.start_time.elapsed().as_millis() as u64;

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

                        tokio::spawn(async move {
                            let hook_ctx = create_hook_context(
                                &crate::auth::Claims {
                                    sub: user_id.clone(),
                                    email: String::new(),
                                    role: "system".to_string(),
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
                                    principal_type: Some(PrincipalType::InternalService),
                                },
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
                        })
                    }
                    Err(e) => {
                        self.phase = LoadingPhase::Complete;
                        Some(InferenceEvent::Error {
                            message: format!("Inference failed: {}", e),
                            recoverable: false,
                        })
                    }
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

    async fn run_inference(&mut self) -> Result<Option<String>, String> {
        // Use InferenceCore for all inference - single unified path
        // This ensures routing enforcement, RAG, evidence recording, and session activity
        // are all handled consistently.

        // Build Claims from stored tenant/user info
        let claims = Claims {
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
        };

        // Convert StreamingInferRequest to InferenceRequestInternal
        let internal_request: InferenceRequestInternal = (&self.request, &claims).into();

        // Execute via InferenceCore - the single entry point for all inference
        let core = InferenceCore::new(&self.state);
        match core.route_and_infer(internal_request, None).await {
            Ok(result) => {
                debug!(
                    text_len = result.text.len(),
                    finish_reason = %result.finish_reason,
                    adapters_used = ?result.adapters_used,
                    unavailable_pinned = ?result.unavailable_pinned_adapters,
                    pinned_fallback = ?result.pinned_routing_fallback,
                    "Received inference response via InferenceCore"
                );

                // Store pinned adapter metadata
                self.unavailable_pinned_adapters = result.unavailable_pinned_adapters.clone();
                self.pinned_routing_fallback = result.pinned_routing_fallback.clone();

                // Store stop controller metadata (PRD: Hard Deterministic Stop Controller)
                self.stop_reason_code = result.stop_reason_code;
                self.stop_reason_token_index = result.stop_reason_token_index;
                self.stop_policy_digest_b3 = result.stop_policy_digest_b3.clone();

                // Mark as complete after returning the response
                self.phase = LoadingPhase::Complete;
                Ok(Some(result.text))
            }
            Err(e) => {
                error!(error = %e, "UDS inference failed");
                Err(format!("Inference failed: {}", e))
            }
        }
    }

    fn format_loading_event(&self, event: InferenceEvent) -> Event {
        let json = serialize_safe(&event, "loading_event");
        Event::default().data(json)
    }
}

/// Internal state for streaming generation
#[allow(dead_code)]
struct StreamState {
    /// Application state (reserved for future token-by-token streaming)
    state: AppState,
    uds_path: std::path::PathBuf,
    prompt: String,
    max_tokens: usize,
    /// Temperature (reserved for future token-by-token streaming)
    temperature: f32,
    request_id: String,
    model_name: String,
    // State machine
    phase: StreamPhase,
    // Generated text for chunking
    generated_text: Option<String>,
    word_index: usize,
    words: Vec<String>,
    // Idle timeout tracking (5 minutes default)
    last_activity: Arc<TokioMutex<std::time::Instant>>,
    idle_timeout: Duration,
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
    // BLAKE3 hash of chat context for replay metadata
    chat_context_hash: Option<String>,
    // Stop controller metadata (PRD: Hard Deterministic Stop Controller)
    stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    stop_reason_token_index: Option<u32>,
    stop_policy_digest_b3: Option<String>,
}

/// Maximum size for words buffer to prevent unbounded growth
const MAX_WORDS_BUFFER_SIZE: usize = 100_000;

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
    GeneratingText,
    StreamingTokens,
    Done,
}

impl StreamState {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: AppState,
        uds_path: std::path::PathBuf,
        prompt: String,
        max_tokens: usize,
        temperature: f32,
        request_id: String,
        model_name: String,
        session_id: Option<String>,
        adapters: Option<Vec<String>>,
        tenant_id: String,
        user_id: String,
        chat_context_hash: Option<String>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            state,
            uds_path,
            prompt,
            max_tokens,
            temperature,
            request_id,
            model_name,
            phase: StreamPhase::Start,
            tenant_id,
            user_id,
            after_hook_fired: false,
            generated_text: None,
            word_index: 0,
            words: Vec::new(),
            last_activity: Arc::new(TokioMutex::new(std::time::Instant::now())),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            cancellation_token,
            session_id,
            adapters,
            chat_context_hash,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
        }
    }

    /// Check if stream has been idle for too long
    async fn is_idle(&self) -> bool {
        let last = self.last_activity.lock().await;
        last.elapsed() > self.idle_timeout
    }

    /// Update last activity timestamp
    async fn update_activity(&self) {
        let mut last = self.last_activity.lock().await;
        *last = std::time::Instant::now();
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
            return Some(StreamEvent::Error("Stream cancelled".to_string()));
        }

        // Check for idle timeout
        if self.is_idle().await {
            warn!(request_id = %self.request_id, "Stream idle timeout (5 minutes)");
            self.phase = StreamPhase::Done;
            return Some(StreamEvent::Error("Stream idle timeout".to_string()));
        }

        // Update activity timestamp
        self.update_activity().await;

        match self.phase {
            StreamPhase::Start => {
                // Send initial role chunk
                self.phase = StreamPhase::GeneratingText;
                Some(StreamEvent::Start)
            }
            StreamPhase::GeneratingText => {
                // Generate the full response via UDS
                match self.generate_response().await {
                    Ok(text) => {
                        // Split into words for streaming simulation
                        let words: Vec<String> = text
                            .split_inclusive(|c: char| c.is_whitespace() || c == '\n')
                            .map(|s| s.to_string())
                            .collect();

                        // Enforce max buffer size to prevent unbounded growth
                        if words.len() > MAX_WORDS_BUFFER_SIZE {
                            warn!(
                                request_id = %self.request_id,
                                words_count = words.len(),
                                max_size = MAX_WORDS_BUFFER_SIZE,
                                "Words buffer exceeded max size, truncating"
                            );
                            self.words = words.into_iter().take(MAX_WORDS_BUFFER_SIZE).collect();
                        } else {
                            self.words = words;
                        }

                        self.generated_text = Some(text);
                        self.word_index = 0;
                        self.phase = StreamPhase::StreamingTokens;
                        // Return first token immediately instead of recursing
                        if !self.words.is_empty() {
                            let word = self.words[0].clone();
                            self.word_index = 1;
                            Some(StreamEvent::Token(word))
                        } else {
                            self.phase = StreamPhase::Done;
                            Some(StreamEvent::Done {
                                finish_reason: "stop".to_string(),
                            })
                        }
                    }
                    Err(e) => {
                        self.phase = StreamPhase::Done;
                        Some(StreamEvent::Error(e))
                    }
                }
            }
            StreamPhase::StreamingTokens => {
                if self.word_index < self.words.len() {
                    let word = self.words[self.word_index].clone();
                    self.word_index += 1;
                    Some(StreamEvent::Token(word))
                } else {
                    self.phase = StreamPhase::Done;
                    Some(StreamEvent::Done {
                        finish_reason: "stop".to_string(),
                    })
                }
            }
            StreamPhase::Done => None,
        }
    }

    async fn generate_response(&mut self) -> Result<String, String> {
        // Use InferenceCore for all inference - single unified path
        // This ensures routing enforcement, RAG, evidence recording, and session activity
        // are all handled consistently.

        // Build InferenceRequestInternal directly from StreamState fields
        let internal_request = InferenceRequestInternal {
            request_id: self.request_id.clone(),
            cpid: self.tenant_id.clone(),
            prompt: self.prompt.clone(),
            stream: true,
            batch_item_id: None,
            rag_enabled: false, // RAG handled earlier in the pipeline
            rag_collection_id: None,
            dataset_version_id: None,
            adapter_stack: None,
            adapters: self.adapters.clone(),
            stack_id: None,
            domain_hint: None,
            stack_version: None,
            stack_determinism_mode: None,
            stack_routing_determinism_mode: None,
            effective_adapter_ids: None,
            determinism_mode: None,
            routing_determinism_mode: None,
            seed_mode: None,
            request_seed: None,
            backend_profile: None,
            coreml_mode: None,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            top_k: None,
            top_p: None,
            seed: None,
            router_seed: None, // deterministic routing (not set for streaming)
            require_evidence: false,
            session_id: self.session_id.clone(),
            pinned_adapter_ids: None, // Pinning not yet exposed in streaming API
            chat_context_hash: self.chat_context_hash.clone(),
            adapter_strength_overrides: None,
            model: Some(self.model_name.clone()),
            stop_policy: None, // StreamState doesn't carry stop_policy yet
            created_at: std::time::Instant::now(),
            worker_auth_token: None,
            policy_mask_digest: None, // Streaming doesn't use policy enforcement hooks
        };

        // Execute via InferenceCore - the single entry point for all inference
        // Use tokio::select! to allow cancellation if client disconnects
        let core = InferenceCore::new(&self.state);
        let inference_future = core.route_and_infer(internal_request, None);

        tokio::select! {
            // Client disconnect - cancel inference
            _ = self.cancellation_token.cancelled() => {
                warn!(
                    request_id = %self.request_id,
                    "Inference cancelled due to client disconnect"
                );
                Err("Stream cancelled by client disconnect".to_string())
            }
            // Normal inference completion
            result = inference_future => {
                match result {
                    Ok(result) => {
                        debug!(
                            text_len = result.text.len(),
                            finish_reason = %result.finish_reason,
                            adapters_used = ?result.adapters_used,
                            "Received inference response via InferenceCore"
                        );

                        // Store stop controller metadata (PRD: Hard Deterministic Stop Controller)
                        self.stop_reason_code = result.stop_reason_code;
                        self.stop_reason_token_index = result.stop_reason_token_index;
                        self.stop_policy_digest_b3 = result.stop_policy_digest_b3.clone();

                        Ok(result.text)
                    }
                    Err(e) => {
                        error!(error = %e, "InferenceCore inference failed");
                        Err(format!("Inference failed: {}", e))
                    }
                }
            }
        }
    }

    fn format_event(&self, event: StreamEvent) -> Event {
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

                tokio::spawn(async move {
                    let hook_ctx = create_hook_context(
                        &crate::auth::Claims {
                            sub: user_id.clone(),
                            email: String::new(),
                            role: "system".to_string(), // Hook fires post-stream, role used for audit only
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
                            principal_type: Some(PrincipalType::InternalService),
                        },
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
            StreamEvent::Error(message) => {
                let error_response = serde_json::json!({
                    "error": {
                        "message": message,
                        "type": "inference_error",
                        "code": "INFERENCE_ERROR"
                    }
                });
                Event::default().data(serialize_safe(&error_response, "stream_error"))
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
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("pin-1"));
        assert!(json.contains("stack_only"));
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
