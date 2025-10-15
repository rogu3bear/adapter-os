//! Telemetry types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryEvent {
    pub event_type: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
}

/// Telemetry bundle response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryBundleResponse {
    pub bundle_id: String,
    pub created_at: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub signature: String,
}

/// Export telemetry bundle request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportTelemetryBundleRequest {
    pub bundle_id: String,
    pub format: String, // "json", "ndjson", "csv"
}

/// Verify bundle signature request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyBundleSignatureRequest {
    pub bundle_id: String,
    pub expected_signature: String,
}

/// Bundle verification response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BundleVerificationResponse {
    pub bundle_id: String,
    pub verified: bool,
    pub signature_match: bool,
    pub timestamp: String,
}
