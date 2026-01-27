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
    /// Stack version (auto-incremented on updates)
    pub version: i64,
    /// Determinism mode for this stack (strict, besteffort, relaxed)
    pub determinism_mode: Option<String>,
    /// Routing determinism mode for adapter selection
    pub routing_determinism_mode: Option<String>,
    /// Optional JSON metadata for stack configuration
    pub metadata_json: Option<String>,
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
