//! Settings management handlers
//!
//! Provides REST endpoints for system settings management.

use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::middleware::require_role;
use crate::runtime_config_store;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::users::Role;
use axum::{extract::Extension, extract::State, http::StatusCode, response::Json};

/// Get current system settings
#[utoipa::path(
    get,
    path = "/v1/settings",
    responses(
        (status = 200, description = "Current system settings", body = SystemSettings),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "settings"
)]
pub async fn get_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemSettings>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Load current configuration from AppState and drop lock before await.
    let mut settings = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        runtime_config_store::settings_from_api_config(&config)
    };

    let loaded = runtime_config_store::load_runtime_config(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to load runtime config: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    if let Some(loaded) = loaded {
        settings.effective_source = Some(loaded.effective_source);
        settings.applied_at = Some(loaded.document.updated_at);
        settings.pending_restart_fields = loaded.document.pending_restart_fields;
    }

    settings.restart_required_fields = vec![
        "server.http_port".to_string(),
        "server.https_port".to_string(),
        "server.uds_socket_path".to_string(),
        "server.production_mode".to_string(),
        "security.jwt_mode".to_string(),
        "security.token_ttl_seconds".to_string(),
        "security.require_mfa".to_string(),
        "security.require_pf_deny".to_string(),
    ];

    Ok(Json(settings))
}

/// Get effective settings with source metadata for managed keys.
#[utoipa::path(
    get,
    path = "/v1/settings/effective",
    responses(
        (status = 200, description = "Effective settings with source metadata", body = EffectiveSettingsResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "settings"
)]
pub async fn get_effective_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<EffectiveSettingsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let settings = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        runtime_config_store::settings_from_api_config(&config)
    };

    let loaded = runtime_config_store::load_runtime_config(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to load runtime config: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(
        runtime_config_store::build_effective_settings_response(&settings, loaded.as_ref()),
    ))
}

/// Reconcile runtime settings dual-write state (file + DB).
#[utoipa::path(
    post,
    path = "/v1/settings/reconcile",
    responses(
        (status = 200, description = "Settings reconciliation result", body = SettingsReconcileResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "settings"
)]
pub async fn reconcile_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SettingsReconcileResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let response = runtime_config_store::reconcile_runtime_config(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to reconcile runtime config: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(response))
}

/// Update system settings (admin only)
#[utoipa::path(
    put,
    path = "/v1/settings",
    request_body = UpdateSettingsRequest,
    responses(
        (status = 200, description = "Settings updated", body = SettingsUpdateResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "settings"
)]
pub async fn update_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsUpdateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Validate settings before persisting
    validate_settings_request(&req).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!("Validation error: {}", e))
                    .with_code("VALIDATION_ERROR"),
            ),
        )
    })?;

    let mut restart_required = false;
    let mut updated_sections = vec![];

    if req.general.is_some() {
        updated_sections.push("general");
    }

    if req.server.is_some() {
        updated_sections.push("server");
        restart_required = true; // Server settings require restart
    }

    if req.security.is_some() {
        updated_sections.push("security");
        restart_required = true; // Security settings require restart
    }

    if req.performance.is_some() {
        updated_sections.push("performance");
    }

    let pending_restart_fields = if restart_required {
        let mut fields = Vec::new();
        if req.server.is_some() {
            fields.extend([
                "server.http_port".to_string(),
                "server.https_port".to_string(),
                "server.uds_socket_path".to_string(),
                "server.production_mode".to_string(),
            ]);
        }
        if req.security.is_some() {
            fields.extend([
                "security.jwt_mode".to_string(),
                "security.token_ttl_seconds".to_string(),
                "security.require_mfa".to_string(),
                "security.require_pf_deny".to_string(),
            ]);
        }
        fields
    } else {
        Vec::new()
    };

    let persisted = runtime_config_store::persist_runtime_update(
        &state.db,
        &req,
        pending_restart_fields,
        Some(claims.sub.clone()),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to persist settings: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    let apply_report =
        runtime_config_store::apply_runtime_overrides(&state.config, &req).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to apply live settings: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    // Log successful settings update
    use crate::audit_helper::{actions, log_success_or_warn, resources};
    log_success_or_warn(
        &state.db,
        &claims,
        actions::SETTINGS_UPDATE,
        resources::SETTINGS,
        Some(&updated_sections.join(",")),
        Some(client_ip.0.as_str()),
    )
    .await;

    let message = if restart_required {
        format!(
            "Settings updated: {}. Restart required for queued fields.",
            updated_sections.join(", ")
        )
    } else {
        format!(
            "Settings updated: {}. Changes applied immediately.",
            updated_sections.join(", ")
        )
    };

    Ok(Json(SettingsUpdateResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        success: true,
        restart_required,
        message,
        applied_live: apply_report.applied_live,
        queued_for_restart: apply_report.queued_for_restart,
        rejected: apply_report.rejected,
        effective_source: Some(persisted.effective_source),
        applied_at: Some(persisted.document.updated_at),
        pending_restart_fields: persisted.document.pending_restart_fields,
    }))
}

/// Validate settings request before persisting
fn validate_settings_request(req: &UpdateSettingsRequest) -> Result<(), String> {
    // Validate server settings
    if let Some(ref server) = req.server {
        if server.http_port == 0 {
            return Err("http_port must be greater than 0".to_string());
        }
        if let Some(https_port) = server.https_port {
            if https_port == 0 {
                return Err("https_port must be greater than 0".to_string());
            }
        }
    }

    // Validate security settings
    if let Some(ref security) = req.security {
        if security.jwt_mode != "eddsa" && security.jwt_mode != "hmac" {
            return Err("jwt_mode must be 'eddsa' or 'hmac'".to_string());
        }
        if security.token_ttl_seconds == 0 {
            return Err("token_ttl_seconds must be greater than 0".to_string());
        }
        if security.token_ttl_seconds > 86400 {
            return Err("token_ttl_seconds must be less than 24 hours (86400)".to_string());
        }
    }

    // Validate performance settings
    if let Some(ref perf) = req.performance {
        if perf.max_adapters == 0 {
            return Err("max_adapters must be greater than 0".to_string());
        }
        if perf.max_workers == 0 {
            return Err("max_workers must be greater than 0".to_string());
        }
        if perf.memory_threshold_pct <= 0.0 || perf.memory_threshold_pct > 1.0 {
            return Err("memory_threshold_pct must be between 0.0 and 1.0".to_string());
        }
        if perf.cache_size_mb == 0 {
            return Err("cache_size_mb must be greater than 0".to_string());
        }
    }

    Ok(())
}
