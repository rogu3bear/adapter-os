//! Weight encryption management handlers
//!
//! Endpoints for managing per-tenant weight encryption keys and querying
//! encryption status of adapter weight files.
//!
//! ## Endpoints
//!
//! - `GET  /v1/tenants/:tenant_id/encryption/keys`   — List encryption keys
//! - `POST /v1/tenants/:tenant_id/encryption/keys`    — Register a new key
//! - `DELETE /v1/tenants/:tenant_id/encryption/keys/:key_id` — Revoke a key
//! - `GET  /v1/tenants/:tenant_id/encryption/status`  — Encryption migration status
//! - `GET  /v1/adapters/:adapter_id/encryption`       — Adapter weight file encryption

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;

use adapteros_db::tenant_weight_encryption::{
    dek_fingerprint, derive_tenant_weight_dek, TenantWeightKey,
};
use adapteros_id::{IdPrefix, TypedId};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx;
use utoipa::ToSchema;

// ===== Request / Response Types =====

/// Request body for registering a new tenant weight encryption key.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterKeyRequest {
    /// Optional metadata JSON to associate with the key.
    #[serde(default)]
    pub metadata: Option<String>,
}

/// A tenant weight encryption key.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantWeightKeyResponse {
    pub id: String,
    pub tenant_id: String,
    pub key_fingerprint: String,
    pub algorithm: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub metadata: Option<String>,
    pub active: bool,
}

/// Response for listing tenant encryption keys.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TenantKeyListResponse {
    pub keys: Vec<TenantWeightKeyResponse>,
    pub total: usize,
}

/// Response for encryption migration status.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EncryptionStatusResponse {
    pub tenant_id: String,
    pub has_active_key: bool,
    pub active_key_fingerprint: Option<String>,
    pub plaintext_file_count: i64,
}

/// A weight file encryption record.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WeightFileEncryptionResponse {
    pub id: String,
    pub adapter_id: String,
    pub tenant_id: String,
    pub file_path: String,
    pub encryption_status: String,
    pub key_fingerprint: Option<String>,
    pub algorithm: Option<String>,
    pub original_digest_hex: String,
    pub encrypted_at: Option<String>,
}

/// Response for listing adapter weight file encryption.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterEncryptionListResponse {
    pub adapter_id: String,
    pub files: Vec<WeightFileEncryptionResponse>,
    pub total: usize,
}

// ===== Helpers =====

/// Validate that the authenticated user's tenant matches the path tenant_id.
fn validate_tenant_isolation(claims: &Claims, tenant_id: &str) -> Result<(), ApiError> {
    if claims.tenant_id != tenant_id {
        return Err(
            ApiError::forbidden("cannot access encryption keys for a different tenant")
                .with_details(format!(
                    "authenticated tenant '{}' does not match path tenant '{}'",
                    claims.tenant_id, tenant_id
                )),
        );
    }
    Ok(())
}

fn key_to_response(key: TenantWeightKey) -> TenantWeightKeyResponse {
    let active = key.revoked_at.is_none();
    TenantWeightKeyResponse {
        id: key.id,
        tenant_id: key.tenant_id,
        key_fingerprint: key.key_fingerprint,
        algorithm: key.algorithm,
        created_at: key.created_at,
        revoked_at: key.revoked_at,
        metadata: key.metadata,
        active,
    }
}

// ===== Handlers =====

/// List tenant weight encryption keys (active and revoked).
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/encryption/keys",
    tag = "encryption",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "List of encryption keys", body = TenantKeyListResponse),
        (status = 403, description = "Tenant mismatch", body = ErrorResponse)
    )
)]
pub async fn list_tenant_keys(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> ApiResult<TenantKeyListResponse> {
    require_permission(&claims, Permission::TenantManage)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Query all keys (active + revoked) for this tenant
    let rows = sqlx::query_as::<_, TenantWeightKey>(
        r#"
        SELECT id, tenant_id, key_fingerprint, algorithm,
               created_at, revoked_at, metadata
        FROM tenant_weight_encryption_keys
        WHERE tenant_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&tenant_id)
    .fetch_all(state.db.pool_result()?)
    .await
    .map_err(|e| {
        ApiError::internal("failed to list tenant encryption keys").with_details(e.to_string())
    })?;

    let total = rows.len();
    let keys: Vec<TenantWeightKeyResponse> = rows.into_iter().map(key_to_response).collect();

    Ok(Json(TenantKeyListResponse { keys, total }))
}

/// Register a new tenant weight encryption key.
///
/// Derives a DEK from the tenant ID using HKDF-SHA256, computes a BLAKE3
/// fingerprint, and stores the key metadata. Only one active key per tenant
/// is supported; registering a new key while one exists returns a conflict.
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/encryption/keys",
    tag = "encryption",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = RegisterKeyRequest,
    responses(
        (status = 201, description = "Key registered", body = TenantWeightKeyResponse),
        (status = 403, description = "Tenant mismatch", body = ErrorResponse),
        (status = 409, description = "Active key already exists", body = ErrorResponse)
    )
)]
pub async fn register_tenant_key(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(body): Json<RegisterKeyRequest>,
) -> ApiResult<TenantWeightKeyResponse> {
    require_permission(&claims, Permission::TenantManage)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Check for existing active key
    let existing = state
        .db
        .get_active_tenant_weight_key(&tenant_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to check existing encryption key")
                .with_details(e.to_string())
        })?;

    if existing.is_some() {
        return Err(ApiError::conflict(
            "tenant already has an active encryption key; revoke it before registering a new one",
        ));
    }

    // Derive DEK and compute fingerprint
    let dek = derive_tenant_weight_dek(&tenant_id);
    let fingerprint = dek_fingerprint(&dek);

    let key_id = TypedId::new(IdPrefix::Tok).to_string();
    let algorithm = "chacha20poly1305".to_string();
    let created_at = Utc::now().to_rfc3339();

    state
        .db
        .register_tenant_weight_key(
            &key_id,
            &tenant_id,
            &fingerprint,
            &algorithm,
            &created_at,
            body.metadata.as_deref(),
        )
        .await
        .map_err(|e| {
            ApiError::internal("failed to register tenant encryption key")
                .with_details(e.to_string())
        })?;

    Ok(Json(TenantWeightKeyResponse {
        id: key_id,
        tenant_id,
        key_fingerprint: fingerprint,
        algorithm,
        created_at,
        revoked_at: None,
        metadata: body.metadata,
        active: true,
    }))
}

/// Revoke a tenant weight encryption key.
///
/// Sets the `revoked_at` timestamp, preventing the key from being used for
/// new encryptions. Existing encrypted files remain accessible via the stored
/// key fingerprint until re-encrypted with a new key.
#[utoipa::path(
    delete,
    path = "/v1/tenants/{tenant_id}/encryption/keys/{key_id}",
    tag = "encryption",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("key_id" = String, Path, description = "Key ID to revoke")
    ),
    responses(
        (status = 200, description = "Key revoked", body = TenantWeightKeyResponse),
        (status = 403, description = "Tenant mismatch", body = ErrorResponse),
        (status = 404, description = "Key not found", body = ErrorResponse)
    )
)]
pub async fn revoke_tenant_key(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((tenant_id, key_id)): Path<(String, String)>,
) -> ApiResult<TenantWeightKeyResponse> {
    require_permission(&claims, Permission::TenantManage)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Verify the key exists and belongs to this tenant
    let key = sqlx::query_as::<_, TenantWeightKey>(
        r#"
        SELECT id, tenant_id, key_fingerprint, algorithm,
               created_at, revoked_at, metadata
        FROM tenant_weight_encryption_keys
        WHERE id = ? AND tenant_id = ?
        "#,
    )
    .bind(&key_id)
    .bind(&tenant_id)
    .fetch_optional(state.db.pool_result()?)
    .await
    .map_err(|e| {
        ApiError::internal("failed to look up encryption key").with_details(e.to_string())
    })?
    .ok_or_else(|| {
        ApiError::not_found("Encryption key").with_details(format!(
            "key '{}' does not exist for tenant '{}'",
            key_id, tenant_id
        ))
    })?;

    if key.revoked_at.is_some() {
        return Err(ApiError::conflict("key is already revoked"));
    }

    let revoked_at = Utc::now().to_rfc3339();

    state
        .db
        .revoke_tenant_weight_key(&key_id, &revoked_at)
        .await
        .map_err(|e| {
            ApiError::internal("failed to revoke encryption key").with_details(e.to_string())
        })?;

    Ok(Json(TenantWeightKeyResponse {
        id: key.id,
        tenant_id: key.tenant_id,
        key_fingerprint: key.key_fingerprint,
        algorithm: key.algorithm,
        created_at: key.created_at,
        revoked_at: Some(revoked_at),
        metadata: key.metadata,
        active: false,
    }))
}

/// Get encryption migration status for a tenant.
///
/// Returns the number of plaintext (unencrypted) weight files and whether
/// the tenant has an active encryption key.
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/encryption/status",
    tag = "encryption",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Encryption status", body = EncryptionStatusResponse),
        (status = 403, description = "Tenant mismatch", body = ErrorResponse)
    )
)]
pub async fn get_encryption_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> ApiResult<EncryptionStatusResponse> {
    require_permission(&claims, Permission::TenantView)?;
    validate_tenant_isolation(&claims, &tenant_id)?;

    let active_key = state
        .db
        .get_active_tenant_weight_key(&tenant_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to get active encryption key").with_details(e.to_string())
        })?;

    let plaintext_count = state
        .db
        .count_plaintext_weight_files(&tenant_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to count plaintext weight files").with_details(e.to_string())
        })?;

    Ok(Json(EncryptionStatusResponse {
        tenant_id,
        has_active_key: active_key.is_some(),
        active_key_fingerprint: active_key.map(|k| k.key_fingerprint),
        plaintext_file_count: plaintext_count,
    }))
}

/// List weight file encryption status for an adapter.
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/encryption",
    tag = "encryption",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter weight file encryption list", body = AdapterEncryptionListResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn list_adapter_encryption(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> ApiResult<AdapterEncryptionListResponse> {
    require_permission(&claims, Permission::AdapterView)?;

    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    let files = state
        .db
        .list_adapter_weight_files(&adapter_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to list adapter weight files").with_details(e.to_string())
        })?;

    let total = files.len();
    let file_responses: Vec<WeightFileEncryptionResponse> = files
        .into_iter()
        .map(|f| WeightFileEncryptionResponse {
            id: f.id,
            adapter_id: f.adapter_id,
            tenant_id: f.tenant_id,
            file_path: f.file_path,
            encryption_status: f.encryption_status,
            key_fingerprint: f.key_fingerprint,
            algorithm: f.algorithm,
            original_digest_hex: f.original_digest_hex,
            encrypted_at: f.encrypted_at,
        })
        .collect();

    Ok(Json(AdapterEncryptionListResponse {
        adapter_id,
        files: file_responses,
        total,
    }))
}
