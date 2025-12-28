// Adapter Export Handler
//
// PRD-ART-01: Adapter Export
//
// This module provides REST API endpoints for:
// - Exporting adapters as .aos files

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    response::Json,
    Extension,
};
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};

// ============================================================================
// Handlers
// ============================================================================

/// Export an adapter as a .aos file
///
/// Returns the .aos file as a binary stream for download.
/// The response includes:
/// - Content-Type: application/octet-stream
/// - Content-Disposition: attachment; filename="{adapter_id}.aos"
/// - X-Adapter-Hash: BLAKE3 content hash for verification
///
/// **Permissions:** Requires `AdapterView` permission.
///
/// # Example
/// ```
/// GET /v1/adapters/{adapter_id}/export
/// ```
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/export",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID to export")
    ),
    responses(
        (status = 200, description = "Adapter file stream", content_type = "application/octet-stream"),
        (status = 404, description = "Adapter not found or no .aos file available", body = ErrorResponse),
        (status = 403, description = "Forbidden - tenant isolation violation", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn export_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::AdapterView).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Reject exports during shutdown to prevent race conditions with cleanup
    if state.boot_state.as_ref().is_some_and(|b| b.is_draining() || b.is_shutting_down()) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Service shutting down")
                    .with_code("DRAINING"),
            ),
        ));
    }

    // Get adapter details
    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to get adapter for export"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get adapter")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!(adapter_id = %adapter_id, "Adapter not found for export");
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &adapter.tenant_id)?;

    // Check if adapter is archived/purged
    if adapter.purged_at.is_some() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter has been purged - .aos file no longer available")
                    .with_code("ADAPTER_PURGED"),
            ),
        ));
    }

    // Get the .aos file path
    let aos_path = adapter.aos_file_path.as_ref().ok_or_else(|| {
        warn!(adapter_id = %adapter_id, "No .aos file path for adapter");
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("No .aos file available for this adapter")
                    .with_code("NO_AOS_FILE"),
            ),
        )
    })?;

    // Verify the file exists
    let path = std::path::Path::new(aos_path);
    if !path.exists() {
        error!(
            tenant_id = %claims.tenant_id,
            adapter_id = %adapter_id,
            path = %aos_path,
            "Adapter .aos file not found on disk"
        );
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Adapter .aos file not found on disk")
                    .with_code("FILE_NOT_FOUND"),
            ),
        ));
    }

    // Open the file for streaming
    let file = tokio::fs::File::open(path).await.map_err(|e| {
        error!(
            tenant_id = %claims.tenant_id,
            adapter_id = %adapter_id,
            path = %aos_path,
            error = %e,
            "Failed to open .aos file"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to open adapter file")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get file metadata for content-length
    let metadata = file.metadata().await.map_err(|e| {
        error!(
            tenant_id = %claims.tenant_id,
            adapter_id = %adapter_id,
            error = %e,
            "Failed to get file metadata"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to read file metadata")
                    .with_code("IO_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Create streaming response body
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    // Build filename for Content-Disposition
    let filename = format!("{}.aos", adapter_id);

    // Get content hash for X-Adapter-Hash header (use aos_file_hash if available, else hash_b3)
    let content_hash = adapter
        .aos_file_hash
        .as_ref()
        .or(Some(&adapter.hash_b3))
        .cloned()
        .unwrap_or_default();

    info!(
        adapter_id = %adapter_id,
        tenant_id = %adapter.tenant_id,
        file_size = metadata.len(),
        actor = %claims.sub,
        "Exporting adapter as .aos file"
    );

    // Audit log: adapter exported
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        "adapter.exported",
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    // Build response with headers
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
            (header::CONTENT_LENGTH, metadata.len().to_string()),
            (
                header::HeaderName::from_static("x-adapter-hash"),
                content_hash,
            ),
            (
                header::HeaderName::from_static("x-adapter-id"),
                adapter_id.clone(),
            ),
        ],
        body,
    ))
}
