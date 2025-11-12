use axum::{
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use lazy_static::lazy_static;
use prometheus::{register_counter_vec, CounterVec};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

// Add mod declarations
mod logger;
mod middleware;
mod ratelimit;

// Minimal types for compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub completion: String,
}

pub struct ApiState {
    pub worker: Arc<TokioMutex<MockWorker>>,
    pub signals_tx: mpsc::Sender<Signal>,
}

struct MockWorker;

impl MockWorker {
    async fn infer(&self, _req: InferenceRequest) -> Result<InferenceResponse, String> {
        Ok(InferenceResponse {
            completion: "Mock response".to_string(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: String,
    pub timestamp: u128,
    pub payload: serde_json::Value,
    pub priority: String,
    pub trace_id: Option<String>,
}

// Assume health_handler
async fn health_handler() -> &'static str {
    "OK"
}

// Rest of the code remains the same...
// ApiError (Rectified - All Variants with trace_id)
#[derive(Debug)]
pub enum ApiError {
    Internal(Option<String>, String),
    BadRequest(Option<String>, String),
    NotFound(Option<String>, String),
    PolicyViolation(Option<String>, String),
    WorkerError(Option<String>, String),
    Unauthorized(Option<String>),
    EgressViolation(Option<String>, String),
    DeterminismViolation(Option<String>, String),
    IsolationViolation(Option<String>, String),
    ValidationFailed(Option<String>, Vec<String>),
    DatabaseError(Option<String>, String),
    CryptoError(Option<String>, String),
    NetworkError(Option<String>, String),
    RateLimitExceeded(Option<String>, String),
}

// From<AosError> (Full Mapping)
impl From<adapteros_core::AosError> for ApiError {
    fn from(err: adapteros_core::AosError) -> Self {
        let trace_id = Some(Uuid::new_v4().to_string());
        match err {
            adapteros_core::AosError::PolicyViolation(msg) => {
                ApiError::PolicyViolation(trace_id, msg)
            }
            adapteros_core::AosError::Worker(msg) => ApiError::WorkerError(trace_id, msg),
            adapteros_core::AosError::InvalidCPID(msg) => ApiError::BadRequest(trace_id, msg),
            adapteros_core::AosError::EgressViolation(msg) => {
                ApiError::EgressViolation(trace_id, msg)
            }
            adapteros_core::AosError::DeterminismViolation(msg) => {
                ApiError::DeterminismViolation(trace_id, msg)
            }
            adapteros_core::AosError::IsolationViolation(msg) => {
                ApiError::IsolationViolation(trace_id, msg)
            }
            adapteros_core::AosError::Validation(msg) => {
                ApiError::ValidationFailed(trace_id, vec![msg])
            }
            adapteros_core::AosError::Database(msg) => ApiError::DatabaseError(trace_id, msg),
            adapteros_core::AosError::Crypto(msg) => ApiError::CryptoError(trace_id, msg),
            adapteros_core::AosError::Network(msg) => ApiError::NetworkError(trace_id, msg),
            _ => ApiError::Internal(trace_id, err.to_string()),
        }
    }
}

// From<tower_http::limit::Error>
impl From<tower_http::limit::Error> for ApiError {
    fn from(_: tower_http::limit::Error) -> Self {
        ApiError::RateLimitExceeded(
            Some(Uuid::new_v4().to_string()),
            "Rate limit exceeded".to_string(),
        )
    }
}

// ErrorResponse (with code, trace_id)
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

// IntoResponse (Full with Status, Code, Trace ID, Retry-After for RateLimit)
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, details, trace_id, code) = match self {
            ApiError::Internal(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
                Some(msg),
                trace,
                "INTERNAL_ERROR".to_string(),
            ),
            ApiError::BadRequest(trace, msg) => (
                StatusCode::BAD_REQUEST,
                "bad request".to_string(),
                Some(msg),
                trace,
                "BAD_REQUEST".to_string(),
            ),
            ApiError::NotFound(trace, msg) => (
                StatusCode::NOT_FOUND,
                "not found".to_string(),
                Some(msg),
                trace,
                "NOT_FOUND".to_string(),
            ),
            ApiError::PolicyViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "policy violation".to_string(),
                Some(msg),
                trace,
                "POLICY_VIOLATION".to_string(),
            ),
            ApiError::WorkerError(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "worker error".to_string(),
                Some(msg),
                trace,
                "WORKER_ERROR".to_string(),
            ),
            ApiError::Unauthorized(trace) => (
                StatusCode::UNAUTHORIZED,
                "unauthorized".to_string(),
                None,
                trace,
                "UNAUTHORIZED".to_string(),
            ),
            ApiError::EgressViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "egress violation".to_string(),
                Some(msg),
                trace,
                "EGRESS_VIOLATION".to_string(),
            ),
            ApiError::DeterminismViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "determinism violation".to_string(),
                Some(msg),
                trace,
                "DETERMINISM_VIOLATION".to_string(),
            ),
            ApiError::IsolationViolation(trace, msg) => (
                StatusCode::FORBIDDEN,
                "isolation violation".to_string(),
                Some(msg),
                trace,
                "ISOLATION_VIOLATION".to_string(),
            ),
            ApiError::ValidationFailed(trace, fields) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation failed".to_string(),
                Some(format!("Validation errors: {:?}", fields)),
                trace,
                "VALIDATION_FAILED".to_string(),
            ),
            ApiError::DatabaseError(trace, msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "database error".to_string(),
                Some(msg),
                trace,
                "DATABASE_ERROR".to_string(),
            ),
            ApiError::CryptoError(trace, msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "crypto error".to_string(),
                Some(msg),
                trace,
                "CRYPTO_ERROR".to_string(),
            ),
            ApiError::NetworkError(trace, msg) => (
                StatusCode::BAD_GATEWAY,
                "network error".to_string(),
                Some(msg),
                trace,
                "NETWORK_ERROR".to_string(),
            ),
            ApiError::RateLimitExceeded(trace, _) => {
                let mut resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("Retry-After", "60")],
                    Json(ErrorResponse {
                        error: "rate limit exceeded".to_string(),
                        details: Some(
                            "Retry after 60 seconds (exponential backoff: next 120 seconds)"
                                .to_string(),
                        ),
                        trace_id: trace.clone(),
                        code: "RATE_LIMIT_EXCEEDED".to_string(),
                    }),
                )
                    .into_response();
                if let Some(id) = trace {
                    resp.headers_mut().insert("X-Error-ID", id.parse().unwrap());
                }
                return resp;
            }
        };

        let mut resp = (
            status,
            Json(ErrorResponse {
                error,
                details,
                trace_id,
                code,
            }),
        )
            .into_response();

        // Client hints for errors
        if status.is_client_error() || status.is_server_error() {
            if let Some(id) = trace_id {
                resp.headers_mut().insert("X-Error-ID", id.parse().unwrap());
            }
        }

        resp
    }
}

// Prometheus Counter
lazy_static! {
    static ref ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "api_errors_total",
        "Total number of API errors by type",
        &["variant"]
    )
    .unwrap();
}

fn increment_error_counter(variant: &str) {
    ERRORS_TOTAL.with_label_values(&[variant]).inc();
}

// Example Handler (inference_handler with Validation)
async fn inference_handler(
    State(state): State<Arc<ApiState>>,
    Json(mut request): Json<InferenceRequest>,
) -> Result<Json<InferenceResponse>, ApiError> {
    let trace_id = Some(Uuid::new_v4().to_string());

    // Validation
    if request.max_tokens < 1 || request.max_tokens > 4096 {
        increment_error_counter("VALIDATION_FAILED");
        return Err(ApiError::ValidationFailed(
            trace_id,
            vec!["max_tokens out of range (1-4096)".to_string()],
        ));
    }

    // Worker logic (mocked for example)
    let response = state
        .worker
        .lock()
        .await
        .infer(request)
        .await
        .map_err(|e| {
            increment_error_counter("WORKER_ERROR");
            ApiError::WorkerError(trace_id.clone(), e.to_string())
        })?;

    // Emit signal with trace_id
    let _ = state.signals_tx.send(Signal {
        signal_type: "inference_complete".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        payload: serde_json::to_value(&response).unwrap_or_default(),
        priority: "normal".to_string(),
        trace_id,
    });

    Ok(Json(response))
}

// Router (Full Stack with Middleware)
pub fn build_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/inference", post(inference_handler))
        .route("/health", get(health_handler)) // Assume health_handler defined
        .route(
            "/metrics",
            get(|| async {
                let encoder = prometheus::TextEncoder::new();
                let mut buffer = vec![];
                encoder.encode(&prometheus::gather(), &mut buffer).unwrap();
                ([("Content-Type", encoder.format_type())], buffer).into_response()
            }),
        )
        .layer(
            ServiceBuilder::new()
                .layer(middleware::panic_recovery_layer()) // Assume defined in middleware.rs
                .layer(middleware::extractor_error_layer()) // Assume defined
                .layer(middleware::error_catcher_layer()) // Assume defined
                .layer(ratelimit::rate_limit_layer(
                    ratelimit::RateLimitConfig::default(),
                )) // Assume defined in ratelimit.rs
                .layer(logger::error_logger_layer()) // Assume defined in logger.rs
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state)
}
