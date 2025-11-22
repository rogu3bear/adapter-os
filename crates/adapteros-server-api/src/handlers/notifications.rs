//! Notification handlers
//!
//! Provides API endpoints for unified notification center (system alerts, messages, mentions, activity).

use crate::audit_helper::{actions, log_success, resources};
use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::permissions::{require_permission, Permission};
use adapteros_db::notifications::NotificationType;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use std::str::FromStr;
use tracing::error;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationResponse {
    pub id: String,
    pub user_id: String,
    pub workspace_id: Option<String>,
    pub type_: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub title: String,
    pub content: Option<String>,
    pub read_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationSummary {
    pub total_count: i64,
    pub unread_count: i64,
}

/// List user notifications
pub async fn list_notifications(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<NotificationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::NotificationView)?;

    let workspace_id = params.get("workspace_id").map(|s| s.as_str());
    let type_filter = params
        .get("type")
        .and_then(|s| NotificationType::from_str(s).ok());
    let unread_only = params
        .get("unread_only")
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(false);
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .or(Some(50));
    let offset = params
        .get("offset")
        .and_then(|s| s.parse::<i64>().ok())
        .or(Some(0));

    let notifications = state
        .db
        .list_user_notifications(
            &claims.sub,
            workspace_id,
            type_filter,
            unread_only,
            limit,
            offset,
        )
        .await
        .map_err(|e| {
            error!("Failed to list notifications: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list notifications")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<NotificationResponse> = notifications
        .into_iter()
        .map(|n| NotificationResponse {
            id: n.id,
            user_id: n.user_id,
            workspace_id: n.workspace_id,
            type_: n.type_,
            target_type: n.target_type,
            target_id: n.target_id,
            title: n.title,
            content: n.content,
            read_at: n.read_at,
            created_at: n.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Get notification summary (unread count)
pub async fn get_notification_summary(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<NotificationSummary>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::NotificationView)?;

    let workspace_id = params.get("workspace_id").map(|s| s.as_str());

    let unread_count = state
        .db
        .get_unread_count(&claims.sub, workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to get unread count: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get notification summary")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    // Get total count (approximate)
    let notifications = state
        .db
        .list_user_notifications(&claims.sub, workspace_id, None, false, Some(1), Some(0))
        .await
        .map_err(|e| {
            error!("Failed to get notification count: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get notification summary")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(NotificationSummary {
        total_count: notifications.len() as i64, // This is approximate, should query COUNT(*) for exact
        unread_count,
    }))
}

/// Mark notification as read
pub async fn mark_notification_read(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(notification_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::NotificationManage)?;

    // Verify notification belongs to user
    let notification = state
        .db
        .get_notification(&notification_id)
        .await
        .map_err(|e| {
            error!("Failed to get notification: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get notification").with_code("INTERNAL_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Notification not found").with_code("NOT_FOUND")),
            )
        })?;

    if notification.user_id != claims.sub {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Can only mark own notifications as read")
                    .with_code("FORBIDDEN"),
            ),
        ));
    }

    state
        .db
        .mark_notification_read(&notification_id)
        .await
        .map_err(|e| {
            error!("Failed to mark notification as read: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to mark notification as read")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::NOTIFICATION_READ,
        resources::NOTIFICATION,
        Some(&notification_id),
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({"status": "read"})))
}

/// Mark all notifications as read
pub async fn mark_all_notifications_read(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::NotificationManage)?;

    let workspace_id = params.get("workspace_id").map(|s| s.as_str());

    let count = state
        .db
        .mark_all_notifications_read(&claims.sub, workspace_id)
        .await
        .map_err(|e| {
            error!("Failed to mark all notifications as read: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to mark all notifications as read")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    log_success(
        &state.db,
        &claims,
        actions::NOTIFICATION_READ_ALL,
        resources::NOTIFICATION,
        workspace_id,
    )
    .await
    .ok();

    Ok(Json(serde_json::json!({"status": "read", "count": count})))
}
