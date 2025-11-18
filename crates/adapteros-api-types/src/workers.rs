//! Worker management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Spawn worker request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SpawnWorkerRequest {
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
}

/// Worker response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: Option<i32>,
    pub status: String,
    pub started_at: String,
    pub last_seen_at: Option<String>,
}

/// Worker status update
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkerStatusUpdate {
    pub worker_id: String,
    pub status: String,
    pub timestamp: String,
}
