use crate::auth::{validate_token, Claims};
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use adapteros_db::users::Role;
use std::str::FromStr;

/// Extract and validate JWT from Authorization header
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
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
                        Json(ErrorResponse {
                            error: "unauthorized".to_string(),
                            details: None, // Don't leak token validation details
                        }),
                    ));
                }
            }
        }
    }

    tracing::warn!("Missing or invalid Authorization header");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: "unauthorized".to_string(),
            details: Some("missing or invalid Authorization header".to_string()),
        }),
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
            Json(ErrorResponse {
                error: "invalid role".to_string(),
                details: None,
            }),
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
        Json(ErrorResponse {
            error: "insufficient permissions".to_string(),
            details: Some(format!("required role: {:?}", required)),
        }),
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
            Json(ErrorResponse {
                error: "invalid role".to_string(),
                details: None,
            }),
        )
    })?;

    if user_role == Role::Admin || roles.contains(&user_role) {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: "insufficient permissions".to_string(),
            details: None,
        }),
    ))
}
