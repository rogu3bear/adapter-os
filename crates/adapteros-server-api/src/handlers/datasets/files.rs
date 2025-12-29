//! Dataset file handlers.
//!
//! This module provides handlers for dataset file operations including:
//! - Listing files within a dataset
//! - Retrieving individual file content
//! - Getting dataset statistics
//! - Validating individual files for format, structure, and content compliance

use crate::auth::Claims;
use crate::error_helpers::{db_error, forbidden, not_found};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{DatasetFileResponse, DatasetStatisticsResponse, ErrorResponse};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::path::Path as StdPath;
use tokio::fs;
use utoipa::ToSchema;

use super::validation::{
    deep_validate_file, quick_validate_file, DatasetValidationResult, ValidationConfig,
    ValidationMode,
};

/// Get files in a dataset
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/files",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "List of files in dataset", body = Vec<DatasetFileResponse>),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_files(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let responses: Vec<DatasetFileResponse> = files
        .into_iter()
        .map(|f| DatasetFileResponse {
            schema_version: "1.0".to_string(),
            file_id: f.id,
            file_name: f.file_name,
            file_path: f.file_path,
            size_bytes: f.size_bytes,
            hash: f.hash_b3,
            mime_type: f.mime_type,
            created_at: f.created_at,
        })
        .collect();

    Ok(Json(responses))
}

/// Get dataset statistics
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/statistics",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Dataset statistics", body = DatasetStatisticsResponse),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_statistics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let stats = state
        .db
        .get_dataset_statistics(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get statistics: {}", e)))?
        .ok_or_else(|| not_found("Statistics for this dataset"))?;

    Ok(Json(DatasetStatisticsResponse {
        schema_version: "1.0".to_string(),
        dataset_id: stats.dataset_id,
        num_examples: stats.num_examples,
        avg_input_length: stats.avg_input_length,
        avg_target_length: stats.avg_target_length,
        language_distribution: stats
            .language_distribution
            .and_then(|s| serde_json::from_str(&s).ok()),
        file_type_distribution: stats
            .file_type_distribution
            .and_then(|s| serde_json::from_str(&s).ok()),
        total_tokens: stats.total_tokens,
        computed_at: stats.computed_at,
    }))
}

/// Path parameters for file content endpoint
#[derive(Debug, serde::Deserialize)]
pub struct DatasetFileContentPath {
    pub dataset_id: String,
    pub file_id: String,
}

/// Stream file content from a dataset
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/files/{file_id}/content",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("file_id" = String, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "File content streamed successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or file not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_file_content(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(path): Path<DatasetFileContentPath>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists and check tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&path.dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the specific file
    let file = state
        .db
        .get_dataset_file(&path.dataset_id, &path.file_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset file: {}", e)))?
        .ok_or_else(|| not_found("File"))?;

    // Read file content
    let file_data = fs::read(&file.file_path)
        .await
        .map_err(|e| db_error(format!("Failed to read file: {}", e)))?;

    // Determine Content-Type from mime_type or default to application/octet-stream
    let content_type = file
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    // Set Content-Disposition for download with original filename
    let content_disposition = format!("attachment; filename=\"{}\"", file.file_name);

    let headers = [
        (header::CONTENT_TYPE, content_type.to_string()),
        (header::CONTENT_DISPOSITION, content_disposition),
    ];

    Ok((headers, file_data).into_response())
}
