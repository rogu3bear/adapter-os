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
use crate::types::{CanonicalRow, DatasetManifest, ErrorResponse, JobResponse};
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

/// Remove the job input directory after the background task completes.
/// Logs a warning on failure rather than propagating — cleanup must not fail the job.
#[cfg(feature = "embeddings")]
async fn cleanup_job_input_dir(dir: &std::path::Path, job_id: &str) {
    if let Err(e) = tokio::fs::remove_dir_all(dir).await {
        tracing::warn!(job_id = %job_id, path = %dir.display(), error = %e, "failed to clean up job input directory");
    }
}

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
    let mut training_strategy: Option<String> = None;
    let mut enrichment_mode: Option<String> = None;

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
            "training_strategy" => {
                training_strategy = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            "enrichment_mode" => {
                enrichment_mode = Some(
                    field
                        .text()
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
                training_strategy,
                enrichment_mode,
            },
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(dataset))
}

/// Create a training dataset directly from a single uploaded document (async).
///
/// Returns a job id immediately, and performs upload -> process -> dataset creation
/// in a background task. Results are written to the `jobs` table.
///
/// Notes:
/// - This is an operational affordance for large uploads; it does not change determinism semantics.
/// - Output artifacts are persisted under `./var/` (repo hygiene).
#[cfg(feature = "embeddings")]
#[utoipa::path(
    post,
    path = "/v1/training/datasets/from-upload/async",
    responses(
        (status = 200, description = "Dataset build job created", body = JobResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 413, description = "Document too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_training_dataset_from_upload_async(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<Json<JobResponse>, ApiError> {
    require_permission(&claims, Permission::DatasetUpload)?;

    let mut dataset_name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut file_bytes: Option<Bytes> = None;
    let mut training_strategy: Option<String> = None;
    let mut enrichment_mode: Option<String> = None;

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
            "training_strategy" => {
                training_strategy = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            "enrichment_mode" => {
                enrichment_mode = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            other => {
                debug!(
                    "Ignoring unknown field in async training dataset upload: {}",
                    other
                );
            }
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| ApiError::bad_request("No file uploaded"))?;
    let file_name = file_name.unwrap_or_else(|| "document".to_string());

    let var_dir = std::env::var("AOS_VAR_DIR").unwrap_or_else(|_| "var".to_string());
    let job_payload = serde_json::json!({
        "file_name": file_name,
        "mime_type": mime_type,
        "dataset_name": dataset_name,
        "description": description,
        "training_strategy": training_strategy,
        "enrichment_mode": enrichment_mode,
        "var_dir": var_dir,
    });
    let payload_str = serde_json::to_string(&job_payload)
        .map_err(|e| ApiError::internal(format!("serialization error: {e}")))?;

    let job_id = state
        .db
        .create_job(
            "training_dataset_from_upload",
            Some(&claims.tenant_id),
            Some(&claims.sub),
            &payload_str,
        )
        .await
        .map_err(|e| ApiError::internal(format!("failed to create job: {e}")))?;

    // Persist uploaded file to var/job-inputs/{job_id}/file for background task
    let input_dir = std::path::PathBuf::from(&var_dir)
        .join("job-inputs")
        .join(&job_id);
    adapteros_core::reject_forbidden_tmp_path(&input_dir, "job-inputs-root")
        .map_err(ApiError::from)?;
    tokio::fs::create_dir_all(&input_dir)
        .await
        .map_err(|e| ApiError::internal(format!("failed to create job input dir: {e}")))?;
    let input_path = input_dir.join("file");
    tokio::fs::write(&input_path, &file_bytes)
        .await
        .map_err(|e| ApiError::internal(format!("failed to write job input: {e}")))?;

    // Spawn background task
    let job_id_for_task = job_id.clone();
    let state_for_task = state.clone();
    let claims_for_task = claims.clone();
    let dataset_name_for_task = dataset_name;
    let description_for_task = description;
    let mime_type_for_task = mime_type;
    let training_strategy_for_task = training_strategy;
    let enrichment_mode_for_task = enrichment_mode;
    let file_name_for_task = file_name;

    tokio::spawn(async move {
        if let Err(e) = state_for_task
            .db
            .update_job_status(&job_id_for_task, "running", None)
            .await
        {
            tracing::warn!(job_id = %job_id_for_task, error = %e, "failed to mark job as running");
        }

        let bytes = match tokio::fs::read(&input_path).await {
            Ok(b) => b,
            Err(e) => {
                if let Err(db_err) = state_for_task
                    .db
                    .update_job_status(
                        &job_id_for_task,
                        "failed",
                        Some(
                            &serde_json::json!({"error": format!("failed to read input: {e}")})
                                .to_string(),
                        ),
                    )
                    .await
                {
                    tracing::warn!(job_id = %job_id_for_task, error = %db_err, "failed to mark job as failed after input read error");
                }
                cleanup_job_input_dir(&input_dir, &job_id_for_task).await;
                return;
            }
        };

        let service = DefaultTrainingDatasetService::new(Arc::new(state_for_task.clone()));
        let res = service
            .create_from_upload(
                &claims_for_task,
                DatasetFromUploadParams {
                    file_name: file_name_for_task,
                    mime_type: mime_type_for_task,
                    data: Bytes::from(bytes),
                    name: dataset_name_for_task,
                    description: description_for_task,
                    training_strategy: training_strategy_for_task,
                    enrichment_mode: enrichment_mode_for_task,
                },
            )
            .await;

        match res {
            Ok(ds) => {
                if let Err(e) = state_for_task
                    .db
                    .update_job_status(
                        &job_id_for_task,
                        "finished",
                        Some(&serde_json::to_string(&ds).unwrap_or_default()),
                    )
                    .await
                {
                    tracing::warn!(job_id = %job_id_for_task, error = %e, "failed to mark job as finished");
                }
            }
            Err((code, body)) => {
                if let Err(e) = state_for_task
                    .db
                    .update_job_status(
                        &job_id_for_task,
                        "failed",
                        Some(
                            &serde_json::json!({"status": code.as_u16(), "error": body.0})
                                .to_string(),
                        ),
                    )
                    .await
                {
                    tracing::warn!(job_id = %job_id_for_task, error = %e, "failed to mark job as failed");
                }
            }
        }

        cleanup_job_input_dir(&input_dir, &job_id_for_task).await;
    });

    Ok(Json(JobResponse {
        id: job_id,
        kind: "training_dataset_from_upload".to_string(),
        status: "queued".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
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

/// Stub when embeddings feature is disabled.
#[cfg(not(feature = "embeddings"))]
#[utoipa::path(
    post,
    path = "/v1/training/datasets/from-upload/async",
    responses(
        (status = 501, description = "Embeddings feature disabled", body = ErrorResponse)
    ),
    tag = "datasets"
)]
pub async fn create_training_dataset_from_upload_async(
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
    let dataset_version_id =
        crate::id_resolver::resolve_any_id(&state.db, &dataset_version_id).await?;

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
    let dataset_version_id =
        crate::id_resolver::resolve_any_id(&state.db, &dataset_version_id).await?;

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
