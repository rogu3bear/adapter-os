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

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use adapteros_core::AosError;
use adapteros_lora_worker::{InferenceRequest, InferenceResponse};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::net::UnixListener;
use tower::{Service, ServiceBuilder};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub mod types;

use adapteros_lora_kernel_api::FusedKernels;

/// API server state
pub struct ApiState<K: FusedKernels> {
    worker: Arc<tokio::sync::Mutex<adapteros_lora_worker::Worker<K>>>,
}

impl<K: FusedKernels> ApiState<K> {
    /// Create new API state with worker
    pub fn new(worker: adapteros_lora_worker::Worker<K>) -> Self {
        Self {
            worker: Arc::new(tokio::sync::Mutex::new(worker)),
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

    // Create API state with worker
    let state = Arc::new(ApiState::new(worker));

    // Create router with middleware
    let app = Router::new()
        .route("/inference", post(inference_handler))
        .route("/health", post(health_handler))
        .route("/adapter", post(adapter_command_handler))
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
                tokio::spawn(async move {
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
    // Forward request to worker
    let mut worker = state.worker.lock().await;
    let response = worker
        .infer(request)
        .await
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    Ok(Json(response))
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Adapter command endpoint
async fn adapter_command_handler<K: FusedKernels>(
    State(state): State<Arc<ApiState<K>>>,
    Json(command): Json<adapteros_lora_worker::AdapterCommand>,
) -> Result<Json<adapteros_lora_worker::AdapterCommandResult>, ApiError> {
    // Forward command to worker
    let mut worker = state.worker.lock().await;
    let result = worker
        .execute_adapter_command(command)
        .map_err(|e| ApiError::WorkerError(e.to_string()))?;

    Ok(Json(result))
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

// Stub chrono for timestamp
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> Self {
            Self
        }
        pub fn to_rfc3339(&self) -> String {
            "2025-01-01T00:00:00Z".to_string()
        }
    }
}
