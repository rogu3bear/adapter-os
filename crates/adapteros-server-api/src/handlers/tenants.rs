use crate::auth::Claims;
use crate::state::AppState;
use adapteros_core::{SnapshotHash, TenantStateSnapshot};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

/// Get tenant snapshot
///
/// Returns the current state snapshot for the specified tenant.
/// The snapshot includes:
/// - All adapters with their metadata
/// - All stacks with adapter references
/// - Router policies
/// - Plugin configurations
/// - Feature flags
/// - Other tenant configs
///
/// PRD 2 Requirement: GET /v1/tenants/{id}/snapshot
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/snapshot",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Snapshot retrieved", body = TenantStateSnapshot),
        (status = 404, description = "Tenant not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_tenant_snapshot(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantStateSnapshot>, (StatusCode, String)> {
    info!(tenant_id = %tenant_id, "Fetching tenant snapshot");

    // Build snapshot from DB
    let snapshot = state
        .db
        .build_tenant_snapshot(&tenant_id)
        .await
        .map_err(|e| {
            error!(tenant_id = %tenant_id, error = %e, "Failed to build snapshot");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    Ok(Json(snapshot))
}

/// Get tenant snapshot hash
///
/// Returns the most recent snapshot hash for the specified tenant.
/// The hash is a deterministic BLAKE3 hash computed over the canonical
/// snapshot representation.
///
/// PRD 2 Requirement: GET /v1/tenants/{id}/snapshot/hash
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/snapshot/hash",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Hash retrieved", body = SnapshotHash),
        (status = 404, description = "No snapshot hash found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_tenant_snapshot_hash(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<SnapshotHash>, (StatusCode, String)> {
    info!(tenant_id = %tenant_id, "Fetching tenant snapshot hash");

    // Get latest hash from DB
    let hash_opt = state
        .db
        .get_tenant_snapshot_hash(&tenant_id)
        .await
        .map_err(|e| {
            error!(tenant_id = %tenant_id, error = %e, "Failed to get snapshot hash");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    match hash_opt {
        Some(state_hash) => Ok(Json(SnapshotHash {
            tenant_id,
            state_hash,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            "No snapshot hash found for tenant".to_string(),
        )),
    }
}

/// Hydrate tenant from DB
///
/// Builds a snapshot from the current DB state, validates consistency,
/// computes a deterministic hash, and stores it in the tenant_snapshots table.
///
/// Validation:
/// - Ensures all stack references point to existing adapters
/// - Fails with 409 Conflict if DB is inconsistent
/// - No partial writes: hash is NOT stored on validation failure
///
/// Idempotency:
/// - Re-hydrating an already hydrated tenant MUST NOT create duplicates
/// - Hash is stored with (tenant_id, state_hash) as composite key
///
/// PRD 2 Requirement: POST /v1/tenants/{id}/hydrate-from-db
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/hydrate-from-db",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Hydration successful", body = SnapshotHash),
        (status = 409, description = "DB inconsistency detected", body = String),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn hydrate_tenant_from_db(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<SnapshotHash>, (StatusCode, String)> {
    info!(tenant_id = %tenant_id, "Hydrating tenant from DB");

    // Hydrate and validate
    let snapshot_hash = state.db.hydrate_from_db(&tenant_id).await.map_err(|e| {
        // Check if it's a validation error (DB inconsistency)
        let error_msg = e.to_string();
        if error_msg.contains("references invalid or cross-tenant adapters")
            || error_msg.contains("Validation")
        {
            error!(
                tenant_id = %tenant_id,
                error = %e,
                "DB inconsistency detected during hydration"
            );
            (StatusCode::CONFLICT, error_msg)
        } else {
            error!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to hydrate tenant"
            );
            (StatusCode::INTERNAL_SERVER_ERROR, error_msg)
        }
    })?;

    info!(
        tenant_id = %tenant_id,
        state_hash = %snapshot_hash.state_hash.to_hex(),
        "Tenant hydration successful"
    );

    Ok(Json(snapshot_hash))
}
