//! Settings management handlers
//!
//! Provides REST endpoints for system settings management.

use crate::auth::AdminClaims;
use crate::middleware::require_role;
use crate::state::AdminAppState;
use crate::types::AdminErrorResponse;
use adapteros_api_types::settings::{
    GeneralSettings, PerformanceSettings, SecuritySettings, ServerSettings,
    SettingsUpdateResponse, SystemSettings, UpdateSettingsRequest,
};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_core::defaults::DEFAULT_SERVER_URL;
use adapteros_db::users::Role;
use axum::{extract::Extension, extract::State, http::StatusCode, response::Json};

/// Get current system settings
#[utoipa::path(
    get,
    path = "/v1/settings",
    responses(
        (status = 200, description = "Current system settings", body = SystemSettings),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    ),
    tag = "settings"
)]
pub async fn get_settings<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
) -> Result<Json<SystemSettings>, (StatusCode, Json<AdminErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Load current configuration from AppState
    let config = state.config().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::new("Configuration lock poisoned").with_code("INTERNAL_ERROR")),
        )
    })?;

    let settings = SystemSettings {
        schema_version: API_SCHEMA_VERSION.to_string(),
        general: GeneralSettings {
            system_name: config
                .general
                .as_ref()
                .and_then(|g| g.system_name.clone())
                .unwrap_or_else(|| "adapterOS".to_string()),
            environment: config
                .general
                .as_ref()
                .and_then(|g| g.environment.clone())
                .unwrap_or_else(|| "production".to_string()),
            api_base_url: config
                .general
                .as_ref()
                .and_then(|g| g.api_base_url.clone())
                .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()),
        },
        server: ServerSettings {
            http_port: config.server.http_port.unwrap_or(8080),
            https_port: config.server.https_port,
            uds_socket_path: config.server.uds_socket.clone(),
            production_mode: config.server.production_mode,
        },
        security: SecuritySettings {
            jwt_mode: config
                .security
                .jwt_mode
                .clone()
                .unwrap_or_else(|| "eddsa".to_string()),
            token_ttl_seconds: config.security.token_ttl_seconds.unwrap_or(28800) as u32, // 8 hours
            require_mfa: config.security.require_mfa.unwrap_or(false),
            egress_enabled: !config.security.require_pf_deny,
            require_pf_deny: config.security.require_pf_deny,
        },
        performance: PerformanceSettings {
            max_adapters: config.performance.max_adapters.unwrap_or(100) as u32,
            max_workers: config.performance.max_workers.unwrap_or(10) as u32,
            memory_threshold_pct: config.performance.memory_threshold_pct.unwrap_or(0.85),
            cache_size_mb: config.performance.cache_size_mb.unwrap_or(1024) as u64,
        },
    };

    Ok(Json(settings))
}

/// Update system settings (admin only)
#[utoipa::path(
    put,
    path = "/v1/settings",
    request_body = UpdateSettingsRequest,
    responses(
        (status = 200, description = "Settings updated", body = SettingsUpdateResponse),
        (status = 400, description = "Validation error", body = AdminErrorResponse),
        (status = 500, description = "Internal server error", body = AdminErrorResponse)
    ),
    tag = "settings"
)]
pub async fn update_settings<S: AdminAppState>(
    State(state): State<S>,
    Extension(claims): Extension<AdminClaims>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsUpdateResponse>, (StatusCode, Json<AdminErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Validate settings before persisting
    validate_settings_request(&req).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                AdminErrorResponse::new(format!("Validation error: {}", e))
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

    // Persist settings to override file
    if let Err(e) = persist_settings_override(&req).await {
        // Log the failure
        state.log_audit_failure(
            &claims,
            "settings.update",
            "settings",
            None,
            &format!("Failed to persist settings: {}", e),
        ).await;

        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                AdminErrorResponse::new(format!("Failed to persist settings: {}", e))
                    .with_code("INTERNAL_ERROR"),
            ),
        ));
    }

    // Log successful settings update
    state.log_audit_success(
        &claims,
        "settings.update",
        "settings",
        Some(&updated_sections.join(",")),
    ).await;

    let message = if restart_required {
        format!(
            "Settings updated: {}. Restart required for changes to take effect.",
            updated_sections.join(", ")
        )
    } else {
        format!(
            "Settings updated: {}. Changes applied immediately.",
            updated_sections.join(", ")
        )
    };

    Ok(Json(SettingsUpdateResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        success: true,
        restart_required,
        message,
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

/// Persist settings to override file
async fn persist_settings_override(req: &UpdateSettingsRequest) -> Result<(), std::io::Error> {
    use std::fs;
    use std::path::Path;

    let override_path = "var/settings_override.json";
    let override_dir = Path::new("var");

    // Ensure var directory exists
    if !override_dir.exists() {
        fs::create_dir_all(override_dir)?;
    }

    // Read existing overrides if present
    let mut existing: UpdateSettingsRequest = if Path::new(override_path).exists() {
        let content = fs::read_to_string(override_path)?;
        serde_json::from_str(&content).unwrap_or(UpdateSettingsRequest {
            general: None,
            server: None,
            security: None,
            performance: None,
        })
    } else {
        UpdateSettingsRequest {
            general: None,
            server: None,
            security: None,
            performance: None,
        }
    };

    // Merge new settings with existing (new settings override existing)
    if req.general.is_some() {
        existing.general = req.general.clone();
    }
    if req.server.is_some() {
        existing.server = req.server.clone();
    }
    if req.security.is_some() {
        existing.security = req.security.clone();
    }
    if req.performance.is_some() {
        existing.performance = req.performance.clone();
    }

    // Write merged settings to file
    let json = serde_json::to_string_pretty(&existing)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    fs::write(override_path, json)?;

    tracing::info!(
        override_path = override_path,
        "Settings override file written successfully"
    );

    Ok(())
}
