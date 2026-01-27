//! Service control handlers
//!
//! Proxy endpoints that forward service control operations to the supervisor API.
//! These handlers provide service start/stop/restart functionality with JWT auth.

use crate::auth::AdminClaims;
use crate::state::{AdminAppState, SupervisorClient};
use crate::types::AdminErrorResponse;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::ToSchema;

/// Request to start/stop a service
#[derive(Debug, Deserialize, ToSchema)]
pub struct ServiceControlRequest {
    /// Service identifier
    pub service_id: String,
}

/// Response from service control operations
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceControlResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Status message
    pub message: String,
}

/// Query parameters for logs endpoint
#[derive(Debug, Deserialize, ToSchema)]
pub struct LogsQuery {
    /// Number of log lines to retrieve
    #[serde(default = "default_log_lines")]
    pub lines: u32,
}

fn default_log_lines() -> u32 {
    100
}

/// Permission required for service control operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Permission to manage nodes/services
    NodeManage,
}

/// Check if claims have the required permission
fn require_permission(
    claims: &AdminClaims,
    _permission: Permission,
) -> Result<(), (StatusCode, Json<AdminErrorResponse>)> {
    // Admin and Operator roles have NodeManage permission
    let role = claims.role.to_lowercase();
    if role == "admin" || role == "operator" {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(AdminErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ))
    }
}

/// Start a service
///
/// POST /v1/services/:service_id/start
#[utoipa::path(
    post,
    path = "/v1/services/{service_id}/start",
    params(
        ("service_id" = String, Path, description = "Service ID to start")
    ),
    responses(
        (status = 200, description = "Service started successfully", body = ServiceControlResponse),
        (status = 404, description = "Service not found", body = AdminErrorResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn start_service<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(service_id = %service_id, user = %claims.sub, "Starting service");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client.start_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service started successfully");
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) if e.is_not_found() => Err((
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::new(e.to_string()).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to start service");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to start service: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}

/// Stop a service
///
/// POST /v1/services/:service_id/stop
#[utoipa::path(
    post,
    path = "/v1/services/{service_id}/stop",
    params(
        ("service_id" = String, Path, description = "Service ID to stop")
    ),
    responses(
        (status = 200, description = "Service stopped successfully", body = ServiceControlResponse),
        (status = 404, description = "Service not found", body = AdminErrorResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn stop_service<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(service_id = %service_id, user = %claims.sub, "Stopping service");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client.stop_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service stopped successfully");
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) if e.is_not_found() => Err((
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::new(e.to_string()).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to stop service");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to stop service: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}

/// Restart a service
///
/// POST /v1/services/:service_id/restart
#[utoipa::path(
    post,
    path = "/v1/services/{service_id}/restart",
    params(
        ("service_id" = String, Path, description = "Service ID to restart")
    ),
    responses(
        (status = 200, description = "Service restarted successfully", body = ServiceControlResponse),
        (status = 404, description = "Service not found", body = AdminErrorResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn restart_service<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(service_id = %service_id, user = %claims.sub, "Restarting service");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client.restart_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service restarted successfully");
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) if e.is_not_found() => Err((
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::new(e.to_string()).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to restart service");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to restart service: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}

/// Start all essential services
///
/// POST /v1/services/essential/start
#[utoipa::path(
    post,
    path = "/v1/services/essential/start",
    responses(
        (status = 200, description = "Essential services started successfully", body = ServiceControlResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn start_essential_services<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(user = %claims.sub, "Starting all essential services");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client.start_essential_services().await {
        Ok(message) => {
            info!("Essential services started successfully");
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!(error = %e, "Failed to start essential services");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to start essential services: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}

/// Stop all essential services
///
/// POST /v1/services/essential/stop
#[utoipa::path(
    post,
    path = "/v1/services/essential/stop",
    responses(
        (status = 200, description = "Essential services stopped successfully", body = ServiceControlResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn stop_essential_services<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(user = %claims.sub, "Stopping all essential services");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client.stop_essential_services().await {
        Ok(message) => {
            info!("Essential services stopped successfully");
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!(error = %e, "Failed to stop essential services");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to stop essential services: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}

/// Get service logs
///
/// GET /v1/services/:service_id/logs?lines=100
#[utoipa::path(
    get,
    path = "/v1/services/{service_id}/logs",
    params(
        ("service_id" = String, Path, description = "Service ID"),
        ("lines" = Option<u32>, Query, description = "Number of log lines to retrieve (default: 100)")
    ),
    responses(
        (status = 200, description = "Service logs retrieved", body = Vec<String>),
        (status = 404, description = "Service not found", body = AdminErrorResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    )
)]
pub async fn get_service_logs<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(service_id): Path<String>,
    Query(params): Query<LogsQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<AdminErrorResponse>)> {
    require_permission(&claims, Permission::NodeManage)?;

    info!(service_id = %service_id, lines = params.lines, user = %claims.sub, "Fetching service logs");

    let client = state.supervisor_client().ok_or_else(|| {
        error!("Supervisor client not configured");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                AdminErrorResponse::new("Supervisor not configured".to_string())
                    .with_code("SUPERVISOR_NOT_CONFIGURED"),
            ),
        )
    })?;

    match client
        .get_service_logs(&service_id, Some(params.lines))
        .await
    {
        Ok(logs) => Ok(Json(logs)),
        Err(e) if e.is_not_found() => Err((
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::new(e.to_string()).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to fetch service logs");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    AdminErrorResponse::new(format!("Failed to fetch logs: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}
