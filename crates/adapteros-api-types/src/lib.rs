//! Shared API types for adapterOS Control Plane
//!
//! This crate provides unified request/response types used across
//! the control plane API, client libraries, and UI components.
//!
//! # Features
//!
//! - `server` (default): Enables axum IntoResponse, utoipa schemas, and server deps
//! - `wasm`: WASM-compatible build with serde types only

// Re-export API_SCHEMA_VERSION from core as the single source of truth
#[cfg(feature = "server")]
pub use adapteros_core::version::API_SCHEMA_VERSION;

// For WASM builds, define a constant directly (must match core::version::API_SCHEMA_VERSION)
#[cfg(not(feature = "server"))]
pub const API_SCHEMA_VERSION: &str = "1.0.0";

/// Get the current API schema version as a String.
/// Used as serde default for response types.
pub fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Default error code for deserialized ErrorResponse.
fn default_error_code() -> String {
    "INTERNAL_ERROR".to_string()
}

pub mod activity;
pub mod adapters;
pub mod admin;
pub mod api_keys;
pub mod auth;
pub mod code_repositories;
pub mod codebase_adapters;
pub mod dashboard;
pub mod dataset_domain;
pub mod defaults;
pub mod diagnostics;
pub mod domain_adapters;
pub mod embeddings;
pub mod errors;
pub mod execution_policy;
pub mod failure_code;
pub mod filesystem;
pub mod git;
pub mod inference;
pub mod metrics;
pub mod model_status;
pub mod models;
pub mod nodes;
pub mod orchestration;
pub mod plans;
pub mod policy;
pub mod prefix_templates;
pub mod provenance;
pub mod repositories;
pub mod review;
pub mod routing;
pub mod run_envelope;
pub mod settings;
pub mod streaming_events;
pub mod system_state;
pub mod system_status;
pub mod telemetry;
pub mod tenant_settings;
pub mod tenants;
pub mod topology;
pub mod training;
pub mod ui;
pub mod update_field;
pub mod workers;

// Re-export commonly used types (server feature for full type access)
pub use activity::*;
#[cfg(feature = "server")]
pub use adapteros_types::coreml::{
    CoreMLGating, CoreMLMode, CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape,
    CoreMLPlacementSpec, CoreMLProjection, CoreMLTargetRef,
};
#[cfg(feature = "server")]
pub use adapteros_types::repository::RepoTier;
pub use adapters::*;
pub use admin::*;
pub use api_keys::*;
pub use auth::*;
// codebase_adapters types are internal-only (legacy, routes removed)
pub use dashboard::*;
pub use dataset_domain::*;
pub use diagnostics::*;
pub use domain_adapters::*;
pub use embeddings::*;
pub use execution_policy::*;
pub use failure_code::FailureCode;
pub use git::*;
pub use inference::*;
pub use metrics::*;
pub use models::*;
pub use nodes::*;
pub use orchestration::*;
pub use plans::*;
pub use policy::*;
pub use prefix_templates::*;
pub use provenance::*;
pub use repositories::*;
pub use review::*;
pub use routing::*;
pub use run_envelope::*;
pub use settings::*;
pub use streaming_events::*;
pub use system_status::*;
pub use tenant_settings::*;
// Note: telemetry types are not re-exported to avoid conflicts with metrics types
pub use errors::*;
pub use model_status::*;
pub use system_state::*;
pub use tenants::*;
pub use topology::*;
pub use training::*;
pub use ui::*;
pub use update_field::UpdateField;
pub use workers::*;

/// Common error response structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ErrorResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Human-readable error message
    #[serde(alias = "error")]
    pub message: String,
    #[serde(default = "default_error_code")]
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<FailureCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Stable debug identifier for a persisted server-side error instance (err-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_id: Option<String>,
    /// Stable fingerprint for bucketing/dedupe
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// UI session correlation (ses-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// AdapterOS diagnostic trace id (trc-...) when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diag_trace_id: Option<String>,
    /// W3C trace id (32-hex) when available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_trace_id: Option<String>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            schema_version: schema_version(),
            message: message.into(),
            code: "INTERNAL_ERROR".to_string(),
            failure_code: None,
            hint: None,
            details: None,
            request_id: None,
            error_id: None,
            fingerprint: None,
            session_id: None,
            diag_trace_id: None,
            otel_trace_id: None,
        }
    }

    /// Set the error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        let code_string = code.into();
        if self.failure_code.is_none() {
            self.failure_code = FailureCode::parse_code(&code_string);
        }
        self.code = code_string;
        self
    }

    /// Set the error details
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Set the error details from string
    pub fn with_string_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(serde_json::json!(details.into()));
        self
    }

    /// Set an actionable hint
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Set the request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Set a structured failure code (smoke-test and UI facing)
    pub fn with_failure_code(mut self, code: FailureCode) -> Self {
        self.failure_code = Some(code);
        self
    }

    /// Attach a failure code when present (no-op on None)
    pub fn with_failure_code_opt(mut self, code: Option<FailureCode>) -> Self {
        self.failure_code = code;
        self
    }

    /// Create an error response with user-friendly message mapping
    /// Note: This method requires access to UserFriendlyErrorMapper from server-api crate
    /// For unified API types, use ErrorResponse::new() and map messages at the server level
    /// For unified API types, use ErrorResponse::new() and map messages at the server level
    pub fn with_user_friendly_message(mut self, user_friendly_msg: impl Into<String>) -> Self {
        self.message = user_friendly_msg.into();
        self
    }
}

#[cfg(feature = "server")]
impl axum::response::IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        use axum::http::StatusCode;
        let status = match self.code.as_str() {
            // 401 Unauthorized
            "UNAUTHORIZED"
            | "TOKEN_EXPIRED"
            | "TOKEN_REVOKED"
            | "INVALID_TOKEN"
            | "MISSING_AUTH"
            | "AUTHENTICATION_ERROR" => StatusCode::UNAUTHORIZED,

            // 403 Forbidden
            "FORBIDDEN"
            | "POLICY_VIOLATION"
            | "PERMISSION_DENIED"
            | "ISOLATION_VIOLATION"
            | "EGRESS_VIOLATION"
            | "POLICY_HASH_MISMATCH"
            | "AUTHORIZATION_ERROR" => StatusCode::FORBIDDEN,

            // 404 Not Found
            "NOT_FOUND" | "ENDPOINT_NOT_FOUND" | "MODEL_NOT_FOUND" => StatusCode::NOT_FOUND,

            // 400 Bad Request
            "BAD_REQUEST"
            | "VALIDATION_ERROR"
            | "API_USAGE_ERROR"
            | "INVALID_INPUT"
            | "INVALID_MANIFEST"
            | "PARSE_ERROR"
            | "CHAT_TEMPLATE_ERROR" => StatusCode::BAD_REQUEST,

            // 409 Conflict
            "CONFLICT" | "MODEL_ACQUISITION_IN_PROGRESS" | "ADAPTER_HASH_MISMATCH" => {
                StatusCode::CONFLICT
            }

            // 413 Payload Too Large
            "PAYLOAD_TOO_LARGE" => StatusCode::PAYLOAD_TOO_LARGE,

            // 429 Too Many Requests
            "TOO_MANY_REQUESTS" | "RATE_LIMITED" | "RESOURCE_EXHAUSTED" => {
                StatusCode::TOO_MANY_REQUESTS
            }

            // 501 Not Implemented
            "NOT_IMPLEMENTED" | "FEATURE_DISABLED" => StatusCode::NOT_IMPLEMENTED,

            // 502 Bad Gateway
            "BAD_GATEWAY"
            | "NETWORK_ERROR"
            | "WORKER_NOT_RESPONDING"
            | "CIRCUIT_BREAKER_OPEN"
            | "CIRCUIT_BREAKER_HALF_OPEN"
            | "DOWNLOAD_FAILED"
            | "HEALTH_CHECK_FAILED" => StatusCode::BAD_GATEWAY,

            // 503 Service Unavailable
            "SERVICE_UNAVAILABLE" | "DRAINING" | "MEMORY_PRESSURE" | "SYSTEM_QUARANTINED" => {
                StatusCode::SERVICE_UNAVAILABLE
            }

            // 504 Gateway Timeout
            "TIMEOUT" | "GATEWAY_TIMEOUT" => StatusCode::GATEWAY_TIMEOUT,

            // 500 Internal Server Error (default)
            "DATABASE_ERROR"
            | "INTERNAL_ERROR"
            | "IO_ERROR"
            | "CRYPTO_ERROR"
            | "SERIALIZATION_ERROR"
            | "METAL_ERROR"
            | "COREML_ERROR"
            | "MLX_ERROR"
            | "WORKER_ERROR"
            | "TRAINING_ERROR"
            | "KERNEL_ERROR"
            | "DETERMINISM_ERROR"
            | "ROUTING_ERROR"
            | "FEDERATION_ERROR"
            | "LIFECYCLE_ERROR"
            | "CONFIG_ERROR"
            | "REGISTRY_ERROR"
            | "GIT_ERROR"
            | "RAG_ERROR"
            | "TELEMETRY_ERROR"
            | "REPLAY_ERROR"
            | "VERIFICATION_ERROR"
            | "CACHE_CORRUPTION"
            | "INVALID_SEALED_DATA" => StatusCode::INTERNAL_SERVER_ERROR,

            // Catch-all for unknown codes
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, axum::Json(self)).into_response()
    }
}

/// Health check response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct HealthResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub status: String,
    pub version: String,
    /// Build identifier (git hash + timestamp, e.g., "a6922d2-20260122153045")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// Model runtime health information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelRuntimeHealth>,
    /// Per-crate version manifest (inference-critical crates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crate_manifest: Option<std::collections::BTreeMap<String, String>>,
    /// BLAKE3 digest of the canonical crate manifest JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crate_manifest_digest: Option<String>,
}

/// Model runtime health summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ModelRuntimeHealth {
    pub total_models: i32,
    pub loaded_count: i32,
    pub healthy: bool,
    pub inconsistencies_count: usize,
}

/// Pagination parameters
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema, utoipa::IntoParams))]
#[serde(rename_all = "snake_case")]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

pub(crate) fn default_page() -> u32 {
    1
}
pub(crate) fn default_limit() -> u32 {
    50
}

/// Paginated response wrapper
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct PaginatedResponse<T> {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub data: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}
