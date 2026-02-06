//! Development bypass handlers
//!
//! Contains handlers for development-only authentication bypasses.
//! Only available in debug builds with the dev-bypass feature.

#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::auth::{
    generate_token_ed25519_with_admin_tenants, generate_token_with_admin_tenants, hash_password,
    issue_access_token_ed25519, issue_access_token_hmac, issue_refresh_token_ed25519,
    issue_refresh_token_hmac, validate_refresh_token_ed25519, validate_refresh_token_hmac,
    AuthMode, Claims, PrincipalType, JWT_ISSUER,
};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::auth_common::{attach_auth_cookie, attach_refresh_cookie, AuthConfig, AuthContext};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::ip_extraction::ClientIp;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::security::{create_session, upsert_user_session};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::state::AppState;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use crate::types::ErrorResponse;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use adapteros_api_types::auth::{LoginResponse, TenantSummary};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use adapteros_api_types::API_SCHEMA_VERSION;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use adapteros_db::users::{Role, User};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use adapteros_db::workspaces::WorkspaceRole;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use blake3;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use chrono::{DateTime, Duration, Utc};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use sqlx;
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use tracing::{info, warn};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use adapteros_id::{TypedId, IdPrefix};

#[cfg(all(feature = "dev-bypass", debug_assertions))]
use super::audit::{log_auth_event, AuthEvent};
#[cfg(all(feature = "dev-bypass", debug_assertions))]
use super::types::{DevBootstrapRequest, DevBootstrapResponse};

#[cfg(all(feature = "dev-bypass", debug_assertions))]
const ADMIN_TENANT_WILDCARD: &str = "*";

/// Development bypass handler - creates admin user session
/// Only available in debug builds - generates proper JWT even in dev mode
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[utoipa::path(
    post,
    path = "/v1/auth/dev-bypass",
    responses(
        (status = 200, description = "Dev bypass successful", body = LoginResponse),
        (status = 403, description = "Not in development mode")
    ),
    tag = "auth"
)]
pub async fn dev_bypass_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Extension(client_ip): Extension<ClientIp>,
) -> Result<(HeaderMap, Json<LoginResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    if !auth_cfg.dev_login_allowed() {
        let guard_claims = Claims {
            sub: "dev-bypass".to_string(),
            email: "dev-bypass@adapteros.local".to_string(),
            role: "system".to_string(),
            roles: vec!["system".to_string()],
            tenant_id: "system".to_string(),
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
            auth_mode: AuthMode::DevBypass,
            principal_type: Some(PrincipalType::DevBypass),
        };
        log_auth_event(
            AuthEvent::DevBypassLogin,
            Some(&guard_claims.sub),
            Some(&guard_claims.email),
            Some(&guard_claims.tenant_id),
            Some(&client_ip.0),
            None,
            Some("dev bypass disabled"),
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("dev bypass not available")
                    .with_code("DEV_BYPASS_DISABLED")
                    .with_string_details("this endpoint is only available in development mode"),
            ),
        ));
    }

    // Extract user agent from headers for session tracking
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Create dev admin user details
    let user_id = "dev-admin-user".to_string();
    let email = "dev-admin@adapteros.local".to_string();
    let role = "admin".to_string();
    let tenant_id = "default".to_string();

    // Ensure the "default" tenant exists in the database
    match sqlx::query_scalar::<_, String>("SELECT id FROM tenants WHERE id = 'default'")
        .fetch_optional(state.db.pool())
        .await
    {
        Ok(Some(_)) => {
            info!(tenant_id = %tenant_id, "Default tenant already exists");
        }
        Ok(None) => {
            info!(tenant_id = %tenant_id, "Creating default tenant for dev mode");
            if let Err(e) = sqlx::query(
                "INSERT INTO tenants (id, name, itar_flag, created_at) VALUES ('default', 'Default', 0, datetime('now'))",
            )
            .execute(state.db.pool())
            .await
            {
                warn!(error = %e, tenant_id = %tenant_id, "Failed to create default tenant");
            }
        }
        Err(e) => {
            warn!(error = %e, tenant_id = %tenant_id, "Failed to check for default tenant");
        }
    }

    // Ensure the dev user exists in the database so /auth/me works
    info!(user_id = %user_id, "Creating/updating dev user in database");
    match state
        .db
        .ensure_user(
            &user_id,
            &email,
            "Developer Admin",
            "", // empty password hash for dev user
            adapteros_db::users::Role::Admin,
            &tenant_id,
        )
        .await
    {
        Ok(()) => {
            info!(user_id = %user_id, "Dev user ensured in database successfully");
        }
        Err(e) => {
            warn!(
                error = %e,
                user_id = %user_id,
                tenant_id = %tenant_id,
                "Failed to ensure dev user exists in database, continuing anyway"
            );
            // Don't fail - the user can still authenticate, they just won't see their profile in /me
        }
    }

    // Ensure a default workspace exists for the dev user
    match sqlx::query_scalar::<_, String>("SELECT id FROM workspaces WHERE created_by = ? LIMIT 1")
        .bind(&user_id)
        .fetch_optional(state.db.pool())
        .await
    {
        Ok(Some(ws_id)) => {
            info!(workspace_id = %ws_id, user_id = %user_id, "Dev user already has a workspace");
            // Ensure the dev user has membership (may have been created without it)
            if let Ok(None) = state
                .db
                .check_workspace_access(&ws_id, &user_id, &tenant_id)
                .await
            {
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
                    warn!(error = %e, workspace_id = %ws_id, user_id = %user_id, "Failed to add dev user as workspace member");
                } else {
                    info!(workspace_id = %ws_id, user_id = %user_id, "Added dev user as workspace owner (retroactive)");
                }
            }
        }
        Ok(None) => {
            let ws_id =
                crate::id_generator::readable_id(adapteros_id::IdPrefix::Wsp, "dev");
            info!(workspace_id = %ws_id, user_id = %user_id, "Creating default workspace for dev user");
            if let Err(e) = sqlx::query(
                "INSERT INTO workspaces (id, name, description, created_by, created_at, updated_at) VALUES (?, 'Default Workspace', 'Auto-created workspace for development', ?, datetime('now'), datetime('now'))",
            )
            .bind(&ws_id)
            .bind(&user_id)
            .execute(state.db.pool())
            .await
            {
                warn!(error = %e, workspace_id = %ws_id, user_id = %user_id, "Failed to create default workspace");
            } else {
                // Add the dev user as owner of the workspace
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
                    warn!(error = %e, workspace_id = %ws_id, user_id = %user_id, "Failed to add dev user as workspace member");
                } else {
                    info!(workspace_id = %ws_id, user_id = %user_id, "Added dev user as workspace owner");
                }
            }
        }
        Err(e) => {
            warn!(error = %e, user_id = %user_id, "Failed to check for existing workspaces");
        }
    }

    // Ensure the dev user has access to "system" workspace (used by frontend for /v1/workspaces/system/active)
    match sqlx::query_scalar::<_, String>("SELECT id FROM workspaces WHERE id = 'system'")
        .fetch_optional(state.db.pool())
        .await
    {
        Ok(Some(_)) => {
            // System workspace exists, ensure dev user has membership
            if let Ok(None) = state
                .db
                .check_workspace_access("system", &user_id, &tenant_id)
                .await
            {
                if let Err(e) = state
                    .db
                    .add_workspace_member(
                        "system",
                        &tenant_id,
                        Some(&user_id),
                        WorkspaceRole::Owner,
                        None,
                        &user_id,
                    )
                    .await
                {
                    warn!(error = %e, workspace_id = "system", user_id = %user_id, "Failed to add dev user to system workspace");
                } else {
                    info!(workspace_id = "system", user_id = %user_id, "Added dev user to system workspace");
                }
            }
        }
        Ok(None) => {
            // Create system workspace
            info!(workspace_id = "system", user_id = %user_id, "Creating system workspace for dev user");
            if let Err(e) = sqlx::query(
                "INSERT INTO workspaces (id, name, description, created_by, created_at, updated_at) VALUES ('system', 'System Workspace', 'System workspace for development', ?, datetime('now'), datetime('now'))",
            )
            .bind(&user_id)
            .execute(state.db.pool())
            .await
            {
                warn!(error = %e, workspace_id = "system", user_id = %user_id, "Failed to create system workspace");
            } else {
                // Add the dev user as owner of the system workspace
                if let Err(e) = state
                    .db
                    .add_workspace_member(
                        "system",
                        &tenant_id,
                        Some(&user_id),
                        WorkspaceRole::Owner,
                        None,
                        &user_id,
                    )
                    .await
                {
                    warn!(error = %e, workspace_id = "system", user_id = %user_id, "Failed to add dev user to system workspace");
                } else {
                    info!(workspace_id = "system", user_id = %user_id, "Added dev user to system workspace");
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to check for system workspace");
        }
    }

    let dev_user = User {
        id: user_id.clone(),
        email: email.clone(),
        display_name: "Developer Admin".to_string(),
        pw_hash: String::new(),
        role: role.clone(),
        disabled: false,
        created_at: Utc::now().to_rfc3339(),
        tenant_id: tenant_id.clone(),
        failed_attempts: 0,
        last_failed_at: None,
        lockout_until: None,
        mfa_enabled: false,
        mfa_secret_enc: None,
        mfa_backup_codes_json: None,
        mfa_enrolled_at: None,
        mfa_last_verified_at: None,
        mfa_recovery_last_used_at: None,
        password_rotated_at: None,
        token_rotated_at: None,
        last_login_at: Some(Utc::now().to_rfc3339()),
    };

    let mut ctx = AuthContext::from_user(dev_user).map_err(|err| {
        warn!(
            error = %err,
            user_id = %user_id,
            tenant_id = %tenant_id,
            "Failed to build dev auth context"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    if role == "admin" {
        ctx.admin_tenants = vec![ADMIN_TENANT_WILDCARD.to_string()];
    }

    let session_id = TypedId::new(IdPrefix::Ses).to_string();
    let roles_vec = vec![ctx.role.to_string()];

    let token = if state.use_ed25519 {
        issue_access_token_ed25519(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &roles_vec,
            &ctx.tenant_id,
            &ctx.admin_tenants,
            None,
            &session_id,
            None,
            &state.ed25519_keypair,
            Some(auth_cfg.access_ttl()),
        )
    } else {
        issue_access_token_hmac(
            &ctx.user.id,
            &ctx.user.email,
            &ctx.role.to_string(),
            &roles_vec,
            &ctx.tenant_id,
            &ctx.admin_tenants,
            None,
            &session_id,
            None,
            &state.jwt_secret,
            Some(auth_cfg.access_ttl()),
        )
    }
    .map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to generate dev bypass token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let rot_id = TypedId::new(IdPrefix::Rot).to_string();
    let refresh_token = if state.use_ed25519 {
        issue_refresh_token_ed25519(
            &ctx.user.id,
            &ctx.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.ed25519_keypair,
            Some(auth_cfg.effective_ttl()),
        )
    } else {
        issue_refresh_token_hmac(
            &ctx.user.id,
            &ctx.tenant_id,
            &roles_vec,
            None,
            &session_id,
            &rot_id,
            &state.jwt_secret,
            Some(auth_cfg.effective_ttl()),
        )
    }
    .map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to generate dev bypass refresh token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
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
        warn!(
            error = %e,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Refresh token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let response_expires_in = std::cmp::max(auth_cfg.access_ttl(), 60);
    let session_ttl = std::cmp::max(auth_cfg.effective_ttl(), response_expires_in);
    let session_expires_at = Utc::now() + Duration::seconds(session_ttl as i64);
    let refresh_expires_at = DateTime::from_timestamp(refresh_claims.exp, 0)
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
        warn!(
            error = %e,
            user_id = %user_id,
            tenant_id = %tenant_id,
            session_id = %session_id,
            "Failed to upsert dev bypass session - aborted"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    log_auth_event(
        AuthEvent::DevBypassLogin,
        Some(&claims.sub),
        Some(&claims.email),
        Some(&claims.tenant_id),
        Some(&client_ip.0),
        Some(&claims.jti),
        None,
    );

    info!(
        user_id = %user_id,
        email = %email,
        ip = %client_ip.0,
        "Dev bypass login successful"
    );

    let admin_tenants = adapteros_db::get_user_tenant_access(&state.db, &ctx.user.id)
        .await
        .unwrap_or_default();
    let role_string = ctx.role.to_string();
    let tenants = collect_tenant_summaries(
        &state,
        &ctx.user.id,
        &role_string,
        &ctx.user.tenant_id,
        &admin_tenants,
    )
    .await?;

    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to attach auth cookie for dev bypass"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut response_headers, &refresh_token, &auth_cfg).map_err(|err| {
        warn!(
            error = %err,
            user_id = %ctx.user.id,
            tenant_id = %ctx.tenant_id,
            session_id = %session_id,
            "Failed to attach refresh cookie for dev bypass"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        response_headers,
        Json(LoginResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            token,
            user_id: ctx.user.id.clone(),
            tenant_id: ctx.user.tenant_id.clone(),
            role: ctx.role.to_string(),
            expires_in: response_expires_in,
            tenants: Some(tenants),
            mfa_level: None,
        }),
    ))
}

#[cfg(all(feature = "dev-bypass", debug_assertions))]
async fn collect_tenant_summaries(
    state: &AppState,
    _user_id: &str,
    _role: &str,
    tenant_id: &str,
    admin_tenants: &[String],
) -> Result<Vec<TenantSummary>, (StatusCode, Json<ErrorResponse>)> {
    let has_wildcard = admin_tenants.iter().any(|t| t == ADMIN_TENANT_WILDCARD);

    if has_wildcard {
        let (db_tenants, _total) = state.db.list_tenants_paginated(100, 0).await.map_err(|e| {
            warn!(error = %e, "Failed to list tenants for dev bypass");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve tenants").with_code("DATABASE_ERROR")),
            )
        })?;

        return Ok(db_tenants
            .into_iter()
            .map(|t| TenantSummary {
                schema_version: API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            })
            .collect());
    }

    let mut tenant_summaries = Vec::new();

    if let Ok(Some(primary)) = state.db.get_tenant(tenant_id).await {
        tenant_summaries.push(TenantSummary {
            schema_version: API_SCHEMA_VERSION.to_string(),
            id: primary.id,
            name: primary.name,
            status: primary.status,
            created_at: Some(primary.created_at),
        });
    }

    for admin_tenant in admin_tenants {
        if admin_tenant != tenant_id {
            if let Ok(Some(tenant)) = state.db.get_tenant(admin_tenant).await {
                tenant_summaries.push(TenantSummary {
                    schema_version: API_SCHEMA_VERSION.to_string(),
                    id: tenant.id,
                    name: tenant.name,
                    status: tenant.status,
                    created_at: Some(tenant.created_at),
                });
            }
        }
    }

    Ok(tenant_summaries)
}

/// Development bootstrap endpoint - creates system tenant and admin user
///
/// This endpoint is only available in debug builds with the dev-bypass feature.
/// It creates the "system" tenant if it doesn't exist, creates an admin user,
/// grants the admin access to the system tenant, and returns a valid JWT token.
///
/// **WARNING**: This endpoint is for development only and should never be enabled in production.
#[cfg(all(feature = "dev-bypass", debug_assertions))]
#[utoipa::path(
    post,
    path = "/v1/dev/bootstrap",
    request_body = DevBootstrapRequest,
    responses(
        (status = 200, description = "Dev environment bootstrapped", body = DevBootstrapResponse),
        (status = 403, description = "Not in development mode"),
        (status = 500, description = "Internal error")
    ),
    tag = "dev"
)]
pub async fn dev_bootstrap_handler(
    State(state): State<AppState>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<DevBootstrapRequest>,
) -> Result<(HeaderMap, Json<DevBootstrapResponse>), (StatusCode, Json<ErrorResponse>)> {
    let auth_cfg = AuthConfig::from_state(&state);

    if !auth_cfg.dev_login_allowed() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("dev bootstrap not available")
                    .with_code("DEV_BOOTSTRAP_DISABLED")
                    .with_string_details("this endpoint is only available in development mode"),
            ),
        ));
    }

    info!(email = %req.email, ip = %client_ip.0, "Dev bootstrap requested");

    // 1. Create or get "system" tenant
    let system_tenant_id = match sqlx::query_scalar::<_, String>(
        "SELECT id FROM tenants WHERE id = 'system'",
    )
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(Some(id)) => {
            info!(tenant_id = %id, "System tenant already exists");
            id
        }
        Ok(None) => {
            // Create system tenant with known ID "system"
            info!(tenant_id = %"system", "Creating system tenant");
            sqlx::query(
                "INSERT INTO tenants (id, name, itar_flag, created_at) VALUES ('system', 'System', 0, datetime('now'))"
            )
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                warn!(
                    error = %e,
                    tenant_id = %"system",
                    "Failed to create system tenant"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to create system tenant").with_code("DATABASE_ERROR")),
                )
            })?;

            // Initialize policy bindings for system tenant
            if let Err(e) = state
                .db
                .initialize_tenant_policy_bindings("system", "system")
                .await
            {
                warn!(
                    error = %e,
                    tenant_id = %"system",
                    "Failed to initialize system tenant policy bindings (non-fatal)"
                );
            }

            "system".to_string()
        }
        Err(e) => {
            warn!(error = %e, tenant_id = %"system", "Failed to check for system tenant");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            ));
        }
    };

    // 2. Create admin user (or get existing)
    let admin_user_id = match state.db.get_user_by_email(&req.email).await {
        Ok(Some(user)) => {
            info!(email = %req.email, user_id = %user.id, "Admin user already exists");
            user.id
        }
        Ok(None) => {
            // Hash password and create user
            let pw_hash = hash_password(&req.password).map_err(|e| {
                warn!(
                    error = %e,
                    email = %req.email,
                    tenant_id = %system_tenant_id,
                    "Failed to hash password"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("password hashing failed").with_code("INTERNAL_ERROR")),
                )
            })?;

            let user_id = state
                .db
                .create_user(
                    &req.email,
                    "Dev Admin",
                    &pw_hash,
                    Role::Admin,
                    &system_tenant_id,
                )
                .await
                .map_err(|e| {
                    warn!(
                        error = %e,
                        email = %req.email,
                        tenant_id = %system_tenant_id,
                        "Failed to create admin user"
                    );
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("user creation failed").with_code("DATABASE_ERROR"),
                        ),
                    )
                })?;

            info!(user_id = %user_id, email = %req.email, "Created dev admin user");
            user_id
        }
        Err(e) => {
            warn!(
                error = %e,
                email = %req.email,
                tenant_id = %system_tenant_id,
                "Failed to check for admin user"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("database error").with_code("DATABASE_ERROR")),
            ));
        }
    };

    // 3. Grant admin access to system tenant
    if let Err(e) = adapteros_db::grant_user_tenant_access(
        &state.db,
        &admin_user_id,
        &system_tenant_id,
        "dev-bootstrap",
        Some("Dev bootstrap auto-grant"),
        None, // No expiration
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to grant tenant access (may already exist)"
        );
        // Non-fatal - may already have access
    }

    // 4. Generate token with admin_tenants populated
    let admin_tenants = vec![system_tenant_id.clone()];

    let token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &admin_user_id,
            &req.email,
            "admin",
            &system_tenant_id,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.effective_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &admin_user_id,
            &req.email,
            "admin",
            &system_tenant_id,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.effective_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to generate token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Validate the generated token
    let claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Token validation failed after generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Create session
    let expires_at = Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64);
    create_session(
        &state.db,
        &claims.jti,
        &admin_user_id,
        &system_tenant_id,
        expires_at.timestamp(),
        Some(&client_ip.0),
        None,
    )
    .await
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to create session"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("session creation failed").with_code("SESSION_ERROR")),
        )
    })?;

    // 5. Attach cookie
    let mut response_headers = HeaderMap::new();
    attach_auth_cookie(&mut response_headers, &token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %admin_user_id,
            tenant_id = %system_tenant_id,
            "Failed to attach auth cookie"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    info!(
        admin_user_id = %admin_user_id,
        email = %req.email,
        system_tenant_id = %system_tenant_id,
        ip = %client_ip.0,
        "Dev bootstrap completed successfully"
    );

    Ok((
        response_headers,
        Json(DevBootstrapResponse {
            system_tenant_id,
            admin_user_id,
            email: req.email,
            password: req.password,
            token,
            message: "Dev environment bootstrapped. Use the token or credentials to authenticate. This endpoint is only available in debug builds.".to_string(),
        }),
    ))
}
