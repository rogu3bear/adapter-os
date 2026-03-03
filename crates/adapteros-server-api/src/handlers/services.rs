//! Service control handlers
///
/// Proxy endpoints that forward service control operations to the supervisor API.
/// These handlers provide service start/stop/restart functionality with JWT auth.
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::supervisor_client::SupervisorClient;
use crate::types::ErrorResponse;
use adapteros_core::AosError;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use utoipa::ToSchema;

/// Request to start/stop a service
#[derive(Debug, Deserialize, ToSchema)]
pub struct ServiceControlRequest {
    pub service_id: String,
}

/// Response from service control operations
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceControlResponse {
    pub success: bool,
    pub message: String,
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
        (status = 404, description = "Service not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn start_service(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require NodeManage permission for service control operations
    if require_permission(&claims, Permission::NodeManage).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ));
    }

    info!(service_id = %service_id, user = %claims.sub, "Starting service");

    let client = match SupervisorClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Supervisor client configuration error");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new(format!("Supervisor not configured: {}", e))
                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
                ),
            ));
        }
    };

    match client.start_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service started successfully");
            if let Err(e) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "start",
                "success",
                &message,
            ) {
                warn!(service_id = %service_id, error = %e, "failed to append service action log");
            }
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(AosError::NotFound(msg)) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(msg).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to start service");
            if let Err(log_err) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "start",
                "failed",
                &e.to_string(),
            ) {
                warn!(
                    service_id = %service_id,
                    error = %log_err,
                    "failed to append service action log"
                );
            }
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to start service: {}", e))
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
        (status = 404, description = "Service not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn stop_service(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require NodeManage permission for service control operations
    if require_permission(&claims, Permission::NodeManage).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ));
    }

    info!(service_id = %service_id, user = %claims.sub, "Stopping service");

    let client = match SupervisorClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Supervisor client configuration error");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new(format!("Supervisor not configured: {}", e))
                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
                ),
            ));
        }
    };

    match client.stop_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service stopped successfully");
            if let Err(e) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "stop",
                "success",
                &message,
            ) {
                warn!(service_id = %service_id, error = %e, "failed to append service action log");
            }
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(AosError::NotFound(msg)) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(msg).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to stop service");
            if let Err(log_err) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "stop",
                "failed",
                &e.to_string(),
            ) {
                warn!(
                    service_id = %service_id,
                    error = %log_err,
                    "failed to append service action log"
                );
            }
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to stop service: {}", e))
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
        (status = 404, description = "Service not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn restart_service(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(service_id): Path<String>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require NodeManage permission for service control operations
    if require_permission(&claims, Permission::NodeManage).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ));
    }

    info!(service_id = %service_id, user = %claims.sub, "Restarting service");

    let client = match SupervisorClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Supervisor client configuration error");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new(format!("Supervisor not configured: {}", e))
                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
                ),
            ));
        }
    };

    match client.restart_service(&service_id).await {
        Ok(message) => {
            info!(service_id = %service_id, "Service restarted successfully");
            if let Err(e) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "restart",
                "success",
                &message,
            ) {
                warn!(service_id = %service_id, error = %e, "failed to append service action log");
            }
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(AosError::NotFound(msg)) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(msg).with_code("NOT_FOUND")),
        )),
        Err(e) => {
            error!(service_id = %service_id, error = %e, "Failed to restart service");
            if let Err(log_err) = crate::local_log_service::append_service_action(
                &service_id,
                &claims.sub,
                "restart",
                "failed",
                &e.to_string(),
            ) {
                warn!(
                    service_id = %service_id,
                    error = %log_err,
                    "failed to append service action log"
                );
            }
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to restart service: {}", e))
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
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn start_essential_services(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require NodeManage permission for service control operations
    if require_permission(&claims, Permission::NodeManage).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ));
    }

    info!(user = %claims.sub, "Starting all essential services");

    let client = match SupervisorClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Supervisor client configuration error");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new(format!("Supervisor not configured: {}", e))
                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
                ),
            ));
        }
    };

    match client.start_essential_services().await {
        Ok(message) => {
            info!("Essential services started successfully");
            if let Err(e) = crate::local_log_service::append_service_action(
                "essential",
                &claims.sub,
                "start",
                "success",
                &message,
            ) {
                warn!(error = %e, "failed to append essential service action log");
            }
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!(error = %e, "Failed to start essential services");
            if let Err(log_err) = crate::local_log_service::append_service_action(
                "essential",
                &claims.sub,
                "start",
                "failed",
                &e.to_string(),
            ) {
                warn!(error = %log_err, "failed to append essential service action log");
            }
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to start essential services: {}", e))
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
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn stop_essential_services(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ServiceControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require NodeManage permission for service control operations
    if require_permission(&claims, Permission::NodeManage).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions".to_string()).with_code("FORBIDDEN")),
        ));
    }

    info!(user = %claims.sub, "Stopping all essential services");

    let client = match SupervisorClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Supervisor client configuration error");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new(format!("Supervisor not configured: {}", e))
                        .with_code("SUPERVISOR_NOT_CONFIGURED"),
                ),
            ));
        }
    };

    match client.stop_essential_services().await {
        Ok(message) => {
            info!("Essential services stopped successfully");
            if let Err(e) = crate::local_log_service::append_service_action(
                "essential",
                &claims.sub,
                "stop",
                "success",
                &message,
            ) {
                warn!(error = %e, "failed to append essential service action log");
            }
            Ok(Json(ServiceControlResponse {
                success: true,
                message,
            }))
        }
        Err(e) => {
            error!(error = %e, "Failed to stop essential services");
            if let Err(log_err) = crate::local_log_service::append_service_action(
                "essential",
                &claims.sub,
                "stop",
                "failed",
                &e.to_string(),
            ) {
                warn!(error = %log_err, "failed to append essential service action log");
            }
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to stop essential services: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            ))
        }
    }
}
