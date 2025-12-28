//! Auth configuration handler
//!
//! Contains the handler for retrieving auth configuration.

use crate::auth_common::AuthConfig;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Json};

use super::types::AuthConfigResponse;

/// Get authentication configuration
///
/// Returns authentication settings for the frontend to use.
/// This endpoint is public (no auth required) so the login page
/// can display dev bypass button when available.
#[utoipa::path(
    get,
    path = "/v1/auth/config",
    responses(
        (status = 200, description = "Auth configuration", body = AuthConfigResponse),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn get_auth_config_handler(
    State(state): State<AppState>,
) -> Result<Json<AuthConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.read().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;
    let auth_cfg = AuthConfig::from_state(&state);

    let response = AuthConfigResponse {
        allow_registration: false, // Registration not implemented yet
        require_email_verification: false,
        access_token_ttl_minutes: (auth_cfg.access_ttl() / 60) as u32,
        session_timeout_minutes: (auth_cfg.effective_ttl() / 60) as u32,
        max_login_attempts: config.auth.lockout_threshold,
        password_min_length: 12,
        mfa_required: config.security.require_mfa.unwrap_or(false),
        allowed_domains: None,
        production_mode: config.server.production_mode,
        dev_token_enabled: config.security.dev_login_enabled,
        dev_bypass_allowed: auth_cfg.dev_login_allowed(),
        jwt_mode: config
            .security
            .jwt_mode
            .clone()
            .unwrap_or_else(|| "eddsa".to_string()),
        token_expiry_hours: (auth_cfg.effective_ttl() / 3600) as u32,
    };

    Ok(Json(response))
}
