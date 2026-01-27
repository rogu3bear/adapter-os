//! Plugin management handlers
//!
//! Provides REST endpoints for plugin enable/disable and status operations.

use crate::auth::AdminClaims;
use crate::middleware::require_any_role;
use crate::state::AdminAppState;
use crate::types::AdminErrorResponse;
use adapteros_core::{PluginHealth, PluginStatus};
use adapteros_db::users::Role;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;
use tracing::info;

/// Enable a plugin for a tenant
#[utoipa::path(
    post,
    path = "/v1/plugins/{name}/enable",
    params(
        ("name" = String, Path, description = "Plugin name")
    ),
    responses(
        (status = 200, description = "Plugin enabled"),
        (status = 500, description = "Failed to enable plugin", body = AdminErrorResponse)
    ),
    tag = "plugins"
)]
pub async fn enable_plugin<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<AdminErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();
    state
        .plugin_registry()
        .enable_for_tenant(&name, &tenant_id, true)
        .await
        .map_err(|e| {
            info!(
                "Failed to enable plugin {} for tenant {}: {}",
                name, tenant_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_ENABLE_FAILED")),
            )
        })?;

    info!(plugin = %name, tenant = %tenant_id, action = "enable", "Plugin state changed");
    Ok(Json(
        json!({ "status": "enabled", "plugin": name, "tenant": tenant_id }),
    ))
}

/// Disable a plugin for a tenant
#[utoipa::path(
    post,
    path = "/v1/plugins/{name}/disable",
    params(
        ("name" = String, Path, description = "Plugin name")
    ),
    responses(
        (status = 200, description = "Plugin disabled"),
        (status = 500, description = "Failed to disable plugin", body = AdminErrorResponse)
    ),
    tag = "plugins"
)]
pub async fn disable_plugin<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<AdminErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();
    state
        .plugin_registry()
        .enable_for_tenant(&name, &tenant_id, false)
        .await
        .map_err(|e| {
            info!(
                "Failed to disable plugin {} for tenant {}: {}",
                name, tenant_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_DISABLE_FAILED")),
            )
        })?;

    info!(plugin = %name, tenant = %tenant_id, action = "disable", "Plugin state changed");
    Ok(Json(
        json!({ "status": "disabled", "plugin": name, "tenant": tenant_id }),
    ))
}

/// Get plugin status for a tenant
#[utoipa::path(
    get,
    path = "/v1/plugins/{name}",
    params(
        ("name" = String, Path, description = "Plugin name")
    ),
    responses(
        (status = 200, description = "Plugin status"),
        (status = 500, description = "Failed to get plugin status", body = AdminErrorResponse)
    ),
    tag = "plugins"
)]
pub async fn plugin_status<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<AdminErrorResponse>)> {
    // Viewer role or above
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;

    let tenant_id = claims.tenant_id.clone();
    let enabled = state
        .plugin_registry()
        .is_enabled_for_tenant(&name, &tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_STATUS_FAILED")),
            )
        })?;

    let health = state
        .plugin_registry()
        .health_all()
        .await
        .get(&tenant_id)
        .and_then(|tenant_health| tenant_health.get(&name).cloned())
        .unwrap_or(PluginHealth {
            status: PluginStatus::Stopped,
            details: None,
        });

    Ok(Json(json!({
        "plugin": name,
        "tenant": tenant_id,
        "enabled": enabled,
        "health": health
    })))
}

/// List all plugins and their status
#[utoipa::path(
    get,
    path = "/v1/plugins",
    responses(
        (status = 200, description = "List of plugins"),
        (status = 500, description = "Failed to list plugins", body = AdminErrorResponse)
    ),
    tag = "plugins"
)]
pub async fn list_plugins<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<AdminErrorResponse>)> {
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;

    let health_map = state.plugin_registry().health_all().await;
    let tenants = state.db().list_tenants().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::new(e.to_string()).with_code("DB_ERROR")),
        )
    })?;

    let mut plugins_list = vec![];

    for tenant in tenants {
        if let Some(tenant_health) = health_map.get(&tenant.id) {
            for (name, h) in tenant_health {
                let enabled = state
                    .plugin_registry()
                    .is_enabled_for_tenant(name, &tenant.id)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                AdminErrorResponse::new(e.to_string()).with_code("PLUGIN_CHECK_FAILED"),
                            ),
                        )
                    })?;

                plugins_list.push(json!({
                    "plugin": name,
                    "tenant": tenant.id,
                    "enabled": enabled,
                    "health": h
                }));
            }
        }
    }

    Ok(Json(json!({ "plugins": plugins_list })))
}
