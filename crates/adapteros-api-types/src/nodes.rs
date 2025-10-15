//! Node management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Register node request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterNodeRequest {
    pub hostname: String,
    pub agent_endpoint: String,
}

/// Node response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeResponse {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
}

/// Node ping response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodePingResponse {
    pub node_id: String,
    pub status: String,
    pub latency_ms: f64,
}

/// Worker info for node details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerInfo {
    pub id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub status: String,
}

/// Node details response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NodeDetailsResponse {
    pub id: String,
    pub hostname: String,
    pub agent_endpoint: String,
    pub status: String,
    pub last_seen_at: Option<String>,
    pub workers: Vec<WorkerInfo>,
    pub recent_logs: Vec<String>,
}
