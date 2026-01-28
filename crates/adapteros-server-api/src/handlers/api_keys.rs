use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
pub use adapteros_api_types::api_keys::{
    ApiKeyInfo, ApiKeyListResponse, CreateApiKeyRequest, CreateApiKeyResponse, RevokeApiKeyResponse,
};
use adapteros_core::AosError;
use adapteros_db::users::Role;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use base64::Engine;
use blake3::Hasher;
use chrono::Utc;
use rand::rngs::OsRng;
use rand::RngCore;
use std::str::FromStr;
use tracing::warn;

fn parse_scopes(scopes: &[String]) -> Result<Vec<Role>, AosError> {
    scopes
        .iter()
        .map(|s| Role::from_str(s))
        .collect::<Result<Vec<Role>, _>>()
}

fn enforce_scope_subset(claims: &Claims, scopes: &[Role]) -> Result<(), AosError> {
    let caller_role = Role::from_str(&claims.role)?;
    if caller_role == Role::Admin {
        return Ok(());
    }

    // Non-admins can only mint keys equal to their own role
    if scopes.iter().any(|s| s != &caller_role) {
        warn!(
            target: "security.api_key",
            caller_id = %claims.sub,
            caller_role = %caller_role,
            requested_scopes = ?scopes,
            "Scope escalation attempt blocked"
        );
        return Err(AosError::Authz(
            "scope not allowed for caller role".to_string(),
        ));
    }
    Ok(())
}

fn generate_token() -> (String, String) {
    let mut raw = [0u8; 32];
    OsRng.fill_bytes(&mut raw);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let mut hasher = Hasher::new();
    hasher.update(token.as_bytes());
    let hash = hasher.finalize().to_hex().to_string();
    (token, hash)
}

#[utoipa::path(
    post,
    path = "/v1/api-keys",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created", body = CreateApiKeyResponse)
    ),
    tag = "auth",
    security(("bearer_token" = []))
)]
pub async fn create_api_key(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let scopes = parse_scopes(&req.scopes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid scopes")
                    .with_code("VALIDATION_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    enforce_scope_subset(&claims, &scopes).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("forbidden")
                    .with_code("FORBIDDEN")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("name required").with_code("VALIDATION_ERROR")),
        ));
    }

    let (token, hash) = generate_token();
    let created_at = Utc::now().to_rfc3339();

    let id = state
        .db
        .create_api_key(
            &claims.tenant_id,
            &claims.sub,
            req.name.trim(),
            &scopes,
            &hash,
        )
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(CreateApiKeyResponse {
        id,
        token,
        created_at,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/api-keys",
    responses(
        (status = 200, description = "List API keys", body = ApiKeyListResponse)
    ),
    tag = "auth",
    security(("bearer_token" = []))
)]
pub async fn list_api_keys(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ApiKeyListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let rows = state
        .db
        .list_api_keys(&claims.tenant_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let api_keys = rows
        .into_iter()
        .map(|row| {
            let scopes = row
                .parsed_scopes()
                .unwrap_or_default()
                .iter()
                .map(|r| r.to_string())
                .collect();
            ApiKeyInfo {
                id: row.id,
                name: row.name,
                scopes,
                created_at: row.created_at,
                revoked_at: row.revoked_at,
            }
        })
        .collect();

    Ok(Json(ApiKeyListResponse { api_keys }))
}

#[utoipa::path(
    delete,
    path = "/v1/api-keys/{id}",
    responses(
        (status = 200, description = "API key revoked", body = RevokeApiKeyResponse)
    ),
    params(
        ("id" = String, Path, description = "API key id")
    ),
    tag = "auth",
    security(("bearer_token" = []))
)]
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<RevokeApiKeyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    state
        .db
        .revoke_api_key(&claims.tenant_id, &id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(RevokeApiKeyResponse { id, revoked: true }))
}
