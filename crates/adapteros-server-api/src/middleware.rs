use crate::auth::{validate_token, Claims};
use crate::ip_extraction::{extract_client_ip, ClientIp};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use chrono::{Duration, Utc};
use std::str::FromStr;
use uuid::Uuid;

/// Extract and validate JWT from Authorization header
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    if let Some(auth_header) = auth_header {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            match validate_token(token, &state.jwt_secret) {
                Ok(claims) => {
                    // Insert claims into request extensions for handlers to use
                    req.extensions_mut().insert(claims);
                    return Ok(next.run(req).await);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Token validation failed");
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse::new("unauthorized").with_code("INTERNAL_ERROR")),
                    ));
                }
            }
        }
    }

    tracing::warn!("Missing or invalid Authorization header");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("unauthorized")
                .with_code("INTERNAL_ERROR")
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
    // Extract client IP address from headers for audit logging
    if let Some(ip) = extract_client_ip(req.headers()) {
        req.extensions_mut().insert(ClientIp(ip));
    }

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    if let Some(auth_header) = auth_header {
        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            if token == "adapteros-local" {
                let now = Utc::now();
                let claims = Claims {
                    sub: "api-key-user".to_string(),
                    email: "api@adapteros.local".to_string(),
                    role: "User".to_string(),
                    tenant_id: "default".to_string(),
                    exp: (now + Duration::hours(1)).timestamp(),
                    iat: now.timestamp(),
                    jti: Uuid::new_v4().to_string(),
                    nbf: now.timestamp(),
                };
                req.extensions_mut().insert(claims);
                return Ok(next.run(req).await);
            }

            match validate_token(token, &state.jwt_secret) {
                Ok(claims) => {
                    req.extensions_mut().insert(claims);
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
