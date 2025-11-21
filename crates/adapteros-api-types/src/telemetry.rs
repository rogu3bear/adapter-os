//! Telemetry types

use adapteros_telemetry::unified_events::TelemetryEvent;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

// Re-export canonical BundleMetadata from adapteros-telemetry-types
pub use adapteros_telemetry_types::BundleMetadata;

/// API telemetry event (DTO for public API)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ApiTelemetryEvent {
    pub event_type: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
}

/// Conversion from canonical TelemetryEvent to API DTO
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ExportTelemetryBundleRequest {
    pub bundle_id: String,
    pub format: String, // "json", "ndjson", "csv"
}

/// Verify bundle signature request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct VerifyBundleSignatureRequest {
    pub bundle_id: String,
    pub expected_signature: String,
}

/// Bundle verification response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BundleVerificationResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub bundle_id: String,
    pub verified: bool,
    pub signature_match: bool,
    pub timestamp: String,
}

