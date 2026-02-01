//! Model Server API Handlers
//!
//! These handlers provide control plane visibility into the Model Server
//! process that provides shared model inference for workers.
//!
//! The Model Server reduces GPU memory usage by loading the model once
//! and serving multiple workers via gRPC over UDS.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;

use crate::state::AppState;

/// Model server status response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelServerStatusResponse {
    /// Whether the model server feature is enabled
    pub enabled: bool,
    /// Whether the model server is currently connected/healthy
    pub connected: bool,
    /// Server address (if configured)
    pub server_addr: Option<String>,
    /// Number of active sessions
    pub active_sessions: u32,
    /// Number of hot adapters cached in model server
    pub hot_adapters: u32,
    /// KV cache utilization percentage (0-100)
    pub kv_cache_utilization: f32,
    /// Total forward pass requests served
    pub total_requests: u64,
    /// Average forward pass latency in milliseconds
    pub avg_latency_ms: f32,
    /// Model currently loaded (if any)
    pub model_name: Option<String>,
}

/// Model server warmup request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WarmupRequest {
    /// Session ID to warm up
    pub session_id: String,
    /// Input token IDs to pre-fill
    pub input_ids: Vec<u32>,
    /// Maximum sequence length for the session
    pub max_seq_len: Option<u32>,
}

/// Model server warmup response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WarmupResponse {
    /// Whether warmup succeeded
    pub success: bool,
    /// Number of tokens cached
    pub cached_tokens: u32,
    /// Warmup latency in milliseconds
    pub latency_ms: f32,
}

/// Model server drain request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DrainRequest {
    /// Grace period in seconds before forced shutdown
    pub grace_period_secs: Option<u32>,
}

/// Model server drain response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DrainResponse {
    /// Whether drain was initiated
    pub initiated: bool,
    /// Number of active sessions being drained
    pub draining_sessions: u32,
    /// Estimated drain completion time in seconds
    pub estimated_completion_secs: u32,
}

/// Get model server status
///
/// Returns the current status of the Model Server, including connection
/// health, loaded model, cache utilization, and performance metrics.
#[utoipa::path(
    get,
    path = "/v1/model-server/status",
    tag = "model-server",
    responses(
        (status = 200, description = "Model server status", body = ModelServerStatusResponse),
        (status = 503, description = "Model server not available")
    )
)]
pub async fn get_model_server_status(State(state): State<AppState>) -> impl IntoResponse {
    // Check if model server is enabled and get status
    let status = state.get_model_server_status().await;

    match status {
        Some(s) => (StatusCode::OK, Json(s)),
        None => {
            // Model server not enabled or not available
            let response = ModelServerStatusResponse {
                enabled: false,
                connected: false,
                server_addr: None,
                active_sessions: 0,
                hot_adapters: 0,
                kv_cache_utilization: 0.0,
                total_requests: 0,
                avg_latency_ms: 0.0,
                model_name: None,
            };
            (StatusCode::OK, Json(response))
        }
    }
}

/// Trigger KV cache warmup
///
/// Pre-populates the KV cache for a session with the given tokens.
/// This reduces latency for the first inference request.
#[utoipa::path(
    post,
    path = "/v1/model-server/warmup",
    tag = "model-server",
    request_body = WarmupRequest,
    responses(
        (status = 200, description = "Warmup completed", body = WarmupResponse),
        (status = 503, description = "Model server not available"),
        (status = 400, description = "Invalid warmup request")
    )
)]
pub async fn warmup_model_server(
    State(state): State<AppState>,
    Json(request): Json<WarmupRequest>,
) -> impl IntoResponse {
    debug!(
        session_id = %request.session_id,
        tokens = request.input_ids.len(),
        "Processing warmup request"
    );

    match state.warmup_model_server(&request).await {
        Ok(response) => {
            info!(
                session_id = %request.session_id,
                cached_tokens = response.cached_tokens,
                latency_ms = response.latency_ms,
                "Warmup completed"
            );
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            warn!(
                session_id = %request.session_id,
                error = %e,
                "Warmup failed"
            );
            let response = WarmupResponse {
                success: false,
                cached_tokens: 0,
                latency_ms: 0.0,
            };
            (StatusCode::SERVICE_UNAVAILABLE, Json(response))
        }
    }
}

/// Request model server drain
///
/// Initiates graceful shutdown of the Model Server. Active sessions
/// are allowed to complete within the grace period before forced shutdown.
#[utoipa::path(
    post,
    path = "/v1/model-server/drain",
    tag = "model-server",
    request_body = DrainRequest,
    responses(
        (status = 200, description = "Drain initiated", body = DrainResponse),
        (status = 503, description = "Model server not available"),
        (status = 409, description = "Drain already in progress")
    )
)]
pub async fn drain_model_server(
    State(state): State<AppState>,
    Json(request): Json<DrainRequest>,
) -> impl IntoResponse {
    let grace_period = request.grace_period_secs.unwrap_or(30);

    info!(grace_period_secs = grace_period, "Processing drain request");

    match state.drain_model_server(grace_period).await {
        Ok(response) => {
            info!(
                draining_sessions = response.draining_sessions,
                estimated_secs = response.estimated_completion_secs,
                "Drain initiated"
            );
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            error!(error = %e, "Drain request failed");
            let response = DrainResponse {
                initiated: false,
                draining_sessions: 0,
                estimated_completion_secs: 0,
            };
            (StatusCode::SERVICE_UNAVAILABLE, Json(response))
        }
    }
}
