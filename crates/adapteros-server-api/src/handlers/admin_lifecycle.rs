//! Admin lifecycle endpoints for maintenance/drain/shutdown control.

use crate::auth::Claims;
use crate::boot_state::BootState;
use crate::middleware::require_any_role;
use crate::runtime_mode::RuntimeMode;
use crate::state::AppState;
use crate::uds_client::UdsClient;
use adapteros_api_types::ErrorResponse;
use adapteros_db::users::Role;
use axum::{extract::State, http::StatusCode, Extension, Json};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tracing::{error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShutdownMode {
    Drain,
    Immediate,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MaintenanceScope {
    ControlPlane,
    Worker,
    All,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestShutdownBody {
    pub reason: String,
    pub mode: ShutdownMode,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestMaintenanceBody {
    pub reason: String,
    pub scope: MaintenanceScope,
}

#[utoipa::path(
    post,
    path = "/admin/lifecycle/request-shutdown",
    request_body = RequestShutdownBody,
    responses(
        (status = 200, description = "Shutdown accepted"),
        (status = 500, description = "Boot state unavailable")
    ),
    tag = "admin"
)]
pub async fn request_shutdown(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<RequestShutdownBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin])?;
    enforce_prod_wildcard_gate(&claims, &state.runtime_mode)?;

    let Some(boot_state) = state.boot_state.clone() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json_error("boot state unavailable")),
        ));
    };

    let tracking_id = Uuid::now_v7().to_string();
    match body.mode {
        ShutdownMode::Drain => {
            boot_state.drain().await;
            info!(
                actor = %claims.sub,
                tracking_id = %tracking_id,
                reason = %body.reason,
                "Admin drain requested"
            );
        }
        ShutdownMode::Immediate => {
            boot_state.drain().await;
            boot_state.stop().await;
            warn!(
                actor = %claims.sub,
                tracking_id = %tracking_id,
                reason = %body.reason,
                "Admin immediate shutdown requested"
            );
        }
    }

    Ok(Json(serde_json::json!({
        "accepted": true,
        "lifecycle": map_boot_state(&boot_state.current_state()),
        "message": body.reason,
        "tracking_id": tracking_id
    })))
}

#[utoipa::path(
    post,
    path = "/admin/lifecycle/request-maintenance",
    request_body = RequestMaintenanceBody,
    responses(
        (status = 200, description = "Maintenance accepted"),
        (status = 500, description = "Boot state unavailable")
    ),
    tag = "admin"
)]
pub async fn request_maintenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<RequestMaintenanceBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin])?;
    enforce_prod_wildcard_gate(&claims, &state.runtime_mode)?;

    let Some(boot_state) = state.boot_state.clone() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json_error("boot state unavailable")),
        ));
    };

    let tracking_id = Uuid::now_v7().to_string();
    let mut worker_results: Vec<WorkerMaintenanceResult> = Vec::new();

    match body.scope {
        MaintenanceScope::ControlPlane => {
            boot_state.maintenance(&body.reason).await;
            info!(
                actor = %claims.sub,
                tracking_id = %tracking_id,
                reason = %body.reason,
                "Control plane maintenance requested"
            );
        }
        MaintenanceScope::Worker | MaintenanceScope::All => {
            // For All scope, also set control plane to maintenance
            if matches!(body.scope, MaintenanceScope::All) {
                boot_state.maintenance(&body.reason).await;
            }

            // Signal workers to enter maintenance mode
            worker_results = signal_workers_maintenance(&state, &body.reason, &claims.sub).await;

            let successful = worker_results.iter().filter(|r| r.success).count();
            let total = worker_results.len();

            if total == 0 {
                info!(
                    actor = %claims.sub,
                    tracking_id = %tracking_id,
                    reason = %body.reason,
                    "Worker maintenance requested but no workers found"
                );
            } else {
                info!(
                    actor = %claims.sub,
                    tracking_id = %tracking_id,
                    reason = %body.reason,
                    successful = successful,
                    total = total,
                    "Worker maintenance signaling completed"
                );
            }
        }
    }

    Ok(Json(serde_json::json!({
        "accepted": true,
        "scope": format!("{:?}", body.scope).to_lowercase(),
        "lifecycle": map_boot_state(&boot_state.current_state()),
        "message": body.reason,
        "tracking_id": tracking_id,
        "workers": worker_results
    })))
}

#[utoipa::path(
    post,
    path = "/admin/lifecycle/safe-restart",
    responses(
        (status = 200, description = "Safe restart initiated"),
        (status = 500, description = "Boot state unavailable")
    ),
    tag = "admin"
)]
pub async fn safe_restart(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin])?;
    enforce_prod_wildcard_gate(&claims, &state.runtime_mode)?;

    let Some(boot_state) = state.boot_state.clone() else {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json_error("boot state unavailable")),
        ));
    };

    // Mark maintenance and begin drain for restart
    boot_state.maintenance("safe-restart").await;
    boot_state.drain().await;

    Ok(Json(serde_json::json!({
        "accepted": true,
        "mode": "safe-restart",
        "restart_executed": false,
        "restart_delegated": true,
        "message": "Drain initiated; external supervisor should restart when safe",
        "lifecycle": map_boot_state(&boot_state.current_state())
    })))
}

fn map_boot_state(state: &BootState) -> String {
    match state {
        BootState::Stopped => "stopped",
        // All booting states (new granular states + legacy aliases)
        BootState::Starting
        | BootState::DbConnecting
        | BootState::Migrating
        | BootState::Seeding
        | BootState::LoadingPolicies
        | BootState::StartingBackend
        | BootState::LoadingBaseModels
        | BootState::LoadingAdapters
        | BootState::WorkerDiscovery => "booting",
        BootState::Ready | BootState::FullyReady => "ready",
        BootState::Degraded => "degraded",
        BootState::Failed => "failed",
        BootState::Maintenance => "maintenance",
        BootState::Draining => "draining",
        BootState::Stopping => "stopping",
    }
    .to_string()
}

fn enforce_prod_wildcard_gate(
    claims: &Claims,
    mode: &Option<RuntimeMode>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if !matches!(mode, Some(RuntimeMode::Prod)) {
        return Ok(());
    }
    let allow_env = std::env::var("AOS_ALLOW_WILDCARD_ADMIN_PROD")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if !allow_env && claims.admin_tenants.iter().any(|t| t == "*") {
        error!(
            "Wildcard admin_tenants is not allowed in prod without AOS_ALLOW_WILDCARD_ADMIN_PROD=1"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(json_error("Wildcard admin_tenants forbidden in prod")),
        ));
    }
    Ok(())
}

fn json_error(msg: &str) -> ErrorResponse {
    ErrorResponse::new("lifecycle")
        .with_code("LIFECYCLE_ERROR")
        .with_string_details(msg)
}

/// Result of signaling maintenance to a worker
#[derive(Debug, Clone, Serialize)]
pub struct WorkerMaintenanceResult {
    pub worker_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub mode: Option<String>,
}

/// Signal all active workers to enter maintenance mode
///
/// Iterates through all active workers in the database and sends a maintenance
/// signal to each via their UDS endpoint. This is a best-effort operation;
/// failures for individual workers are logged but do not stop signaling other workers.
async fn signal_workers_maintenance(
    state: &AppState,
    reason: &str,
    actor: &str,
) -> Vec<WorkerMaintenanceResult> {
    let mut results = Vec::new();

    // Get all active workers from the database
    let workers = match state.db.list_active_workers().await {
        Ok(workers) => workers,
        Err(e) => {
            error!(
                error = %e,
                actor = %actor,
                "Failed to list active workers for maintenance signaling"
            );
            return results;
        }
    };

    if workers.is_empty() {
        info!(actor = %actor, "No active workers found for maintenance signaling");
        return results;
    }

    // Signal each worker in parallel using futures
    // Each future creates its own UDS client since we need separate connections
    let signal_futures: Vec<_> = workers
        .iter()
        .map(|worker| {
            let worker_id = worker.id.clone();
            let uds_path = worker.uds_path.clone();
            let reason = reason.to_string();

            async move {
                let path = Path::new(&uds_path);
                // Create a UDS client with a reasonable timeout for maintenance signals
                let client = UdsClient::new(Duration::from_secs(10));

                match client
                    .signal_maintenance(path, "drain", Some(&reason))
                    .await
                {
                    Ok(response) => {
                        info!(
                            worker_id = %worker_id,
                            mode = %response.mode,
                            drain_flag_set = response.drain_flag_set,
                            "Worker maintenance signal accepted"
                        );
                        WorkerMaintenanceResult {
                            worker_id,
                            success: true,
                            error: None,
                            mode: Some(response.mode),
                        }
                    }
                    Err(e) => {
                        warn!(
                            worker_id = %worker_id,
                            uds_path = %uds_path,
                            error = %e,
                            "Failed to signal worker maintenance"
                        );
                        WorkerMaintenanceResult {
                            worker_id,
                            success: false,
                            error: Some(e.to_string()),
                            mode: None,
                        }
                    }
                }
            }
        })
        .collect();

    // Execute all signals concurrently
    results = join_all(signal_futures).await;

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_boot_state_maps_public_lifecycle() {
        assert_eq!(map_boot_state(&BootState::Maintenance), "maintenance");
        assert_eq!(map_boot_state(&BootState::Draining), "draining");
        assert_eq!(map_boot_state(&BootState::Ready), "ready");
        assert_eq!(map_boot_state(&BootState::Stopped), "stopped");
    }

    #[test]
    fn worker_maintenance_result_serializes_correctly() {
        let result = WorkerMaintenanceResult {
            worker_id: "worker-123".to_string(),
            success: true,
            error: None,
            mode: Some("drain".to_string()),
        };

        let json = serde_json::to_value(&result).expect("should serialize");
        assert_eq!(json["worker_id"], "worker-123");
        assert_eq!(json["success"], true);
        assert!(json["error"].is_null());
        assert_eq!(json["mode"], "drain");
    }

    #[test]
    fn worker_maintenance_result_with_error_serializes_correctly() {
        let result = WorkerMaintenanceResult {
            worker_id: "worker-456".to_string(),
            success: false,
            error: Some("Connection refused".to_string()),
            mode: None,
        };

        let json = serde_json::to_value(&result).expect("should serialize");
        assert_eq!(json["worker_id"], "worker-456");
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "Connection refused");
        assert!(json["mode"].is_null());
    }
}
