//! Error types for the API layer.

use adapteros_api_types::{ErrorResponse, FailureCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Standard error envelope returned by the API for all 4xx/5xx responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiErrorBody {
    /// Machine-readable error code (e.g., "ADAPTER_NOT_FOUND")
    pub code: String,
    /// Human-readable message suitable for UI display
    pub message: String,
    /// Actionable hint for common failures
    pub hint: String,
    /// Optional developer-facing detail (stack trace, context, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Correlation ID that matches the `x-request-id` header and server logs
    pub request_id: String,
}

/// Structured UMA backpressure error payload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UmaBackpressureError {
    /// UMA pressure level (Low, Medium, High, Critical)
    pub level: String,
    /// Suggested retry interval in seconds
    pub retry_after_secs: u32,
    /// Suggested client action
    pub action: String,
}

impl UmaBackpressureError {
    pub fn new(level: impl Into<String>) -> Self {
        Self {
            level: level.into(),
            retry_after_secs: 30,
            action: "reduce max_tokens or retry later".to_string(),
        }
    }
}

impl From<UmaBackpressureError> for ErrorResponse {
    fn from(err: UmaBackpressureError) -> Self {
        ErrorResponse::new("service under memory pressure")
            .with_code("BACKPRESSURE")
            .with_details(serde_json::json!({
                "level": err.level,
                "retry_after_secs": err.retry_after_secs,
                "action": err.action,
            }))
    }
}

/// Backpressure response for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BackpressureResponse {
    /// Memory pressure level (e.g., "high", "critical")
    pub level: String,
    /// Suggested retry delay in seconds
    pub retry_after_secs: u64,
    /// Suggested action to take
    pub suggested_action: String,
}

/// Structured error details from worker responses
///
/// This enum mirrors `adapteros_lora_worker::InferenceErrorDetails` for
/// deserialization from worker UDS responses and API transport.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum WorkerErrorDetails {
    /// Model cache budget exceeded during eviction
    #[serde(rename = "cache_budget_exceeded")]
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Generic worker error (fallback for unstructured errors)
    #[serde(rename = "worker_error")]
    WorkerError {
        /// Error message
        message: String,
    },
}

/// Error type for inference operations
#[derive(Debug, Clone)]
pub enum InferenceError {
    /// Prompt validation failed
    ValidationError(String),
    /// Worker not available
    WorkerNotAvailable(String),
    /// Worker communication failed
    WorkerError(String),
    /// Request timeout
    Timeout(String),
    /// Request cancelled due to client disconnect
    ClientClosed(String),
    /// RAG retrieval failed
    RagError(String),
    /// Permission denied
    PermissionDenied(String),
    /// Memory pressure too high
    BackpressureError(String),
    /// Routing was bypassed (should never happen)
    RoutingBypass(String),
    /// Base model not ready for routing
    ModelNotReady(String),
    /// No compatible worker available for the required manifest
    NoCompatibleWorker {
        required_hash: String,
        tenant_id: String,
        available_count: usize,
        /// Specific reason why no compatible workers were found
        reason: String,
    },
    /// Worker discovery failed but system is in degraded mode (dev mode only)
    ///
    /// This error indicates that no compatible worker was found after retries,
    /// but the system is in dev mode and can operate in a degraded state.
    WorkerDegraded {
        tenant_id: String,
        /// Reason for degradation
        reason: String,
    },
    /// Adapter not found or not loadable (archived/purged)
    AdapterNotFound(String),
    /// Worker ID unavailable for token generation
    ///
    /// When worker authentication is enabled (signing keypair present), we require
    /// a valid worker_id to generate tokens. This error occurs when worker selection
    /// fails to provide a worker_id.
    WorkerIdUnavailable {
        /// Tenant ID for the request
        tenant_id: String,
        /// Reason worker ID is unavailable
        reason: String,
    },
    /// Model cache budget exceeded in worker
    ///
    /// This error occurs when the worker's model cache cannot free enough
    /// memory to accommodate a new model load.
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Policy violation blocked inference
    PolicyViolation {
        /// Tenant ID for the request
        tenant_id: String,
        /// ID of the policy that was violated
        policy_id: String,
        /// Reason for the violation
        reason: String,
    },
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            Self::WorkerNotAvailable(msg) => write!(f, "Worker not available: {}", msg),
            Self::WorkerError(msg) => write!(f, "Worker error: {}", msg),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::ClientClosed(msg) => write!(f, "Client closed request: {}", msg),
            Self::RagError(msg) => write!(f, "RAG error: {}", msg),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            Self::BackpressureError(msg) => write!(f, "Backpressure: {}", msg),
            Self::RoutingBypass(msg) => write!(f, "Routing bypass: {}", msg),
            Self::ModelNotReady(msg) => write!(f, "Model not ready: {}", msg),
            Self::NoCompatibleWorker {
                required_hash,
                tenant_id,
                available_count,
                reason,
            } => write!(
                f,
                "No compatible worker for tenant {} with manifest {} ({} workers available). Reason: {}",
                tenant_id, required_hash, available_count, reason
            ),
            Self::WorkerDegraded { tenant_id, reason } => write!(
                f,
                "Worker degraded for tenant {}: {}",
                tenant_id, reason
            ),
            Self::AdapterNotFound(msg) => write!(f, "Adapter not found: {}", msg),
            Self::WorkerIdUnavailable { tenant_id, reason } => write!(
                f,
                "Worker ID unavailable for tenant {}: {}",
                tenant_id, reason
            ),
            Self::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                ..
            } => write!(
                f,
                "Model cache budget exceeded: needed {} MB, freed {} MB (pinned={}, active={}), max {} MB",
                needed_mb, freed_mb, pinned_count, active_count, max_mb
            ),
            Self::PolicyViolation {
                tenant_id,
                policy_id,
                reason,
            } => write!(
                f,
                "Policy violation for tenant {} (policy: {}): {}",
                tenant_id, policy_id, reason
            ),
        }
    }
}

impl std::error::Error for InferenceError {}

impl InferenceError {
    /// Convert to HTTP status code
    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::WorkerNotAvailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::WorkerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Timeout(_) => StatusCode::REQUEST_TIMEOUT,
            Self::ClientClosed(_) => StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
            Self::RagError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::PermissionDenied(_) => StatusCode::FORBIDDEN,
            Self::BackpressureError(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::RoutingBypass(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ModelNotReady(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::NoCompatibleWorker { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::WorkerDegraded { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::AdapterNotFound(_) => StatusCode::NOT_FOUND,
            Self::WorkerIdUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::CacheBudgetExceeded { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::PolicyViolation { .. } => StatusCode::FORBIDDEN,
        }
    }

    /// Convert to error code string
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::ValidationError(_) => "VALIDATION_ERROR",
            Self::WorkerNotAvailable(_) => "SERVICE_UNAVAILABLE",
            Self::WorkerError(_) => "INTERNAL_ERROR",
            Self::Timeout(_) => "REQUEST_TIMEOUT",
            Self::ClientClosed(_) => "CLIENT_CLOSED_REQUEST",
            Self::RagError(_) => "RAG_ERROR",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::BackpressureError(_) => "BACKPRESSURE",
            Self::RoutingBypass(_) => "ROUTING_BYPASS",
            Self::ModelNotReady(_) => "MODEL_NOT_READY",
            Self::NoCompatibleWorker { .. } => "NO_COMPATIBLE_WORKER",
            Self::WorkerDegraded { .. } => "WORKER_DEGRADED",
            Self::AdapterNotFound(_) => "ADAPTER_NOT_FOUND",
            Self::WorkerIdUnavailable { .. } => "WORKER_ID_UNAVAILABLE",
            Self::CacheBudgetExceeded { .. } => "CACHE_BUDGET_EXCEEDED",
            Self::PolicyViolation { .. } => "POLICY_VIOLATION",
        }
    }

    /// Map to structured failure codes for observability.
    pub fn failure_code(&self) -> Option<FailureCode> {
        match self {
            Self::PermissionDenied(_) => Some(FailureCode::TenantAccessDenied),
            Self::BackpressureError(_) => Some(FailureCode::OutOfMemory),
            Self::RoutingBypass(_) | Self::ModelNotReady(_) => Some(FailureCode::PolicyDivergence),
            Self::WorkerError(msg) | Self::WorkerNotAvailable(msg) => {
                let lower = msg.to_lowercase();
                if lower.contains("out of memory") || lower.contains("oom") {
                    Some(FailureCode::OutOfMemory)
                } else if lower.contains("load") || lower.contains("model") {
                    Some(FailureCode::ModelLoadFailed)
                } else if lower.contains("fallback") {
                    Some(FailureCode::BackendFallback)
                } else {
                    None
                }
            }
            Self::Timeout(_) => None,
            Self::ValidationError(_) => None,
            Self::ClientClosed(_) => None,
            Self::RagError(msg) => {
                if msg.to_lowercase().contains("trace") {
                    Some(FailureCode::TraceWriteFailed)
                } else {
                    None
                }
            }
            Self::NoCompatibleWorker { .. } => Some(FailureCode::BackendFallback),
            Self::WorkerDegraded { .. } => Some(FailureCode::BackendFallback),
            Self::AdapterNotFound(_) => None,
            Self::WorkerIdUnavailable { .. } => Some(FailureCode::BackendFallback),
            Self::CacheBudgetExceeded { .. } => Some(FailureCode::OutOfMemory),
            Self::PolicyViolation { .. } => Some(FailureCode::PolicyDivergence),
        }
    }
}

/// Convert InferenceError to ErrorResponse for API compatibility
impl From<InferenceError> for (axum::http::StatusCode, axum::Json<ErrorResponse>) {
    fn from(err: InferenceError) -> Self {
        let status = err.status_code();
        let code = err.error_code();
        let message = err.to_string();
        let failure_code = err.failure_code();
        let mut response = ErrorResponse::new(&message).with_code(code);
        if let Some(fc) = failure_code {
            response = response.with_failure_code(fc);
        }
        (status, axum::Json(response))
    }
}
