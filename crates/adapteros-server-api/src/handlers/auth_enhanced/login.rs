use crate::auth::{
    issue_access_token_ed25519, issue_access_token_hmac, issue_refresh_token_ed25519,
    issue_refresh_token_hmac, validate_refresh_token_ed25519, validate_refresh_token_hmac,
    verify_password,
};
use crate::auth_common::{
    attach_auth_cookie, attach_csrf_cookie, attach_refresh_cookie, AuthConfig,
};
use crate::ip_extraction::ClientIp;
use crate::security::{check_login_lockout, track_auth_attempt, upsert_user_session};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{LoginRequest, LoginResponse, TenantSummary};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension,
    Json,
};
use chrono::{Duration, Utc};
use tracing::{info, warn};
use uuid::Uuid;

/// Authenticate user via email/password
///
/// Returns a JWT token and user info on success.
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "User disabled"),
        (status = 500, description = "System error")
    ),
    tag = "auth"
)]
pub async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);
    // 1. Resolve email (support username field for legacy/UI compat)
    let email = if !req.email.is_empty() {
        &req.email
    } else if let Some(ref u) = req.username {
        u
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Email or username is required")
                    .with_code("MISSING_CREDENTIALS"),
            ),
        ));
    };

    // Extract request metadata
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let ip_address = client_ip.0.clone();

    // 2. Check for lockout/rate limiting (FAIL-CLOSED: deny on error)
    match check_login_lockout(&state.db, email, &ip_address).await {
        Ok(Some(lockout)) => {
            // Log detailed reason internally, but return generic message to user
            warn!(
                email = %email,
                ip = %ip_address,
                reason = %lockout.reason,
                "Login blocked due to lockout"
            );
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(
                    ErrorResponse::new("Too many login attempts. Please try again later.")
                        .with_code("ACCOUNT_LOCKED"),
                ),
            ));
        }
        Ok(None) => {
            // No lockout, continue with login
        }
        Err(e) => {
            // FAIL-CLOSED: Database error during lockout check denies login
            warn!(
                error = %e,
                email = %email,
                ip = %ip_address,
                "Lockout check failed, denying login for security"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Authentication failed").with_code("INTERNAL_ERROR")),
            ));
        }
    }

    // 3. Fetch user
    let user = state.db.get_user_by_email(email).await.map_err(|e| {
        // Log detailed error internally for debugging
        warn!(error = %e, email = %email, "Database error during login");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            // Return generic message to user - don't leak database details
            Json(ErrorResponse::new("Authentication failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // 4. Verify user existence with timing-safe fallback
    let user = match user {
        Some(u) => u,
        None => {
            let _ = verify_password(&req.password, "invalid");
            if let Err(e) = track_auth_attempt(
                &state.db,
                email,
                &ip_address,
                false,
                Some("invalid_credentials"),
            )
            .await
            {
                warn!(error = %e, email = %email, "Failed to record auth attempt");
            }
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("Invalid credentials").with_code("INVALID_CREDENTIALS")),
            ));
        }
    };

    // 5. Verify password
    let verification = verify_password(&req.password, &user.pw_hash).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Password verification error");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    if !verification.valid {
        if let Err(e) = track_auth_attempt(
            &state.db,
            &user.email,
            &ip_address,
            false,
            Some("invalid_password"),
        )
        .await
        {
            warn!(error = %e, user_id = %user.id, "Failed to record auth attempt");
        }
        info!(user_id = %user.id, "Failed login attempt (invalid password)");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Invalid credentials").with_code("INVALID_CREDENTIALS")),
        ));
    }

    // 6. Check if disabled
    if user.disabled {
        if let Err(e) = track_auth_attempt(
            &state.db,
            &user.email,
            &ip_address,
            false,
            Some("account_disabled"),
        )
        .await
        {
            warn!(error = %e, user_id = %user.id, "Failed to record auth attempt");
        }
        info!(user_id = %user.id, "Failed login attempt (account disabled)");
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Account is disabled").with_code("ACCOUNT_DISABLED")),
        ));
    }

    // 7. Generate Session and Token
    let session_id = Uuid::new_v4().to_string();
    let rot_id = format!("rot-{}", Uuid::new_v4());
    let token_ttl_seconds = auth_cfg.access_ttl();
    let session_ttl_seconds = auth_cfg.effective_ttl();
    let now = Utc::now();

    // Determine admin tenants (placeholder logic preserved)
    let roles_vec = vec![user.role.clone()];
    let admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &user.id)
        .await
        .unwrap_or_default();

    // Generate access token
    let token = if state.use_ed25519 {
        issue_access_token_ed25519(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &user.tenant_id,
            &admin_tenants,
            None,
            &session_id,
            None,
            &state.ed25519_keypair,
            Some(token_ttl_seconds),
        )
    } else {
        issue_access_token_hmac(
            &user.id,
            &user.email,
            &user.role,
            &roles_vec,
            &user.tenant_id,
            &admin_tenants,
            None,
            &session_id,
            None,
            &state.jwt_secret,
            Some(token_ttl_seconds),
        )
    }
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Token generation failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR")),
        )
    })?;

    // Generate refresh token
    let refresh_token = if state.use_ed25519 {
        issue_refresh_token_ed25519(
            &user.id,
            &user.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.ed25519_keypair,
            Some(session_ttl_seconds),
        )
    } else {
        issue_refresh_token_hmac(
            &user.id,
            &user.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.jwt_secret,
            Some(session_ttl_seconds),
        )
    }
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Refresh token generation failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR")),
        )
    })?;

    let refresh_claims = if state.use_ed25519 {
        validate_refresh_token_ed25519(
            &refresh_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        validate_refresh_token_hmac(&refresh_token, &state.hmac_keys, &state.jwt_secret)
    }
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Refresh token validation failed after generation");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("TOKEN_GENERATION_ERROR")),
        )
    })?;

    let response_expires_in = std::cmp::max(token_ttl_seconds, 60);
    let session_ttl = std::cmp::max(session_ttl_seconds, response_expires_in);
    let session_expires_at = Utc::now() + Duration::seconds(session_ttl as i64);
    let refresh_expires_at = chrono::DateTime::from_timestamp(refresh_claims.exp, 0)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let refresh_hash = blake3::hash(refresh_token.as_bytes()).to_hex().to_string();

    upsert_user_session(
        &state.db,
        &session_id,
        &user.id,
        &user.tenant_id,
        None,
        Some(&rot_id),
        Some(&refresh_hash),
        session_expires_at.timestamp(),
        &refresh_expires_at.to_rfc3339(),
        Some(&ip_address),
        user_agent.as_deref(),
        false,
    )
    .await
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to create auth session");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("SESSION_CREATION_ERROR")),
        )
    })?;

    if let Err(e) =
        track_auth_attempt(&state.db, &user.email, &ip_address, true, None).await
    {
        warn!(error = %e, user_id = %user.id, "Failed to record auth attempt");
    }

    // 8. Update Last Login
    let now_str = now.to_rfc3339();
    if let Err(e) = state.db.update_user_last_login(&user.id, &now_str).await {
        warn!(error = %e, user_id = %user.id, "Failed to update last login timestamp");
        // Non-fatal
    }

    // 9. Rehash password if needed (upgrade)
    if verification.needs_rehash {
        // We'd need to spawn a background task or do it here.
        // For now, we skip auto-rehash to keep it simple and safe.
    }

    info!(user_id = %user.id, tenant_id = %user.tenant_id, "Login successful");

    // 10. Construct Response
    // We need to fetch accessible tenants for the response summary
    // Since we don't have full admin_tenants logic in `User` struct yet (it's in Claims usually),
    // we'll just return the primary tenant for now.
    let tenants = vec![TenantSummary {
        schema_version: API_SCHEMA_VERSION.to_string(),
        id: user.tenant_id.clone(),
        name: user.tenant_id.clone(), // We might need to fetch tenant name, but ID is sufficient for Summary if name unknown
        status: None,
        created_at: None,
    }];
    // Ideally we fetch the tenant to get the real name.

    // Attempt to fetch tenant details for better UX
    let tenants = match state.db.get_tenant(&user.tenant_id).await {
        Ok(Some(t)) => vec![TenantSummary {
            schema_version: API_SCHEMA_VERSION.to_string(),
            id: t.id,
            name: t.name,
            status: t.status,
            created_at: Some(t.created_at),
        }],
        _ => tenants,
    };

    // 11. Attach httpOnly cookies for browser auth
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to attach auth cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("COOKIE_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &refresh_token, &auth_cfg).map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to attach refresh cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("COOKIE_ERROR")),
        )
    })?;
    // Generate and attach CSRF token for double-submit protection
    let csrf_token = Uuid::new_v4().to_string();
    attach_csrf_cookie(
        &mut response_headers,
        &csrf_token,
        &auth_cfg,
        session_ttl_seconds,
    )
    .map_err(|e| {
        warn!(error = %e, user_id = %user.id, "Failed to attach CSRF cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Authentication failed").with_code("COOKIE_ERROR")),
        )
    })?;

    Ok((
        response_headers,
        Json(LoginResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            token,
            user_id: user.id,
            tenant_id: user.tenant_id,
            role: user.role,
            expires_in: token_ttl_seconds,
            tenants: Some(tenants),
            mfa_level: None, // MFA not implemented yet
        }),
    ))
}
