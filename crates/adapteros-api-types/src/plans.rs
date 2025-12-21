//! Plan management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Build plan request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BuildPlanRequest {
    pub tenant_id: String,
    pub manifest_hash_b3: String,
}

/// Plan response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PlanResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub layout_hash_b3: Option<String>,
    pub status: String,
    pub created_at: String,
}

/// Plan details response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PlanDetailsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub tenant_id: String,
    pub manifest_hash_b3: String,
    pub kernel_hash_b3: Option<String>,
    pub routing_config: serde_json::Value,
    pub created_at: String,
}

/// Plan rebuild response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PlanRebuildResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub old_plan_id: String,
    pub new_plan_id: String,
    pub diff_summary: String,
    pub timestamp: String,
}

/// Compare plans request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ComparePlansRequest {
    pub plan_id_1: String,
    pub plan_id_2: String,
}

/// Plan comparison response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PlanComparisonResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub plan_id_1: String,
    pub plan_id_2: String,
    pub differences: Vec<String>,
    pub identical: bool,
}
