//! Policy quarantine API types.
//!
//! Request and response types for policy quarantine management operations.

use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use utoipa::ToSchema;

/// Request to clear policy quarantine violations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(ToSchema))]
pub struct ClearQuarantineRequest {
    /// Optional policy pack ID to clear. If None, clears all violations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<String>,

    /// Optional Control Plane ID scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpid: Option<String>,

    /// If true, reload baseline cache from database before clearing.
    /// This is useful for rollback scenarios.
    #[serde(default)]
    pub rollback: bool,

    /// Operator identity performing the clear operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

/// Response from clearing policy quarantine violations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(ToSchema))]
pub struct ClearQuarantineResponse {
    /// Whether the operation was successful.
    pub success: bool,

    /// List of policy pack IDs that were cleared.
    pub cleared_packs: Vec<String>,

    /// Number of violations that were cleared.
    pub violations_cleared: usize,

    /// Human-readable message describing the result.
    pub message: String,

    /// Whether cache was reloaded (rollback mode).
    #[serde(default)]
    pub cache_reloaded: bool,
}

/// Request to rollback policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(ToSchema))]
pub struct RollbackQuarantineRequest {
    /// Optional Control Plane ID scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpid: Option<String>,

    /// Operator identity performing the rollback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

/// Response from rollback operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(ToSchema))]
pub struct RollbackQuarantineResponse {
    /// Whether the operation was successful.
    pub success: bool,

    /// Number of violations that were cleared.
    pub violations_cleared: usize,

    /// Human-readable message describing the result.
    pub message: String,

    /// Whether the system is still quarantined after rollback.
    pub still_quarantined: bool,
}

/// Current quarantine status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(ToSchema))]
pub struct QuarantineStatusResponse {
    /// Whether the system is currently quarantined.
    pub quarantined: bool,

    /// Violation summary if quarantined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation_summary: Option<String>,

    /// Number of active violations.
    pub violation_count: usize,

    /// Human-readable status message.
    pub message: String,
}
