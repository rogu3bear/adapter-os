//! Dataset CRUD handlers (list, get, delete).

use super::helpers::{
    ensure_dataset_file_within_root, map_validation_diagnostics, map_validation_errors,
    map_validation_status,
};
use super::types::ListDatasetsQuery;
use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{DatasetListResponse, DatasetResponse};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tracing::{error, info};

/// List all datasets
#[utoipa::path(
    get,
    path = "/v1/datasets",
    params(ListDatasetsQuery),
    responses(
        (status = 200, description = "List of datasets", body = DatasetListResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_datasets(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListDatasetsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetList)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let _offset = params.offset.unwrap_or(0);
    let workspace_id = params.workspace_id.clone();

    let datasets = if let Some(ref ws_id) = workspace_id {
        let workspace_access = state
            .db
            .check_workspace_access_with_admin(
                ws_id,
                &claims.sub,
                &claims.tenant_id,
                &claims.admin_tenants,
            )
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to check workspace access: {}", e)))?;
        if workspace_access.is_none() {
            return Err(ApiError::forbidden(
                "Access denied: you are not a member of this workspace",
            ));
        }

        state
            .db
            .list_training_datasets_for_workspace(&claims.tenant_id, ws_id, limit)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list datasets: {}", e)))?
    } else {
        state
            .db
            .list_training_datasets_for_tenant(&claims.tenant_id, limit)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to list datasets: {}", e)))?
    };

    // Tenant isolation enforced at database level via list_training_datasets_for_tenant
    let is_admin = claims.role == "admin";
    let mut responses: Vec<DatasetResponse> = Vec::new();

    for d in datasets.into_iter().filter(|d| {
        // Non-admin users can only see datasets belonging to their tenant
        if !is_admin {
            match &d.tenant_id {
                Some(dt) if dt != &claims.tenant_id => return false,
                None => return false, // Datasets without tenant_id are hidden from non-admins
                _ => {}
            }
        }
        true
    }) {
        let latest_trusted = state
            .db
            .get_latest_trusted_dataset_version_for_dataset(&d.id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to load dataset versions: {}", e)))?;
        let (dataset_version_id, trust_state) = latest_trusted
            .map(|(v, trust)| (Some(v.id), Some(trust)))
            .unwrap_or((None, None));
        let validation_errors = d.validation_errors.clone();

        responses.push(DatasetResponse {
            schema_version: "1.0".to_string(),
            dataset_id: d.id,
            dataset_version_id,
            name: d.name,
            description: d.description,
            file_count: d.file_count,
            total_size_bytes: d.total_size_bytes,
            format: d.format,
            hash: d.hash_b3,
            dataset_hash_b3: Some(d.dataset_hash_b3),
            storage_path: d.storage_path,
            status: d.status,
            workspace_id: d.workspace_id,
            validation_status: map_validation_status(&d.validation_status),
            validation_errors: map_validation_errors(validation_errors.clone()),
            validation_diagnostics: map_validation_diagnostics(validation_errors),
            trust_state,
            created_by: d.created_by.unwrap_or_else(|| "system".to_string()),
            created_at: d.created_at,
            updated_at: d.updated_at,
            dataset_type: d.dataset_type,
            display_name: None,
        });
    }

    let total = responses.len() as i64;
    Ok(Json(DatasetListResponse {
        schema_version: "1.0".to_string(),
        datasets: responses,
        total,
    }))
}

/// Get a specific dataset
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Dataset details", body = DatasetResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    if let Some(ref ws_id) = dataset.workspace_id {
        let access = state
            .db
            .check_workspace_access_with_admin(
                ws_id,
                &claims.sub,
                &claims.tenant_id,
                &claims.admin_tenants,
            )
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to check workspace access: {}", e)))?;
        if access.is_none() {
            return Err(ApiError::forbidden(
                "Access denied: you are not a member of this workspace",
            ));
        }
    }

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let latest_trusted = state
        .db
        .get_latest_trusted_dataset_version_for_dataset(&dataset.id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to load dataset versions: {}", e)))?;
    let (dataset_version_id, trust_state) = latest_trusted
        .map(|(v, trust)| (Some(v.id), Some(trust)))
        .unwrap_or((None, None));

    Ok(Json(DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: dataset.id,
        dataset_version_id,
        name: dataset.name,
        description: dataset.description,
        file_count: dataset.file_count,
        total_size_bytes: dataset.total_size_bytes,
        format: dataset.format,
        hash: dataset.hash_b3,
        dataset_hash_b3: Some(dataset.dataset_hash_b3),
        storage_path: dataset.storage_path,
        status: dataset.status,
        workspace_id: dataset.workspace_id,
        validation_status: map_validation_status(&dataset.validation_status),
        validation_errors: map_validation_errors(dataset.validation_errors.clone()),
        validation_diagnostics: map_validation_diagnostics(dataset.validation_errors),
        trust_state,
        created_by: dataset.created_by.unwrap_or_else(|| "system".to_string()),
        created_at: dataset.created_at,
        updated_at: dataset.updated_at,
        dataset_type: dataset.dataset_type,
        display_name: None,
    }))
}

/// Delete a dataset
#[utoipa::path(
    delete,
    path = "/v1/datasets/{dataset_id}",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 204, description = "Dataset deleted successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn delete_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<crate::auth::Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    // Get dataset to find storage path
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation before deletion - non-admin users can only delete their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be deleted by admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Check if dataset can be safely deleted (not in use by adapters or active training jobs)
    state
        .db
        .validate_dataset_deletion(&dataset_id)
        .await
        .map_err(ApiError::db_error)?;

    // Delete from database (cascades to files and statistics)
    state
        .db
        .delete_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to delete dataset: {}", e)))?;

    // Delete files from filesystem
    let storage_path = dataset.storage_path.trim();
    if !storage_path.is_empty() {
        let safe_path =
            ensure_dataset_file_within_root(&state, std::path::Path::new(storage_path)).await?;
        if tokio::fs::try_exists(&safe_path).await.unwrap_or(false) {
            tokio::fs::remove_dir_all(&safe_path)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to delete dataset files at {}: {}",
                        safe_path.display(),
                        e
                    );
                    // Don't fail the request if filesystem cleanup fails
                    e
                })
                .ok();
        }
    }

    info!("Deleted dataset {} and its files", dataset_id);

    // Audit log: dataset deleted
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_DELETE,
        crate::audit_helper::resources::DATASET,
        Some(&dataset_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::NO_CONTENT)
}
