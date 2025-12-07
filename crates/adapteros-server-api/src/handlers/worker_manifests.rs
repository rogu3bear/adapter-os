use crate::error_helpers::{db_error_msg, internal_error_msg, not_found_with_details};
use crate::state::AppState;
use adapteros_api_types::workers::WorkerManifestFetchResponse;
use adapteros_core::B3Hash;
use adapteros_manifest::ManifestV3;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tracing::{error, info};

#[derive(Deserialize)]
pub struct WorkerManifestPath {
    pub tenant_id: String,
    pub manifest_hash: String,
}

/// Fetch manifest content by hash for workers (hash-validated)
///
/// Tenant-scoped: the manifest must belong to the requested tenant.
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/manifests/{manifest_hash}",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("manifest_hash" = String, Path, description = "BLAKE3 manifest hash (hex)")
    ),
    responses(
        (status = 200, description = "Manifest content with hash verification", body = WorkerManifestFetchResponse),
        (status = 404, description = "Manifest not found", body = crate::types::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::types::ErrorResponse)
    ),
    tag = "workers"
)]
pub async fn fetch_manifest_by_hash(
    State(state): State<AppState>,
    Path(path): Path<WorkerManifestPath>,
) -> std::result::Result<
    Json<WorkerManifestFetchResponse>,
    (StatusCode, Json<crate::types::ErrorResponse>),
> {
    let WorkerManifestPath {
        tenant_id,
        manifest_hash,
    } = path;

    let record = state
        .db
        .get_manifest_by_hash(&manifest_hash)
        .await
        .map_err(|e| db_error_msg("failed to fetch manifest", e))?;

    let manifest = match record {
        Some(rec) if rec.tenant_id == tenant_id => rec,
        Some(_) | None => {
            return Err(not_found_with_details(
                "manifest not found",
                format!(
                    "No manifest for tenant_id={} with hash={}",
                    tenant_id, manifest_hash
                ),
            ))
        }
    };

    // Verify stored hash matches body (defensive)
    let computed_hash = B3Hash::hash(manifest.body_json.as_bytes()).to_hex();
    if computed_hash != manifest_hash {
        error!(
            stored_hash = %manifest_hash,
            computed_hash = %computed_hash,
            "Manifest hash mismatch for stored record"
        );
        return Err(internal_error_msg(
            "manifest hash mismatch",
            "stored manifest hash does not match computed hash",
        ));
    }

    // Parse and emit YAML for consumers while preserving canonical JSON for hashing
    let manifest_struct: ManifestV3 = serde_json::from_str(&manifest.body_json).map_err(|e| {
        internal_error_msg(
            "failed to parse manifest JSON",
            format!("Manifest parse error: {}", e),
        )
    })?;
    let manifest_yaml = serde_yaml::to_string(&manifest_struct)
        .map_err(|e| internal_error_msg("failed to render manifest YAML", e.to_string()))?;

    info!(
        tenant_id = %tenant_id,
        manifest_hash = %manifest_hash,
        "Manifest fetched by hash"
    );

    Ok(Json(WorkerManifestFetchResponse {
        manifest_hash,
        manifest_json: manifest.body_json,
        manifest_yaml,
    }))
}
