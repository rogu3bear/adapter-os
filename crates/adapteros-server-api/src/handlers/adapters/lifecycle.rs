// Adapter Lifecycle Handlers
//
// This module provides REST API endpoints for:
// - Adapter activation (workspace-scoped)
// - Manual adapter lifecycle promotion/demotion
//
// Lifecycle states: Draft -> Training -> Ready -> Active -> Deprecated -> Retired (or Failed)
// Transitions are logged with telemetry events including actor, reason, old/new states.

use crate::audit_helper::log_preflight_result;
use crate::auth::Claims;
use crate::handlers::adapters::preflight_adapter::run_api_preflight;
use crate::handlers::workspaces::build_active_state_response;
use crate::ip_extraction::ClientIp;
use crate::permissions::{require_permission, Permission};
use crate::services::{AdapterService, DefaultAdapterService};
use crate::state::AppState;
use crate::types::*;
use adapteros_core::preflight::PreflightConfig;
use adapteros_db::adapter_snapshots::CreateSnapshotParams;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use utoipa::ToSchema;

// ============================================================================
// In-Flight Adapter Guard
// ============================================================================

/// Check if an adapter is currently being used for inference.
///
/// Returns an error response if the adapter is in-flight and should not be modified.
/// This prevents race conditions where adapter lifecycle changes could affect
/// running inference requests.
///
/// ## In-Flight Guard (ANCHOR, AUDIT, RECTIFY)
///
/// - **ANCHOR**: `is_adapter_in_flight()` enforces invariant before lifecycle transitions
/// - **AUDIT**: Tracks `in_flight_guard_allows` and `in_flight_guard_blocks` counters
/// - **RECTIFY**: Returns CONFLICT status with actionable error message for retry
fn check_adapter_not_in_flight(
    state: &AppState,
    adapter_id: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if let Some(ref tracker) = state.inference_state_tracker {
        // Use is_adapter_in_flight() which updates AUDIT metrics
        if tracker.is_adapter_in_flight(adapter_id) {
            tracing::warn!(
                adapter_id = %adapter_id,
                total_blocks = tracker.in_flight_guard_blocks(),
                "Lifecycle modification blocked: adapter is being used for inference"
            );
            return Err((
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::new("Adapter is currently in use for inference")
                        .with_code(adapteros_core::error_codes::ADAPTER_IN_FLIGHT)
                        .with_string_details(format!(
                            "Adapter '{}' cannot be modified while active inference requests are using it. \
                             Wait for in-flight requests to complete or use a graceful drain period.",
                            adapter_id
                        )),
                ),
            ));
        }
    }
    Ok(())
}

// ============================================================================
// Types
// ============================================================================

/// Lifecycle state transition request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct LifecycleTransitionRequest {
    /// Reason for the transition (required for audit trail)
    pub reason: String,
}

/// Lifecycle state transition response
#[derive(Debug, Serialize, ToSchema)]
pub struct LifecycleTransitionResponse {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub reason: String,
    pub actor: String,
    pub timestamp: String,
}

#[derive(Debug, Default, Deserialize, Serialize, ToSchema)]
pub struct AdapterActivateRequest {
    /// Workspace identifier; defaults to caller tenant
    #[serde(default)]
    pub workspace_id: Option<String>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Activate an adapter for a workspace and update workspace active state.
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/activate",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID to activate")
    ),
    request_body = AdapterActivateRequest,
    responses(
        (status = 200, description = "Adapter activated", body = crate::handlers::workspaces::WorkspaceActiveStateResponse),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Adapter not found")
    ),
    tag = "adapters"
)]
pub async fn activate_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
    Json(req): Json<AdapterActivateRequest>,
) -> Result<
    Json<crate::handlers::workspaces::WorkspaceActiveStateResponse>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::AdapterLoad)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let workspace_id = req
        .workspace_id
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());

    // Enforce workspace membership when caller scopes outside their tenant
    if workspace_id != claims.tenant_id {
        let access = state
            .db
            .check_workspace_access_with_admin(
                &workspace_id,
                &claims.sub,
                &claims.tenant_id,
                &claims.admin_tenants,
            )
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    workspace_id = %workspace_id,
                    user_id = %claims.sub,
                    "Failed to check workspace access"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to check workspace access")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        if access.is_none() {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("workspace access denied")
                        .with_code("TENANT_ISOLATION_ERROR"),
                ),
            ));
        }
    }

    let adapter = state
        .db
        .get_adapter_for_tenant(&workspace_id, &adapter_id)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                adapter_id = %adapter_id,
                workspace_id = %workspace_id,
                "Failed to load adapter for tenant"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load adapter")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // =========================================================================
    // PREFLIGHT GATING: Run preflight checks before activation
    // =========================================================================
    // This ensures content_hash_b3, manifest_hash, and other requirements
    // are validated consistently with swap operations.
    let preflight_config = PreflightConfig::with_actor(&workspace_id, &claims.sub);
    let preflight_result = run_api_preflight(&adapter, &state.db, &preflight_config).await;

    // Audit log the preflight result including any bypasses used (Gap 4 fix)
    log_preflight_result(
        &state.db,
        &claims,
        &adapter_id,
        &preflight_result,
        Some(client_ip.0.as_str()),
    )
    .await;

    if !preflight_result.passed {
        // Get primary error code for the response
        let primary_code = preflight_result
            .primary_error_code()
            .map(|c| c.as_str())
            .unwrap_or("PREFLIGHT_FAILED");

        // Build detailed error message
        let details = preflight_result
            .failures
            .iter()
            .map(|f| format!("[{}] {}", f.code.as_str(), f.message))
            .collect::<Vec<_>>()
            .join("; ");

        info!(
            adapter_id = %adapter_id,
            error_code = %primary_code,
            "Adapter activation blocked by preflight checks"
        );

        return Err((
            StatusCode::PRECONDITION_FAILED,
            Json(
                ErrorResponse::new("Adapter activation blocked by preflight checks")
                    .with_code(primary_code)
                    .with_string_details(details),
            ),
        ));
    }

    // Ensure a training snapshot exists to satisfy lifecycle policy
    let snapshot_exists = state
        .db
        .get_adapter_training_snapshot(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load training snapshot")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .is_some();

    if !snapshot_exists {
        if let Ok(Some(job)) = state
            .db
            .get_training_job_by_adapter(&adapter_id, &workspace_id)
            .await
        {
            let metadata: Option<serde_json::Value> = job
                .metadata_json
                .as_ref()
                .and_then(|raw| serde_json::from_str(raw).ok());
            let manifest_hash = metadata
                .as_ref()
                .and_then(|m| m.get("manifest_hash_b3"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let dataset_hash = metadata
                .as_ref()
                .and_then(|m| m.get("dataset_hash_b3"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let chunk_hash = manifest_hash
                .or(dataset_hash)
                .or_else(|| adapter.content_hash_b3.clone())
                .unwrap_or_else(|| adapter.hash_b3.clone());

            let documents_json = serde_json::json!([{
                "dataset_id": job.dataset_id,
                "dataset_version_ids": job
                    .data_spec_json
                    .as_ref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            }])
            .to_string();
            let chunk_cfg = job
                .data_spec_json
                .clone()
                .unwrap_or_else(|| "{}".to_string());
            let _ = state
                .db
                .create_training_snapshot(CreateSnapshotParams {
                    adapter_id: adapter_id.clone(),
                    training_job_id: job.id.clone(),
                    collection_id: job.collection_id.clone(),
                    documents_json,
                    chunk_manifest_hash: chunk_hash,
                    chunking_config_json: chunk_cfg,
                    dataset_id: job.dataset_id.clone(),
                    dataset_version_id: job.dataset_version_id.clone(),
                    dataset_hash_b3: None,
                })
                .await;
        }
    }

    // Enforce lifecycle promotion to active (strict - not best-effort)
    state
        .db
        .transition_adapter_lifecycle(&adapter_id, "active", "workspace_activate", &claims.sub)
        .await
        .map_err(|e| {
            error!(
                adapter_id = %adapter_id,
                error = %e,
                "Failed to promote adapter to active lifecycle state"
            );
            (
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::new("Cannot activate adapter: lifecycle transition denied")
                        .with_code("LIFECYCLE_TRANSITION_DENIED")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Derive manifest hash from training metadata or adapter content hash
    let manifest_hash = if let Ok(Some(job)) = state
        .db
        .get_training_job_by_adapter(&adapter_id, &workspace_id)
        .await
    {
        job.metadata_json
            .as_ref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|v| {
                v.get("manifest_hash_b3")
                    .and_then(|s| s.as_str().map(|s| s.to_string()))
            })
    } else {
        None
    }
    .or(adapter.content_hash_b3.clone())
    .or(adapter.aos_file_hash.clone());

    let state_record = state
        .db
        .upsert_workspace_active_state(
            &workspace_id,
            adapter.base_model_id.as_deref(),
            None,
            Some(std::slice::from_ref(&adapter_id)),
            manifest_hash.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update workspace active state")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response =
        build_active_state_response(&state, workspace_id.clone(), Some(state_record)).await?;

    Ok(Json(response))
}

/// Manually promote adapter to next lifecycle tier
///
/// Transitions: Unloaded -> Cold -> Warm -> Hot -> Resident
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.lifecycle.promoted` event with metadata:
/// - adapter_id, old_state, new_state, actor, reason, timestamp
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/lifecycle/promote
/// {
///   "reason": "Manual promotion for production deployment"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/lifecycle/promote",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = LifecycleTransitionRequest,
    responses(
        (status = 200, description = "Adapter promoted successfully", body = LifecycleTransitionResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Already at maximum state", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn promote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require AdapterLoad permission (Operator and Admin roles have this)
    require_permission(&claims, Permission::AdapterLoad)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Guard: prevent modification of in-flight adapters
    check_adapter_not_in_flight(&state, &adapter_id)?;

    // Use the adapter service to promote lifecycle
    let service = DefaultAdapterService::new(Arc::new(state.clone()));
    let actor = claims.sub.clone();

    let result = service
        .promote_lifecycle(&adapter_id, &claims.tenant_id, &req.reason, &actor)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to promote adapter lifecycle"
            );
            match e {
                adapteros_core::error::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Adapter not found")
                            .with_code("ADAPTER_NOT_FOUND")
                            .with_string_details(format!(
                                "Adapter '{}' does not exist for tenant '{}'. Verify the adapter ID is correct.",
                                adapter_id, claims.tenant_id
                            )),
                    ),
                ),
                adapteros_core::error::AosError::Validation(msg) => (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&msg).with_code("LIFECYCLE_PROMOTION_INVALID")),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to promote adapter lifecycle state")
                            .with_code("LIFECYCLE_PROMOTION_FAILED")
                            .with_string_details(format!(
                                "Adapter '{}' could not be promoted to the next lifecycle tier. Technical details: {}",
                                adapter_id, e
                            )),
                    ),
                ),
            }
        })?;

    // Audit log: adapter lifecycle promoted
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LIFECYCLE_PROMOTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(LifecycleTransitionResponse {
        adapter_id: result.adapter_id,
        old_state: result.old_state,
        new_state: result.new_state,
        reason: result.reason,
        actor,
        timestamp: result.timestamp,
    }))
}

/// Manually demote adapter to previous lifecycle tier
///
/// Transitions: Resident -> Hot -> Warm -> Cold -> Unloaded
///
/// **Permissions:** Requires `Operator` or `Admin` role.
///
/// **Telemetry:** Emits `adapter.lifecycle.demoted` event.
///
/// # Example
/// ```
/// POST /v1/adapters/{adapter_id}/lifecycle/demote
/// {
///   "reason": "Reducing memory pressure"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/lifecycle/demote",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = LifecycleTransitionRequest,
    responses(
        (status = 200, description = "Adapter demoted successfully", body = LifecycleTransitionResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 400, description = "Already at minimum state", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn demote_adapter_lifecycle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
    Json(req): Json<LifecycleTransitionRequest>,
) -> Result<Json<LifecycleTransitionResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require AdapterUnload permission (Operator and Admin roles have this)
    require_permission(&claims, Permission::AdapterUnload)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Guard: prevent modification of in-flight adapters
    check_adapter_not_in_flight(&state, &adapter_id)?;

    // Use the adapter service to demote lifecycle
    let service = DefaultAdapterService::new(Arc::new(state.clone()));
    let actor = claims.sub.clone();

    let result = service
        .demote_lifecycle(&adapter_id, &claims.tenant_id, &req.reason, &actor)
        .await
        .map_err(|e| {
            error!(
                tenant_id = %claims.tenant_id,
                adapter_id = %adapter_id,
                error = %e,
                "Failed to demote adapter lifecycle"
            );
            match e {
                adapteros_core::error::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Adapter not found")
                            .with_code("ADAPTER_NOT_FOUND")
                            .with_string_details(format!(
                                "Adapter '{}' does not exist for tenant '{}'. Verify the adapter ID is correct.",
                                adapter_id, claims.tenant_id
                            )),
                    ),
                ),
                adapteros_core::error::AosError::Validation(msg) => (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&msg).with_code("LIFECYCLE_DEMOTION_INVALID")),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to demote adapter lifecycle state")
                            .with_code("LIFECYCLE_DEMOTION_FAILED")
                            .with_string_details(format!(
                                "Adapter '{}' could not be demoted to the previous lifecycle tier. Technical details: {}",
                                adapter_id, e
                            )),
                    ),
                ),
            }
        })?;

    // Audit log: adapter lifecycle demoted
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_LIFECYCLE_DEMOTE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(LifecycleTransitionResponse {
        adapter_id: result.adapter_id,
        old_state: result.old_state,
        new_state: result.new_state,
        reason: result.reason,
        actor,
        timestamp: result.timestamp,
    }))
}
