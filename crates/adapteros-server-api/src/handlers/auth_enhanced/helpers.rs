//! Internal helper functions for auth handlers
//!
//! Contains utility functions used across multiple auth handlers.

use crate::auth::{AuthMode, Claims, PrincipalType, JWT_ISSUER};
use crate::state::AppState;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_db::users::User;
use adapteros_db::Db;
use adapteros_telemetry::{build_auth_event, make_auth_payload};

/// Wildcard for admin tenant access
pub const ADMIN_TENANT_WILDCARD: &str = "*";

/// Build minimal claims for audit logging from a user record
pub fn audit_claims_for_user(user: &User, tenant_id: &str) -> Claims {
    Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        role: user.role.clone(),
        roles: vec![user.role.clone()],
        tenant_id: tenant_id.to_string(),
        admin_tenants: vec![],
        device_id: None,
        session_id: None,
        mfa_level: None,
        rot_id: None,
        exp: 0,
        iat: 0,
        jti: String::new(),
        nbf: 0,
        iss: JWT_ISSUER.to_string(),
        auth_mode: AuthMode::BearerToken,
        principal_type: Some(PrincipalType::User),
    }
}

/// Extract a token from cookies by name
pub fn extract_cookie_token(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            let prefix = format!("{name}=");
            for cookie in cookies.split(';') {
                let trimmed = cookie.trim();
                if let Some(token) = trimmed.strip_prefix(&prefix) {
                    return Some(token.to_string());
                }
            }
            None
        })
}

/// Log an authentication event to the audit log
#[allow(clippy::too_many_arguments)]
pub async fn log_auth_event(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    status: &str,
    error_message: Option<&str>,
    ip_address: Option<&str>,
) {
    let _ = db
        .log_audit(
            &claims.sub,
            &claims.role,
            &claims.tenant_id,
            action,
            resource_type,
            resource_id,
            status,
            error_message,
            ip_address,
            None,
        )
        .await;
}

/// Emit a telemetry event for authentication
pub async fn emit_auth_event(
    state: &AppState,
    principal_id: &str,
    tenant_id: &str,
    flow_type: &str,
    success: bool,
    error_code: Option<&str>,
) {
    let identity = IdentityEnvelope::new(
        tenant_id.to_string(),
        "api".to_string(),
        "auth".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    let payload = make_auth_payload(
        principal_id.to_string(),
        tenant_id.to_string(),
        flow_type.to_string(),
        success,
        error_code.map(|c| c.to_string()),
    );

    if let Ok(event) = build_auth_event(identity, payload) {
        let _ = state.telemetry_buffer.push(event).await;
    }
}

/// Log a failed token refresh attempt for audit purposes.
/// This captures failures where we may not have valid claims.
pub async fn log_refresh_failure(
    db: &Db,
    session_id: Option<&str>,
    user_id: Option<&str>,
    tenant_id: Option<&str>,
    error_code: &str,
    error_detail: &str,
    ip_address: Option<&str>,
) {
    let metadata = serde_json::json!({ "error_code": error_code });
    let _ = db
        .log_audit(
            user_id.unwrap_or("unknown"),
            "unknown",
            tenant_id.unwrap_or("unknown"),
            "auth.refresh_failed",
            "session",
            session_id,
            "failure",
            Some(error_detail),
            ip_address,
            Some(&metadata.to_string()),
        )
        .await;
}

/// Emit telemetry event for failed refresh
pub async fn emit_refresh_failure(
    state: &AppState,
    user_id: Option<&str>,
    tenant_id: Option<&str>,
    error_code: &str,
) {
    emit_auth_event(
        state,
        user_id.unwrap_or("unknown"),
        tenant_id.unwrap_or("unknown"),
        "refresh",
        false,
        Some(error_code),
    )
    .await;
}
