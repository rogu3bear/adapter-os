//! Tenant management handlers
//!
//! Contains handlers for listing and switching tenants.

use crate::auth::{
    generate_token_ed25519_with_admin_tenants, generate_token_with_admin_tenants, Claims,
};
use crate::auth_common::{attach_auth_cookie, attach_refresh_cookie, AuthConfig};
use crate::security::create_session;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{SwitchTenantRequest, SwitchTenantResponse, TenantListResponse};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{Duration, Utc};
use tracing::warn;

use super::helpers::ADMIN_TENANT_WILDCARD;
use super::tokens::collect_tenant_summaries;

/// List tenants the current user can access (for tenant picker)
#[utoipa::path(
    get,
    path = "/v1/auth/tenants",
    responses(
        (status = 200, description = "User tenants", body = TenantListResponse),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth"
)]
pub async fn list_user_tenants_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<TenantListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenants = collect_tenant_summaries(
        &state,
        &claims.sub,
        &claims.role,
        &claims.tenant_id,
        &claims.admin_tenants,
    )
    .await?;

    Ok(Json(TenantListResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenants,
    }))
}

/// Switch active tenant (re-issue access + refresh tokens)
#[utoipa::path(
    post,
    path = "/v1/auth/tenants/switch",
    request_body = SwitchTenantRequest,
    responses(
        (status = 200, description = "Tenant switched", body = SwitchTenantResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Tenant access denied")
    ),
    tag = "auth"
)]
pub async fn switch_tenant_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SwitchTenantRequest>,
) -> Result<(HeaderMap, Json<SwitchTenantResponse>), (StatusCode, Json<ErrorResponse>)> {
    let target_tenant = req.tenant_id;
    let dev_no_auth = cfg!(debug_assertions) && std::env::var("AOS_DEV_NO_AUTH").is_ok();

    // Fast path: same tenant
    if target_tenant == claims.tenant_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("already on tenant")
                    .with_code("TENANT_ALREADY_ACTIVE")
                    .with_string_details("requested tenant is already active"),
            ),
        ));
    }

    // Verify access
    let mut allowed = false;
    if claims.role == "admin" {
        if claims
            .admin_tenants
            .iter()
            .any(|t| t == ADMIN_TENANT_WILDCARD)
            || claims.admin_tenants.contains(&target_tenant)
        {
            allowed = true;
        } else if let Ok(grants) =
            adapteros_db::get_user_tenant_access(&state.db, &claims.sub).await
        {
            allowed = grants.contains(&target_tenant);
        }
    } else if claims.tenant_id == target_tenant {
        allowed = true;
    }

    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("tenant access denied")
                    .with_code("TENANT_ACCESS_DENIED")
                    .with_failure_code(adapteros_api_types::FailureCode::TenantAccessDenied)
                    .with_string_details(
                        "You have no role in this tenant. Request access from an admin.",
                    ),
            ),
        ));
    }

    // Dev no-auth bypass: synthesize a response without hitting user DB/session tables.
    if dev_no_auth {
        let tenants = collect_tenant_summaries(
            &state,
            &claims.sub,
            &claims.role,
            &target_tenant,
            &claims.admin_tenants,
        )
        .await?;

        let headers = HeaderMap::new();
        return Ok((
            headers,
            Json(SwitchTenantResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                token: "dev-no-auth".to_string(),
                user_id: claims.sub.clone(),
                tenant_id: target_tenant,
                role: claims.role.clone(),
                expires_in: (claims.exp - claims.iat).max(0) as u64,
                tenants: Some(tenants),
                mfa_level: None,
            }),
        ));
    }

    // Load user to get role/email (authoritative)
    let user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            warn!(
                error = %e,
                user_id = %claims.sub,
                target_tenant = %target_tenant,
                "Failed to load user for tenant switch"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("user not found").with_code("UNAUTHORIZED")),
            )
        })?;

    let auth_cfg = AuthConfig::from_state(&state);

    let admin_tenants = if user.role == "admin" {
        adapteros_db::get_user_tenant_access(&state.db, &user.id)
            .await
            .unwrap_or_else(|_| claims.admin_tenants.clone())
    } else {
        vec![]
    };

    let access_token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.access_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.access_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to generate access token for tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let refresh_token = if state.use_ed25519 {
        generate_token_ed25519_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.ed25519_keypair,
            auth_cfg.effective_ttl(),
        )
    } else {
        generate_token_with_admin_tenants(
            &user.id,
            &user.email,
            &user.role,
            &target_tenant,
            &admin_tenants,
            &state.jwt_secret,
            auth_cfg.effective_ttl(),
        )
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to generate refresh token for tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("token generation failed").with_code("INTERNAL_ERROR")),
        )
    })?;

    let access_claims = if state.use_ed25519 {
        crate::auth::validate_token_ed25519(
            &access_token,
            &state.ed25519_public_keys,
            &state.ed25519_public_key,
        )
    } else {
        crate::auth::validate_token(&access_token, &state.hmac_keys, state.jwt_secret.as_slice())
    }
    .map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Token validation failed after tenant switch generation"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
        )
    })?;

    let expires_at = Utc::now() + Duration::seconds(auth_cfg.effective_ttl() as i64);
    if let Err(e) = create_session(
        &state.db,
        &access_claims.jti,
        &user.id,
        &target_tenant,
        &expires_at.to_rfc3339(),
        None,
        None,
    )
    .await
    {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to create session during tenant switch"
        );
    }

    let tenants =
        collect_tenant_summaries(&state, &user.id, &user.role, &target_tenant, &admin_tenants)
            .await?;

    let mut headers = HeaderMap::new();
    attach_auth_cookie(&mut headers, &access_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to attach auth cookie during tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;
    attach_refresh_cookie(&mut headers, &refresh_token, &auth_cfg).map_err(|e| {
        warn!(
            error = %e,
            user_id = %user.id,
            tenant_id = %target_tenant,
            "Failed to attach refresh cookie during tenant switch"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("cookie error").with_code("INTERNAL_ERROR")),
        )
    })?;

    Ok((
        headers,
        Json(SwitchTenantResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            token: access_token,
            user_id: user.id,
            tenant_id: target_tenant,
            role: user.role,
            expires_in: auth_cfg.access_ttl(),
            tenants: Some(tenants),
            mfa_level: None,
        }),
    ))
}
