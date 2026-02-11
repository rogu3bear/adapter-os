//! First-class error types (ErrorInstance + ErrorBucket)
//!
//! These types are public API surfaces for querying persisted errors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorSource {
    Ui,
    Api,
    Worker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Network,
    Auth,
    Validation,
    Server,
    Decode,
    Timeout,
    Worker,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warn,
    Error,
    Fatal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorInstance {
    pub id: String, // err-...
    pub created_at_unix_ms: i64,
    pub tenant_id: String,
    pub source: ErrorSource,
    pub error_code: String,
    pub kind: ErrorKind,
    pub severity: ErrorSeverity,
    pub message_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_dev: Option<String>,
    pub fingerprint: String,
    pub tags_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diag_trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorBucket {
    pub fingerprint: String,
    pub tenant_id: String,
    pub error_code: String,
    pub kind: ErrorKind,
    pub severity: ErrorSeverity,
    pub first_seen_unix_ms: i64,
    pub last_seen_unix_ms: i64,
    pub count: i64,
    pub sample_error_ids_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
pub struct ListErrorsQuery {
    /// Inclusive lower bound (unix ms). Preferred public query field.
    pub since: Option<i64>,
    /// Inclusive upper bound (unix ms). Preferred public query field.
    pub until: Option<i64>,
    /// Cursor for pagination (`created_at_unix_ms < after`). Preferred field.
    pub after: Option<i64>,
    /// Back-compat field name.
    pub since_unix_ms: Option<i64>,
    /// Back-compat field name.
    pub until_unix_ms: Option<i64>,
    pub limit: Option<u32>,
    /// Back-compat field name.
    pub after_created_at_unix_ms: Option<i64>,
    pub error_code: Option<String>,
    pub fingerprint: Option<String>,
    pub request_id: Option<String>,
    pub diag_trace_id: Option<String>,
    pub session_id: Option<String>,
    pub source: Option<ErrorSource>,
    pub severity: Option<ErrorSeverity>,
    pub kind: Option<ErrorKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListErrorsResponse {
    pub items: Vec<ErrorInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct GetErrorResponse {
    pub item: ErrorInstance,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
pub struct ListErrorBucketsQuery {
    pub limit: Option<u32>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListErrorBucketsResponse {
    pub items: Vec<ErrorBucket>,
}
