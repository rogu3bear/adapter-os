//! API error types
//!
//! Unified error handling for API requests.

use adapteros_api_types::{ErrorResponse, FailureCode};
use thiserror::Error;

/// Result type for API operations
pub type ApiResult<T> = Result<T, ApiError>;

/// API error types
#[derive(Debug, Error, Clone)]
pub enum ApiError {
    /// Request was aborted (user cancelled)
    #[error("Request aborted")]
    Aborted,

    /// Network error (connection failed, timeout, etc.)
    #[error("Network error: {0}")]
    Network(String),

    /// HTTP error with status code
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    /// Authentication error (401)
    #[error("Authentication required")]
    Unauthorized,

    /// Authorization error (403)
    #[error("Access denied: {0}")]
    Forbidden(String),

    /// Not found error (404)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Validation error (400/422)
    #[error("Validation error: {0}")]
    Validation(String),

    /// Server error (5xx)
    #[error("Server error: {0}")]
    Server(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Rate limited (429)
    #[error("Rate limited, retry after {retry_after:?}ms")]
    RateLimited { retry_after: Option<u64> },

    /// Structured error response from server
    #[error("{error}")]
    Structured {
        error: String,
        code: String,
        failure_code: Option<FailureCode>,
        hint: Option<String>,
        details: Option<serde_json::Value>,
        request_id: Option<String>,
        error_id: Option<String>,
        fingerprint: Option<String>,
        session_id: Option<String>,
        diag_trace_id: Option<String>,
        otel_trace_id: Option<String>,
    },
}

impl ApiError {
    /// Create from HTTP status and body
    pub fn from_response(status: u16, body: &str, request_id_header: Option<String>) -> Self {
        // Try to parse as ErrorResponse
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(body) {
            return Self::Structured {
                error: err.message,
                code: err.code.clone(),
                failure_code: err
                    .failure_code
                    .or_else(|| FailureCode::parse_code(&err.code)),
                hint: err.hint,
                details: err.details,
                request_id: err.request_id.or(request_id_header),
                error_id: err.error_id,
                fingerprint: err.fingerprint,
                session_id: err.session_id,
                diag_trace_id: err.diag_trace_id,
                otel_trace_id: err.otel_trace_id,
            };
        }

        match status {
            401 => Self::Unauthorized,
            403 => Self::Forbidden(body.to_string()),
            404 => Self::NotFound(body.to_string()),
            400 | 422 => Self::Validation(body.to_string()),
            429 => Self::RateLimited { retry_after: None },
            500..=599 => Self::Server(body.to_string()),
            _ => Self::Http {
                status,
                message: body.to_string(),
            },
        }
    }

    /// Preferred debug identifier for user support: error_id (err-...) then request_id (req-...).
    pub fn debug_id(&self) -> Option<&str> {
        match self {
            Self::Structured {
                error_id: Some(id), ..
            } => Some(id.as_str()),
            Self::Structured {
                request_id: Some(id),
                ..
            } => Some(id.as_str()),
            _ => None,
        }
    }

    /// Check if this error indicates the user should re-authenticate
    pub fn requires_auth(&self) -> bool {
        match self {
            Self::Unauthorized => true,
            Self::Structured { code, .. } => matches!(
                code.as_str(),
                "UNAUTHORIZED"
                    | "TOKEN_EXPIRED"
                    | "TOKEN_REVOKED"
                    | "INVALID_TOKEN"
                    | "MISSING_AUTH"
                    | "AUTHENTICATION_ERROR"
            ),
            _ => false,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } | Self::Server(_) => true,
            Self::Structured {
                failure_code: Some(code),
                ..
            } => code.is_retryable(),
            _ => false,
        }
    }

    /// Check if this error indicates the request was aborted
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }

    /// Check if this error indicates a 404/not-found condition
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::NotFound(_) => true,
            Self::Http { status, .. } if *status == 404 => true,
            Self::Structured { code, .. } => matches!(
                code.as_str(),
                "NOT_FOUND"
                    | "ENDPOINT_NOT_FOUND"
                    | "MODEL_NOT_FOUND"
                    | "ADAPTER_NOT_FOUND"
                    | "RESOURCE_NOT_FOUND"
            ),
            _ => false,
        }
    }

    /// Get the error code if available
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Structured { code, .. } => Some(code),
            Self::Unauthorized => Some("UNAUTHORIZED"),
            Self::Forbidden(_) => Some("FORBIDDEN"),
            Self::NotFound(_) => Some("NOT_FOUND"),
            Self::Validation(_) => Some("VALIDATION_ERROR"),
            Self::RateLimited { .. } => Some("RATE_LIMITED"),
            Self::Server(_) => Some("SERVER_ERROR"),
            _ => None,
        }
    }

    /// Get the structured failure code if available
    pub fn failure_code(&self) -> Option<FailureCode> {
        match self {
            Self::Structured { failure_code, .. } => *failure_code,
            _ => None,
        }
    }

    /// Get any server-provided hint for the error.
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Structured { hint, .. } => hint.as_deref(),
            _ => None,
        }
    }

    /// Check if this error has a specific failure code
    pub fn has_failure_code(&self, code: FailureCode) -> bool {
        self.failure_code() == Some(code)
    }

    /// Check if this is an in-flight adapter error (HTTP 409 with ADAPTER_IN_FLIGHT code)
    pub fn is_adapter_in_flight(&self) -> bool {
        self.code() == Some("ADAPTER_IN_FLIGHT")
    }

    /// Get user-friendly message for display.
    ///
    /// Returns a context-appropriate message for specific error codes,
    /// falling back to the standard error message for others. This method is the
    /// single source of truth for user-facing API error copy.
    pub fn user_message(&self) -> String {
        if self.is_adapter_in_flight() {
            "This adapter is currently in use for inference. Please wait for active requests to complete before making changes.".to_string()
        } else {
            match self {
                Self::Unauthorized => "Session expired. Log in again.".to_string(),
                Self::Forbidden(_) => {
                    "You don't have access to this action. Contact an admin if you need access."
                        .to_string()
                }
                Self::NotFound(_) => "Not found. Check the URL or try again.".to_string(),
                Self::Validation(_) => {
                    "Some fields are invalid. Fix the highlighted fields and retry.".to_string()
                }
                Self::Network(_) => {
                    "Can't reach the server. Check your connection and retry.".to_string()
                }
                Self::RateLimited { retry_after } => retry_after
                    .map(|ms| {
                        let secs = (ms / 1000).max(1);
                        format!("Too many requests. Retry in {}s.", secs)
                    })
                    .unwrap_or_else(|| "Too many requests. Retry in a moment.".to_string()),
                Self::Server(_) => "Server error. Retry in a moment.".to_string(),
                Self::Http { status, .. } => match status {
                    401 => "Session expired. Log in again.".to_string(),
                    403 => {
                        "You don't have access to this action. Contact an admin if you need access."
                            .to_string()
                    }
                    404 => "Not found. Check the URL or try again.".to_string(),
                    502 => "Upstream service unavailable. Retry in a moment.".to_string(),
                    503 => "Service temporarily unavailable. Retry in a moment.".to_string(),
                    504 => "Request timed out. Retry in a moment.".to_string(),
                    _ => self.to_string(),
                },
                Self::Structured {
                    error,
                    code,
                    failure_code,
                    hint,
                    details,
                    ..
                } => {
                    let base = user_message_for_code(code, *failure_code, error, details.as_ref());
                    apply_hint(base, hint.as_deref())
                }
                _ => self.to_string(),
            }
        }
    }
}

/// Extract the specific violation reason for POLICY_VIOLATION errors.
///
/// The server sends violation details in two places:
/// - `details`: string with "Request violates N policy pack(s): pack: reason; ..."
/// - `error` (message): either generic "policy violation" or specific error text
///
/// We prefer `details` (most specific), then fall back to `error` if it carries
/// more information than the bare "policy violation" string.
fn policy_violation_message(error: &str, details: Option<&serde_json::Value>) -> String {
    const GENERIC: &str = "Blocked by policy. Contact an admin if you need access.";

    // 1. Try the details field — server puts the full violation text there
    if let Some(detail_str) = details.and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return format!("Blocked by policy: {detail_str}");
    }

    // 2. Fall back to the error message if it's more specific than "policy violation"
    let lower = error.to_lowercase();
    if !error.is_empty() && lower != "policy violation" {
        return format!("Blocked by policy: {error}");
    }

    GENERIC.to_string()
}

fn apply_hint(mut message: String, hint: Option<&str>) -> String {
    let Some(hint) = hint.map(str::trim).filter(|h| !h.is_empty()) else {
        return message;
    };
    if !message.ends_with('.') {
        message.push('.');
    }
    message.push_str(" Next: ");
    message.push_str(hint);
    message
}

fn user_message_for_code(
    code: &str,
    failure_code: Option<FailureCode>,
    error: &str,
    details: Option<&serde_json::Value>,
) -> String {
    let message = match code {
        // -- 401 Unauthorized: authentication errors --
        "UNAUTHORIZED" | "TOKEN_EXPIRED" | "TOKEN_REVOKED" | "INVALID_TOKEN" | "MISSING_AUTH" => {
            "Session expired. Log in again.".to_string()
        }
        "TOKEN_MISSING" => "No authentication token provided. Log in to continue.".to_string(),
        "TOKEN_INVALID" => "Authentication token is invalid. Log in again.".to_string(),
        "TOKEN_SIGNATURE_INVALID" => {
            "Token signature verification failed. Log in again.".to_string()
        }
        "INVALID_ISSUER" => {
            "Token issuer mismatch. Check your authentication provider.".to_string()
        }
        "INVALID_AUDIENCE" => {
            "Token audience mismatch. Check your authentication configuration.".to_string()
        }
        "INVALID_API_KEY" => "API key is invalid or not found.".to_string(),
        "SESSION_EXPIRED" => "Your session has expired. Log in again.".to_string(),
        "SESSION_LOCKED" => {
            "Session is locked due to suspicious activity. Contact an admin.".to_string()
        }
        "DEVICE_MISMATCH" => "Device mismatch detected. Log in again from this device.".to_string(),
        "INVALID_CREDENTIALS" | "MISSING_CREDENTIALS" => {
            "Invalid username or password.".to_string()
        }
        "ACCOUNT_DISABLED" => "Account is disabled. Contact an admin.".to_string(),
        "ACCOUNT_LOCKED" => {
            "Account is locked due to too many failed attempts. Try again later.".to_string()
        }
        "USER_NOT_FOUND" => "Account not found. Check your credentials.".to_string(),
        "WEAK_PASSWORD" => "Password is too weak. Use a stronger password.".to_string(),
        "INVALID_EMAIL" => "Invalid email address format.".to_string(),
        "EMAIL_EXISTS" => "An account with this email already exists.".to_string(),
        "REGISTRATION_DISABLED" => "Registration is currently disabled.".to_string(),
        "RATE_LIMIT_EXCEEDED" => "Too many attempts. Try again later.".to_string(),

        // -- 403 Forbidden: authorization and policy errors --
        "POLICY_VIOLATION" => policy_violation_message(error, details),
        "FORBIDDEN" | "PERMISSION_DENIED" | "AUTHORIZATION_ERROR" => {
            "You don't have access to this action. Contact an admin if you need access.".to_string()
        }
        "REPO_ARCHIVED" => "This repository is archived and read-only.".to_string(),
        "SIGNATURE_INVALID" => "The cryptographic signature is invalid.".to_string(),
        "SIGNATURE_REQUIRED" => {
            "A cryptographic signature is required for this operation.".to_string()
        }
        "TENANT_ISOLATION_ERROR" => {
            "Cross-tenant access denied. You can only access your own workspace.".to_string()
        }
        "CSRF_ERROR" => "Security token expired. Refresh the page and try again.".to_string(),
        "INSUFFICIENT_ROLE" => "Your role does not have permission for this action.".to_string(),
        "MFA_REQUIRED" => {
            "Multi-factor authentication required. Complete MFA to continue.".to_string()
        }
        "POLICY_ERROR" => "Policy evaluation failed. Contact an admin.".to_string(),
        "DETERMINISM_VIOLATION" => {
            "Determinism invariant violated. This operation cannot proceed.".to_string()
        }
        "EGRESS_VIOLATION" => "Network egress blocked by security policy.".to_string(),
        "SSRF_BLOCKED" => {
            "Request blocked: target resolves to a private network address.".to_string()
        }
        "ISOLATION_VIOLATION" => "Tenant isolation boundary violated.".to_string(),
        "PERFORMANCE_VIOLATION" => "Performance budget exceeded for this operation.".to_string(),
        "ANOMALY_DETECTED" => {
            "Anomalous behavior detected. Operation paused for review.".to_string()
        }
        "SYSTEM_QUARANTINED" => {
            "System is in quarantine mode. Operations are restricted.".to_string()
        }
        "ADAPTER_TENANT_MISMATCH" => "This adapter belongs to a different workspace.".to_string(),
        "INTEGRITY_VIOLATION" => {
            "Data integrity check failed. The data may be corrupted.".to_string()
        }
        "CHECKPOINT_INTEGRITY_FAILED" => {
            "Checkpoint signature verification failed. The checkpoint may be tampered.".to_string()
        }

        // -- 404 Not Found --
        "NOT_FOUND" | "ENDPOINT_NOT_FOUND" | "MODEL_NOT_FOUND" | "ADAPTER_NOT_FOUND" => {
            "Not found. Check the URL or try again.".to_string()
        }
        "REPO_NOT_FOUND" => "Repository not found.".to_string(),
        "VERSION_NOT_FOUND" => "Version not found.".to_string(),
        "CACHE_ENTRY_NOT_FOUND" => "Cache entry not found. It may have been evicted.".to_string(),

        // -- 400 Bad Request: validation and parse errors --
        "BAD_REQUEST" => "Invalid request. Check your input and try again.".to_string(),
        "VALIDATION_ERROR" => {
            "Some fields are invalid. Fix the highlighted fields and retry.".to_string()
        }
        "SERIALIZATION_ERROR" => "Data format error. Check your request format.".to_string(),
        "PARSE_ERROR" => "Could not parse the request. Check the input format.".to_string(),
        "INVALID_HASH" => "Invalid hash format. Expected a BLAKE3 hex string.".to_string(),
        "INVALID_CPID" => "Invalid checkpoint ID format.".to_string(),
        "INVALID_MANIFEST" => "Adapter manifest is malformed. Check required fields.".to_string(),
        "ADAPTER_NOT_IN_MANIFEST" => "Adapter not found in the manifest.".to_string(),
        "ADAPTER_NOT_IN_EFFECTIVE_SET" => {
            "Adapter is not in the effective adapter set for this request.".to_string()
        }
        "KERNEL_LAYOUT_MISMATCH" => {
            "Kernel layout does not match expected configuration. Rebuild the plan.".to_string()
        }
        "CHAT_TEMPLATE_ERROR" => {
            "Chat template processing failed. Check the template format.".to_string()
        }
        "MISSING_FIELD" => "A required field is missing from the request.".to_string(),
        "INVALID_TENANT_ID" => "Invalid tenant ID format.".to_string(),
        "INVALID_SESSION_ID" => "Invalid session ID format.".to_string(),
        "INVALID_SEALED_DATA" => "Sealed data integrity check failed.".to_string(),
        "FEATURE_DISABLED" => "This feature is currently disabled.".to_string(),
        "PREFLIGHT_FAILED" => {
            "Preflight checks failed. Run 'aosctl doctor' for details.".to_string()
        }
        "INCOMPATIBLE_SCHEMA_VERSION" => {
            "Schema version is incompatible. Update required.".to_string()
        }
        "ADAPTER_BASE_MODEL_MISMATCH" | "BASE_MODEL_MISMATCH" => {
            "Adapter was trained on a different base model than the one loaded.".to_string()
        }
        "DETERMINISM_ERROR" => {
            "Determinism validation failed. Check seed configuration.".to_string()
        }
        "HASH_INTEGRITY_FAILURE" => {
            "Data integrity check failed — the content hash does not match.".to_string()
        }
        "INCOMPATIBLE_BASE_MODEL" => {
            "The base model is incompatible with this adapter.".to_string()
        }
        "UNSUPPORTED_BACKEND" => "This inference backend is not supported.".to_string(),
        "VERSION_NOT_PROMOTABLE" => {
            "This version cannot be promoted in its current state.".to_string()
        }

        // -- 409 Conflict --
        "CONFLICT" => "Conflict detected. Another operation may be in progress.".to_string(),
        "ADAPTER_HASH_MISMATCH" => {
            "Adapter content hash mismatch. Re-upload or verify the adapter.".to_string()
        }
        "ADAPTER_LAYER_HASH_MISMATCH" => {
            "Adapter layer hash mismatch. The adapter file may be corrupted.".to_string()
        }
        "POLICY_HASH_MISMATCH" => {
            "Policy hash mismatch. The policy pack may have been updated.".to_string()
        }
        "PROMOTION_ERROR" => "Adapter promotion failed. Check promotion requirements.".to_string(),
        "MODEL_ACQUISITION_IN_PROGRESS" => {
            "Model download is already in progress. Please wait.".to_string()
        }
        "DUPLICATE_REQUEST" => {
            "Duplicate request detected. Your previous request is being processed.".to_string()
        }
        "REPO_ALREADY_EXISTS" => "A repository with this name already exists.".to_string(),

        // -- 422 Unprocessable --
        "REASONING_LOOP_DETECTED" => {
            "Reasoning loop detected. The model is repeating itself. Try rephrasing.".to_string()
        }

        // -- 429 Too Many Requests --
        "TOO_MANY_REQUESTS" => "Too many requests. Retry in a moment.".to_string(),
        "THUNDERING_HERD_REJECTED" => {
            "Too many concurrent requests. Try again shortly.".to_string()
        }

        // -- 499 Client Closed --
        "CLIENT_CLOSED_REQUEST" => "Request cancelled.".to_string(),

        // -- 500 Internal Server Error --
        "INTERNAL_ERROR" | "INTERNAL_SERVER_ERROR" => {
            "Internal server error. Retry in a moment.".to_string()
        }
        "EXPORT_FAILED" | "EXPORT_ERROR" => {
            "Export operation failed. Check storage and retry.".to_string()
        }
        "DATABASE_ERROR" | "DB_ERROR" => "Database error. Retry in a moment.".to_string(),
        "CRYPTO_ERROR" => "Cryptographic operation failed. Check key configuration.".to_string(),
        "CONFIG_ERROR" => "Server configuration error. Contact an admin.".to_string(),
        "RAG_ERROR" => "Document retrieval failed. Check the knowledge base.".to_string(),
        "ROUTING_BYPASS" => "Internal routing error. Retry in a moment.".to_string(),
        "REPLAY_ERROR" => "Replay operation failed. Check the replay bundle.".to_string(),
        "MIGRATION_FILE_MISSING" => {
            "Database migration file missing. Contact your administrator.".to_string()
        }
        "MIGRATION_CHECKSUM_MISMATCH" => "Database migration integrity check failed.".to_string(),
        "SCHEMA_VERSION_MISMATCH" => {
            "Database schema version mismatch. A migration may be needed.".to_string()
        }
        "RATE_LIMITER_NOT_CONFIGURED" => "Rate limiting is not configured.".to_string(),

        // -- 502 Bad Gateway --
        "BASE_LLM_ERROR" => "Base model error. Retry in a moment or check worker logs.".to_string(),
        "UDS_CONNECTION_FAILED" => {
            "Worker connection failed. Check if the worker is running.".to_string()
        }
        "INVALID_RESPONSE" => "Invalid response from worker. Retry in a moment.".to_string(),
        "DOWNLOAD_FAILED" => "File download failed. Check network connectivity.".to_string(),

        // -- 503 Service Unavailable --
        "WORKER_NOT_RESPONDING" | "NO_COMPATIBLE_WORKER" | "WORKER_DEGRADED" => {
            "Worker unavailable. Retry in a moment or check worker health.".to_string()
        }
        "MODEL_NOT_READY" | "ADAPTER_NOT_LOADED" | "ADAPTER_NOT_LOADABLE" => {
            "Model not ready. Retry in a moment or check model loading status.".to_string()
        }
        "CACHE_BUDGET_EXCEEDED" => {
            "Model cache is full. Free resources or retry later.".to_string()
        }
        "BACKPRESSURE" | "MEMORY_PRESSURE" | "OUT_OF_MEMORY" => {
            "System is under memory pressure. Reduce request size or retry later.".to_string()
        }
        "GPU_UNAVAILABLE" => {
            "GPU unavailable. Retry in a moment or check worker health.".to_string()
        }
        "SERVICE_UNAVAILABLE" | "BAD_GATEWAY" | "NETWORK_ERROR" | "CIRCUIT_BREAKER_OPEN" => {
            "Service temporarily unavailable. Retry in a moment.".to_string()
        }
        "CIRCUIT_BREAKER_HALF_OPEN" => "Service is recovering. Retry in a moment.".to_string(),
        "CACHE_EVICTION" => "Cache eviction in progress. Retry in a moment.".to_string(),
        "CACHE_STALE" => "Cached data is stale. Refreshing — retry shortly.".to_string(),
        "EVENT_GAP_DETECTED" => {
            "Event stream gap detected. Some updates may have been missed.".to_string()
        }
        "STREAM_DISCONNECTED" => "Real-time connection lost. Reconnecting.".to_string(),
        "HEALTH_CHECK_FAILED" => "Health check failed. The system may be starting up.".to_string(),
        "CPU_THROTTLED" => "System is CPU-throttled. Reduce load or retry later.".to_string(),
        "DISK_FULL" => "Disk space exhausted. Free storage and retry.".to_string(),
        "FD_EXHAUSTED" => "System file descriptor limit reached. Contact an admin.".to_string(),
        "THREAD_POOL_SATURATED" => "All worker threads are busy. Retry in a moment.".to_string(),
        "TEMP_DIR_UNAVAILABLE" => "Temporary storage unavailable. Contact an admin.".to_string(),
        "WORKER_ID_UNAVAILABLE" => {
            "Worker identification unavailable. Retry in a moment.".to_string()
        }

        // -- 504 Gateway Timeout --
        "REQUEST_TIMEOUT" | "GATEWAY_TIMEOUT" | "TIMEOUT" => {
            "Request timed out. Retry in a moment.".to_string()
        }

        // -- Boot-time errors --
        "DEV_BYPASS_IN_RELEASE" => "Dev bypass is not allowed in release builds.".to_string(),
        "JWT_MODE_NOT_CONFIGURED" => "JWT authentication is not properly configured.".to_string(),
        "API_KEY_MODE_NOT_CONFIGURED" => {
            "API key authentication is not properly configured.".to_string()
        }

        // -- Payload --
        "PAYLOAD_TOO_LARGE" => "Request is too large. Reduce the payload size.".to_string(),

        // -- Dataset / training --
        "DATASET_NOT_FOUND" => {
            if let Some(detail) = details.and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                format!("Dataset not found: {detail}")
            } else {
                "Dataset not found. Check the dataset ID and try again.".to_string()
            }
        }
        "DATASET_VERSION_NOT_FOUND" => {
            "Dataset version not found. The version may have been deleted.".to_string()
        }
        "DATASET_ERROR" => {
            if let Some(detail) = details.and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                format!("Dataset error: {detail}")
            } else {
                error.to_string()
            }
        }
        "TRAINING_ERROR" | "TRAINING_START_FAILED" => {
            if let Some(detail) = details.and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                format!("Training failed: {detail}")
            } else {
                "Training job could not be started. Check the configuration and try again."
                    .to_string()
            }
        }
        "WORKER_CAPABILITY_MISSING" => {
            "No worker with the required capabilities is available.".to_string()
        }
        "INVALID_ADAPTER_TYPE" => "Invalid adapter type. Check the supported types.".to_string(),
        "DATASET_EMPTY" => "Dataset is empty. Upload data before starting training.".to_string(),
        "DATASET_TRUST_BLOCKED" => {
            "Dataset trust check failed. The dataset has been blocked.".to_string()
        }
        "DATASET_TRUST_NEEDS_APPROVAL" => {
            "Dataset requires trust approval before it can be used for training.".to_string()
        }

        // -- Adapter lifecycle --
        "LIFECYCLE_TRANSITION_DENIED" => {
            "Lifecycle transition denied. Check adapter requirements.".to_string()
        }
        "LIFECYCLE_PROMOTION_FAILED" | "LIFECYCLE_PROMOTION_INVALID" => {
            "Adapter promotion failed. Check promotion gates and requirements.".to_string()
        }
        "LIFECYCLE_DEMOTION_FAILED" | "LIFECYCLE_DEMOTION_INVALID" => {
            "Adapter demotion failed. Check if the adapter is in use.".to_string()
        }
        "ADAPTER_IN_USE" => {
            "Adapter is in use by an active stack. Remove it from stacks first.".to_string()
        }

        // -- Fallback --
        _ => {
            let lower = error.to_lowercase();
            if lower.contains("inference failed") {
                "Inference failed. Retry in a moment or check worker health.".to_string()
            } else {
                error.to_string()
            }
        }
    };

    if let Some(code) = failure_code {
        match code {
            FailureCode::OutOfMemory => {
                return "System is out of memory. Reduce request size or retry later.".to_string();
            }
            FailureCode::WorkerOverloaded
            | FailureCode::CpuThrottled
            | FailureCode::ThreadPoolSaturated
            | FailureCode::GpuUnavailable => {
                return "Workers are at capacity. Retry in a moment or check worker health."
                    .to_string();
            }
            FailureCode::TenantAccessDenied => {
                return "You don't have access to this workspace. Contact an admin if you need access."
                    .to_string();
            }
            FailureCode::ModelLoadFailed => {
                return "Model failed to load. Retry in a moment or check worker logs.".to_string();
            }
            FailureCode::BackendFallback => {
                return "No compatible worker available. Check model availability or retry later."
                    .to_string();
            }
            _ => {}
        }
    }

    message
}

/// Format structured error details (field-level validation errors) into a user-readable string.
/// Extracts details from `ApiError::Structured` variant's `details` field.
pub fn format_structured_details(error: &ApiError) -> String {
    match error {
        ApiError::Structured {
            details: Some(details),
            error: err_msg,
            ..
        } => {
            if let Some(errors) = details.get("errors").and_then(|v| v.as_array()) {
                let rendered: Vec<String> = errors
                    .iter()
                    .filter_map(|entry| {
                        let msg = entry
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or_default();
                        if msg.is_empty() {
                            return None;
                        }
                        let mut parts = vec![msg.to_string()];
                        if let Some(field) = entry.get("field_name").and_then(|f| f.as_str()) {
                            parts.push(format!("field {}", field));
                        }
                        if let Some(file) = entry.get("file_path").and_then(|f| f.as_str()) {
                            parts.push(file.to_string());
                        }
                        if let Some(line) = entry.get("line_number").and_then(|l| l.as_i64()) {
                            parts.push(format!("line {}", line));
                        }
                        Some(parts.join(" \u{00b7} "))
                    })
                    .collect();
                if !rendered.is_empty() {
                    return format!("{}: {}", err_msg, rendered.join("; "));
                }
            }

            if let Some(obj) = details.as_object() {
                let field_errors: Vec<String> = obj
                    .iter()
                    .filter(|(k, _)| k.as_str() != "errors")
                    .filter_map(|(field, value)| {
                        value.as_str().map(|msg| format!("{}: {}", field, msg))
                    })
                    .collect();
                if !field_errors.is_empty() {
                    return field_errors.join(". ");
                }
            }

            error.user_message()
        }
        _ => error.user_message(),
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("AbortError") {
            Self::Aborted
        } else {
            Self::Network(msg)
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_response_401() {
        let error = ApiError::from_response(401, "Unauthorized", None);
        assert!(matches!(error, ApiError::Unauthorized));
        assert!(error.requires_auth());
        assert_eq!(error.code(), Some("UNAUTHORIZED"));
    }

    #[test]
    fn test_from_response_403() {
        let error = ApiError::from_response(403, "Access denied", None);
        assert!(matches!(error, ApiError::Forbidden(_)));
        assert!(!error.requires_auth());
        assert_eq!(error.code(), Some("FORBIDDEN"));
    }

    #[test]
    fn test_from_response_404() {
        let error = ApiError::from_response(404, "Resource not found", None);
        assert!(matches!(error, ApiError::NotFound(_)));
        assert_eq!(error.code(), Some("NOT_FOUND"));
    }

    #[test]
    fn test_from_response_429() {
        let error = ApiError::from_response(429, "Too many requests", None);
        assert!(matches!(error, ApiError::RateLimited { .. }));
        assert!(error.is_retryable());
    }

    #[test]
    fn test_from_response_500() {
        let error = ApiError::from_response(500, "Internal server error", None);
        assert!(matches!(error, ApiError::Server(_)));
        assert!(error.is_retryable());
        assert_eq!(error.code(), Some("SERVER_ERROR"));
    }

    #[test]
    fn test_from_response_structured() {
        // FailureCode uses SCREAMING_SNAKE_CASE serde format
        let body = r#"{"error":"Worker is overloaded","code":"WORKER_OVERLOADED","failure_code":"WORKER_OVERLOADED"}"#;
        let error = ApiError::from_response(503, body, None);

        assert!(matches!(error, ApiError::Structured { .. }));
        if let ApiError::Structured {
            error,
            code,
            failure_code,
            ..
        } = &error
        {
            assert_eq!(error, "Worker is overloaded");
            assert_eq!(code, "WORKER_OVERLOADED");
            assert_eq!(*failure_code, Some(FailureCode::WorkerOverloaded));
        }
    }

    #[test]
    fn test_from_response_structured_without_failure_code() {
        let body = r#"{"error":"Invalid request","code":"VALIDATION_FAILED"}"#;
        let error = ApiError::from_response(400, body, None);

        assert!(matches!(error, ApiError::Structured { .. }));
        if let ApiError::Structured { error, code, .. } = &error {
            assert_eq!(error, "Invalid request");
            assert_eq!(code, "VALIDATION_FAILED");
        }
    }

    #[test]
    fn test_network_error_retryable() {
        let error = ApiError::Network("Connection refused".to_string());
        assert!(error.is_retryable());
    }

    #[test]
    fn test_validation_error_not_retryable() {
        let error = ApiError::Validation("Invalid email".to_string());
        assert!(!error.is_retryable());
    }

    #[test]
    fn test_is_adapter_in_flight() {
        let body = r#"{"error":"Adapter is currently in use","code":"ADAPTER_IN_FLIGHT"}"#;
        let error = ApiError::from_response(409, body, None);

        assert!(error.is_adapter_in_flight());
        assert!(error
            .user_message()
            .contains("currently in use for inference"));
    }

    #[test]
    fn test_is_adapter_in_flight_false_for_other_errors() {
        let error = ApiError::Validation("Invalid input".to_string());
        assert!(!error.is_adapter_in_flight());
    }

    #[test]
    fn test_user_message_regular_error() {
        let error = ApiError::NotFound("Resource not found".to_string());
        assert_eq!(
            error.user_message(),
            "Not found. Check the URL or try again."
        );
    }

    #[test]
    fn test_requires_auth_for_structured_unauthorized() {
        let error = ApiError::Structured {
            error: "unauthorized".to_string(),
            code: "UNAUTHORIZED".to_string(),
            failure_code: None,
            hint: None,
            details: None,
            request_id: None,
            error_id: None,
            fingerprint: None,
            session_id: None,
            diag_trace_id: None,
            otel_trace_id: None,
        };
        assert!(error.requires_auth());
    }

    #[test]
    fn test_policy_violation_with_details_string() {
        let body = r#"{"error":"policy violation","code":"POLICY_VIOLATION","details":"Request violates 1 policy pack(s): safety: prohibited content detected"}"#;
        let error = ApiError::from_response(403, body, None);
        assert_eq!(
            error.user_message(),
            "Blocked by policy: Request violates 1 policy pack(s): safety: prohibited content detected"
        );
    }

    #[test]
    fn test_policy_violation_with_specific_error_message() {
        let body =
            r#"{"error":"adapter exceeds maximum rank for tenant","code":"POLICY_VIOLATION"}"#;
        let error = ApiError::from_response(403, body, None);
        assert_eq!(
            error.user_message(),
            "Blocked by policy: adapter exceeds maximum rank for tenant"
        );
    }

    #[test]
    fn test_policy_violation_generic_fallback() {
        let body = r#"{"error":"policy violation","code":"POLICY_VIOLATION"}"#;
        let error = ApiError::from_response(403, body, None);
        assert_eq!(
            error.user_message(),
            "Blocked by policy. Contact an admin if you need access."
        );
    }

    #[test]
    fn test_policy_violation_details_preferred_over_error_message() {
        let body = r#"{"error":"some specific error","code":"POLICY_VIOLATION","details":"detailed violation: egress denied for endpoint api.example.com"}"#;
        let error = ApiError::from_response(403, body, None);
        // details should take precedence over error message
        assert!(error
            .user_message()
            .contains("egress denied for endpoint api.example.com"));
    }

    #[test]
    fn test_policy_violation_with_hint() {
        let body = r#"{"error":"policy violation","code":"POLICY_VIOLATION","details":"training blocked: insufficient quota","hint":"upgrade your plan"}"#;
        let error = ApiError::from_response(403, body, None);
        assert_eq!(
            error.user_message(),
            "Blocked by policy: training blocked: insufficient quota. Next: upgrade your plan"
        );
    }

    #[test]
    fn test_user_message_applies_hint() {
        let error = ApiError::Structured {
            error: "Service unavailable".to_string(),
            code: "SERVICE_UNAVAILABLE".to_string(),
            failure_code: None,
            hint: Some("retry in a moment".to_string()),
            details: None,
            request_id: None,
            error_id: None,
            fingerprint: None,
            session_id: None,
            diag_trace_id: None,
            otel_trace_id: None,
        };
        assert_eq!(
            error.user_message(),
            "Service temporarily unavailable. Retry in a moment. Next: retry in a moment"
        );
    }

    /// Helper: build a structured error from a code string and return its user_message().
    fn msg_for(code: &str) -> String {
        let body = format!(r#"{{"error":"test error","code":"{}"}}"#, code);
        ApiError::from_response(400, &body, None).user_message()
    }

    // --- New error code mapping tests ---

    #[test]
    fn test_user_message_bad_request_codes() {
        assert_eq!(
            msg_for("BAD_REQUEST"),
            "Invalid request. Check your input and try again."
        );
        assert_eq!(
            msg_for("VALIDATION_ERROR"),
            "Some fields are invalid. Fix the highlighted fields and retry."
        );
        assert_eq!(
            msg_for("INVALID_HASH"),
            "Invalid hash format. Expected a BLAKE3 hex string."
        );
        assert_eq!(
            msg_for("MISSING_FIELD"),
            "A required field is missing from the request."
        );
        assert_eq!(
            msg_for("ADAPTER_BASE_MODEL_MISMATCH"),
            "Adapter was trained on a different base model than the one loaded."
        );
    }

    #[test]
    fn test_user_message_auth_codes() {
        assert_eq!(
            msg_for("TOKEN_MISSING"),
            "No authentication token provided. Log in to continue."
        );
        assert_eq!(
            msg_for("TOKEN_SIGNATURE_INVALID"),
            "Token signature verification failed. Log in again."
        );
        assert_eq!(
            msg_for("INVALID_API_KEY"),
            "API key is invalid or not found."
        );
        assert_eq!(
            msg_for("SESSION_EXPIRED"),
            "Your session has expired. Log in again."
        );
        assert_eq!(
            msg_for("INVALID_CREDENTIALS"),
            "Invalid username or password."
        );
    }

    #[test]
    fn test_user_message_forbidden_codes() {
        assert_eq!(
            msg_for("TENANT_ISOLATION_ERROR"),
            "Cross-tenant access denied. You can only access your own workspace."
        );
        assert_eq!(
            msg_for("CSRF_ERROR"),
            "Security token expired. Refresh the page and try again."
        );
        assert_eq!(
            msg_for("EGRESS_VIOLATION"),
            "Network egress blocked by security policy."
        );
        assert_eq!(
            msg_for("CHECKPOINT_INTEGRITY_FAILED"),
            "Checkpoint signature verification failed. The checkpoint may be tampered."
        );
    }

    #[test]
    fn test_user_message_conflict_codes() {
        assert_eq!(
            msg_for("CONFLICT"),
            "Conflict detected. Another operation may be in progress."
        );
        assert_eq!(
            msg_for("ADAPTER_HASH_MISMATCH"),
            "Adapter content hash mismatch. Re-upload or verify the adapter."
        );
        assert_eq!(
            msg_for("DUPLICATE_REQUEST"),
            "Duplicate request detected. Your previous request is being processed."
        );
    }

    #[test]
    fn test_user_message_server_error_codes() {
        assert_eq!(
            msg_for("INTERNAL_ERROR"),
            "Internal server error. Retry in a moment."
        );
        assert_eq!(
            msg_for("DATABASE_ERROR"),
            "Database error. Retry in a moment."
        );
        assert_eq!(
            msg_for("CONFIG_ERROR"),
            "Server configuration error. Contact an admin."
        );
    }

    #[test]
    fn test_user_message_gateway_codes() {
        assert_eq!(
            msg_for("BASE_LLM_ERROR"),
            "Base model error. Retry in a moment or check worker logs."
        );
        assert_eq!(
            msg_for("UDS_CONNECTION_FAILED"),
            "Worker connection failed. Check if the worker is running."
        );
        assert_eq!(
            msg_for("DOWNLOAD_FAILED"),
            "File download failed. Check network connectivity."
        );
    }

    #[test]
    fn test_user_message_resource_exhaustion_codes() {
        assert_eq!(
            msg_for("DISK_FULL"),
            "Disk space exhausted. Free storage and retry."
        );
        assert_eq!(
            msg_for("FD_EXHAUSTED"),
            "System file descriptor limit reached. Contact an admin."
        );
        assert_eq!(
            msg_for("THREAD_POOL_SATURATED"),
            "Workers are at capacity. Retry in a moment or check worker health."
        );
        assert_eq!(
            msg_for("CIRCUIT_BREAKER_HALF_OPEN"),
            "Service is recovering. Retry in a moment."
        );
    }

    #[test]
    fn test_user_message_misc_codes() {
        assert_eq!(
            msg_for("REASONING_LOOP_DETECTED"),
            "Reasoning loop detected. The model is repeating itself. Try rephrasing."
        );
        assert_eq!(
            msg_for("TOO_MANY_REQUESTS"),
            "Too many requests. Retry in a moment."
        );
        assert_eq!(msg_for("CLIENT_CLOSED_REQUEST"), "Request cancelled.");
        assert_eq!(
            msg_for("PAYLOAD_TOO_LARGE"),
            "Request is too large. Reduce the payload size."
        );
    }

    #[test]
    fn test_user_message_boot_time_codes() {
        assert_eq!(
            msg_for("DEV_BYPASS_IN_RELEASE"),
            "Dev bypass is not allowed in release builds."
        );
        assert_eq!(
            msg_for("JWT_MODE_NOT_CONFIGURED"),
            "JWT authentication is not properly configured."
        );
        assert_eq!(
            msg_for("API_KEY_MODE_NOT_CONFIGURED"),
            "API key authentication is not properly configured."
        );
    }

    #[test]
    fn test_user_message_cache_entry_not_found() {
        assert_eq!(
            msg_for("CACHE_ENTRY_NOT_FOUND"),
            "Cache entry not found. It may have been evicted."
        );
    }

    #[test]
    fn test_user_message_fallback_still_works() {
        // Unknown codes should fall through to the error message
        assert_eq!(msg_for("TOTALLY_UNKNOWN_CODE"), "test error");
    }
}
