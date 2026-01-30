//! Adapter stack domain types

use serde::{Deserialize, Serialize};

/// Adapter stack record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StackRecord {
    /// Unique stack identifier
    pub id: String,
    /// Tenant this stack belongs to
    pub tenant_id: String,
    /// Human-readable stack name
    pub name: String,
    /// Optional stack description
    pub description: Option<String>,
    /// JSON-encoded list of adapter IDs in this stack
    pub adapter_ids_json: String,
    /// Workflow type (e.g., sequential, parallel)
    pub workflow_type: Option<String>,
    /// Current lifecycle state (active, deprecated, archived)
    pub lifecycle_state: String,
    /// Creation timestamp
    pub created_at: String,
    /// Last update timestamp
    pub updated_at: String,
    /// User or system that created the stack
    pub created_by: Option<String>,
    /// Stack version (semantic version string, e.g., "1.0.0")
    pub version: String,
    /// Determinism mode for this stack (strict, besteffort, relaxed)
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for adapter selection
    pub routing_determinism_mode: Option<String>,
    /// Optional JSON metadata for stack configuration
    pub metadata_json: Option<String>,
}

impl StackRecord {
    /// Returns the major version number as i64 for telemetry correlation.
    ///
    /// Parses the semantic version string (e.g., "1.0.0") and returns
    /// the major version number. Falls back to 1 if parsing fails.
    pub fn version_number(&self) -> i64 {
        self.version
            .split('.')
            .next()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1)
    }
}

/// Request to create a new adapter stack
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateStackRequest {
    /// Tenant to create the stack in
    pub tenant_id: String,
    /// Human-readable stack name
    pub name: String,
    /// Optional stack description
    pub description: Option<String>,
    /// List of adapter IDs to include in the stack
    pub adapter_ids: Vec<String>,
    /// Workflow type (e.g., sequential, parallel)
    pub workflow_type: Option<String>,
    /// Determinism mode (strict, besteffort, relaxed)
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for adapter selection
    pub routing_determinism_mode: Option<String>,
}
