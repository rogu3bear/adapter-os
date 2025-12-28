//! Dataset safety and trust handlers.

use super::helpers::stream_preview_file;
use super::types::{
    DatasetTrustOverrideRequest, TrustOverrideRequest, TrustOverrideResponse,
    UpdateDatasetSafetyRequest, UpdateDatasetSafetyResponse,
};
use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, forbidden, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use std::collections::HashMap;
use tracing::{info, warn};

/// Update semantic/safety statuses for a dataset version (Tier 2).
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/safety",
    params(("dataset_id" = String, Path, description = "Dataset ID")),
    request_body = UpdateDatasetSafetyRequest,
    responses(
        (status = 200, description = "Safety statuses updated", body = UpdateDatasetSafetyResponse),
        (status = 404, description = "Dataset not found"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn update_dataset_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<UpdateDatasetSafetyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    // Compute overall safety for validation record
    let overall_safety = {
        let statuses = [
            body.pii_status.as_deref().unwrap_or("unknown"),
            body.toxicity_status.as_deref().unwrap_or("unknown"),
            body.leak_status.as_deref().unwrap_or("unknown"),
            body.anomaly_status.as_deref().unwrap_or("unknown"),
        ];
        if statuses
            .iter()
            .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"))
        {
            "block".to_string()
        } else if statuses.iter().any(|s| s.eq_ignore_ascii_case("warn")) {
            "warn".to_string()
        } else if statuses.iter().all(|s| s.eq_ignore_ascii_case("unknown")) {
            "unknown".to_string()
        } else {
            "clean".to_string()
        }
    };

    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            body.pii_status.as_deref(),
            body.toxicity_status.as_deref(),
            body.leak_status.as_deref(),
            body.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| db_error(format!("Failed to update safety status: {}", e)))?;

    let _ = state
        .db
        .record_dataset_version_validation_run(
            &version_id,
            "tier2_safety",
            &overall_safety,
            None,
            None,
            None,
            Some(claims.sub.as_str()),
        )
        .await;

    Ok(Json(UpdateDatasetSafetyResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state,
        overall_safety_status: overall_safety,
    }))
}

/// Admin override for dataset trust_state.
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/trust_override",
    params(("dataset_id" = String, Path, description = "Dataset ID")),
    request_body = TrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied", body = TrustOverrideResponse),
        (status = 404, description = "Dataset not found"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn override_dataset_trust(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<TrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            &body.trust_state,
            body.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create trust override: {}", e)))?;

    Ok(Json(TrustOverrideResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state: body.trust_state,
    }))
}

/// Get a preview of dataset contents
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/preview",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("limit" = Option<i32>, Query, description = "Number of examples to preview")
    ),
    responses(
        (status = 200, description = "Dataset preview", body = serde_json::Value),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn preview_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only preview their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be previewed by admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut examples = Vec::new();
    let mut count = 0;

    // Stream read files for memory efficiency
    for file in files {
        if count >= limit {
            break;
        }

        match stream_preview_file(
            std::path::Path::new(&file.file_path),
            &dataset.format,
            limit - count,
        )
        .await
        {
            Ok(mut file_examples) => {
                count += file_examples.len();
                examples.append(&mut file_examples);
            }
            Err(e) => {
                warn!("Failed to preview file {}: {}", file.file_name, e);
                continue;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "format": dataset.format,
        "total_examples": examples.len(),
        "examples": examples
    })))
}

/// Apply a trust override to the latest dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/trust_override",
    request_body = DatasetTrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied"),
        (status = 400, description = "Invalid override"),
        (status = 404, description = "Dataset not found"),
    ),
    tag = "datasets"
)]
pub async fn apply_dataset_trust_override(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(payload): Json<DatasetTrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // Tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    let allowed_states = [
        "allowed",
        "allowed_with_warning",
        "blocked",
        "needs_approval",
    ];
    if !allowed_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case(payload.override_state.as_str()))
    {
        return Err(bad_request("Invalid override_state"));
    }

    let version_id = state
        .db
        .ensure_dataset_version_exists(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?;

    state
        .db
        .create_dataset_version_override(
            &version_id,
            payload.override_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create override: {}", e)))?;

    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "dataset_version_id": version_id,
        "effective_trust_state": effective,
    })))
}

/// Apply trust override to a specific dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/trust-override",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID")
    ),
    request_body = DatasetTrustOverrideRequest,
    responses(
        (status = 200, description = "Trust override applied successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 400, description = "Invalid override state"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn apply_dataset_version_trust_override(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
    Json(payload): Json<DatasetTrustOverrideRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Validate override state
    let allowed_states = [
        "allowed",
        "allowed_with_warning",
        "blocked",
        "needs_approval",
    ];
    if !allowed_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case(payload.override_state.as_str()))
    {
        return Err(bad_request(
            "Invalid override_state. Must be one of: allowed, allowed_with_warning, blocked, needs_approval",
        ));
    }

    // Create the override (this automatically propagates trust changes via DB triggers)
    state
        .db
        .create_dataset_version_override(
            &version_id,
            payload.override_state.as_str(),
            payload.reason.as_deref(),
            &claims.sub,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create override: {}", e)))?;

    // Get the effective trust state after override
    let effective = state
        .db
        .get_effective_trust_state(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to read effective trust_state: {}", e)))?
        .unwrap_or_else(|| "unknown".to_string());

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        override_state = %payload.override_state,
        effective_state = %effective,
        actor = %claims.sub,
        "Applied dataset version trust override"
    );

    Ok(Json(serde_json::json!({
        "dataset_id": dataset_id,
        "dataset_version_id": version_id,
        "override_state": payload.override_state,
        "effective_trust_state": effective,
        "reason": payload.reason,
    })))
}

/// Update safety signals for a specific dataset version
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions/{version_id}/safety",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("version_id" = String, Path, description = "Dataset version ID")
    ),
    request_body = UpdateDatasetSafetyRequest,
    responses(
        (status = 200, description = "Safety status updated successfully", body = UpdateDatasetSafetyResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn update_dataset_version_safety(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dataset_id, version_id)): Path<(String, String)>,
    Json(body): Json<UpdateDatasetSafetyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Validate dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    }

    // Validate version exists and belongs to the dataset
    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset version: {}", e)))?
        .ok_or_else(|| not_found("Dataset version"))?;

    if version.dataset_id != dataset_id {
        return Err(bad_request(
            "Version does not belong to the specified dataset",
        ));
    }

    // Enforce tenant isolation on version
    if let Some(ref version_tenant_id) = version.tenant_id {
        validate_tenant_isolation(&claims, version_tenant_id)?;
    }

    // Compute overall safety for validation record
    let overall_safety = {
        let statuses = [
            body.pii_status.as_deref().unwrap_or("unknown"),
            body.toxicity_status.as_deref().unwrap_or("unknown"),
            body.leak_status.as_deref().unwrap_or("unknown"),
            body.anomaly_status.as_deref().unwrap_or("unknown"),
        ];
        if statuses
            .iter()
            .any(|s| s.eq_ignore_ascii_case("block") || s.eq_ignore_ascii_case("unsafe"))
        {
            "block".to_string()
        } else if statuses.iter().any(|s| s.eq_ignore_ascii_case("warn")) {
            "warn".to_string()
        } else if statuses.iter().all(|s| s.eq_ignore_ascii_case("unknown")) {
            "unknown".to_string()
        } else {
            "clean".to_string()
        }
    };

    // Update safety status (this automatically propagates trust changes via DB layer)
    let trust_state = state
        .db
        .update_dataset_version_safety_status(
            &version_id,
            body.pii_status.as_deref(),
            body.toxicity_status.as_deref(),
            body.leak_status.as_deref(),
            body.anomaly_status.as_deref(),
        )
        .await
        .map_err(|e| db_error(format!("Failed to update safety status: {}", e)))?;

    // Record validation run for audit trail
    let _ = state
        .db
        .record_dataset_version_validation_run(
            &version_id,
            "tier2_safety",
            &overall_safety,
            None,
            None,
            None,
            Some(claims.sub.as_str()),
        )
        .await;

    info!(
        dataset_id = %dataset_id,
        version_id = %version_id,
        trust_state = %trust_state,
        overall_safety = %overall_safety,
        actor = %claims.sub,
        "Updated dataset version safety status"
    );

    Ok(Json(UpdateDatasetSafetyResponse {
        dataset_id,
        dataset_version_id: version_id,
        trust_state,
        overall_safety_status: overall_safety,
    }))
}
