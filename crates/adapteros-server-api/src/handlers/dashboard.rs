//! Dashboard configuration handlers
//!
//! Provides API endpoints for per-user dashboard widget customization.
//! Supports show/hide widgets, custom ordering, and reset to role defaults.

use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::handlers::{AppState, Claims, ErrorResponse};
use crate::ip_extraction::ClientIp;
use crate::permissions::{require_permission, Permission};
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
#[utoipa::path(
    get,
    path = "/v1/dashboard/config",
    responses(
        (status = 200, description = "Dashboard configuration", body = adapteros_api_types::dashboard::GetDashboardConfigResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "dashboard"
)]
pub async fn get_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<GetDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DashboardView)?;

    let user_id = &claims.sub;

    let widgets = state.db.get_dashboard_config(user_id).await.map_err(|e| {
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
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        widgets: response_widgets,
    }))
}

/// Update dashboard configuration for the authenticated user
///
/// Accepts a list of widget configurations and updates them in a single transaction.
/// This allows the client to send the entire dashboard state at once.
#[utoipa::path(
    put,
    path = "/v1/dashboard/config",
    request_body = adapteros_api_types::dashboard::UpdateDashboardConfigRequest,
    responses(
        (status = 200, description = "Dashboard configuration updated", body = adapteros_api_types::dashboard::UpdateDashboardConfigResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "dashboard"
)]
pub async fn update_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<UpdateDashboardConfigRequest>,
) -> Result<Json<UpdateDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DashboardManage)?;

    let user_id = &claims.sub;

    if req.widgets.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("No widget configurations provided").with_code("BAD_REQUEST")),
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
            error!(
                "Failed to update dashboard config for user {}: {}",
                user_id, e
            );
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::DASHBOARD_CONFIG_UPDATE,
        resources::DASHBOARD_CONFIG,
        Some(&claims.sub),
        Some(client_ip.0.as_str()),
    )
    .await;

    Ok(Json(UpdateDashboardConfigResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        success: true,
        updated_count,
    }))
}

/// Reset dashboard configuration to role defaults
///
/// Deletes all custom widget configurations for the user, allowing the client
/// to revert to role-based defaults.
#[utoipa::path(
    post,
    path = "/v1/dashboard/config/reset",
    responses(
        (status = 200, description = "Dashboard configuration reset", body = adapteros_api_types::dashboard::ResetDashboardConfigResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "dashboard"
)]
pub async fn reset_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
) -> Result<Json<ResetDashboardConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DashboardManage)?;

    let user_id = &claims.sub;

    state
        .db
        .reset_dashboard_config(user_id)
        .await
        .map_err(|e| {
            error!(
                "Failed to reset dashboard config for user {}: {}",
                user_id, e
            );
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

    log_success_or_warn(
        &state.db,
        &claims,
        actions::DASHBOARD_CONFIG_RESET,
        resources::DASHBOARD_CONFIG,
        Some(&claims.sub),
        Some(client_ip.0.as_str()),
    )
    .await;

    Ok(Json(ResetDashboardConfigResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        success: true,
        message: "Dashboard configuration reset to defaults".to_string(),
    }))
}
