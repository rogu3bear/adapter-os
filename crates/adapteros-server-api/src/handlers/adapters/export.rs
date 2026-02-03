// Adapter Export Handler
//
// PRD-ART-01: Adapter Export
//
// This module provides REST API endpoints for:
// - Exporting adapters as .aos files

use crate::adapter_helpers::fetch_adapter_for_tenant;
use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
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
use blake3;
use serde_json;
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
) -> Result<impl IntoResponse, ApiError> {
    // Permission check
    require_permission(&claims, Permission::AdapterView)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id).await?;

    // Reject exports during shutdown to prevent race conditions with cleanup
    if state
        .boot_state
        .as_ref()
        .is_some_and(|b| b.is_draining() || b.is_shutting_down())
    {
        return Err(ApiError::service_unavailable("Service shutting down"));
    }

    // Get adapter details and validate tenant isolation
    let adapter = fetch_adapter_for_tenant(&state.db, &claims, &adapter_id).await?;

    // === ISSUE 5: Validate required SBOM fields are present before export ===
    let mut missing_artifacts: Vec<&str> = Vec::new();

    // name field - adapter must have a name
    if adapter.name.is_empty() {
        missing_artifacts.push("name");
    }

    // version field - adapter must have a version (String type, not Option)
    if adapter.version.is_empty() {
        missing_artifacts.push("version");
    }

    // checksum field - adapter must have hash_b3 or aos_file_hash
    // hash_b3 is String, aos_file_hash is Option<String>
    if adapter.hash_b3.is_empty() && adapter.aos_file_hash.as_ref().is_none_or(|h| h.is_empty()) {
        missing_artifacts.push("checksum");
    }

    // dependencies field - check if metadata_json contains dependencies array
    let has_dependencies = adapter
        .metadata_json
        .as_ref()
        .and_then(|json_str| serde_json::from_str::<serde_json::Value>(json_str).ok())
        .and_then(|v| v.get("dependencies").cloned())
        .and_then(|deps| deps.as_array().cloned())
        .is_some_and(|arr| !arr.is_empty());

    if !has_dependencies {
        missing_artifacts.push("dependencies");
    }

    if !missing_artifacts.is_empty() {
        warn!(
            tenant_id = %claims.tenant_id,
            adapter_id = %adapter_id,
            missing = ?missing_artifacts,
            "Export rejected: missing required SBOM artifacts"
        );
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "MISSING_ARTIFACTS",
            format!(
                "Adapter export omits required artifacts: [{}]",
                missing_artifacts.join(", ")
            ),
        ));
    }

    // Check if adapter is archived/purged
    if adapter.purged_at.is_some() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "ADAPTER_PURGED",
            "Adapter has been purged - .aos file no longer available",
        ));
    }

    // Get the .aos file path
    let aos_path = adapter.aos_file_path.as_ref().ok_or_else(|| {
        warn!(adapter_id = %adapter_id, "No .aos file path for adapter");
        ApiError::new(
            StatusCode::NOT_FOUND,
            "NO_AOS_FILE",
            "No .aos file available for this adapter",
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
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "FILE_NOT_FOUND",
            "Adapter .aos file not found on disk",
        ));
    }

    // === ISSUE 2: Re-compute BLAKE3 hash and verify against stored aos_file_hash ===
    if let Some(stored_hash) = &adapter.aos_file_hash {
        let file_data = tokio::fs::read(path).await.map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to read .aos file for checksum verification"
            );
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "IO_ERROR",
                "Failed to verify adapter file integrity",
            )
            .with_details(e.to_string())
        })?;

        let computed_hash = blake3::hash(&file_data).to_hex().to_string();

        if &computed_hash != stored_hash {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                stored_hash = %stored_hash,
                computed_hash = %computed_hash,
                "Archive checksum mismatch - file may be corrupted or tampered"
            );
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CHECKSUM_MISMATCH",
                "Adapter archive checksum does not match",
            )
            .with_details(format!(
                "Stored hash {} does not match computed hash {}",
                stored_hash, computed_hash
            )));
        }

        info!(
            adapter_id = %adapter_id,
            hash = %computed_hash,
            "Archive checksum verified successfully"
        );
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
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "IO_ERROR",
            "Failed to open adapter file",
        )
        .with_details(e.to_string())
    })?;

    // Get file metadata for content-length
    let metadata = file.metadata().await.map_err(|e| {
        error!(
            tenant_id = %claims.tenant_id,
            adapter_id = %adapter_id,
            error = %e,
            "Failed to get file metadata"
        );
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "IO_ERROR",
            "Failed to read file metadata",
        )
        .with_details(e.to_string())
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
