// Adapter Version Archive/Unarchive Handlers
//
// This module provides REST API endpoints for:
// - Archiving adapter versions
// - Unarchiving adapter versions

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use tracing::{error, info};

// ============================================================================
// Handlers
// ============================================================================

/// Archive an adapter version.
///
/// Archived versions are hidden from normal use but retain their lifecycle_state
/// for audit purposes. Use unarchive to restore visibility.
#[utoipa::path(
    post,
    path = "/v1/adapter-versions/{version_id}/archive",
    params(
        ("version_id" = String, Path, description = "Adapter version ID to archive"),
    ),
    request_body = adapteros_api_types::training::ArchiveAdapterVersionRequest,
    responses(
        (status = 200, description = "Version archived successfully", body = adapteros_api_types::training::ArchiveAdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn archive_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
    Json(_req): Json<adapteros_api_types::training::ArchiveAdapterVersionRequest>,
) -> Result<
    Json<adapteros_api_types::training::ArchiveAdapterVersionResponse>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::AdapterLoad)?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Verify version exists and belongs to tenant
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for archive"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(version_id.clone()),
                ),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &version.tenant_id)?;

    state
        .db
        .archive_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                version_id = %version_id,
                error = %e,
                "Failed to archive adapter version"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to archive version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        version_id = %version_id,
        tenant_id = %claims.tenant_id,
        actor = %claims.sub,
        "Archived adapter version"
    );

    Ok(Json(
        adapteros_api_types::training::ArchiveAdapterVersionResponse {
            version_id,
            is_archived: true,
            updated_at: chrono::Utc::now().to_rfc3339(),
        },
    ))
}

/// Unarchive an adapter version.
///
/// Restores visibility of an archived version.
#[utoipa::path(
    post,
    path = "/v1/adapter-versions/{version_id}/unarchive",
    params(
        ("version_id" = String, Path, description = "Adapter version ID to unarchive"),
    ),
    responses(
        (status = 200, description = "Version unarchived successfully", body = adapteros_api_types::training::ArchiveAdapterVersionResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn unarchive_adapter_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(version_id): Path<String>,
) -> Result<
    Json<adapteros_api_types::training::ArchiveAdapterVersionResponse>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::AdapterLoad)?;
    let version_id = crate::id_resolver::resolve_any_id(&state.db, &version_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Verify version exists and belongs to tenant
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for unarchive"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(version_id.clone()),
                ),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &version.tenant_id)?;

    state
        .db
        .unarchive_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                version_id = %version_id,
                error = %e,
                "Failed to unarchive adapter version"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to unarchive version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        version_id = %version_id,
        tenant_id = %claims.tenant_id,
        actor = %claims.sub,
        "Unarchived adapter version"
    );

    Ok(Json(
        adapteros_api_types::training::ArchiveAdapterVersionResponse {
            version_id,
            is_archived: false,
            updated_at: chrono::Utc::now().to_rfc3339(),
        },
    ))
}
