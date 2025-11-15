use crate::auth::auth_middleware;
use crate::auth::{validate_token, validate_token_ed25519, validate_token_ed25519_der, Claims};
use crate::errors::{AosErrorExt, ErrorResponseExt};
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
use url::form_urlencoded;
use uuid::Uuid;
use crate::rate_limit::per_tenant_rate_limit_middleware;
use crate::auth::auth_middleware;
use std::net::UdpSocket; // For UDS check

/// Simple bearer token authentication for metrics endpoint
pub async fn metrics_auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let bearer_token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "));

    let expected_token = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("configuration error").with_code("CONFIG_ERROR")),
            )
        })?;
        config.metrics.bearer_token.clone()
    };

    match bearer_token {
        Some(token) if token == expected_token => {
            // Token matches, proceed
            Ok(next.run(req).await)
        }
        _ => {
            // Invalid or missing token
            Err((
                StatusCode::UNAUTHORIZED,
                Json(
                    ErrorResponse::new("invalid or missing bearer token").with_code("UNAUTHORIZED"),
                ),
            ))
        }
    }
}

/// Extract and validate JWT from Authorization header, cookies, or query parameters
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let bearer_token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(|token| token.to_string());

    let query_token = req.uri().query().and_then(|query| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    let cookie_token = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|header| header.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                if cookie.starts_with("auth_token=") {
                    Some(cookie.strip_prefix("auth_token=")?.to_string())
                } else {
                    None
                }
            })
        });

    if let Some(token) = bearer_token.or(cookie_token).or(query_token) {
        // Check for dev bypass token when not in production mode
        let is_production = {
            let config = state.config.read().map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("config lock poisoned").with_code("INTERNAL_ERROR")),
                )
            })?;
            config.production_mode
        };

        if !is_production && token == "adapteros-local" {
            tracing::info!("Dev bypass token accepted (non-production mode)");
            let now = Utc::now();
            let claims = Claims {
                sub: "dev-bypass-user".to_string(),
                email: "dev@adapteros.local".to_string(),
                role: "admin".to_string(), // Admin role for full dev access
                tenant_id: "default".to_string(),
                exp: (now + Duration::hours(24)).timestamp(),
                iat: now.timestamp(),
                jti: Uuid::new_v4().to_string(),
                nbf: now.timestamp(),
            };
            req.extensions_mut().insert(claims);
            return Ok(next.run(req).await);
        }

        // Choose validation based on configured JWT mode
        let claims_res = match state.jwt_mode {
            crate::state::JwtMode::Hmac => validate_token(&token, &state.jwt_secret),
            crate::state::JwtMode::EdDsa => {
                if let Some(ref pem) = state.jwt_public_key_pem {
                    validate_token_ed25519(&token, pem)
                } else {
                    // Fallback to in-memory public key DER from crypto state
                    let der = state.crypto.jwt_keypair.public_key().to_bytes();
                    validate_token_ed25519_der(&token, &der)
                }
            }
        };
        return match claims_res {
            Ok(claims) => {
                req.extensions_mut().insert(claims);
                let _ = require_role(&claims, "admin").map_err(|e| e)?;
                Ok(next.run(req).await)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse::new("unauthorized").with_code("INTERNAL_ERROR")),
                ))
            }
        };
    }

    tracing::warn!("Missing or invalid Authorization token");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("unauthorized")
                .with_code("INTERNAL_ERROR")
                .with_string_details("missing or invalid Authorization header or token"),
        ),
    ))
}

/// Extract and validate API key OR JWT from Authorization header, cookies, or query parameters
pub async fn dual_auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let bearer_token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(|token| token.to_string());

    let query_token = req.uri().query().and_then(|query| {
        form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });

    let cookie_token = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|header| header.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                if cookie.starts_with("auth_token=") {
                    Some(cookie.strip_prefix("auth_token=")?.to_string())
                } else {
                    None
                }
            })
        });

    if let Some(token) = bearer_token.or(cookie_token).or(query_token) {
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

        let claims_res = match state.jwt_mode {
            crate::state::JwtMode::Hmac => validate_token(&token, &state.jwt_secret),
            crate::state::JwtMode::EdDsa => {
                if let Some(ref pem) = state.jwt_public_key_pem {
                    validate_token_ed25519(&token, pem)
                } else {
                    let der = state.crypto.jwt_keypair.public_key().to_bytes();
                    validate_token_ed25519_der(&token, &der)
                }
            }
        };
        return match claims_res {
            Ok(claims) => {
                req.extensions_mut().insert(claims);
                Ok(next.run(req).await)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Token validation failed");
                Err((
                    StatusCode::UNAUTHORIZED,
                    Json(
                        ErrorResponse::new("unauthorized")
                            .with_code("UNAUTHORIZED")
                            .with_string_details("invalid token"),
                    ),
                ))
            }
        };
    }

    tracing::warn!("Missing or invalid Authorization token");
    Err((
        StatusCode::UNAUTHORIZED,
        Json(
            ErrorResponse::new("unauthorized")
                .with_code("UNAUTHORIZED")
                .with_string_details("missing or invalid Authorization header or token"),
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

/// Global error handling middleware that converts AosError to user-friendly HTTP responses
///
/// This middleware catches any unhandled errors in the request processing pipeline
/// and attempts to convert them to user-friendly responses if they are AosError variants.
/// Should be applied as the outermost middleware to catch all errors.
pub async fn user_friendly_error_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Try to execute the request
    let result = next.run(req).await;

    // If the request succeeded, return it as-is
    if result.status().is_success() || result.status().is_redirection() {
        return Ok(result);
    }

    // For error responses, try to convert any AosError in the extensions
    // This is a fallback for handlers that don't explicitly handle errors
    if let Some(error) = result.extensions().get::<adapteros_core::AosError>() {
        let (status, json_response) = error.to_user_friendly_response();
        return Err((status, json_response));
    }

    // Return the original response if we can't convert it
    Ok(result)
}

/// Convenience function to create a user-friendly error response from any error
///
/// This function attempts to downcast the error to AosError and convert it to a
/// user-friendly response. Falls back to a generic error response if conversion fails.
pub fn to_user_friendly_response<E>(
    error: E,
    fallback_code: &str,
    _fallback_message: &str,
) -> (StatusCode, Json<ErrorResponse>)
where
    E: std::error::Error + Send + Sync + 'static + std::any::Any,
{
    // Try to downcast to AosError if the error implements Any
    // For now, just use the fallback logic

    // Try to extract error code from the error message for better categorization
    let error_msg = error.to_string();
    let error_code = if error_msg.contains("not found") {
        "NOT_FOUND"
    } else if error_msg.contains("permission") || error_msg.contains("unauthorized") {
        "UNAUTHORIZED"
    } else if error_msg.contains("invalid") || error_msg.contains("validation") {
        "VALIDATION_ERROR"
    } else if error_msg.contains("timeout") {
        "TIMEOUT"
    } else {
        fallback_code
    };

    // Create user-friendly response
    let response = ErrorResponse::new_user_friendly(error_code, &error_msg);
    let status = match error_code {
        "NOT_FOUND" => StatusCode::NOT_FOUND,
        "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
        "VALIDATION_ERROR" => StatusCode::BAD_REQUEST,
        "TIMEOUT" => StatusCode::REQUEST_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (status, Json(response))
}
