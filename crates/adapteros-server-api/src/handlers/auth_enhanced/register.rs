//! User registration handler
//!
//! Allows users to self-register with email and password.
//! Each registered user gets their own isolated tenant.

use crate::auth::{hash_password, validate_refresh_token_ed25519, validate_refresh_token_hmac};
use crate::auth_common::{
    attach_auth_cookie, attach_refresh_cookie, issue_access_token, issue_refresh_token,
    AccessTokenParams, AuthConfig, RefreshTokenParams,
};
use crate::ip_extraction::ClientIp;
use crate::security::{check_registration_rate_limit, track_registration_attempt, upsert_user_session};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use adapteros_db::workspaces::WorkspaceRole;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{Duration, Utc};
use tracing::{info, warn};
use uuid::Uuid;

use super::audit::{log_auth_event, log_rate_limit_event, AuthEvent};
use super::types::{RegisterRequest, RegisterResponse};
use super::validation::{is_valid_email, normalize_email};

/// Minimum password length requirement
const MIN_PASSWORD_LENGTH: usize = 12;

/// User self-registration endpoint
///
/// Creates a new user with their own isolated tenant.
/// Returns JWT token for immediate authentication.
#[utoipa::path(
    post,
    path = "/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Registration successful", body = RegisterResponse),
        (status = 400, description = "Invalid email or password"),
        (status = 403, description = "Registration disabled"),
        (status = 409, description = "Email already registered"),
        (status = 500, description = "Internal error")
    ),
    tag = "auth"
)]
pub async fn register_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<RegisterRequest>,
) -> Result<(HeaderMap, Json<RegisterResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    // Check if registration is enabled (scope config to avoid holding across await)
    let registration_enabled = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.security.allow_registration.unwrap_or(false)
    };

    if !registration_enabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("registration is disabled")
                    .with_code("REGISTRATION_DISABLED")
                    .with_string_details("user self-registration is not enabled on this server"),
            ),
        ));
    }

    // Check registration rate limit per IP
    if check_registration_rate_limit(&state.db, &client_ip.0)
        .await
        .unwrap_or(false)
    {
        log_rate_limit_event("registration", Some(&client_ip.0), 0, 5);
        log_auth_event(
            AuthEvent::RateLimitExceeded,
            None,
            None,
            None,
            Some(&client_ip.0),
            None,
            Some("registration_rate_limit"),
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(
                ErrorResponse::new("too many registration attempts")
                    .with_code("RATE_LIMIT_EXCEEDED")
                    .with_string_details("please wait before attempting to register again"),
            ),
        ));
    }

    // Normalize and validate email format (RFC 5322 compliant)
    let email = normalize_email(&req.email);
    if !is_valid_email(&email) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid email format")
                    .with_code("INVALID_EMAIL")
                    .with_string_details("please provide a valid email address"),
            ),
        ));
    }

    // Validate password length
    if req.password.len() < MIN_PASSWORD_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("password too short")
                    .with_code("WEAK_PASSWORD")
                    .with_string_details(format!(
                        "password must be at least {} characters",
                        MIN_PASSWORD_LENGTH
                    )),
            ),
        ));
    }

    // Check if email is already registered
    match state.db.get_user_by_email(&email).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::new("email already registered")
                        .with_code("EMAIL_EXISTS")
                        .with_string_details("an account with this email already exists"),
                ),
            ));
        }
        Ok(None) => {} // Good - email not taken
        Err(e) => {
            warn!(error = %e, email = %email, "Failed to check email uniqueness");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            ));
        }
    }

    // Generate IDs
    let tenant_id = format!("tenant-{}", Uuid::now_v7());
    let display_name = req
        .display_name
        .unwrap_or_else(|| email.split('@').next().unwrap_or("User").to_string());

    info!(email = %email, tenant_id = %tenant_id, "Creating new user registration");

    // Create tenant
    if let Err(e) = sqlx::query(
        "INSERT INTO tenants (id, name, itar_flag, created_at) VALUES (?, ?, 0, datetime('now'))",
    )
    .bind(&tenant_id)
    .bind(&display_name)
    .execute(state.db.pool())
    .await
    {
        warn!(error = %e, tenant_id = %tenant_id, "Failed to create tenant");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("failed to create tenant").with_code("DATABASE_ERROR")),
        ));
    }

    // Initialize tenant policy bindings
    if let Err(e) = state
        .db
        .initialize_tenant_policy_bindings(&tenant_id, &tenant_id)
        .await
    {
        warn!(
            error = %e,
            tenant_id = %tenant_id,
            "Failed to initialize tenant policy bindings (non-fatal)"
        );
    }

    // Hash password
    let pw_hash = hash_password(&req.password).map_err(|e| {
        warn!(error = %e, email = %email, "Failed to hash password");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("password hashing failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create user
    let user_id = state
        .db
        .create_user(&email, &display_name, &pw_hash, Role::Admin, &tenant_id)
        .await
        .map_err(|e| {
            warn!(error = %e, email = %email, tenant_id = %tenant_id, "Failed to create user");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("user creation failed").with_code("DATABASE_ERROR")),
            )
        })?;

    info!(user_id = %user_id, email = %email, tenant_id = %tenant_id, "User created successfully");

    // Create default workspace for user
    let ws_id = format!("ws-{}", Uuid::now_v7());
    if let Err(e) = sqlx::query(
        "INSERT INTO workspaces (id, name, description, created_by, created_at, updated_at) VALUES (?, 'Default Workspace', 'Auto-created workspace', ?, datetime('now'), datetime('now'))",
    )
    .bind(&ws_id)
    .bind(&user_id)
    .execute(state.db.pool())
    .await
    {
        warn!(error = %e, workspace_id = %ws_id, user_id = %user_id, "Failed to create default workspace");
    } else {
        // Add user as workspace owner
        if let Err(e) = state
            .db
            .add_workspace_member(
                &ws_id,
                &tenant_id,
                Some(&user_id),
                WorkspaceRole::Owner,
                None,
                &user_id,
            )
            .await
        {
            warn!(
                error = %e,
                workspace_id = %ws_id,
                user_id = %user_id,
                "Failed to add user as workspace owner"
            );
        }
    }

    // Grant user access to their tenant
    if let Err(e) = adapteros_db::grant_user_tenant_access(
        &state.db,
        &user_id,
        &tenant_id,
        "registration",
        Some("Auto-granted on registration"),
        None,
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %user_id,
            tenant_id = %tenant_id,
            "Failed to grant tenant access (may already exist)"
        );
    }

    // Extract user agent for session tracking
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Generate tokens
    let session_id = format!("sess-{}", Uuid::now_v7());
    let role = "admin".to_string();
    let roles_vec = vec![role.clone()];
    let admin_tenants = vec![tenant_id.clone()];

    let access_params = AccessTokenParams {
        user_id: &user_id,
        email: &email,
        role: &role,
        roles: &roles_vec,
        tenant_id: &tenant_id,
        admin_tenants: &admin_tenants,
        device_id: None,
        session_id: &session_id,
        mfa_level: None,
    };
    let token = issue_access_token(&state, &access_params, Some(auth_cfg.access_ttl())).map_err(
        |e| {
            warn!(error = %e, user_id = %user_id, "Failed to generate access token");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
            )
        },
    )?;

    let rot_id = format!("rot-{}", Uuid::now_v7());
    let refresh_params = RefreshTokenParams {
        user_id: &user_id,
        tenant_id: &tenant_id,
        roles: &roles_vec,
        device_id: None,
        session_id: &session_id,
        rot_id: &rot_id,
    };
    let refresh_token =
        issue_refresh_token(&state, &refresh_params, Some(auth_cfg.effective_ttl())).map_err(
            |e| {
                warn!(error = %e, user_id = %user_id, "Failed to generate refresh token");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
                )
            },
        )?;

    // Validate refresh token to get expiry
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
        warn!(error = %e, user_id = %user_id, "Refresh token validation failed after generation");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create session record
    let response_expires_in = std::cmp::max(auth_cfg.access_ttl(), 60);
    let session_ttl = std::cmp::max(auth_cfg.effective_ttl(), response_expires_in);
    let session_expires_at = Utc::now() + Duration::seconds(session_ttl as i64);
    let refresh_expires_at = chrono::DateTime::from_timestamp(refresh_claims.exp, 0)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let refresh_hash = blake3::hash(refresh_token.as_bytes()).to_hex().to_string();

    upsert_user_session(
        &state.db,
        &session_id,
        &user_id,
        &tenant_id,
        None,
        Some(&rot_id),
        Some(&refresh_hash),
        session_expires_at.timestamp(),
        &refresh_expires_at.to_rfc3339(),
        Some(&client_ip.0),
        user_agent.as_deref(),
        false,
    )
    .await
    .map_err(|e| {
        warn!(error = %e, user_id = %user_id, "Failed to create session");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // Attach cookies
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|e| {
        warn!(error = %e, user_id = %user_id, "Failed to attach auth cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &refresh_token, &auth_cfg).map_err(|e| {
        warn!(error = %e, user_id = %user_id, "Failed to attach refresh cookie");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Track registration attempt for rate limiting
    if let Err(e) = track_registration_attempt(&state.db, &client_ip.0).await {
        warn!(error = %e, ip = %client_ip.0, "Failed to track registration attempt (non-fatal)");
    }

    log_auth_event(
        AuthEvent::RegistrationSuccess,
        Some(&user_id),
        None, // Don't log email on success (privacy)
        Some(&tenant_id),
        Some(&client_ip.0),
        Some(&session_id),
        None,
    );

    Ok((
        response_headers,
        Json(RegisterResponse {
            user_id,
            tenant_id,
            token,
            expires_in: response_expires_in,
        }),
    ))
}
