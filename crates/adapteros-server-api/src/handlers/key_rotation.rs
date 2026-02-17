//! Key rotation event handlers
//!
//! Endpoints for recording, listing, and pruning key rotation events.
//! Key rotation is a security-critical operation; all mutating endpoints
//! require `TenantManage` permission.
//!
//! ## Endpoints
//!
//! - `GET    /v1/security/key-rotations`     — List rotation events (protected)
//! - `POST   /v1/security/key-rotations`     — Record a rotation event (protected)
//! - `DELETE /v1/security/key-rotations`      — Prune old rotation events (protected)

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;

use adapteros_id::{IdPrefix, TypedId};
use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ===== Request / Response Types =====

/// Query parameters for listing key rotation events.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListKeyRotationsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Query parameters for pruning old key rotation events.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PruneKeyRotationsQuery {
    /// Delete rotation events older than this many days. Defaults to 90.
    #[serde(default = "default_older_than_days")]
    pub older_than_days: i64,
}

fn default_older_than_days() -> i64 {
    90
}

/// Request body for triggering a key rotation event.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerKeyRotationRequest {
    /// Fingerprint of the new key.
    pub key_fingerprint: String,
    /// Type of rotation: `scheduled`, `manual`, or `emergency`.
    pub rotation_type: String,
    /// Identity of who initiated the rotation (e.g. email or daemon name).
    pub rotated_by: String,
    /// Fingerprint of the previous key being rotated out, if applicable.
    pub prev_key_fingerprint: Option<String>,
    /// Number of data encryption keys re-encrypted during this rotation.
    #[serde(default)]
    pub deks_reencrypted: i64,
    /// Arbitrary JSON metadata attached to the rotation event.
    pub metadata: Option<String>,
}

/// A single key rotation event in API responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KeyRotationEventResponse {
    pub id: String,
    pub key_fingerprint: String,
    pub rotation_type: String,
    pub rotated_at: String,
    pub rotated_by: String,
    pub prev_key_fingerprint: Option<String>,
    pub deks_reencrypted: i64,
    pub metadata: Option<String>,
}

/// Paginated list of key rotation events.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct KeyRotationListResponse {
    pub events: Vec<KeyRotationEventResponse>,
    pub limit: i64,
    pub offset: i64,
}

/// Result of a prune operation.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PruneKeyRotationsResponse {
    pub pruned_count: u64,
    pub older_than_days: i64,
}

// ===== Handlers =====

/// List key rotation events.
///
/// Returns a paginated list of key rotation events ordered by most recent first.
#[utoipa::path(
    get,
    path = "/v1/security/key-rotations",
    tag = "security",
    params(ListKeyRotationsQuery),
    responses(
        (status = 200, description = "List of key rotation events", body = KeyRotationListResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn list_key_rotations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListKeyRotationsQuery>,
) -> ApiResult<KeyRotationListResponse> {
    require_permission(&claims, Permission::TenantManage)?;

    let limit = query.limit.clamp(1, 100);
    let offset = query.offset.max(0);

    let rows = state
        .db
        .list_key_rotations(limit, offset)
        .await
        .map_err(|e| {
            ApiError::internal("failed to list key rotation events").with_details(e.to_string())
        })?;

    let events = rows
        .into_iter()
        .map(|row| KeyRotationEventResponse {
            id: row.id,
            key_fingerprint: row.key_fingerprint,
            rotation_type: row.rotation_type,
            rotated_at: row.rotated_at,
            rotated_by: row.rotated_by,
            prev_key_fingerprint: row.prev_key_fingerprint,
            deks_reencrypted: row.deks_reencrypted,
            metadata: row.metadata,
        })
        .collect();

    Ok(Json(KeyRotationListResponse {
        events,
        limit,
        offset,
    }))
}

/// Record a key rotation event.
///
/// Generates a `rot-{ulid}` ID and persists the rotation event to the database.
/// This endpoint is called after a key rotation has been performed to maintain
/// an auditable history of all key changes.
#[utoipa::path(
    post,
    path = "/v1/security/key-rotations",
    tag = "security",
    request_body = TriggerKeyRotationRequest,
    responses(
        (status = 201, description = "Key rotation event recorded", body = KeyRotationEventResponse),
        (status = 400, description = "Invalid rotation type", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn trigger_key_rotation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<TriggerKeyRotationRequest>,
) -> ApiResult<KeyRotationEventResponse> {
    require_permission(&claims, Permission::TenantManage)?;

    // Validate rotation_type matches the CHECK constraint in the migration
    let valid_types = ["scheduled", "manual", "emergency"];
    if !valid_types.contains(&body.rotation_type.as_str()) {
        return Err(
            ApiError::bad_request("invalid rotation_type").with_details(format!(
                "rotation_type must be one of: {}",
                valid_types.join(", ")
            )),
        );
    }

    let id = TypedId::new(IdPrefix::Rot);
    let rotated_at = Utc::now().to_rfc3339();

    state
        .db
        .record_key_rotation(
            id.as_str(),
            &body.key_fingerprint,
            &body.rotation_type,
            &rotated_at,
            &body.rotated_by,
            body.prev_key_fingerprint.as_deref(),
            body.deks_reencrypted,
            body.metadata.as_deref(),
        )
        .await
        .map_err(|e| {
            ApiError::internal("failed to record key rotation event").with_details(e.to_string())
        })?;

    Ok(Json(KeyRotationEventResponse {
        id: id.to_string(),
        key_fingerprint: body.key_fingerprint,
        rotation_type: body.rotation_type,
        rotated_at,
        rotated_by: body.rotated_by,
        prev_key_fingerprint: body.prev_key_fingerprint,
        deks_reencrypted: body.deks_reencrypted,
        metadata: body.metadata,
    }))
}

/// Prune old key rotation events.
///
/// Deletes rotation events older than the specified number of days (default 90).
/// This is a maintenance operation to keep the rotation history manageable.
#[utoipa::path(
    delete,
    path = "/v1/security/key-rotations",
    tag = "security",
    params(PruneKeyRotationsQuery),
    responses(
        (status = 200, description = "Prune result", body = PruneKeyRotationsResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn prune_key_rotations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<PruneKeyRotationsQuery>,
) -> ApiResult<PruneKeyRotationsResponse> {
    require_permission(&claims, Permission::TenantManage)?;

    let older_than_days = query.older_than_days.max(1);

    let pruned_count = state
        .db
        .prune_old_rotations(older_than_days)
        .await
        .map_err(|e| {
            ApiError::internal("failed to prune key rotation events").with_details(e.to_string())
        })?;

    Ok(Json(PruneKeyRotationsResponse {
        pruned_count,
        older_than_days,
    }))
}
