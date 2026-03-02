//! Tenant switch handler for multi-tenant session context switching.
//!
//! Allows authenticated users to switch their active tenant context without
//! re-authenticating. The user must have explicit access to the target tenant
//! via their `admin_tenants` claim or be switching to their primary tenant.
//!
//! # Security
//!
//! - Verifies user has access to target tenant before switching
//! - Issues new tokens with the target tenant as the active context
//! - Preserves the existing session (no re-authentication required)
//! - Logs all tenant switch attempts for audit compliance

use super::audit::{log_auth_event, AuthEvent};
use crate::auth::Claims;
use crate::auth_common::{
    attach_auth_cookies, issue_access_token, issue_refresh_token, AccessTokenParams, AuthConfig,
    RefreshTokenParams,
};
use crate::ip_extraction::ClientIp;
use crate::security::{check_tenant_access, upsert_user_session};
use crate::state::AppState;
use crate::tenant_visibility::{is_reserved_internal_tenant_id, SYSTEM_TENANT_ID};
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{SwitchTenantRequest, SwitchTenantResponse, TenantSummary};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_id::{IdPrefix, TypedId};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::Utc;
use tracing::warn;

/// Switch the authenticated user's active tenant context
///
/// This endpoint allows users to switch to a different tenant they have access to.
/// The user must have explicit access via `admin_tenants` or be switching to their
/// primary tenant.
///
/// # Security
///
/// - Requires valid authentication (JWT token)
/// - Verifies tenant access via `check_tenant_access`
/// - Issues new access and refresh tokens scoped to the target tenant
/// - Preserves the session ID for continuity
/// - All attempts are logged for audit compliance
#[utoipa::path(
    post,
    path = "/v1/auth/tenants/switch",
    request_body = SwitchTenantRequest,
    responses(
        (status = 200, description = "Tenant switched successfully", body = SwitchTenantResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Access denied to target tenant"),
        (status = 404, description = "Target tenant not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth"
)]
pub async fn switch_tenant_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<SwitchTenantRequest>,
) -> Result<(HeaderMap, Json<SwitchTenantResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);
    let ip_address = client_ip.0.clone();
    let session_id = claims
        .session_id
        .clone()
        .unwrap_or_else(|| claims.jti.clone());

    // 1. Validate request
    let target_tenant_id = req.tenant_id.trim();
    if target_tenant_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("tenant_id is required").with_code("MISSING_TENANT_ID")),
        ));
    }
    if is_reserved_internal_tenant_id(target_tenant_id)
        && claims.tenant_id != SYSTEM_TENANT_ID
        && claims.role != SYSTEM_TENANT_ID
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied to target tenant")
                    .with_code("TENANT_ACCESS_DENIED")
                    .with_string_details(format!(
                        "Tenant '{}' is reserved for internal platform use",
                        target_tenant_id
                    )),
            ),
        ));
    }

    // 2. Check if user has access to the target tenant
    // Uses the shared tenant isolation logic which supports:
    // - Same tenant (always allowed)
    // - admin_tenants list (explicit grants)
    // - Wildcard "*" (admin with all-tenant access)
    // - Dev mode bypass for admin role (debug builds only)
    if !check_tenant_access(&claims, target_tenant_id) {
        // Log denied access attempt
        log_auth_event(
            AuthEvent::TenantSwitchDenied,
            Some(&claims.sub),
            Some(&claims.email),
            Some(&claims.tenant_id),
            Some(&ip_address),
            Some(&session_id),
            Some(&format!(
                "target_tenant={}, admin_tenants={:?}",
                target_tenant_id, claims.admin_tenants
            )),
        );

        warn!(
            user_id = %claims.sub,
            user_email = %claims.email,
            current_tenant = %claims.tenant_id,
            target_tenant = %target_tenant_id,
            admin_tenants = ?claims.admin_tenants,
            "Tenant switch denied - user lacks access"
        );

        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied to target tenant")
                    .with_code("TENANT_ACCESS_DENIED")
                    .with_string_details(format!(
                        "User does not have access to tenant '{}'",
                        target_tenant_id
                    )),
            ),
        ));
    }

    // 3. Verify target tenant exists
    let target_tenant = state
        .db
        .get_tenant(target_tenant_id)
        .await
        .map_err(|e| {
            warn!(error = %e, tenant_id = %target_tenant_id, "Database error checking tenant");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to verify tenant").with_code("DATABASE_ERROR")),
            )
        })?
        .ok_or_else(|| {
            warn!(tenant_id = %target_tenant_id, user_id = %claims.sub, "Target tenant not found");
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Target tenant not found")
                        .with_code("TENANT_NOT_FOUND")
                        .with_string_details(format!(
                            "Tenant '{}' does not exist",
                            target_tenant_id
                        )),
                ),
            )
        })?;

    // 4. Fetch user to ensure still active
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            warn!(error = %e, user_id = %claims.sub, "Database error fetching user for tenant switch");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to verify user").with_code("DATABASE_ERROR")),
            )
        })?
        .ok_or_else(|| {
            warn!(user_id = %claims.sub, "User not found during tenant switch");
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("User not found").with_code("USER_NOT_FOUND")),
            )
        })?;

    if user.disabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Account is disabled").with_code("ACCOUNT_DISABLED")),
        ));
    }

    // 5. Get user's tenant access list (for the new token)
    let admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &user.id)
        .await
        .unwrap_or_default();

    // 6. Generate new tokens with target tenant as the active context
    let token_ttl_seconds = auth_cfg.access_ttl();
    let session_ttl_seconds = auth_cfg.effective_ttl();
    let roles_vec = vec![user.role.clone()];
    let rot_id = TypedId::new(IdPrefix::Rot).to_string();

    // Issue new access token with the TARGET tenant as the active tenant
    let access_params = AccessTokenParams {
        user_id: &user.id,
        email: &user.email,
        role: &user.role,
        roles: &roles_vec,
        tenant_id: target_tenant_id, // Switch to target tenant
        admin_tenants: &admin_tenants,
        device_id: claims.device_id.as_deref(),
        session_id: &session_id,
        mfa_level: claims.mfa_level.as_deref(),
    };
    let new_access_token =
        issue_access_token(&state, &access_params, Some(token_ttl_seconds)).map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to generate access token for tenant switch");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
            )
        })?;

    // Issue new refresh token for the switched tenant context
    let refresh_params = RefreshTokenParams {
        user_id: &user.id,
        tenant_id: target_tenant_id, // Switch to target tenant
        roles: &roles_vec,
        device_id: claims.device_id.as_deref(),
        session_id: &session_id,
        rot_id: &rot_id,
    };
    let new_refresh_token =
        issue_refresh_token(&state, &refresh_params, Some(session_ttl_seconds)).map_err(|e| {
            warn!(error = %e, user_id = %user.id, "Failed to generate refresh token for tenant switch");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Token generation failed").with_code("TOKEN_ERROR")),
            )
        })?;

    // 7. Update session with new tenant context
    let refresh_hash = blake3::hash(new_refresh_token.as_bytes())
        .to_hex()
        .to_string();
    let session_expires_at = Utc::now() + chrono::Duration::seconds(session_ttl_seconds as i64);
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Err(e) = upsert_user_session(
        &state.db,
        &session_id,
        &user.id,
        target_tenant_id, // Update session to target tenant
        claims.device_id.as_deref(),
        Some(&rot_id),
        Some(&refresh_hash),
        session_expires_at.timestamp(),
        &session_expires_at.to_rfc3339(),
        Some(&ip_address),
        user_agent.as_deref(),
        false,
    )
    .await
    {
        warn!(error = %e, session_id = %session_id, "Failed to update session for tenant switch");
        // Non-fatal - tokens are valid but session record may be stale
    }

    // 8. Log successful tenant switch
    log_auth_event(
        AuthEvent::TenantSwitchSuccess,
        Some(&user.id),
        None, // Don't log email on success (privacy)
        Some(target_tenant_id),
        Some(&ip_address),
        Some(&session_id),
        Some(&format!("from_tenant={}", claims.tenant_id)),
    );

    // 9. Build tenant summary for response
    let tenant_summary = TenantSummary {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: target_tenant.id.clone(),
        name: target_tenant.name.clone(),
        status: target_tenant.status.clone(),
        created_at: Some(target_tenant.created_at.clone()),
    };

    // 10. Attach cookies for browser auth
    let mut response_headers = HeaderMap::new();
    let csrf_token = TypedId::new(IdPrefix::Tok).to_string();
    attach_auth_cookies(
        &mut response_headers,
        &new_access_token,
        &new_refresh_token,
        &csrf_token,
        &auth_cfg,
        session_ttl_seconds,
    )
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to attach auth cookies");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Cookie error").with_code("COOKIE_ERROR")),
        )
    })?;

    // 11. Return response
    Ok((
        response_headers,
        Json(SwitchTenantResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            token: new_access_token,
            user_id: user.id,
            tenant_id: target_tenant.id,
            role: user.role,
            expires_in: token_ttl_seconds,
            tenants: Some(vec![tenant_summary]),
            mfa_level: claims.mfa_level,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthMode, PrincipalType};

    fn make_claims(tenant_id: &str, admin_tenants: Vec<&str>) -> Claims {
        let now = chrono::Utc::now().timestamp();
        Claims {
            sub: "user-switch-test".to_string(),
            email: "switch@example.com".to_string(),
            role: "admin".to_string(),
            roles: vec!["admin".to_string()],
            tenant_id: tenant_id.to_string(),
            admin_tenants: admin_tenants.into_iter().map(|s| s.to_string()).collect(),
            device_id: None,
            session_id: Some("sess-switch".to_string()),
            mfa_level: None,
            rot_id: None,
            exp: now + 3600,
            iat: now,
            jti: "jti-switch".to_string(),
            nbf: now,
            iss: crate::auth::JWT_ISSUER.to_string(),
            auth_mode: AuthMode::BearerToken,
            principal_type: Some(PrincipalType::User),
        }
    }

    #[test]
    fn test_check_tenant_access_same_tenant() {
        let claims = make_claims("tenant-a", vec![]);
        // User can always access their own tenant
        assert!(check_tenant_access(&claims, "tenant-a"));
    }

    #[test]
    fn test_check_tenant_access_different_tenant_no_grant() {
        let claims = make_claims("tenant-a", vec![]);
        // User cannot access different tenant without explicit grant
        assert!(!check_tenant_access(&claims, "tenant-b"));
    }

    #[test]
    fn test_check_tenant_access_with_explicit_grant() {
        let claims = make_claims("tenant-a", vec!["tenant-b", "tenant-c"]);
        // User can access granted tenants
        assert!(check_tenant_access(&claims, "tenant-b"));
        assert!(check_tenant_access(&claims, "tenant-c"));
        // But not ungranted tenants
        assert!(!check_tenant_access(&claims, "tenant-d"));
    }

    #[test]
    fn test_check_tenant_access_wildcard() {
        let claims = make_claims("system", vec!["*"]);
        // Wildcard grants access to all tenants
        assert!(check_tenant_access(&claims, "any-tenant"));
        assert!(check_tenant_access(&claims, "another-tenant"));
    }
}
