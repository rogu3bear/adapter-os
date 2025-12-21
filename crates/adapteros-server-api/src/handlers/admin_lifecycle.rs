//! Admin lifecycle endpoints for maintenance/drain/shutdown control.

use crate::auth::Claims;
use crate::boot_state::BootState;
use crate::middleware::require_any_role;
use crate::runtime_mode::RuntimeMode;
use crate::state::AppState;
use adapteros_api_types::ErrorResponse;
use adapteros_db::users::Role;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Deserialize;
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

    match body.scope {
        MaintenanceScope::ControlPlane | MaintenanceScope::All => {
            boot_state.maintenance(&body.reason).await;
        }
        MaintenanceScope::Worker => {
            // Worker maintenance signaling is best-effort; control plane will surface maintenance flag.
            warn!(
                actor = %claims.sub,
                "Worker maintenance requested; worker signaling not yet implemented"
            );
        }
    }

    Ok(Json(serde_json::json!({
        "accepted": true,
        "scope": format!("{:?}", body.scope).to_lowercase(),
        "lifecycle": map_boot_state(&boot_state.current_state()),
        "message": body.reason
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
        | BootState::WorkerDiscovery
        | BootState::Booting
        | BootState::InitializingDb => "booting",
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
}
