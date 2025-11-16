// !(Dashboard configuration types for per-user widget customization

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Dashboard widget configuration for a single widget
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardWidgetConfig {
    pub id: String,
    pub user_id: String,
    pub widget_id: String,
    pub enabled: bool,
    pub position: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Request to get dashboard configuration for the current user
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetDashboardConfigRequest {
    // Currently empty - uses authenticated user from JWT
    // Future: could add filtering options
}

/// Response containing all widget configurations for a user
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetDashboardConfigResponse {
    pub widgets: Vec<DashboardWidgetConfig>,
}

/// Single widget configuration update
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WidgetConfigUpdate {
    pub widget_id: String,
    pub enabled: bool,
    pub position: i32,
}

/// Request to update dashboard widget configurations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateDashboardConfigRequest {
    pub widgets: Vec<WidgetConfigUpdate>,
}

/// Response after updating dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateDashboardConfigResponse {
    pub success: bool,
    pub updated_count: usize,
}

/// Request to reset dashboard to role defaults
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResetDashboardConfigRequest {
    // Currently empty - uses authenticated user from JWT
}

/// Response after resetting dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResetDashboardConfigResponse {
    pub success: bool,
    pub message: String,
}
