//! Activity event handlers
//!
//! Provides API endpoints for activity event creation and feed retrieval.
//! Activity events track user actions and collaboration events.

use crate::handlers::{AppState, Claims, ErrorResponse};
use adapteros_db::activity_events::ActivityEventType;
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::error;

#[derive(Debug, Deserialize)]
pub struct CreateActivityEventRequest {
    pub workspace_id: Option<String>,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActivityEventResponse {
    pub id: String,
    pub workspace_id: Option<String>,
    pub user_id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

/// Create an activity event (internal use)
pub async fn create_activity_event(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateActivityEventRequest>,
) -> Result<Json<ActivityEventResponse>, (StatusCode, Json<ErrorResponse>)> {
    let event_type = ActivityEventType::from_str(&req.event_type).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid event type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let event_id = state
        .db
        .create_activity_event(
            req.workspace_id.as_deref(),
            &claims.sub,
            &claims.tenant_id,
            event_type,
            req.target_type.as_deref(),
            req.target_id.as_deref(),
            req.metadata_json.as_deref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to create activity event: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create activity event")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let event = state
        .db
        .get_activity_event(&event_id)
        .await
        .map_err(|e| {
            error!("Failed to get created activity event: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve created activity event")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Activity event not found after creation")
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    Ok(Json(ActivityEventResponse {
        id: event.id,
        workspace_id: event.workspace_id,
        user_id: event.user_id,
        tenant_id: event.tenant_id,
        event_type: event.event_type,
        target_type: event.target_type,
        target_id: event.target_id,
        metadata_json: event.metadata_json,
        created_at: event.created_at,
    }))
}

/// List activity events
pub async fn list_activity_events(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ActivityEventResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let workspace_id = params.get("workspace_id").map(|s| s.as_str());
    let user_id = params.get("user_id").map(|s| s.as_str());
    let tenant_id = params
        .get("tenant_id")
        .or(Some(&claims.tenant_id))
        .map(|s| s.as_str());
    let event_type = params
        .get("event_type")
        .and_then(|s| ActivityEventType::from_str(s).ok());
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .or(Some(50));
    let offset = params
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .or(Some(0));

    let events = state
        .db
        .list_activity_events(workspace_id, user_id, tenant_id, event_type, limit, offset)
        .await
        .map_err(|e| {
            error!("Failed to list activity events: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list activity events")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<ActivityEventResponse> = events
        .into_iter()
        .map(|e| ActivityEventResponse {
            id: e.id,
            workspace_id: e.workspace_id,
            user_id: e.user_id,
            tenant_id: e.tenant_id,
            event_type: e.event_type,
            target_type: e.target_type,
            target_id: e.target_id,
            metadata_json: e.metadata_json,
            created_at: e.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// List user workspace activity (for activity feed)
pub async fn list_user_workspace_activity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ActivityEventResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Get user's workspace IDs
    let workspaces = state
        .db
        .list_user_workspaces(&claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to list user workspaces: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list user workspaces")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    let workspace_ids: Vec<&str> = workspaces.iter().map(|w| w.id.as_str()).collect();

    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .or(Some(50));

    let events = state
        .db
        .list_user_workspace_activity(&claims.sub, &claims.tenant_id, &workspace_ids, limit)
        .await
        .map_err(|e| {
            error!("Failed to list user workspace activity: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list activity")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<ActivityEventResponse> = events
        .into_iter()
        .map(|e| ActivityEventResponse {
            id: e.id,
            workspace_id: e.workspace_id,
            user_id: e.user_id,
            tenant_id: e.tenant_id,
            event_type: e.event_type,
            target_type: e.target_type,
            target_id: e.target_id,
            metadata_json: e.metadata_json,
            created_at: e.created_at,
        })
        .collect();

    Ok(Json(responses))
}
