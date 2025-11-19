///! Enhanced authentication middleware with comprehensive security checks
///!
///! Features:
///! - JWT validation with Ed25519/HMAC support
///! - Token revocation checking
///! - IP access control (allowlist/denylist)
///! - Rate limiting per tenant
///! - Session tracking and activity updates

use crate::auth::{validate_token, validate_token_ed25519, Claims};
use crate::ip_extraction::{extract_client_ip, ClientIp};
use crate::security::{
    check_ip_access, check_rate_limit, is_token_revoked, update_session_activity, AccessDecision,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::identity::IdentityEnvelope;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use tracing::{debug, warn};

/// Enhanced authentication middleware with all security checks
///
/// 1. Extracts and validates JWT
/// 2. Checks token revocation
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

    // Extract token from Authorization header or query parameter
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let query_token = req.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    let token = auth_header
        .and_then(|header| header.strip_prefix("Bearer "))
        .or(query_token.as_deref());

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

    let token = auth_header
        .and_then(|header| header.strip_prefix("Bearer "))
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
