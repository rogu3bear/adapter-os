//! Telemetry types

#[cfg(feature = "server")]
use adapteros_telemetry::unified_events::TelemetryEvent;
use serde::{Deserialize, Serialize};

use crate::schema_version;

// Re-export canonical BundleMetadata from adapteros-telemetry-types
pub use adapteros_telemetry::types::BundleMetadata;

/// API telemetry event (DTO for public API)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ApiTelemetryEvent {
    pub event_type: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
}

/// Conversion from canonical TelemetryEvent to API DTO
#[cfg(feature = "server")]
impl From<TelemetryEvent> for ApiTelemetryEvent {
    fn from(ev: TelemetryEvent) -> Self {
        ApiTelemetryEvent {
            event_type: ev.event_type,
            timestamp: ev.timestamp.to_rfc3339(),
            data: ev.metadata.unwrap_or_else(|| serde_json::json!({})),
            tenant_id: Some(ev.identity.tenant_id),
            user_id: ev.user_id,
        }
    }
}

/// Telemetry bundle response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TelemetryBundleResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub bundle_id: String,
    pub created_at: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub signature: String,
}

/// Export telemetry bundle request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ExportTelemetryBundleRequest {
    pub bundle_id: String,
    pub format: String, // "json", "ndjson", "csv"
}

/// Verify bundle signature request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct VerifyBundleSignatureRequest {
    pub bundle_id: String,
    pub expected_signature: String,
}

/// Bundle verification response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct BundleVerificationResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub bundle_id: String,
    pub verified: bool,
    pub signature_match: bool,
    pub timestamp: String,
}

// ============================================================================
// Client Error Reporting Types
// ============================================================================

/// Client-side error report sent from UI to server for persistent logging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClientErrorReport {
    /// Error type classification (e.g., "Network", "Http", "Validation", "Server")
    pub error_type: String,
    /// Error message (max 2000 chars enforced server-side)
    pub message: String,
    /// Error code if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Structured failure code as string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    /// HTTP status code if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    /// Current page/route where error occurred
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Browser user agent string
    pub user_agent: String,
    /// ISO 8601 timestamp when error occurred
    pub timestamp: String,
    /// Additional error details (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Response to client error report
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClientErrorResponse {
    /// Server-generated UUID for tracking this error
    pub error_id: String,
    /// ISO 8601 timestamp when error was received
    pub received_at: String,
}

// =============================================================================
// Client Error Query Types (for Error Dashboard)
// =============================================================================

/// Response for list client errors endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClientErrorsListResponse {
    pub errors: Vec<ClientErrorItem>,
    pub total: usize,
}

/// Individual client error item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClientErrorItem {
    pub id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub error_type: String,
    pub message: String,
    pub code: Option<String>,
    pub failure_code: Option<String>,
    pub http_status: Option<i32>,
    pub page: Option<String>,
    pub client_timestamp: String,
    pub created_at: String,
}

/// Response for error statistics endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ClientErrorStatsResponse {
    pub total_count: i64,
    pub error_type_counts: Vec<ErrorTypeCount>,
    pub http_status_counts: Vec<HttpStatusCount>,
    pub errors_per_hour: Vec<HourlyErrorCount>,
}

/// Error count by type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorTypeCount {
    pub error_type: String,
    pub count: i64,
}

/// Error count by HTTP status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HttpStatusCount {
    pub http_status: i32,
    pub count: i64,
}

/// Hourly error count
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HourlyErrorCount {
    pub hour: String,
    pub count: i64,
}

// =============================================================================
// Error Alert Types
// =============================================================================

/// Error alert rule for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorAlertRuleResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub error_type_pattern: Option<String>,
    pub http_status_pattern: Option<String>,
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    pub cooldown_minutes: i32,
    pub severity: String,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Request to create an error alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CreateErrorAlertRuleRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    pub threshold_count: i32,
    pub threshold_window_minutes: i32,
    #[serde(default = "default_cooldown")]
    pub cooldown_minutes: i32,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

fn default_cooldown() -> i32 {
    15
}

fn default_severity() -> String {
    "warning".to_string()
}

/// Request to update an error alert rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct UpdateErrorAlertRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_window_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_channels: Option<serde_json::Value>,
}

/// List alert rules response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorAlertRulesListResponse {
    pub rules: Vec<ErrorAlertRuleResponse>,
    pub total: usize,
}

/// Error alert history item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorAlertHistoryResponse {
    pub id: String,
    pub rule_id: String,
    pub rule_name: Option<String>,
    pub tenant_id: String,
    pub triggered_at: String,
    pub error_count: i32,
    pub sample_error_ids: Option<Vec<String>>,
    pub acknowledged_at: Option<String>,
    pub acknowledged_by: Option<String>,
    pub resolved_at: Option<String>,
    pub resolution_note: Option<String>,
}

/// List alert history response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ErrorAlertHistoryListResponse {
    pub alerts: Vec<ErrorAlertHistoryResponse>,
    pub total: usize,
}

/// Request to acknowledge an alert
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AcknowledgeAlertRequest {
    // Body can be empty - user ID comes from auth
}

/// Request to resolve an alert
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ResolveAlertRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,
}
