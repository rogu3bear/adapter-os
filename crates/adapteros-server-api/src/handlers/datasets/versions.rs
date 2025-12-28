//! Dataset version handlers.

use super::types::{CreateDatasetVersionRequest, CreateDatasetVersionResponse};
use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, forbidden, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{DatasetVersionSummary, DatasetVersionsResponse, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

/// List all versions for a dataset (ordered latest-first) with effective trust_state.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Dataset versions", body = DatasetVersionsResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_dataset_versions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Ensure dataset exists and enforce tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let versions = state
        .db
        .list_dataset_versions_for_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to list dataset versions: {}", e)))?;

    let summaries: Vec<DatasetVersionSummary> = versions
        .into_iter()
        .map(|(version, trust_state)| DatasetVersionSummary {
            dataset_version_id: version.id,
            version_number: version.version_number,
            version_label: version.version_label,
            hash_b3: Some(version.hash_b3),
            storage_path: Some(version.storage_path),
            trust_state: Some(trust_state),
            created_at: version.created_at,
        })
        .collect();

    Ok(Json(DatasetVersionsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        dataset_id,
        versions: summaries,
    }))
}

/// Create a dataset version explicitly (e.g., to pin a manifest before training).
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/versions",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = CreateDatasetVersionRequest,
    responses(
        (status = 200, description = "Dataset version created", body = CreateDatasetVersionResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(body): Json<CreateDatasetVersionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let manifest_json = if let Some(v) = body.manifest_json {
        Some(
            serde_json::to_string(&v)
                .map_err(|e| bad_request(format!("invalid manifest_json: {}", e)))?,
        )
    } else {
        None
    };

    let version_id = state
        .db
        .create_training_dataset_version(
            &dataset_id,
            dataset.tenant_id.as_deref(),
            body.version_label.as_deref(),
            &dataset.storage_path,
            &dataset.hash_b3,
            body.manifest_path.as_deref(),
            manifest_json.as_deref(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| db_error(format!("Failed to create dataset version: {}", e)))?;

    let version = state
        .db
        .get_training_dataset_version(&version_id)
        .await
        .map_err(|e| db_error(format!("Failed to fetch created dataset version: {}", e)))?
        .ok_or_else(|| {
            crate::error_helpers::internal_error("Dataset version was created but not found")
        })?;

    Ok(Json(CreateDatasetVersionResponse {
        dataset_id,
        dataset_version_id: version_id,
        version_number: version.version_number,
        trust_state: version.trust_state,
        created_at: version.created_at,
    }))
}
