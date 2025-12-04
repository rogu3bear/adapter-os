///! Enhanced authentication middleware with comprehensive security checks
///!
///! Features:
///! - JWT validation with Ed25519/HMAC support
///! - Token revocation checking (individual + per-tenant baseline)
///! - IP access control (allowlist/denylist)
///! - Rate limiting per tenant
///! - Session tracking and activity updates

use crate::auth::{validate_token, validate_token_ed25519, Claims};
use crate::ip_extraction::{extract_client_ip, ClientIp};
use crate::security::{
    check_ip_access, check_rate_limit, get_tenant_token_baseline, is_token_revoked,
    update_session_activity, AccessDecision,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use sqlx;
use adapteros_core::identity::IdentityEnvelope;
use axum::{
    extract::State,
    http::{header, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use chrono;
use tracing::{debug, warn};

/// Extract auth_token from Cookie header
fn extract_token_from_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("auth_token=") {
                    return Some(token.to_string());
                }
            }
            None
        })
}

/// Enhanced authentication middleware with all security checks
///
/// 1. Extracts and validates JWT
/// 2. Checks token revocation (individual + per-tenant baseline)
/// 3. Validates IP access (allowlist/denylist)
/// 4. Checks rate limiting
/// 5. Updates session activity
pub async fn enhanced_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Extract client IP address
    let client_ip = extract_client_ip(req.headers());
    if let Some(ip) = &client_ip {
        req.extensions_mut().insert(ClientIp(ip.clone()));
    }

    // Extract token from Authorization header, query parameter, or cookie
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let query_token = req.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    // Also try to extract token from cookies for browser-based authentication
    let cookie_token = extract_token_from_cookie(req.headers());

    let token = auth_header
        .and_then(|header| header.strip_prefix("Bearer "))
        .or(query_token.as_deref())
        .or(cookie_token.as_deref());

    let token = token.ok_or_else(|| {
        warn!("Missing Authorization header");
        (
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("missing authorization")
                    .with_code("UNAUTHORIZED")
                    .with_string_details("missing Authorization header"),
            ),
        )
    })?;

    // Validate JWT
    let claims = if state.use_ed25519 {
        validate_token_ed25519(token, &state.ed25519_public_key).map_err(|e| {
            warn!(error = %e, "Ed25519 token validation failed");
            (
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("invalid token")
                        .with_code("UNAUTHORIZED")
                        .with_string_details("token validation failed"),
                ),
            )
        })?
    } else {
        validate_token(token, &state.jwt_secret).map_err(|e| {
            warn!(error = %e, "HMAC token validation failed");
            (
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("invalid token")
                        .with_code("UNAUTHORIZED")
                        .with_string_details("token validation failed"),
                ),
            )
        })?
    };

    // Debug logging for JWT validation - helps diagnose auth issues in development
    #[cfg(debug_assertions)]
    debug!(
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        admin_tenants = ?claims.admin_tenants,
        jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
        "JWT validated successfully"
    );

    // Check token revocation
    if is_token_revoked(&state.db, &claims.jti)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to check token revocation");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
    {
        warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked token used");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("token revoked")
                    .with_code("TOKEN_REVOKED")
                    .with_string_details("this token has been revoked"),
            ),
        ));
    }

    // Check per-tenant token revocation baseline - PRD-03
    if let Ok(Some(baseline)) = get_tenant_token_baseline(&state.db, &claims.tenant_id).await {
        if let Ok(baseline_ts) = chrono::DateTime::parse_from_rfc3339(&baseline) {
            let token_iat = chrono::DateTime::from_timestamp(claims.iat, 0);
            if let Some(iat_time) = token_iat {
                if iat_time.timestamp() < baseline_ts.timestamp() {
                    warn!(
                        tenant_id = %claims.tenant_id,
                        token_iat = claims.iat,
                        baseline = %baseline,
                        "Token rejected: issued before tenant revocation baseline"
                    );
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(
                            ErrorResponse::new("Token has been revoked due to tenant-wide security action")
                                .with_code("TOKEN_REVOKED_BY_TENANT"),
                        ),
                    ));
                }
            }
        }
    }

    // Validate tenant exists (with caching to avoid per-request DB queries)
    // Skip for admin role who can access any tenant
    if claims.role != "admin" {
        let tenant_id = &claims.tenant_id;

        // Check cache first (60s TTL)
        let tenant_valid = if let Some(cached) = state.dashboard_cache.tenant_exists(tenant_id).await
        {
            debug!(tenant_id = %tenant_id, cached = true, "Tenant validation from cache");
            cached
        } else {
            // Cache miss - query DB
            let exists: bool = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM tenants WHERE id = ? LIMIT 1",
            )
            .bind(tenant_id)
            .fetch_optional(state.db.pool())
            .await
            .ok()
            .flatten()
            .is_some();

            // Update cache
            state
                .dashboard_cache
                .set_tenant_exists(tenant_id.clone(), exists)
                .await;
            debug!(tenant_id = %tenant_id, exists = exists, cached = false, "Tenant validation from DB");
            exists
        };

        if !tenant_valid {
            warn!(
                tenant_id = %tenant_id,
                user_id = %claims.sub,
                "Token references non-existent tenant"
            );
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("invalid tenant")
                        .with_code("TENANT_NOT_FOUND")
                        .with_string_details("tenant no longer exists"),
                ),
            ));
        }
    }

    // Check IP access control
    if let Some(ip) = &client_ip {
        let access = check_ip_access(&state.db, ip, Some(&claims.tenant_id))
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to check IP access");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
                )
            })?;

        if access == AccessDecision::Deny {
            warn!(
                ip = %ip,
                tenant_id = %claims.tenant_id,
                user_id = %claims.sub,
                "IP address denied"
            );
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("ip address denied")
                        .with_code("IP_DENIED")
                        .with_string_details("your IP address is not allowed"),
                ),
            ));
        }
    }

    // Check rate limiting
    let rate_limit = check_rate_limit(&state.db, &claims.tenant_id)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to check rate limit");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

    if !rate_limit.allowed {
        warn!(
            tenant_id = %claims.tenant_id,
            current = %rate_limit.current_count,
            limit = %rate_limit.limit,
            "Rate limit exceeded"
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(
                ErrorResponse::new("rate limit exceeded")
                    .with_code("RATE_LIMIT_EXCEEDED")
                    .with_string_details(format!(
                        "rate limit: {}/{} requests, resets at {}",
                        rate_limit.current_count, rate_limit.limit, rate_limit.reset_at
                    )),
            ),
        ));
    }

    // Update session activity
    if let Err(e) = update_session_activity(&state.db, &claims.jti).await {
        // Log but don't fail request
        debug!(error = %e, jti = %claims.jti, "Failed to update session activity");
    }

    // Insert claims and identity into request extensions
    let tenant_id = claims.tenant_id.clone();
    req.extensions_mut().insert(claims);

    let identity = IdentityEnvelope::new(
        tenant_id,
        "api".to_string(),
        "middleware".to_string(),
        IdentityEnvelope::default_revision(),
    );
    req.extensions_mut().insert(identity);

    Ok(next.run(req).await)
}

/// Lightweight auth middleware without security checks (for internal/trusted endpoints)
pub async fn basic_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    // Also try to extract token from cookies for browser-based authentication
    let cookie_token = extract_token_from_cookie(req.headers());

    let token = auth_header
        .and_then(|header| header.strip_prefix("Bearer "))
        .or(cookie_token.as_deref())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("missing authorization").with_code("UNAUTHORIZED")),
            )
        })?;

    let claims = if state.use_ed25519 {
        validate_token_ed25519(token, &state.ed25519_public_key)
    } else {
        validate_token(token, &state.jwt_secret)
    }
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("invalid token").with_code("UNAUTHORIZED")),
        )
    })?;

    // Debug logging for JWT validation - helps diagnose auth issues in development
    #[cfg(debug_assertions)]
    debug!(
        user_id = %claims.sub,
        tenant_id = %claims.tenant_id,
        admin_tenants = ?claims.admin_tenants,
        jwt_algorithm = if state.use_ed25519 { "Ed25519" } else { "HMAC" },
        "JWT validated successfully (basic auth)"
    );

    // SECURITY: Check if token has been revoked (critical for basic_auth_middleware)
    if is_token_revoked(&state.db, &claims.jti)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to check token revocation");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
    {
        warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked token used in basic auth");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(
                ErrorResponse::new("token revoked")
                    .with_code("TOKEN_REVOKED")
                    .with_string_details("this token has been revoked"),
            ),
        ));
    }

    // Check per-tenant token revocation baseline - PRD-03
    if let Ok(Some(baseline)) = get_tenant_token_baseline(&state.db, &claims.tenant_id).await {
        if let Ok(baseline_ts) = chrono::DateTime::parse_from_rfc3339(&baseline) {
            let token_iat = chrono::DateTime::from_timestamp(claims.iat, 0);
            if let Some(iat_time) = token_iat {
                if iat_time.timestamp() < baseline_ts.timestamp() {
                    warn!(
                        tenant_id = %claims.tenant_id,
                        token_iat = claims.iat,
                        baseline = %baseline,
                        "Token rejected: issued before tenant revocation baseline (basic auth)"
                    );
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(
                            ErrorResponse::new("Token has been revoked due to tenant-wide security action")
                                .with_code("TOKEN_REVOKED_BY_TENANT"),
                        ),
                    ));
                }
            }
        }
    }

    let tenant_id = claims.tenant_id.clone();
    req.extensions_mut().insert(claims);

    let identity = IdentityEnvelope::new(
        tenant_id,
        "api".to_string(),
        "middleware".to_string(),
        IdentityEnvelope::default_revision(),
    );
    req.extensions_mut().insert(identity);

    Ok(next.run(req).await)
}
