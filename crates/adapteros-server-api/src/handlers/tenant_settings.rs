//! Tenant settings handlers
//!
//! Provides REST endpoints for managing tenant-level settings
//! that control default stack/adapter behavior.
//!
//! Endpoints:
//! - GET /v1/tenants/{tenant_id}/settings - Get tenant settings
//! - PUT /v1/tenants/{tenant_id}/settings - Update tenant settings

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::{DeterminismPolicyKnobs, TenantSettingsResponse, UpdateTenantSettingsRequest};
use adapteros_db::UpdateTenantSettingsParams;
use axum::{extract::Extension, extract::Path, extract::State, http::StatusCode, response::Json};
use tracing::{info, warn};

/// Get tenant settings
///
/// Returns the tenant's settings for controlling default stack/adapter behavior.
/// If no settings are configured, returns defaults (all switches FALSE).
///
/// # Errors
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 500: Internal server error
pub async fn get_tenant_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantSettingsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = crate::id_resolver::resolve_any_id(&state.db, &tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    let settings = state
        .db
        .get_tenant_settings(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    // Load execution policy from tenant_execution_policies
    let determinism_policy = match state.db.get_execution_policy_or_default(&tenant_id).await {
        Ok(policy) => {
            // Convert DeterminismPolicy to DeterminismPolicyKnobs for API response
            Some(DeterminismPolicyKnobs {
                allowed_modes: if policy.determinism.allowed_modes.is_empty() {
                    None
                } else {
                    Some(policy.determinism.allowed_modes)
                },
                pins_outside_effective: policy
                    .routing
                    .as_ref()
                    .map(|r| r.pin_enforcement.clone()),
                fallback_allowed: Some(policy.determinism.allow_fallback),
            })
        }
        Err(e) => {
            warn!(tenant_id = %tenant_id, error = %e, "Failed to load execution policy, returning None");
            None
        }
    };

    // Parse settings_json to serde_json::Value if present
    let settings_json = settings
        .settings_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    // created_at/updated_at are empty strings for defaults, convert to None
    let created_at = if settings.created_at.is_empty() {
        None
    } else {
        Some(settings.created_at)
    };
    let updated_at = if settings.updated_at.is_empty() {
        None
    } else {
        Some(settings.updated_at)
    };

    Ok(Json(TenantSettingsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id: settings.tenant_id,
        use_default_stack_on_chat_create: settings.use_default_stack_on_chat_create,
        use_default_stack_on_infer_session: settings.use_default_stack_on_infer_session,
        settings_json,
        determinism_policy,
        created_at,
        updated_at,
    }))
}

/// Update tenant settings
///
/// Updates the tenant's settings for controlling default stack/adapter behavior.
/// All fields are optional - only provided fields will be updated.
///
/// # Errors
/// - 400: Bad request - invalid settings_json
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 500: Internal server error
pub async fn update_tenant_settings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateTenantSettingsRequest>,
) -> Result<Json<TenantSettingsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = crate::id_resolver::resolve_any_id(&state.db, &tenant_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Convert settings_json Value to String if present
    let settings_json = req
        .settings_json
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Invalid settings_json: {}", e))),
            )
        })?;

    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: req.use_default_stack_on_chat_create,
        use_default_stack_on_infer_session: req.use_default_stack_on_infer_session,
        settings_json,
    };

    let settings = state
        .db
        .upsert_tenant_settings(&tenant_id, params)
        .await
        .map_err(ApiError::db_error)?;

    info!(
        tenant_id = %tenant_id,
        use_default_stack_on_chat_create = %settings.use_default_stack_on_chat_create,
        use_default_stack_on_infer_session = %settings.use_default_stack_on_infer_session,
        "Tenant settings updated"
    );

    // Load execution policy from tenant_execution_policies
    // Note: determinism_policy updates go through execution policy endpoints, not here
    let determinism_policy = match state.db.get_execution_policy_or_default(&tenant_id).await {
        Ok(policy) => {
            Some(DeterminismPolicyKnobs {
                allowed_modes: if policy.determinism.allowed_modes.is_empty() {
                    None
                } else {
                    Some(policy.determinism.allowed_modes)
                },
                pins_outside_effective: policy
                    .routing
                    .as_ref()
                    .map(|r| r.pin_enforcement.clone()),
                fallback_allowed: Some(policy.determinism.allow_fallback),
            })
        }
        Err(e) => {
            warn!(tenant_id = %tenant_id, error = %e, "Failed to load execution policy, returning None");
            None
        }
    };

    // Parse settings_json to serde_json::Value if present
    let settings_json = settings
        .settings_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(Json(TenantSettingsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id: settings.tenant_id,
        use_default_stack_on_chat_create: settings.use_default_stack_on_chat_create,
        use_default_stack_on_infer_session: settings.use_default_stack_on_infer_session,
        settings_json,
        determinism_policy,
        created_at: Some(settings.created_at),
        updated_at: Some(settings.updated_at),
    }))
}
