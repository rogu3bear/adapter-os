use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::services::{DatasetDomain, DatasetDomainService, SamplingConfig};
#[cfg(feature = "embeddings")]
use crate::services::{
    DatasetFromUploadParams, DefaultTrainingDatasetService, TrainingDatasetService,
};
use crate::state::AppState;
#[cfg(feature = "embeddings")]
use crate::types::DatasetResponse;
use crate::types::{CanonicalRow, DatasetManifest, ErrorResponse};
#[cfg(feature = "embeddings")]
use axum::response::IntoResponse;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
#[cfg(feature = "embeddings")]
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;
#[cfg(feature = "embeddings")]
use tracing::debug;
use utoipa::{IntoParams, ToSchema};

/// Create a training dataset directly from a single uploaded document.
/// This wraps upload -> process -> dataset creation to produce a dataset_id
/// suitable for downstream training flows.
#[cfg(feature = "embeddings")]
#[utoipa::path(
    post,
    path = "/v1/training/datasets/from-upload",
    responses(
        (status = 200, description = "Dataset created successfully", body = DatasetResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 413, description = "Document too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_training_dataset_from_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetUpload)?;

    let mut dataset_name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut file_bytes: Option<Bytes> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "name" => {
                dataset_name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            "description" => {
                description = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                mime_type = field.content_type().map(|ct| ct.to_string());
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            other => {
                debug!(
                    "Ignoring unknown field in training dataset upload: {}",
                    other
                );
            }
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError::bad_request("No file uploaded"))?;
    let file_name = file_name.unwrap_or_else(|| "document".to_string());

    let service = DefaultTrainingDatasetService::new(Arc::new(state.clone()));
    let dataset = service
        .create_from_upload(
            &claims,
            DatasetFromUploadParams {
                file_name,
                mime_type,
                data: file_bytes,
                name: dataset_name,
                description,
            },
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(dataset))
}

/// Stub when embeddings feature is disabled.
#[cfg(not(feature = "embeddings"))]
#[utoipa::path(
    post,
    path = "/v1/training/datasets/from-upload",
    responses(
        (status = 501, description = "Embeddings feature disabled", body = ErrorResponse)
    ),
    tag = "datasets"
)]
pub async fn create_training_dataset_from_upload(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    _multipart: Multipart,
) -> Result<Json<ErrorResponse>, ApiError> {
    Err(ApiError::not_implemented(
        "Training dataset upload requires the 'embeddings' feature to be enabled",
    ))
}

/// Query params for deterministic row streaming.
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct StreamRowsQuery {
    #[serde(default)]
    pub split: Option<String>,
    #[serde(default)]
    pub shuffle_seed: Option<String>,
}

/// Fetch a normalized dataset manifest for a specific dataset version.
#[utoipa::path(
    get,
    path = "/v1/training/dataset_versions/{dataset_version_id}/manifest",
    params(
        ("dataset_version_id" = String, Path, description = "Dataset version identifier")
    ),
    responses(
        (status = 200, description = "Manifest fetched", body = DatasetManifest),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_training_dataset_manifest(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_version_id): Path<String>,
) -> Result<Json<DatasetManifest>, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    let version = state
        .db
        .get_training_dataset_version(&dataset_version_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Dataset version not found"))?;

    let tenant_id = version
        .tenant_id
        .as_deref()
        .unwrap_or(&claims.tenant_id)
        .to_string();
    validate_tenant_isolation(&claims, &tenant_id)?;

    let service = DatasetDomainService::new(Arc::new(state));
    let manifest = service
        .get_manifest(&dataset_version_id, &tenant_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Dataset manifest not found"))?;

    Ok(Json(manifest))
}

/// Deterministically stream normalized rows by dataset version and optional split.
#[utoipa::path(
    get,
    path = "/v1/training/dataset_versions/{dataset_version_id}/rows",
    params(
        ("dataset_version_id" = String, Path, description = "Dataset version identifier"),
        StreamRowsQuery
    ),
    responses(
        (status = 200, description = "Rows streamed", body = [CanonicalRow]),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset version not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn stream_training_dataset_rows(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_version_id): Path<String>,
    Query(params): Query<StreamRowsQuery>,
) -> Result<Json<Vec<CanonicalRow>>, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    let version = state
        .db
        .get_training_dataset_version(&dataset_version_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Dataset version not found"))?;

    let tenant_id = version
        .tenant_id
        .as_deref()
        .unwrap_or(&claims.tenant_id)
        .to_string();
    validate_tenant_isolation(&claims, &tenant_id)?;

    let sampling = SamplingConfig {
        split: params.split.clone(),
        shuffle_seed: params.shuffle_seed.clone(),
    };

    let service = DatasetDomainService::new(Arc::new(state));
    let rows = service
        .stream_rows(&dataset_version_id, &tenant_id, sampling)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(rows))
}
