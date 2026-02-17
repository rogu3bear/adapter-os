use crate::schema_version;
use serde::{Deserialize, Serialize};

/// Routing rule response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RoutingRuleResponse {
    pub id: String,
    pub identity_dataset_id: String,
    pub condition_logic: String,
    pub target_adapter_id: String,
    pub priority: i64,
    pub created_at: String,
    pub created_by: Option<String>,
}

/// Create routing rule request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CreateRoutingRuleRequest {
    pub identity_dataset_id: String,
    pub condition_logic: String,
    pub target_adapter_id: String,
    pub priority: i64,
}

/// Update routing rule request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UpdateRoutingRuleRequest {
    pub condition_logic: Option<String>,
    pub target_adapter_id: Option<String>,
    pub priority: Option<i64>,
}

/// Routing rules response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct RoutingRulesResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub rules: Vec<RoutingRuleResponse>,
}
