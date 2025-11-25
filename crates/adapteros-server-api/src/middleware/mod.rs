//! Middleware modules for AdapterOS API
//!
//! Provides cross-cutting concerns:
//! - Authentication and authorization
//! - API versioning and deprecation
//! - Request ID tracking
//! - Compression
//! - Caching (ETags, conditional requests)

use crate::auth::{validate_token, Claims};
use crate::ip_extraction::{extract_client_ip, ClientIp};
use crate::security::is_token_revoked;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_db::users::Role;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use chrono::{Duration, Utc};
use std::env;
use std::str::FromStr;
use uuid::Uuid;

pub mod caching;
pub mod compression;
pub mod request_id;
pub mod versioning;

pub use caching::{caching_middleware, CacheControl};
pub use compression::compression_middleware;
pub use request_id::request_id_middleware;
pub use versioning::{versioning_middleware, ApiVersion, DeprecationInfo};

fn dev_no_auth_enabled() -> bool {
    cfg!(debug_assertions)
        && env::var("AOS_DEV_NO_AUTH")
            .map(|v| {
                let lower = v.to_ascii_lowercase();
                matches!(lower.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
}

fn dev_no_auth_claims() -> Claims {
    let now = Utc::now();
    Claims {
        sub: "dev-no-auth".to_string(),
        email: "dev-no-auth@adapteros.local".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        tenant_id: "system".to_string(),
        exp: (now + Duration::hours(8)).timestamp(),
        iat: now.timestamp(),
        jti: Uuid::new_v4().to_string(),
        nbf: now.timestamp(),
    }
}

/// Extract client IP address from request headers (applies to all routes)
pub async fn client_ip_middleware(mut req: Request<axum::body::Body>, next: Next) -> Response {
    // Extract and inject client IP into request extensions
    // Always insert a ClientIp - use extracted IP or fallback to "unknown"
    let ip = extract_client_ip(req.headers()).unwrap_or_else(|| "127.0.0.1".to_string());
    req.extensions_mut().insert(ClientIp(ip));
    next.run(req).await
}

/// Extract and validate JWT from Authorization header
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if dev_no_auth_enabled() {
        let claims = dev_no_auth_claims();
        let tenant_id = claims.tenant_id.clone();
        tracing::info!("Dev no-auth bypass enabled; skipping authentication");
        req.extensions_mut().insert(claims);
        let identity = IdentityEnvelope::new(
            tenant_id,
            "api".to_string(),
            "middleware".to_string(),
            IdentityEnvelope::default_revision(),
        );
        req.extensions_mut().insert(identity);
        return Ok(next.run(req).await);
    }

    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }

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

    if let Some(token) = token {
        match validate_token(token, &state.jwt_secret) {
            Ok(claims) => {
                // Check if token has been revoked
                if let Err(e) = is_token_revoked(&state.db, &claims.jti).await {
                    tracing::warn!(error = %e, "Failed to check token revocation");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
                    ));
                }

                if is_token_revoked(&state.db, &claims.jti)
                    .await
                    .unwrap_or(false)
                {
                    tracing::warn!(jti = %claims.jti, user_id = %claims.sub, "Revoked token used");
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(
                            ErrorResponse::new("token revoked")
                                .with_code("TOKEN_REVOKED")
                                .with_string_details("this token has been revoked"),
                        ),
                    ));
                }

                // Extract tenant_id before moving claims
                let tenant_id = claims.tenant_id.clone();
                // Insert claims into request extensions for handlers to use
                req.extensions_mut().insert(claims);
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(), // or specific
                    IdentityEnvelope::default_revision(),
                );
                req.extensions_mut().insert(identity);
                return Ok(next.run(req).await);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse::new("unauthorized").with_code("UNAUTHORIZED")),
                ));
            }
        }
    }

    tracing::warn!("Missing or invalid Authorization header");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("unauthorized")
                .with_code("UNAUTHORIZED")
                .with_string_details("missing or invalid Authorization header"),
        ),
    ))
}

/// Extract and validate API key OR JWT from Authorization header
pub async fn dual_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if dev_no_auth_enabled() {
        let claims = dev_no_auth_claims();
        let tenant_id = claims.tenant_id.clone();
        tracing::info!("Dev no-auth bypass enabled; skipping authentication");
        req.extensions_mut().insert(claims);
        let identity = IdentityEnvelope::new(
            tenant_id,
            "api".to_string(),
            "middleware".to_string(),
            IdentityEnvelope::default_revision(),
        );
        req.extensions_mut().insert(identity);
        return Ok(next.run(req).await);
    }

    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }

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

    if let Some(token) = token {
        // SECURITY: Only allow debug bypass in development mode
        #[cfg(debug_assertions)]
        {
            if token == "adapteros-local" {
                let now = Utc::now();
                let claims = Claims {
                    sub: "api-key-user".to_string(),
                    email: "api@adapteros.local".to_string(),
                    role: "User".to_string(),
                    roles: vec!["User".to_string()],
                    tenant_id: "default".to_string(),
                    exp: (now + Duration::hours(1)).timestamp(),
                    iat: now.timestamp(),
                    jti: Uuid::new_v4().to_string(),
                    nbf: now.timestamp(),
                };
                let tenant_id = claims.tenant_id.clone();
                tracing::debug!("Using debug bypass token (dev mode only)");
                req.extensions_mut().insert(claims);
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(),
                    IdentityEnvelope::default_revision(),
                );
                req.extensions_mut().insert(identity);
                return Ok(next.run(req).await);
            }
        }

        match validate_token(token, &state.jwt_secret) {
            Ok(claims) => {
                let tenant_id = claims.tenant_id.clone();
                req.extensions_mut().insert(claims);
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "api".to_string(),
                    "middleware".to_string(), // or specific
                    IdentityEnvelope::default_revision(),
                );
                req.extensions_mut().insert(identity);
                return Ok(next.run(req).await);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(
                        ErrorResponse::new("unauthorized")
                            .with_code("UNAUTHORIZED")
                            .with_string_details("invalid token"),
                    ),
                ));
            }
        }
    }

    tracing::warn!("Missing or invalid Authorization header");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("unauthorized")
                .with_code("UNAUTHORIZED")
                .with_string_details("missing or invalid Authorization header"),
        ),
    ))
}

/// Require specific role for access
pub fn require_role(
    claims: &Claims,
    required: Role,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("invalid role").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Admin can access everything
    if user_role == Role::Admin {
        return Ok(());
    }

    // Check specific role requirements
    if user_role == required {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(
            ErrorResponse::new("insufficient permissions")
                .with_code("FORBIDDEN")
                .with_string_details(format!("required role: {:?}", required)),
        ),
    ))
}

/// Check if user has any of the specified roles
pub fn require_any_role(
    claims: &Claims,
    roles: &[Role],
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("invalid role").with_code("INTERNAL_ERROR")),
        )
    })?;

    if user_role == Role::Admin || roles.contains(&user_role) {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
    ))
}
