//! Dashboard configuration handlers
//!
//! Provides API endpoints for per-user dashboard widget customization.
//! Supports show/hide widgets, custom ordering, and reset to role defaults.

use crate::handlers::{AppState, Claims, ErrorResponse};
use adapteros_api_types::dashboard::*;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

/// Get dashboard configuration for the authenticated user
///
/// Returns all widget configurations (enabled/disabled, position) for the current user.
/// If no configuration exists, returns an empty list (client should use role defaults).
pub async fn get_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<GetDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = &claims.sub;

    let widgets = state
        .db
        .get_dashboard_config(user_id)
        .await
        .map_err(|e| {
            error!("Failed to get dashboard config for user {}: {}", user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve dashboard configuration")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response_widgets = widgets
        .into_iter()
        .map(|w| DashboardWidgetConfig {
            id: w.id,
            user_id: w.user_id,
            widget_id: w.widget_id,
            enabled: w.enabled,
            position: w.position,
            created_at: w.created_at,
            updated_at: w.updated_at,
        })
        .collect();

    Ok(Json(GetDashboardConfigResponse {
        widgets: response_widgets,
    }))
}

/// Update dashboard configuration for the authenticated user
///
/// Accepts a list of widget configurations and updates them in a single transaction.
/// This allows the client to send the entire dashboard state at once.
pub async fn update_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UpdateDashboardConfigRequest>,
) -> Result<Json<UpdateDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = &claims.sub;

    if req.widgets.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("No widget configurations provided")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Convert request widgets to database format
    let widget_updates: Vec<(String, bool, i32)> = req
        .widgets
        .into_iter()
        .map(|w| (w.widget_id, w.enabled, w.position))
        .collect();

    let updated_count = state
        .db
        .update_dashboard_config(user_id, widget_updates)
        .await
        .map_err(|e| {
            error!("Failed to update dashboard config for user {}: {}", user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update dashboard configuration")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        "Updated {} widget configurations for user {}",
        updated_count, user_id
    );

    Ok(Json(UpdateDashboardConfigResponse {
        success: true,
        updated_count,
    }))
}

/// Reset dashboard configuration to role defaults
///
/// Deletes all custom widget configurations for the user, allowing the client
/// to revert to role-based defaults.
pub async fn reset_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ResetDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = &claims.sub;

    state
        .db
        .reset_dashboard_config(user_id)
        .await
        .map_err(|e| {
            error!("Failed to reset dashboard config for user {}: {}", user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to reset dashboard configuration")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!("Reset dashboard configuration for user {}", user_id);

    Ok(Json(ResetDashboardConfigResponse {
        success: true,
        message: "Dashboard configuration reset to defaults".to_string(),
    }))
}
