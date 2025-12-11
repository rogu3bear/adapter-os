//! Unified API error type for AdapterOS handlers
//!
//! Provides a single `ApiError` type that implements `IntoResponse` directly,
//! enabling cleaner error handling without manual tuple construction.
//!
//! # Usage
//!
//! ```ignore
//! use crate::api_error::{ApiError, ApiResult};
//!
//! pub async fn my_handler(State(state): State<AppState>) -> ApiResult<MyResponse> {
//!     let adapter = state.db.get_adapter(&id).await
//!         .map_err(|e| ApiError::db_error(e))?;
//!
//!     let adapter = adapter.ok_or_else(|| ApiError::not_found("Adapter"))?;
//!
//!     Ok(Json(MyResponse { ... }))
//! }
//! ```
//!
//! # Builder Pattern
//!
//! ```ignore
//! // Add details to any error
//! ApiError::internal("processing failed")
//!     .with_details(format!("step {} of {}", current, total))
//!
//! // Add request ID for tracing
//! ApiError::bad_request("invalid input")
//!     .with_request_id(&request_id)
//! ```

use crate::types::ErrorResponse;
use adapteros_core::AosError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::borrow::Cow;
use tracing::error;

/// Unified API error type implementing IntoResponse
///
/// This type replaces the `(StatusCode, Json<ErrorResponse>)` tuple pattern,
/// providing a cleaner API with builder methods and automatic error conversion.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: Cow<'static, str>,
    message: String,
    details: Option<String>,
    request_id: Option<String>,
}

/// Standard API result type - handlers should use this
pub type ApiResult<T> = Result<Json<T>, ApiError>;

impl ApiError {
    /// Create a new ApiError with the given status, code, and message
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: Cow::Borrowed(code),
            message: message.into(),
            details: None,
            request_id: None,
        }
    }

    /// Add details to the error
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add request ID for tracing
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }

    // --- Constructors for common error types ---

    /// Database error - 500 Internal Server Error
    pub fn db_error<E: std::fmt::Display>(e: E) -> Self {
        error!("Database error: {}", e);
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            e.to_string(),
        )
    }

    /// Internal error - 500 Internal Server Error
    pub fn internal(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        error!("Internal error: {}", msg);
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
    }

    /// Not found - 404 Not Found
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("{} not found", resource.into()),
        )
    }

    /// Not found with custom message - 404 Not Found
    pub fn not_found_msg(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", msg)
    }

    /// Bad request - 400 Bad Request
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_REQUEST", msg)
    }

    /// Unauthorized - 401 Unauthorized
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
    }

    /// Forbidden - 403 Forbidden
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", msg)
    }

    /// Conflict - 409 Conflict
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", msg)
    }

    /// Payload too large - 413 Payload Too Large
    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "PAYLOAD_TOO_LARGE", msg)
    }

    /// Not implemented - 501 Not Implemented
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, "FEATURE_DISABLED", msg)
    }

    /// Service unavailable - 503 Service Unavailable
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE", msg)
    }

    /// Gateway timeout - 504 Gateway Timeout
    pub fn gateway_timeout(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, "GATEWAY_TIMEOUT", msg)
    }

    /// Bad gateway - 502 Bad Gateway
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        error!("Bad gateway: {}", msg);
        Self::new(StatusCode::BAD_GATEWAY, "BAD_GATEWAY", msg)
    }

    // --- Artifact-specific error codes (PRD-ART-01) ---

    /// Incompatible schema version - adapter manifest uses unsupported version
    pub fn incompatible_schema_version(file_version: &str, current_version: &str) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "INCOMPATIBLE_SCHEMA_VERSION",
            format!(
                "Schema version {} is newer than supported {}. Update AdapterOS.",
                file_version, current_version
            ),
        )
    }

    /// Incompatible base model - base model not found or unavailable
    pub fn incompatible_base_model(model: &str) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "INCOMPATIBLE_BASE_MODEL",
            format!("Base model '{}' not found or not available", model),
        )
    }

    /// Unsupported backend family - adapter requires unsupported backend
    pub fn unsupported_backend(backend: &str) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "UNSUPPORTED_BACKEND",
            format!("Unsupported backend family: {}", backend),
        )
    }

    /// Hash integrity failure - computed hash doesn't match manifest
    pub fn hash_integrity_failure(expected: &str, computed: &str) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "HASH_INTEGRITY_FAILURE",
            format!(
                "Weights hash mismatch: manifest says {}, computed {}",
                expected, computed
            ),
        )
    }

    /// Signature required - tenant policy requires signed adapters
    pub fn signature_required() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "SIGNATURE_REQUIRED",
            "Tenant policy requires signed adapters",
        )
    }

    /// Signature invalid - adapter signature verification failed
    pub fn signature_invalid(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "SIGNATURE_INVALID", msg)
    }

    /// Export failed - adapter export operation failed
    pub fn export_failed(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        error!("Export failed: {}", msg);
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "EXPORT_FAILED", msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error_response = ErrorResponse::new(&self.message).with_code(self.code);

        if let Some(details) = self.details {
            error_response = error_response.with_string_details(details);
        }

        if let Some(request_id) = self.request_id {
            let existing = error_response
                .details
                .as_ref()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            error_response = error_response
                .with_string_details(format!("{}. Request ID: {}", existing, request_id));
        }

        (self.status, Json(error_response)).into_response()
    }
}

/// Convert from the old tuple error format for gradual migration
impl From<(StatusCode, Json<ErrorResponse>)> for ApiError {
    fn from((status, Json(response)): (StatusCode, Json<ErrorResponse>)) -> Self {
        let ErrorResponse {
            code,
            error,
            details,
            ..
        } = response;

        Self {
            status,
            code: if code.is_empty() {
                Cow::Borrowed("LEGACY_ERROR")
            } else {
                Cow::Owned(code)
            },
            message: error,
            details: details.and_then(|v| v.as_str().map(|s| s.to_string())),
            request_id: None,
        }
    }
}

/// Convert ApiError back to old tuple format for backwards compatibility
///
/// This allows handlers still using the old return type to use `require_permission`
/// and other functions that now return `ApiError`.
impl From<ApiError> for (StatusCode, Json<ErrorResponse>) {
    fn from(err: ApiError) -> Self {
        let mut response = ErrorResponse::new(&err.message).with_code(err.code);
        if let Some(details) = err.details {
            response = response.with_string_details(details);
        }
        (err.status, Json(response))
    }
}

/// Automatic conversion from AosError (the core error type)
///
/// Maps all 58 AosError variants to appropriate HTTP status codes:
/// - 400 Bad Request: Validation, parse, format errors
/// - 401 Unauthorized: Authentication failures
/// - 403 Forbidden: Authorization, policy violations
/// - 404 Not Found: Resource not found
/// - 409 Conflict: Hash mismatches, acquisition in progress
/// - 500 Internal Server Error: System, database, infrastructure errors
/// - 502 Bad Gateway: External service failures
/// - 503 Service Unavailable: Resource exhaustion, circuit breakers
/// - 504 Gateway Timeout: Request timeouts
impl From<AosError> for ApiError {
    fn from(err: AosError) -> Self {
        match &err {
            // ========== 400 Bad Request (10 variants) ==========
            AosError::InvalidHash(_) => {
                ApiError::new(StatusCode::BAD_REQUEST, "INVALID_HASH", err.to_string())
            }
            AosError::InvalidCPID(_) => {
                ApiError::new(StatusCode::BAD_REQUEST, "INVALID_CPID", err.to_string())
            }
            AosError::Serialization(_) => ApiError::new(
                StatusCode::BAD_REQUEST,
                "SERIALIZATION_ERROR",
                err.to_string(),
            ),
            AosError::Parse(_) => {
                ApiError::new(StatusCode::BAD_REQUEST, "PARSE_ERROR", err.to_string())
            }
            AosError::InvalidManifest(_) => {
                ApiError::new(StatusCode::BAD_REQUEST, "INVALID_MANIFEST", err.to_string())
            }
            AosError::AdapterNotInManifest { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "ADAPTER_NOT_IN_MANIFEST",
                err.to_string(),
            ),
            AosError::AdapterNotInEffectiveSet { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "ADAPTER_NOT_IN_EFFECTIVE_SET",
                err.to_string(),
            ),
            AosError::KernelLayoutMismatch { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "KERNEL_LAYOUT_MISMATCH",
                err.to_string(),
            ),
            AosError::ChatTemplate(_) => ApiError::new(
                StatusCode::BAD_REQUEST,
                "CHAT_TEMPLATE_ERROR",
                err.to_string(),
            ),
            AosError::Validation(_) => {
                ApiError::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", err.to_string())
            }
            AosError::InvalidSealedData { .. } => ApiError::new(
                StatusCode::BAD_REQUEST,
                "INVALID_SEALED_DATA",
                err.to_string(),
            ),
            AosError::FeatureDisabled { .. } => {
                ApiError::new(StatusCode::BAD_REQUEST, "FEATURE_DISABLED", err.to_string())
            }

            // ========== 401 Unauthorized (1 variant) ==========
            AosError::Auth(_) => ApiError::unauthorized(err.to_string()),

            // ========== 403 Forbidden (9 variants) ==========
            AosError::Authz(_) => ApiError::forbidden(err.to_string()),
            AosError::PolicyViolation(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "POLICY_VIOLATION", err.to_string())
            }
            AosError::Policy(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "POLICY_ERROR", err.to_string())
            }
            AosError::DeterminismViolation(_) => ApiError::new(
                StatusCode::FORBIDDEN,
                "DETERMINISM_VIOLATION",
                err.to_string(),
            ),
            AosError::EgressViolation(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "EGRESS_VIOLATION", err.to_string())
            }
            AosError::IsolationViolation(_) => ApiError::new(
                StatusCode::FORBIDDEN,
                "ISOLATION_VIOLATION",
                err.to_string(),
            ),
            AosError::PerformanceViolation(_) => ApiError::new(
                StatusCode::FORBIDDEN,
                "PERFORMANCE_VIOLATION",
                err.to_string(),
            ),
            AosError::Anomaly(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "ANOMALY_DETECTED", err.to_string())
            }
            AosError::Quarantined(_) => {
                ApiError::new(StatusCode::FORBIDDEN, "SYSTEM_QUARANTINED", err.to_string())
            }

            // ========== 404 Not Found (2 variants) ==========
            AosError::NotFound(_) => ApiError::not_found_msg(err.to_string()),
            AosError::ModelNotFound { .. } => {
                ApiError::new(StatusCode::NOT_FOUND, "MODEL_NOT_FOUND", err.to_string())
            }

            // ========== 409 Conflict (4 variants) ==========
            AosError::AdapterHashMismatch { .. } => ApiError::new(
                StatusCode::CONFLICT,
                "ADAPTER_HASH_MISMATCH",
                err.to_string(),
            ),
            AosError::AdapterLayerHashMismatch { .. } => ApiError::new(
                StatusCode::CONFLICT,
                "ADAPTER_LAYER_HASH_MISMATCH",
                err.to_string(),
            ),
            AosError::PolicyHashMismatch { .. } => ApiError::new(
                StatusCode::CONFLICT,
                "POLICY_HASH_MISMATCH",
                err.to_string(),
            ),
            AosError::Promotion(_) => {
                ApiError::new(StatusCode::CONFLICT, "PROMOTION_ERROR", err.to_string())
            }
            AosError::ModelAcquisitionInProgress { .. } => ApiError::new(
                StatusCode::CONFLICT,
                "MODEL_ACQUISITION_IN_PROGRESS",
                err.to_string(),
            ),

            // ========== 502 Bad Gateway (6 variants) ==========
            AosError::Http(_) => ApiError::bad_gateway(err.to_string()),
            AosError::Network(_) => {
                ApiError::new(StatusCode::BAD_GATEWAY, "NETWORK_ERROR", err.to_string())
            }
            AosError::BaseLLM(_) => {
                ApiError::new(StatusCode::BAD_GATEWAY, "BASE_LLM_ERROR", err.to_string())
            }
            AosError::UdsConnectionFailed { .. } => ApiError::new(
                StatusCode::BAD_GATEWAY,
                "UDS_CONNECTION_FAILED",
                err.to_string(),
            ),
            AosError::InvalidResponse { .. } => {
                ApiError::new(StatusCode::BAD_GATEWAY, "INVALID_RESPONSE", err.to_string())
            }
            AosError::DownloadFailed { .. } => {
                ApiError::new(StatusCode::BAD_GATEWAY, "DOWNLOAD_FAILED", err.to_string())
            }

            // ========== 503 Service Unavailable (7 variants) ==========
            AosError::ResourceExhaustion(_) => ApiError::service_unavailable(err.to_string()),
            AosError::MemoryPressure(_) => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "MEMORY_PRESSURE",
                err.to_string(),
            ),
            AosError::Unavailable(_) => ApiError::service_unavailable(err.to_string()),
            AosError::WorkerNotResponding { .. } => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "WORKER_NOT_RESPONDING",
                err.to_string(),
            ),
            AosError::CircuitBreakerOpen { .. } => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "CIRCUIT_BREAKER_OPEN",
                err.to_string(),
            ),
            AosError::CircuitBreakerHalfOpen { .. } => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "CIRCUIT_BREAKER_HALF_OPEN",
                err.to_string(),
            ),
            AosError::HealthCheckFailed { .. } => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "HEALTH_CHECK_FAILED",
                err.to_string(),
            ),
            AosError::AdapterNotLoaded { .. } => ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "ADAPTER_NOT_LOADED",
                err.to_string(),
            ),

            // ========== 504 Gateway Timeout (1 variant) ==========
            AosError::Timeout { duration } => {
                ApiError::gateway_timeout(format!("request timed out after {:?}", duration))
            }

            // ========== 500 Internal Server Error (remaining ~18 variants) ==========
            // Infrastructure errors - logged and returned as internal
            AosError::Io(_)
            | AosError::Crypto(_)
            | AosError::Mtl(_)
            | AosError::Replay(_)
            | AosError::Verification(_)
            | AosError::Sqlx(_)
            | AosError::Registry(_)
            | AosError::Sqlite(_)
            | AosError::Artifact(_)
            | AosError::Plan(_)
            | AosError::Kernel(_)
            | AosError::CoreML(_)
            | AosError::Mlx(_)
            | AosError::Worker(_)
            | AosError::Telemetry(_)
            | AosError::Quantization(_)
            | AosError::Node(_)
            | AosError::Job(_)
            | AosError::Memory(_)
            | AosError::Rag(_)
            | AosError::Lifecycle(_)
            | AosError::Git(_)
            | AosError::Training(_)
            | AosError::Autograd(_)
            | AosError::Toolchain(_)
            | AosError::Internal(_)
            | AosError::DeterministicExecutor(_)
            | AosError::System(_)
            | AosError::Platform(_)
            | AosError::Config(_)
            | AosError::Database(_)
            | AosError::RngError { .. }
            | AosError::EncryptionFailed { .. }
            | AosError::DecryptionFailed { .. }
            | AosError::DatabaseError { .. }
            | AosError::Routing(_)
            | AosError::Federation(_)
            | AosError::SegmentHashMismatch { .. }
            | AosError::MissingSegment { .. }
            | AosError::MissingCanonicalSegment
            | AosError::CacheCorruption { .. }
            | AosError::Other(_) => {
                error!("Internal error: {}", err);
                ApiError::internal(err.to_string())
            }

            // Wrapper - log context and return internal error
            // Note: We can't fully unwrap WithContext without Clone on AosError,
            // so we include the full error message which contains both context and source
            AosError::WithContext { context, source } => {
                error!("Error with context '{}': {}", context, source);
                ApiError::internal(err.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::AosError;

    #[test]
    fn test_api_error_into_response() {
        let error = ApiError::not_found("Adapter");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_api_error_with_details() {
        let error = ApiError::internal("failed").with_details("step 1 of 3");
        assert_eq!(error.details, Some("step 1 of 3".to_string()));
    }

    #[test]
    fn test_db_error_logs() {
        let error = ApiError::db_error("connection failed");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.code, "DATABASE_ERROR");
    }

    #[test]
    fn test_adapter_gate_errors_map_to_bad_request() {
        let effective_err = AosError::AdapterNotInEffectiveSet {
            adapter_id: "adapter-x".to_string(),
            effective_set: vec!["adapter-a".to_string(), "adapter-b".to_string()],
        };
        let api_err: ApiError = effective_err.into();
        assert_eq!(api_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.code, "ADAPTER_NOT_IN_EFFECTIVE_SET");

        let manifest_err = AosError::AdapterNotInManifest {
            adapter_id: "adapter-y".to_string(),
            available: vec!["adapter-a".to_string()],
        };
        let api_err: ApiError = manifest_err.into();
        assert_eq!(api_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.code, "ADAPTER_NOT_IN_MANIFEST");
    }

    #[test]
    fn test_legacy_tuple_conversion_preserves_code() {
        let legacy: (StatusCode, Json<ErrorResponse>) = (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("legacy failure").with_code("LEGACY_CODE")),
        );

        let api_err: ApiError = legacy.into();
        assert_eq!(api_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.code, "LEGACY_CODE");
        assert_eq!(api_err.message, "legacy failure");
    }
}
