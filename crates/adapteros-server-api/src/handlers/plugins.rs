use crate::handlers::{require_any_role, AppState, Claims, ErrorResponse};
use crate::state::AppState;
use adapteros_db::users::Role;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;
use tracing::info;
use adapteros_db::Db; // assume

pub async fn enable_plugin(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();
    state.plugin_registry.enable_for_tenant(&name, &tenant_id, true).await
        .map_err(|e| {
            info!("Failed to enable plugin {} for tenant {}: {}", name, tenant_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string()).with_code("PLUGIN_ENABLE_FAILED"))
            )
        })?;

    info!(plugin = %name, tenant = %tenant_id, action = "enable", "Plugin state changed");
    Ok(Json(json!({ "status": "enabled", "plugin": name, "tenant": tenant_id })))
}

pub async fn disable_plugin(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();
    state.plugin_registry.enable_for_tenant(&name, &tenant_id, false).await
        .map_err(|e| {
            info!("Failed to disable plugin {} for tenant {}: {}", name, tenant_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string()).with_code("PLUGIN_DISABLE_FAILED"))
            )
        })?;

    info!(plugin = %name, tenant = %tenant_id, action = "disable", "Plugin state changed");
    Ok(Json(json!({ "status": "disabled", "plugin": name, "tenant": tenant_id })))
}

pub async fn plugin_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Viewer role or above
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;

    let tenant_id = claims.tenant_id.clone();
    let enabled = state.plugin_registry.is_enabled_for_tenant(&name, &tenant_id).await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string()).with_code("PLUGIN_STATUS_FAILED"))
            )
        })?;

    let health = state.plugin_registry.health_all().await.get(&name).cloned().unwrap_or_else(|| {
        adapteros_core::PluginHealth {
            status: adapteros_core::PluginStatus::Stopped,
            details: None,
        }
    });

    Ok(Json(json!({
        "plugin": name,
        "tenant": tenant_id,
        "enabled": enabled,
        "health": health
    })))
}

pub async fn list_plugins(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Viewer, Role::Operator, Role::Admin])?;

    let health = state.plugin_registry.health_all().await;
    let tenants = state.db.list_tenants().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string()).with_code("DB_ERROR"))
        )
    })?;

    let mut plugins_list = vec![];

    for tenant in tenants {
        for (name, h) in &health {
            let enabled = state.plugin_registry.is_enabled_for_tenant(name, &tenant.id).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(&e.to_string()).with_code("PLUGIN_CHECK_FAILED"))
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

    Ok(Json(json!({ "plugins": plugins_list })))
}
