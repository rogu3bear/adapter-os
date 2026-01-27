//! Node domain types
//!
//! This module provides the single source of truth for Node-related data structures.

use serde::{Deserialize, Serialize};

/// Core node record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct Node {
    /// Unique identifier for the node
    pub id: String,
    /// Hostname of the node
    pub hostname: String,
    /// Agent endpoint URL
    pub agent_endpoint: String,
    /// Status of the node (active, idle, offline, maintenance)
    pub status: String,
    /// Last seen timestamp (ISO 8601)
    pub last_seen_at: Option<String>,
    /// Optional labels for the node (JSON string)
    pub labels_json: Option<String>,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
}

/// Detailed node information including workers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct NodeDetail {
    /// Base node information
    #[serde(flatten)]
    pub node: Node,
    /// List of workers serving on this node
    pub workers: Vec<String>, // Placeholder for WorkerInfo consolidation
}
