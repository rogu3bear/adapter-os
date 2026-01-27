//! Activity event types.

use serde::{Deserialize, Serialize};

/// Request body for creating an activity event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CreateActivityEventRequest {
    /// Optional workspace ID to associate with the event
    pub workspace_id: Option<String>,
    /// Type of event (e.g., "adapter_loaded", "training_started", "inference_completed")
    pub event_type: String,
    /// Optional target resource type (e.g., "adapter", "model", "training_job")
    pub target_type: Option<String>,
    /// Optional target resource ID
    pub target_id: Option<String>,
    /// Optional JSON metadata for additional event context
    pub metadata_json: Option<String>,
}

/// Query parameters for listing activity events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[serde(rename_all = "snake_case")]
pub struct ListActivityEventsParams {
    /// Filter by workspace ID
    pub workspace_id: Option<String>,
    /// Filter by user ID
    pub user_id: Option<String>,
    /// Filter by tenant ID (defaults to authenticated user's tenant)
    pub tenant_id: Option<String>,
    /// Filter by event type
    pub event_type: Option<String>,
    /// Maximum number of events to return (default: 50)
    pub limit: Option<i64>,
    /// Number of events to skip (default: 0)
    pub offset: Option<i64>,
}

/// Query parameters for activity feed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::IntoParams))]
#[serde(rename_all = "snake_case")]
pub struct ActivityFeedParams {
    /// Maximum number of events to return (default: 50)
    pub limit: Option<i64>,
}

/// Activity event response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ActivityEventResponse {
    /// Unique event ID
    pub id: String,
    /// Workspace ID if event is workspace-scoped
    pub workspace_id: Option<String>,
    /// User who triggered the event
    pub user_id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Type of event
    pub event_type: String,
    /// Target resource type
    pub target_type: Option<String>,
    /// Target resource ID
    pub target_id: Option<String>,
    /// Additional JSON metadata
    pub metadata_json: Option<String>,
    /// ISO 8601 timestamp when event was created
    pub created_at: String,
}
