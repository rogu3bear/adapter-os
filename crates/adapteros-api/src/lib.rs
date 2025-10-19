//! AdapterOS API Types and Error Handling
//!
//! This crate provides:
//! - API request/response types
//! - Error handling for HTTP endpoints
//! - Serialization support for all API types
//! - Unix Domain Socket HTTP server implementation
//!
//! # Examples
//!
//! ```rust
//! use adapteros_api::{ApiError, LoginRequest, LoginResponse};
//! use axum::response::IntoResponse;
//!
//! // Handle API errors
//! let error = ApiError::NotFound("User not found".to_string());
//! let response = error.into_response();
//! ```
//!
//! References:
//! - Unix Domain Sockets: https://man7.org/linux/man-pages/man7/unix.7.html
//! - Hyper HTTP Server: https://docs.rs/hyper/latest/hyper/
//! - Axum Web Framework: https://docs.rs/axum/latest/axum/

use adapteros_core::AosError;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_worker::{InferenceRequest, InferenceResponse};
use axum::http::HeaderMap;
use axum::response::sse::{Event, Sse};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower::{Service, ServiceBuilder};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub mod types;

use adapteros_lora_kernel_api::FusedKernels;

/// API server state
pub struct ApiState<K: FusedKernels> {
    worker: Arc<tokio::sync::Mutex<adapteros_lora_worker::Worker<K>>>,
    signals_tx: broadcast::Sender<Signal>,
}

impl<K: FusedKernels> ApiState<K> {
    /// Create new API state with worker
    pub fn new(
        worker: adapteros_lora_worker::Worker<K>,
        signals_tx: broadcast::Sender<Signal>,
    ) -> Self {
        Self {
            worker: Arc::new(tokio::sync::Mutex::new(worker)),
            signals_tx,
        }
    }
}

/// Start UDS server with worker
///
/// Creates a Unix Domain Socket HTTP server with streaming responses
/// and proper error handling for AdapterOS inference requests.
pub async fn serve_uds_with_worker<K: FusedKernels + 'static, P: AsRef<Path>>(
    socket_path: P,
    worker: adapteros_lora_worker::Worker<K>,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path.as_ref();

    // Remove existing socket file if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Create parent directory if it doesn't exist
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create Unix listener
    let listener = UnixListener::bind(socket_path)?;

    // Set socket permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    println!("🚀 AdapterOS UDS server listening");
    println!("   Socket: {}", socket_path.display());
    println!("   Permissions: 600 (owner read/write only)");

    // Create API state with worker and broadcast channel for signals
    let (signals_tx, _signals_rx) = broadcast::channel::<Signal>(1024);

    // Bridge worker internal signals (WorkerSignal) to API Signal type
    let (worker_tx, mut worker_rx) =
        broadcast::channel::<adapteros_lora_worker::WorkerSignal>(1024);
    let bridge_tx = signals_tx.clone();
    // Spawn a bridge task to forward worker signals to API signals
    tokio::spawn(async move {
        while let Ok(sig) = worker_rx.recv().await {
            let api_sig = Signal {
                signal_type: sig.signal_type,
                timestamp: sig.timestamp,
                payload: sig.payload,
                priority: "normal".to_string(),
                trace_id: None,
            };
            let _ = bridge_tx.send(api_sig);
        }
    });

    let mut worker = worker;
    worker.set_signal_tx(worker_tx);

    let state = Arc::new(ApiState::new(worker, signals_tx.clone()));

    // Create router with middleware
    let app = Router::new()
        .route("/inference", post(inference_handler))
        // Accept both GET and POST for health to match clients; include policy info
        .route("/health", post(health_handler::<K>))
        .route("/health", get(health_handler::<K>))
        .route("/adapter", post(adapter_command_handler))
        // Adapter lifecycle and profiling endpoints for CLI compatibility
        .route("/adapters", get(list_adapters_handler::<K>))
        .route("/adapter/:id", get(adapter_profile_handler::<K>))
        .route("/adapter/:id/:cmd", post(adapter_lifecycle_handler::<K>))
        .route("/profile/snapshot", get(profile_snapshot_handler::<K>))
        .route("/warmup", post(warmup_handler::<K>))
        .route("/signals", get(signals_handler::<K>))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state);

    // Accept connections and serve
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                // Create TokioIo wrapper for the Unix stream
                let io = hyper_util::rt::TokioIo::new(stream);

                // Clone app for this connection
                let tower_service = app.clone();

                // Spawn task to handle connection
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    // Use hyper's service_fn with proper tower adapter
                    let hyper_service = hyper::service::service_fn(
                        |request: hyper::Request<hyper::body::Incoming>| {
                            // Clone for this request
                            let mut tower_service_clone = tower_service.clone();

                            async move {
                                match tower_service_clone.call(request).await {
                                    Ok(response) => Ok::<_, hyper::Error>(response),
                                    Err(err) => {
                                        tracing::error!("Tower service error: {}", err);
                                        // Return 500 Internal Server Error
                                        let body = axum::body::Body::from("Internal Server Error");
                                        Ok(hyper::Response::builder()
                                            .status(500)
                                            .body(body)
                                            .expect("Failed to build error response"))
                                    }
                                }
                            }
                        },
                    );

                    // Create hyper server builder for this connection
                    let builder = Builder::new(TokioExecutor::new());

                    if let Err(err) = builder.serve_connection(io, hyper_service).await {
                        tracing::error!("Connection error: {}", err);
                    }
                });
            }
            Err(e) => {
                tracing::error!("Fatal accept error, breaking out of server loop: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Inference endpoint handler
async fn inference_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    Json(request): Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    // Emit start signal
    let _ = state.signals_tx.send(Signal {
        signal_type: "inference_start".to_string(),
        timestamp: current_millis(),
        payload: serde_json::json!({ "cpid": request.cpid, "max_tokens": request.max_tokens }),
        priority: "normal".to_string(),
        trace_id: None,
    });

    // Forward request to worker; worker policy remains source of truth
    let mut worker = state.worker.lock().await;
    let response = worker
        .infer(request)
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    // Emit completion signal
    let _ = state.signals_tx.send(Signal {
        signal_type: "complete".to_string(),
        timestamp: current_millis(),
        payload: serde_json::to_value(&response).unwrap_or(serde_json::json!({})),
        priority: "normal".to_string(),
        trace_id: None,
    });

    Ok(Json(response))
}

/// Health check endpoint
async fn health_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
) -> impl IntoResponse {
    let worker = state.worker.lock().await;
    let evidence_required = worker.policy_requires_open_book();
    let abstain_threshold = worker.policy_abstain_threshold();
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "evidence_required": evidence_required,
        "abstain_threshold": abstain_threshold,
    }))
}

/// Adapter command endpoint
async fn adapter_command_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
    Json(command): Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    admin_guard(&headers)?;
    // Forward command to worker
    let mut worker = state.worker.lock().await;
    let result = worker
        .execute_adapter_command(command)
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    Ok(Json(result))
}

/// List adapters (CLI-compatible view)
async fn list_adapters_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let items = worker.list_adapter_states_view();
    Ok((StatusCode::OK, Json(items)))
}

/// Get adapter profile (CLI-compatible view)
async fn adapter_profile_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
    axum::extract::Path(adapter_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    if let Some(profile) = worker.adapter_profile_view(&adapter_id) {
        Ok((StatusCode::OK, Json(profile)))
    } else {
        Err(ApiError::NotFound(format!(
            "Adapter not found: {}",
            adapter_id
        )))
    }
}

/// Adapter lifecycle operations (promote/demote/pin/unpin)
async fn adapter_lifecycle_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
    axum::extract::Path((adapter_id, cmd)): axum::extract::Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let res = match cmd.as_str() {
        "promote" => worker.promote_adapter_by_id(&adapter_id),
        "demote" => worker.demote_adapter_by_id(&adapter_id),
        "pin" => worker.pin_adapter_by_id(&adapter_id),
        "unpin" => worker.unpin_adapter_by_id(&adapter_id),
        // Functional stand-in: mark adapter as loaded/unloaded via lifecycle
        "load" => worker.promote_adapter_by_id(&adapter_id),
        "unload" => worker.demote_adapter_by_id(&adapter_id),
        other => Err(adapteros_core::AosError::Validation(format!(
            "Unknown adapter command: {}",
            other
        ))),
    };

    match res {
        Ok(()) => Ok((StatusCode::OK, Json(serde_json::json!({ "ok": true })))),
        Err(e) => Err(ApiError::WorkerError(e.to_string())),
    }
}

/// Profiling snapshot (CLI-compatible)
async fn profile_snapshot_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let worker = state.worker.lock().await;
    let snapshot = worker.profiling_snapshot_json();
    Ok((StatusCode::OK, Json(snapshot)))
}

/// Execute a warmup routine on the worker
async fn warmup_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    admin_guard(&headers)?;
    let mut worker = state.worker.lock().await;
    let report = worker
        .warmup()
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;
    Ok((StatusCode::OK, Json(report)))
}

/// Stream signals as Server-Sent Events
async fn signals_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, ApiError> {
    admin_guard(&headers)?;
    let rx = state.signals_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(signal) => {
            let json = serde_json::to_string(&signal).unwrap_or("{}".to_string());
            Some(Ok(Event::default().event("signal").data(json)))
        }
        Err(_) => None,
    });
    Ok(Sse::new(stream))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Signal {
    #[serde(rename = "type")]
    signal_type: String,
    timestamp: u128,
    payload: serde_json::Value,
    priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
}

fn current_millis() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// -----------------------------
// Admin gating and auth helpers
// -----------------------------

fn admin_enabled() -> bool {
    std::env::var("AOS_API_ENABLE_ADMIN")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn admin_guard(headers: &HeaderMap) -> Result<(), ApiError> {
    if !admin_enabled() {
        return Err(ApiError::Unauthorized);
    }

    if let Ok(token) = std::env::var("AOS_API_ADMIN_TOKEN") {
        // If token configured, require header X-Admin-Token to match
        if let Some(hv) = headers.get("X-Admin-Token") {
            if hv.to_str().unwrap_or("") == token {
                return Ok(());
            }
        }
        return Err(ApiError::Unauthorized);
    }

    Ok(())
}

/// API error response (matches aos-cp-api pattern)
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// API error type
#[derive(Debug)]
pub enum ApiError {
    Internal(String),
    BadRequest(String),
    NotFound(String),
    PolicyViolation(String),
    WorkerError(String),
    Unauthorized,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details) = match self {
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
                Some(msg),
            ),
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "bad request".to_string(),
                Some(msg),
            ),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not found".to_string(), Some(msg)),
            ApiError::PolicyViolation(msg) => (
                StatusCode::FORBIDDEN,
                "policy violation".to_string(),
                Some(msg),
            ),
            ApiError::WorkerError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "worker error".to_string(),
                Some(msg),
            ),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string(), None),
        };

        (status, Json(ErrorResponse { error, details })).into_response()
    }
}

// Conversion from AosError
impl From<AosError> for ApiError {
    fn from(err: AosError) -> Self {
        match err {
            AosError::PolicyViolation(msg) => ApiError::PolicyViolation(msg),
            AosError::Worker(msg) => ApiError::WorkerError(msg),
            AosError::InvalidCPID(msg) | AosError::InvalidHash(msg) => ApiError::BadRequest(msg),
            _ => ApiError::Internal(err.to_string()),
        }
    }
}
