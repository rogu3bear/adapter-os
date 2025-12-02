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

use crate::auth::Claims;
use crate::security::check_tenant_access;
use crate::state::AppState;
use crate::types::*;
use crate::uds_client::UdsClient;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_lora_rag::{PgVectorIndex, EmbeddingModel};
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
    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,
    /// Collection ID for scoping RAG retrieval to specific document collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Session ID for linking inference to chat sessions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
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
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    // Extract adapter ID from request
    let adapter_id = if let Some(adapters) = &req.adapters {
        if adapters.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapters list cannot be empty when provided")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
        adapters[0].clone()
    } else if let Some(stack) = &req.adapter_stack {
        if stack.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("adapter_stack list cannot be empty when provided")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
        stack[0].clone()
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("must specify adapters or adapter_stack")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    };

    // Check UMA pressure
    let pressure_str = state.uma_monitor.get_current_pressure().to_string();
    let is_high_pressure = pressure_str == "High" || pressure_str == "Critical";
    if is_high_pressure {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("service under memory pressure")
                    .with_code("BACKPRESSURE")
                    .with_string_details(format!(
                        "level={}, retry_after_secs=30, action=reduce max_tokens or retry later",
                        pressure_str
                    )),
            ),
        ));
    }

    info!(
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens,
        adapter_id = %adapter_id,
        "Starting streaming inference with loading progress"
    );

    // Audit log: inference execution start
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    // Create the loading progress stream
    let stream = stream_with_loading_progress(&state, req, adapter_id, claims.tenant_id.clone()).await;

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
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    // Check UMA pressure
    let pressure_str = state.uma_monitor.get_current_pressure().to_string();
    let is_high_pressure = pressure_str == "High" || pressure_str == "Critical";
    if is_high_pressure {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("service under memory pressure")
                    .with_code("BACKPRESSURE")
                    .with_string_details(format!(
                        "level={}, retry_after_secs=30, action=reduce max_tokens or retry later",
                        pressure_str
                    )),
            ),
        ));
    }

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
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(
                            ErrorResponse::new("Access denied to session")
                                .with_code("FORBIDDEN")
                                .with_string_details("Session does not belong to your tenant"),
                        ),
                    ));
                }
            }
            Ok(None) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Session not found")
                            .with_code("NOT_FOUND"),
                    ),
                ));
            }
            Err(e) => {
                error!(
                    request_id = %request_id,
                    session_id = %session_id,
                    error = %e,
                    "Failed to validate session access"
                );
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate session access")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }
    }

    info!(
        request_id = %request_id,
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens,
        collection_id = ?req.collection_id,
        session_id = ?req.session_id,
        "Starting streaming inference"
    );

    // Collection-scoped RAG integration
    // When collection_id is provided, retrieve relevant context and augment the prompt
    let augmented_prompt = if let Some(collection_id) = &req.collection_id {
        // CRITICAL: Validate collection belongs to user's tenant
        match state.db.get_collection(collection_id).await {
            Ok(Some(collection)) => {
                if collection.tenant_id != claims.tenant_id {
                    warn!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        user_tenant = %claims.tenant_id,
                        collection_tenant = %collection.tenant_id,
                        "Collection access denied - tenant mismatch"
                    );
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(
                            ErrorResponse::new("Access denied to collection")
                                .with_code("FORBIDDEN")
                                .with_string_details("Collection does not belong to your tenant"),
                        ),
                    ));
                }
            }
            Ok(None) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Collection not found")
                            .with_code("NOT_FOUND"),
                    ),
                ));
            }
            Err(e) => {
                error!(
                    request_id = %request_id,
                    collection_id = %collection_id,
                    error = %e,
                    "Failed to validate collection access"
                );
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate collection access")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }

        if let Some(ref embedding_model) = state.embedding_model {
            match retrieve_rag_context(
                &state,
                &claims.tenant_id,
                collection_id,
                &req.prompt,
                embedding_model.clone(),
                &request_id,
                req.session_id.as_deref(),
            )
            .await
            {
                Ok(context) if !context.is_empty() => {
                    info!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        context_len = context.len(),
                        "Augmented prompt with RAG context"
                    );
                    format!(
                        "Use the following context to answer the question.\n\n\
                         Context:\n{}\n\n\
                         Question: {}",
                        context, req.prompt
                    )
                }
                Ok(_) => {
                    debug!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        "No relevant context found in collection"
                    );
                    req.prompt.clone()
                }
                Err(e) => {
                    warn!(
                        request_id = %request_id,
                        collection_id = %collection_id,
                        error = %e,
                        "RAG retrieval failed, proceeding without context"
                    );
                    req.prompt.clone()
                }
            }
        } else {
            warn!(
                request_id = %request_id,
                "Embedding model not configured, skipping RAG retrieval"
            );
            req.prompt.clone()
        }
    } else {
        req.prompt.clone()
    };

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));

    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
    )
    .await;

    // Get available workers
    let workers = state.db.list_all_workers().await.map_err(|e| {
        error!(error = %e, "Failed to list workers");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Resolve UDS path
    let uds_path_buf = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/adapteros.sock".to_string());
        std::path::PathBuf::from(fallback)
    };

    // Clone data for the stream
    let request_id_clone = request_id.clone();
    let model_name_clone = model_name.clone();
    let prompt = augmented_prompt; // Use RAG-augmented prompt if available
    let max_tokens = req.max_tokens;
    let temperature = req.temperature;
    let session_id = req.session_id.clone();
    let adapters = req.adapters.clone();

    // Create the SSE stream
    let stream = stream::unfold(
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
        ),
        |mut stream_state| async move {
            match stream_state.next_event().await {
                Some(event) => {
                    let sse_event = stream_state.format_event(event);
                    Some((Ok(sse_event), stream_state))
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
) -> impl Stream<Item = Result<Event, Infallible>> {
    let state_clone = state.clone();
    let adapter_id_clone = adapter_id.clone();
    let tenant_id_clone = tenant_id.clone();

    stream::unfold(
        LoadingStreamState::new(state_clone, request, adapter_id_clone, tenant_id_clone),
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
    phase: LoadingPhase,
    start_time: std::time::Instant,
    token_count: usize,
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
    fn new(state: AppState, request: StreamingInferRequest, adapter_id: String, tenant_id: String) -> Self {
        Self {
            state,
            request,
            adapter_id,
            tenant_id,
            phase: LoadingPhase::CheckingState,
            start_time: std::time::Instant::now(),
            token_count: 0,
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
                        Some(InferenceEvent::Done {
                            total_tokens: self.token_count,
                            latency_ms,
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
                // Check if adapter is in a ready state (runtime state, not lifecycle state)
                // current_state tracks runtime memory state: unloaded, cold, warm, hot, resident
                // lifecycle_state tracks metadata state: draft, active, deprecated, retired
                let is_ready = matches!(
                    adapter.current_state.as_str(),
                    "warm" | "hot" | "resident"
                );
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

        // Wrap the actual load operation with LoadCoordinator
        // This ensures only one request actually triggers the load
        let _handle = state
            .load_coordinator
            .load_or_wait(&adapter_id.clone(), || async move {
                // Get available workers
                let workers = state.db.list_all_workers().await.map_err(|e| {
                    adapteros_core::AosError::Database(format!("Failed to list workers: {}", e))
                })?;

                if workers.is_empty() {
                    return Err(adapteros_core::AosError::Lifecycle(
                        "No workers available".to_string(),
                    ));
                }

                // Send load request to worker via UDS using HTTP POST
                let uds_path = std::path::PathBuf::from(&workers[0].uds_path);
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
                    path: std::path::PathBuf::from(&workers[0].uds_path),
                    memory_bytes: 0,
                    metadata: adapteros_lora_lifecycle::loader::AdapterMetadata {
                        num_parameters: 0,
                        rank: None,
                        target_modules: vec![],
                    },
                })
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
        // This is a placeholder - in production, this would stream tokens from the worker
        // For now, we generate a simple response similar to the existing implementation

        // Get available workers
        let workers = self
            .state
            .db
            .list_all_workers()
            .await
            .map_err(|e| format!("Failed to list workers: {}", e))?;

        if workers.is_empty() {
            return Err("No workers available".to_string());
        }

        let uds_path = std::path::PathBuf::from(&workers[0].uds_path);
        let uds_client = UdsClient::new(Duration::from_secs(60));

        // Build worker inference request
        let worker_request = WorkerInferRequest {
            cpid: uuid::Uuid::new_v4().to_string(),
            prompt: self.request.prompt.clone(),
            max_tokens: self.request.max_tokens,
            require_evidence: self.request.require_evidence,
        };

        // Send request to worker
        match uds_client.infer(&uds_path, worker_request).await {
            Ok(response) => {
                if let Some(text) = response.text {
                    // For now, return the whole text as a single token
                    // In production, this would stream tokens individually
                    debug!(
                        text_len = text.len(),
                        status = %response.status,
                        "Received inference response"
                    );

                    // Link session trace after successful inference
                    if let Some(session_id) = &self.request.session_id {
                        if let Err(e) = self
                            .state
                            .db
                            .add_session_trace(session_id, "adapter", &self.adapter_id)
                            .await
                        {
                            warn!(
                                session_id = %session_id,
                                adapter_id = %self.adapter_id,
                                error = %e,
                                "Failed to add session trace"
                            );
                        }
                        if let Err(e) = self.state.db.update_chat_session_activity(session_id).await {
                            warn!(
                                session_id = %session_id,
                                error = %e,
                                "Failed to update session activity"
                            );
                        }
                    }

                    // Mark as complete after returning the response
                    self.phase = LoadingPhase::Complete;
                    Ok(Some(text))
                } else {
                    Err("No text in response".to_string())
                }
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
}

/// Maximum size for words buffer to prevent unbounded growth
const MAX_WORDS_BUFFER_SIZE: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
enum StreamPhase {
    Start,
    GeneratingText,
    StreamingTokens,
    Done,
}

impl StreamState {
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
            generated_text: None,
            word_index: 0,
            words: Vec::new(),
            last_activity: Arc::new(TokioMutex::new(std::time::Instant::now())),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            cancellation_token: CancellationToken::new(),
            session_id,
            adapters,
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

    async fn generate_response(&self) -> Result<String, String> {
        // Create UDS client with 60 second timeout
        let uds_client = UdsClient::new(Duration::from_secs(60));

        // Build worker inference request
        let worker_request = WorkerInferRequest {
            cpid: uuid::Uuid::new_v4().to_string(),
            prompt: self.prompt.clone(),
            max_tokens: self.max_tokens,
            require_evidence: false,
        };

        // Send request to worker
        match uds_client.infer(&self.uds_path, worker_request).await {
            Ok(response) => {
                if let Some(text) = response.text {
                    debug!(
                        text_len = text.len(),
                        status = %response.status,
                        "Received inference response"
                    );

                    // Link session trace after successful inference
                    if let Some(session_id) = &self.session_id {
                        if let Some(adapters) = &self.adapters {
                            for adapter_id in adapters {
                                if let Err(e) = self
                                    .state
                                    .db
                                    .add_session_trace(session_id, "adapter", adapter_id)
                                    .await
                                {
                                    warn!(
                                        session_id = %session_id,
                                        adapter_id = %adapter_id,
                                        error = %e,
                                        "Failed to add session trace"
                                    );
                                }
                            }
                        }
                        if let Err(e) = self.state.db.update_chat_session_activity(session_id).await {
                            warn!(
                                session_id = %session_id,
                                error = %e,
                                "Failed to update session activity"
                            );
                        }
                    }

                    Ok(text)
                } else {
                    Err("No text in response".to_string())
                }
            }
            Err(e) => {
                error!(error = %e, "UDS inference failed");
                Err(format!("Inference failed: {}", e))
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
                    }],
                };
                Event::default().data(serialize_safe(&chunk, "stream_token"))
            }
            StreamEvent::Done { finish_reason } => {
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

/// Parse a RAG doc_id to extract the base document_id and chunk_index.
///
/// RAG doc_ids follow the format `{document_id}__chunk_{index}`.
/// Returns (document_id, chunk_index) if parsing succeeds.
fn parse_rag_doc_id(doc_id: &str) -> Option<(String, i32)> {
    const CHUNK_SEPARATOR: &str = "__chunk_";

    if let Some(pos) = doc_id.rfind(CHUNK_SEPARATOR) {
        let document_id = doc_id[..pos].to_string();
        let chunk_index_str = &doc_id[pos + CHUNK_SEPARATOR.len()..];
        if let Ok(chunk_index) = chunk_index_str.parse::<i32>() {
            return Some((document_id, chunk_index));
        }
    }
    None
}

/// Retrieve relevant context from RAG for a given query
///
/// This function:
/// 1. Encodes the query using the embedding model
/// 2. Retrieves top-k similar documents from the vector index
/// 3. Filters results by collection membership (efficient ID-only check)
/// 4. Concatenates the retrieved text chunks as context
/// 5. Stores evidence entries in the database (batched) for audit trails
///
/// Returns the concatenated context string, or empty string if no results.
async fn retrieve_rag_context(
    state: &AppState,
    tenant_id: &str,
    collection_id: &str,
    query: &str,
    embedding_model: Arc<dyn EmbeddingModel + Send + Sync>,
    request_id: &str,
    session_id: Option<&str>,
) -> adapteros_core::Result<String> {
    // Retrieve more candidates since we may filter some out by collection membership
    const CANDIDATE_K: usize = 15;
    const TOP_K: usize = 5;
    const MAX_CONTEXT_CHARS: usize = 4000;

    // Encode the query
    let query_embedding = embedding_model.encode_text(query)?;

    // Get the embedding model hash for index creation
    let model_hash = embedding_model.model_hash();
    let dimension = embedding_model.dimension();

    // Create RAG index using the database pool
    let index = PgVectorIndex::new_sqlite(state.db_pool.clone(), model_hash, dimension);

    // Retrieve candidate documents (more than TOP_K since we'll filter by collection)
    let all_results = index.retrieve(tenant_id, &query_embedding, CANDIDATE_K).await?;

    // Get document IDs that belong to the specified collection (efficient - just IDs)
    let collection_doc_ids: std::collections::HashSet<String> = state
        .db
        .list_collection_document_ids(collection_id)
        .await?
        .into_iter()
        .collect();

    // Filter results by collection membership using parsed document_id
    // RAG doc_id format is `{document_id}__chunk_{index}`, we need to extract document_id
    let results: Vec<_> = all_results
        .into_iter()
        .filter(|doc| {
            if let Some((document_id, _)) = parse_rag_doc_id(&doc.doc_id) {
                collection_doc_ids.contains(&document_id)
            } else {
                // If we can't parse the doc_id, try direct match (backwards compatibility)
                collection_doc_ids.contains(&doc.doc_id)
            }
        })
        .take(TOP_K)
        .collect();

    debug!(
        collection_id = %collection_id,
        collection_doc_count = collection_doc_ids.len(),
        candidate_count = CANDIDATE_K,
        filtered_results = results.len(),
        "Filtered RAG results by collection membership"
    );

    if results.is_empty() {
        return Ok(String::new());
    }

    // Concatenate results with truncation first to compute context hash
    let mut context = String::new();
    for (i, doc) in results.iter().enumerate() {
        if context.len() + doc.text.len() > MAX_CONTEXT_CHARS {
            break;
        }
        if i > 0 {
            context.push_str("\n\n---\n\n");
        }
        context.push_str(&doc.text);
    }

    // Compute context hash for evidence
    let context_hash = adapteros_core::B3Hash::hash(context.as_bytes());

    // Build evidence entries with proper document_id, chunk_id, and page_number
    // Note: chunk_id must be the actual document_chunks.id (FK constraint)
    let mut evidence_params_list = Vec::with_capacity(results.len());

    for (rank, doc) in results.iter().enumerate() {
        let (document_id, chunk_id, page_number) =
            if let Some((doc_id, chunk_index)) = parse_rag_doc_id(&doc.doc_id) {
                // Look up chunk metadata for chunk_id (FK) and page_number
                match state
                    .db
                    .get_chunk_by_document_and_index(&doc_id, chunk_index)
                    .await
                {
                    Ok(Some(chunk)) => {
                        // Use actual chunk.id for FK constraint
                        (doc_id, chunk.id, chunk.page_number)
                    }
                    Ok(None) => {
                        debug!(
                            document_id = %doc_id,
                            chunk_index = chunk_index,
                            "Chunk not found in document_chunks table, skipping evidence"
                        );
                        // Skip this entry - can't store evidence without valid FK
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            document_id = %doc_id,
                            chunk_index = chunk_index,
                            error = %e,
                            "Failed to look up chunk metadata, skipping evidence"
                        );
                        // Skip this entry - can't store evidence without valid FK
                        continue;
                    }
                }
            } else {
                // Can't parse doc_id - skip evidence (need valid FKs)
                debug!(
                    doc_id = %doc.doc_id,
                    "Cannot parse RAG doc_id, skipping evidence"
                );
                continue;
            };

        evidence_params_list.push(adapteros_db::CreateEvidenceParams {
            inference_id: request_id.to_string(),
            session_id: session_id.map(|s| s.to_string()),
            message_id: None,
            document_id,
            chunk_id,
            page_number,
            document_hash: doc.span_hash.to_hex(),
            chunk_hash: doc.span_hash.to_hex(),
            relevance_score: doc.score as f64,
            rank: rank as i32,
            context_hash: context_hash.to_hex(),
        });
    }

    // Batch insert all evidence entries in a single transaction
    match state
        .db
        .create_inference_evidence_batch(evidence_params_list)
        .await
    {
        Ok(ids) => {
            debug!(
                inference_id = %request_id,
                evidence_count = ids.len(),
                "Stored RAG evidence entries"
            );
        }
        Err(e) => {
            warn!(
                inference_id = %request_id,
                error = %e,
                "Failed to store RAG evidence entries"
            );
        }
    }

    info!(
        tenant_id = %tenant_id,
        collection_id = %collection_id,
        num_results = results.len(),
        context_len = context.len(),
        embedding_model_hash = %model_hash.to_hex(),
        "Retrieved RAG context with evidence"
    );

    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("100"));
        assert!(json.contains("5000"));
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
        // Standard format: document_id__chunk_index
        let result = parse_rag_doc_id("doc-123__chunk_0");
        assert_eq!(result, Some(("doc-123".to_string(), 0)));

        let result = parse_rag_doc_id("my-document-uuid__chunk_42");
        assert_eq!(result, Some(("my-document-uuid".to_string(), 42)));

        // Document ID with underscores (edge case - uses rfind)
        let result = parse_rag_doc_id("doc_with_underscores__chunk_5");
        assert_eq!(result, Some(("doc_with_underscores".to_string(), 5)));
    }

    #[test]
    fn test_parse_rag_doc_id_invalid() {
        // Missing __chunk_ separator
        assert_eq!(parse_rag_doc_id("doc-123"), None);
        assert_eq!(parse_rag_doc_id("doc-123_chunk_0"), None);

        // Invalid chunk index (not a number)
        assert_eq!(parse_rag_doc_id("doc-123__chunk_abc"), None);

        // Empty document ID
        assert_eq!(parse_rag_doc_id("__chunk_0"), Some(("".to_string(), 0)));

        // Negative chunk index (should still parse as i32)
        assert_eq!(
            parse_rag_doc_id("doc__chunk_-1"),
            Some(("doc".to_string(), -1))
        );
    }

    #[test]
    fn test_parse_rag_doc_id_special_cases() {
        // UUID-style document ID
        let result = parse_rag_doc_id("550e8400-e29b-41d4-a716-446655440000__chunk_99");
        assert_eq!(
            result,
            Some(("550e8400-e29b-41d4-a716-446655440000".to_string(), 99))
        );

        // Document ID with path-like format (sanitized)
        let result = parse_rag_doc_id("documents_report_2024_pdf__chunk_0");
        assert_eq!(
            result,
            Some(("documents_report_2024_pdf".to_string(), 0))
        );
    }
}
