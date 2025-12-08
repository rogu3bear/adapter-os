mod chunked;
mod fs_utils;
mod hashing;
mod paths;
mod progress;
mod tenant;

use self::chunked::{assemble_chunks, expected_chunks, persist_chunk, prepare_session};
use self::fs_utils::{
    clean_dataset_dir, clean_temp, ensure_dirs, finalize_file_move, write_temp_file,
};
use self::hashing::{hash_file, hash_multi};
use self::paths::{resolve_dataset_root, DatasetPaths};
use self::progress::emit_progress;
use self::tenant::bind_dataset_to_tenant;
use super::chunked_upload::{
    CompressionFormat, FileValidator, DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::citations::build_dataset_index;
use crate::error_helpers::{bad_request, db_error, internal_error, not_found, payload_too_large};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::B3Hash;
use adapteros_db::training_datasets::DatasetFile;
use adapteros_deterministic_exec::spawn_deterministic;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Sse,
    },
    Extension, Json,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Maximum file size (100MB)
const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Maximum total upload size (500MB)
const MAX_TOTAL_SIZE: usize = 500 * 1024 * 1024;

/// Buffer size for streaming operations (64KB)
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Validation batch size to reduce database transaction overhead
const VALIDATION_BATCH_SIZE: usize = 10;

/// Map validation status: 'pending' → 'draft' for API responses
fn map_validation_status(status: &str) -> DatasetValidationStatus {
    match status {
        "validating" => DatasetValidationStatus::Validating,
        "valid" => DatasetValidationStatus::Valid,
        "invalid" => DatasetValidationStatus::Invalid,
        "failed" => DatasetValidationStatus::Failed,
        "pending" => DatasetValidationStatus::Draft,
        _ => DatasetValidationStatus::Draft,
    }
}

fn map_validation_errors(errors: Option<String>) -> Option<Vec<String>> {
    errors.and_then(|raw| {
        serde_json::from_str::<Vec<String>>(&raw)
            .ok()
            .or_else(|| Some(vec![raw]))
    })
}

/// Query parameters for listing datasets
#[derive(Deserialize, ToSchema, IntoParams)]
pub struct ListDatasetsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub format: Option<String>,
    pub validation_status: Option<String>,
}

/// Request to initiate a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiateChunkedUploadRequest {
    /// File name being uploaded
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Content type (e.g., application/gzip)
    pub content_type: Option<String>,
    /// Chunk size preference (will be clamped to valid range)
    pub chunk_size: Option<usize>,
}

/// Response from initiating a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiateChunkedUploadResponse {
    /// Unique session identifier
    pub session_id: String,
    /// Chunk size that will be used
    pub chunk_size: usize,
    /// Expected number of chunks
    pub expected_chunks: usize,
    /// Whether compression is detected
    pub compression_format: String,
}

/// Query parameters for uploading a chunk
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
pub struct UploadChunkQuery {
    /// Index of this chunk (0-based)
    pub chunk_index: usize,
}

/// Response from uploading a chunk
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadChunkResponse {
    /// Session ID
    pub session_id: String,
    /// Chunk index that was uploaded
    pub chunk_index: usize,
    /// BLAKE3 hash of this chunk
    pub chunk_hash: String,
    /// Total chunks received so far
    pub chunks_received: usize,
    /// Total expected chunks
    pub expected_chunks: usize,
    /// Is upload complete (all chunks received)?
    pub is_complete: bool,
    /// Resume token for resuming from next chunk (if not complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

/// Request to complete a chunked upload and create the dataset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteChunkedUploadRequest {
    /// Dataset name (optional, defaults to file name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Dataset description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Dataset format (e.g., "jsonl", "json", "csv")
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "jsonl".to_string()
}

/// Response from completing a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteChunkedUploadResponse {
    /// Created dataset ID
    pub dataset_id: String,
    /// Dataset name
    pub name: String,
    /// Final BLAKE3 hash of assembled file
    pub hash: String,
    /// Total file size in bytes
    pub total_size_bytes: i64,
    /// Storage path
    pub storage_path: String,
    /// Timestamp when dataset was created
    pub created_at: String,
}

/// Response for getting upload session status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadSessionStatusResponse {
    /// Session ID
    pub session_id: String,
    /// Original file name
    pub file_name: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size for this upload
    pub chunk_size: usize,
    /// Expected number of chunks
    pub expected_chunks: usize,
    /// Number of chunks received
    pub chunks_received: usize,
    /// List of chunk indices that have been received
    pub received_chunk_indices: Vec<usize>,
    /// Whether all chunks have been received
    pub is_complete: bool,
    /// Session creation timestamp (RFC3339)
    pub created_at: String,
    /// Compression format detected
    pub compression_format: String,
}

/// Upload files to create a new dataset
#[utoipa::path(
    post,
    path = "/v1/datasets/upload",
    responses(
        (status = 200, description = "Dataset created successfully", body = UploadDatasetResponse),
        (status = 400, description = "Invalid request"),
        (status = 413, description = "File too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn upload_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    let dataset_id = Uuid::now_v7().to_string();
    let paths = DatasetPaths::new(resolve_dataset_root(&state));
    ensure_dirs([
        paths.files.as_path(),
        paths.temp.as_path(),
        paths.chunked.as_path(),
        paths.logs.as_path(),
    ])
    .await?;

    let dataset_path = paths.dataset_dir(&dataset_id);
    let temp_path = paths.dataset_temp_dir(&dataset_id);

    ensure_dirs([dataset_path.as_path(), temp_path.as_path()]).await?;

    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &dataset_id,
        "upload",
        None,
        0.0,
        "Starting dataset upload...".to_string(),
        None,
        Some(0),
    );

    let mut uploaded_files = Vec::new();
    let mut total_size = 0usize;
    let mut dataset_name = String::new();
    let mut dataset_description = String::new();
    let mut dataset_format = "jsonl".to_string();
    let mut file_count = 0;

    // Process multipart form
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                dataset_name = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read name field: {}", e)))?;
            }
            "description" => {
                dataset_description = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read description field: {}", e)))?;
            }
            "format" => {
                dataset_format = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read format field: {}", e)))?;
            }
            "file" | "files" => {
                let file_name = field
                    .file_name()
                    .ok_or_else(|| bad_request("File must have a name"))?
                    .to_string();

                let content_type = field
                    .content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                // Stream file to temporary location
                let temp_file_path = temp_path.join(&file_name);

                let data = field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read file data: {}", e)))?;

                let file_size = data.len();

                // Check file size limits
                if file_size > MAX_FILE_SIZE {
                    clean_temp(&temp_path).await;
                    return Err(payload_too_large(&format!(
                        "File {} exceeds maximum size of {}MB",
                        file_name,
                        MAX_FILE_SIZE / 1024 / 1024
                    )));
                }

                total_size += file_size;
                if total_size > MAX_TOTAL_SIZE {
                    clean_temp(&temp_path).await;
                    return Err(payload_too_large(&format!(
                        "Total upload size exceeds maximum of {}MB",
                        MAX_TOTAL_SIZE / 1024 / 1024
                    )));
                }

                // Write file
                write_temp_file(&temp_file_path, &data).await?;

                // Compute hash using B3Hash
                let file_hash = hash_file(&data);

                // Move file to permanent location
                let permanent_path = dataset_path.join(&file_name);
                finalize_file_move(&temp_file_path, &permanent_path).await?;

                file_count += 1;

                // Send progress event for this file
                emit_progress(
                    state.dataset_progress_tx.as_ref(),
                    &dataset_id,
                    "upload",
                    Some(file_name.clone()),
                    if file_count > 0 {
                        (file_count as f32 / 10.0).min(100.0)
                    } else {
                        0.0
                    },
                    format!("Uploaded {} ({} bytes)", file_name, file_size),
                    None,
                    Some(file_count),
                );

                uploaded_files.push(DatasetFile {
                    id: Uuid::now_v7().to_string(),
                    dataset_id: dataset_id.clone(),
                    file_name: file_name.clone(),
                    file_path: permanent_path.to_string_lossy().to_string(),
                    size_bytes: file_size as i64,
                    hash_b3: file_hash,
                    mime_type: Some(content_type),
                    created_at: chrono::Utc::now().to_rfc3339(),
                });

                info!(
                    "Uploaded file {} ({} bytes) for dataset {}",
                    file_name, file_size, dataset_id
                );
            }
            _ => {
                // Ignore unknown fields
                debug!("Ignoring unknown field: {}", name);
            }
        }
    }

    // Clean up temp directory
    clean_temp(&temp_path).await;

    if uploaded_files.is_empty() {
        clean_dataset_dir(&dataset_path).await;
        return Err(bad_request("No files uploaded"));
    }

    if dataset_name.is_empty() {
        dataset_name = format!("Dataset {}", &dataset_id[0..8]);
    }

    // Compute dataset hash from all file hashes using B3Hash
    let file_hashes: Vec<String> = uploaded_files.iter().map(|f| f.hash_b3.clone()).collect();
    let dataset_hash = hash_multi(&file_hashes);

    // Store in database - associate dataset with the user's tenant
    let dataset_id_result = state
        .db
        .create_training_dataset(
            &dataset_name,
            if dataset_description.is_empty() {
                None
            } else {
                Some(&dataset_description)
            },
            &dataset_format,
            &dataset_hash,
            &dataset_path.to_string_lossy(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| db_error(format!("Failed to create dataset record: {}", e)))?;

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await?;

    // Add file records to database
    for file in &uploaded_files {
        state
            .db
            .add_dataset_file(
                &dataset_id,
                &file.file_name,
                &file.file_path,
                file.size_bytes,
                &file.hash_b3,
                file.mime_type.as_deref(),
            )
            .await
            .map_err(|e| {
                error!("Failed to add file record: {}", e);
                db_error(format!("Failed to add file record: {}", e))
            })?;
    }

    info!(
        "Created dataset {} with {} files, total size {} bytes",
        dataset_id,
        uploaded_files.len(),
        total_size
    );

    // Audit log: dataset uploaded
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_UPLOAD,
        crate::audit_helper::resources::DATASET,
        Some(&dataset_id),
    )
    .await;

    // Build citation index for training files (best-effort)
    if let Err(e) = build_dataset_index(&state, &dataset_id, &claims.tenant_id).await {
        warn!(
            dataset_id = %dataset_id,
            error = %e,
            "Failed to build dataset citation index"
        );
    }

    Ok(Json(UploadDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: dataset_id.clone(),
        name: dataset_name,
        description: if dataset_description.is_empty() {
            None
        } else {
            Some(dataset_description)
        },
        file_count: uploaded_files.len() as i32,
        total_size_bytes: total_size as i64,
        format: dataset_format,
        hash: dataset_hash,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Initiate a chunked upload for files > 10MB
#[utoipa::path(
    post,
    path = "/v1/datasets/chunked-upload/initiate",
    request_body = InitiateChunkedUploadRequest,
    responses(
        (status = 200, description = "Upload session initiated", body = InitiateChunkedUploadResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn initiate_chunked_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<InitiateChunkedUploadRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate total size
    if request.total_size == 0 {
        return Err(bad_request("File size must be greater than 0"));
    }

    if request.total_size > MAX_TOTAL_SIZE as u64 {
        return Err(payload_too_large(&format!(
            "File size exceeds maximum of {}MB",
            MAX_TOTAL_SIZE / 1024 / 1024
        )));
    }

    // Determine chunk size
    let chunk_size = request.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);
    let chunk_size = chunk_size.max(MIN_CHUNK_SIZE).min(MAX_CHUNK_SIZE);

    // Calculate expected chunks
    let expected_chunks = expected_chunks(request.total_size, chunk_size);

    // Detect compression
    let content_type = request
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let compression = CompressionFormat::from_content_type(&content_type);

    let paths = DatasetPaths::new(resolve_dataset_root(&state));

    // Use shared session manager from AppState
    let session = prepare_session(
        &state,
        &paths,
        &request.file_name,
        request.total_size,
        &content_type,
        chunk_size,
        compression.clone(),
    )
    .await?
    .0;

    info!(
        "Initiated chunked upload session {} for file {} ({} bytes, {} chunks)",
        session.session_id, request.file_name, request.total_size, expected_chunks
    );

    Ok(Json(InitiateChunkedUploadResponse {
        session_id: session.session_id,
        chunk_size,
        expected_chunks,
        compression_format: format!("{:?}", compression),
    }))
}

/// List all datasets
#[utoipa::path(
    get,
    path = "/v1/datasets",
    params(ListDatasetsQuery),
    responses(
        (status = 200, description = "List of datasets", body = Vec<DatasetResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_datasets(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListDatasetsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetList)?;

    let limit = params.limit.unwrap_or(50).min(100);
    let _offset = params.offset.unwrap_or(0);

    let datasets = state
        .db
        .list_training_datasets_for_tenant(&claims.tenant_id, limit)
        .await
        .map_err(|e| db_error(format!("Failed to list datasets: {}", e)))?;

    // Tenant isolation enforced at database level via list_training_datasets_for_tenant
    let is_admin = claims.role == "admin";
    let responses: Vec<DatasetResponse> = datasets
        .into_iter()
        .filter(|d| {
            // Non-admin users can only see datasets belonging to their tenant
            if !is_admin {
                match &d.tenant_id {
                    Some(dt) if dt != &claims.tenant_id => return false,
                    None => return false, // Datasets without tenant_id are hidden from non-admins
                    _ => {}
                }
            }
            true
        })
        .map(|d| DatasetResponse {
            schema_version: "1.0".to_string(),
            dataset_id: d.id,
            name: d.name,
            description: d.description,
            file_count: d.file_count,
            total_size_bytes: d.total_size_bytes,
            format: d.format,
            hash: d.hash_b3,
            storage_path: d.storage_path,
            validation_status: map_validation_status(&d.validation_status),
            validation_errors: map_validation_errors(d.validation_errors),
            created_by: d.created_by.unwrap_or_else(|| "system".to_string()),
            created_at: d.created_at,
            updated_at: d.updated_at,
        })
        .collect();

    Ok(Json(responses))
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

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
        use crate::error_helpers::forbidden;
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    Ok(Json(DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: dataset.id,
        name: dataset.name,
        description: dataset.description,
        file_count: dataset.file_count,
        total_size_bytes: dataset.total_size_bytes,
        format: dataset.format,
        hash: dataset.hash_b3,
        storage_path: dataset.storage_path,
        validation_status: map_validation_status(&dataset.validation_status),
        validation_errors: map_validation_errors(dataset.validation_errors),
        created_by: dataset.created_by.unwrap_or_else(|| "system".to_string()),
        created_at: dataset.created_at,
        updated_at: dataset.updated_at,
    }))
}

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
        use crate::error_helpers::forbidden;
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
        use crate::error_helpers::forbidden;
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

/// Validate a dataset
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/validate",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = ValidateDatasetRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateDatasetResponse),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn validate_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(request): Json<ValidateDatasetRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetValidate)?;

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only validate their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be validated by admins
        use crate::error_helpers::forbidden;
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Set status to 'validating' at start
    state
        .db
        .update_dataset_validation(&dataset_id, "validating", None)
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    // Send initial validation event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &dataset_id,
        "validation",
        None,
        0.0,
        "Starting dataset validation...".to_string(),
        Some(dataset.file_count as i32),
        Some(0),
    );

    // Get dataset files
    let files = state
        .db
        .get_dataset_files(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset files: {}", e)))?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;
    let total_files = files.len() as f32;
    let mut processed_files = 0;

    // Validate each file
    for file in &files {
        // Check file exists
        if !tokio::fs::try_exists(&file.file_path)
            .await
            .unwrap_or(false)
        {
            validation_errors.push(format!(
                "File {} does not exist at path {}",
                file.file_name, file.file_path
            ));
            is_valid = false;
            processed_files += 1;
            emit_progress(
                state.dataset_progress_tx.as_ref(),
                &dataset_id,
                "validation",
                Some(file.file_name.clone()),
                if total_files > 0.0 {
                    (processed_files as f32 / total_files) * 100.0
                } else {
                    0.0
                },
                format!("Validating {}", file.file_name),
                Some(files.len() as i32),
                Some(processed_files as i32),
            );
            continue;
        }

        // Verify file hash with streaming to avoid loading entire file
        match validate_file_hash_streaming(std::path::Path::new(&file.file_path), &file.hash_b3)
            .await
        {
            Ok(matches) => {
                if !matches {
                    validation_errors.push(format!("File {} hash mismatch", file.file_name));
                    is_valid = false;
                }
            }
            Err(e) => {
                validation_errors
                    .push(format!("Failed to validate file {}: {}", file.file_name, e));
                is_valid = false;
                continue;
            }
        }

        // Format-specific validation with quick checks
        if request.check_format.unwrap_or(true) {
            if let Err(e) = FileValidator::quick_validate(
                std::path::Path::new(&file.file_path),
                &dataset.format,
                STREAM_BUFFER_SIZE,
            )
            .await
            {
                validation_errors.push(format!(
                    "File {} format validation failed: {}",
                    file.file_name, e
                ));
                is_valid = false;
            }
        }

        processed_files += 1;

        // Send progress event for this file
        emit_progress(
            state.dataset_progress_tx.as_ref(),
            &dataset_id,
            "validation",
            Some(file.file_name.clone()),
            if total_files > 0.0 {
                (processed_files as f32 / total_files) * 100.0
            } else {
                0.0
            },
            format!("Validated {}", file.file_name),
            Some(files.len() as i32),
            Some(processed_files as i32),
        );
    }

    // Update validation status in database - set to "invalid" if validation failed
    let validation_status = if is_valid { "valid" } else { "invalid" };
    let validation_errors_str = if validation_errors.is_empty() {
        None
    } else {
        Some(validation_errors.join("; "))
    };

    state
        .db
        .update_dataset_validation(
            &dataset_id,
            validation_status,
            validation_errors_str.as_deref(),
        )
        .await
        .map_err(|e| {
            // On database error, try to reset status to 'invalid' to prevent stuck 'validating' state
            let db_clone = state.db.clone();
            let dataset_id_clone = dataset_id.clone();
            tokio::spawn(async move {
                let _ = db_clone
                    .update_dataset_validation(
                        &dataset_id_clone,
                        "invalid",
                        Some("Validation failed due to internal error"),
                    )
                    .await;
            });
            internal_error(format!("Failed to update validation status: {}", e))
        })?;

    Ok(Json(ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid,
        validation_status: map_validation_status(validation_status),
        errors: if validation_errors.is_empty() {
            None
        } else {
            Some(validation_errors)
        },
        validated_at: chrono::Utc::now().to_rfc3339(),
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
        use crate::error_helpers::forbidden;
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Get dataset to find storage path
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation before deletion - non-admin users can only delete their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be deleted by admins
        use crate::error_helpers::forbidden;
        return Err(forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Delete from database (cascades to files and statistics)
    state
        .db
        .delete_training_dataset(&dataset_id)
        .await
        .map_err(|e| db_error(format!("Failed to delete dataset: {}", e)))?;

    // Delete files from filesystem
    if tokio::fs::try_exists(&dataset.storage_path)
        .await
        .unwrap_or(false)
    {
        tokio::fs::remove_dir_all(&dataset.storage_path)
            .await
            .map_err(|e| {
                error!(
                    "Failed to delete dataset files at {}: {}",
                    dataset.storage_path, e
                );
                // Don't fail the request if filesystem cleanup fails
                e
            })
            .ok();
    }

    info!("Deleted dataset {} and its files", dataset_id);

    // Audit log: dataset deleted
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_DELETE,
        crate::audit_helper::resources::DATASET,
        Some(&dataset_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for progress stream
#[derive(Deserialize, ToSchema)]
pub struct ProgressStreamQuery {
    pub dataset_id: Option<String>,
}

/// Stream dataset upload and processing progress via SSE
///
/// This endpoint establishes a Server-Sent Events (SSE) connection that streams
/// progress events for dataset operations. Clients can connect to receive real-time
/// updates about:
/// - File upload progress (percentage, current file)
/// - Dataset validation progress (files processed, validation results)
/// - Statistics computation progress
///
/// Events are JSON objects with the following fields:
/// - `dataset_id`: The ID of the dataset being processed
/// - `event_type`: One of "upload", "validation", or "statistics"
/// - `current_file`: The file currently being processed (optional)
/// - `percentage_complete`: Overall progress as a percentage (0-100)
/// - `total_files`: Total number of files in the dataset (optional)
/// - `files_processed`: Number of files processed so far (optional)
/// - `message`: Human-readable status message
/// - `timestamp`: RFC3339 formatted timestamp
///
/// Example client usage (JavaScript):
/// ```javascript
/// const eventSource = new EventSource('/v1/datasets/upload/progress?dataset_id=abc123');
/// eventSource.onmessage = (event) => {
///   const progress = JSON.parse(event.data);
///   console.log(`${progress.message}: ${progress.percentage_complete}%`);
/// };
/// ```
#[utoipa::path(
    get,
    path = "/v1/datasets/upload/progress",
    params(
        ("dataset_id" = Option<String>, Query, description = "Optional filter by dataset ID")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of dataset progress"),
        (status = 503, description = "Progress streaming not available")
    ),
    tag = "datasets"
)]
pub async fn dataset_upload_progress(
    State(state): State<AppState>,
    Query(query): Query<ProgressStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Get progress broadcast channel from state
    let rx = state
        .dataset_progress_tx
        .as_ref()
        .ok_or_else(|| internal_error("Dataset progress streaming not available"))?
        .subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        match result {
            Ok(event) => {
                // Filter by dataset_id if specified
                if let Some(ref dataset_id) = query.dataset_id {
                    if event.dataset_id != *dataset_id {
                        return None;
                    }
                }

                // Convert to SSE event
                match serde_json::to_string(&event) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ===== Optimization Helper Functions =====

/// Validate file hash using streaming to avoid loading entire file into memory
async fn validate_file_hash_streaming(
    file_path: &std::path::Path,
    expected_hash: &str,
) -> Result<bool, String> {
    // Parse expected hash
    let expected =
        B3Hash::from_hex(expected_hash).map_err(|e| format!("Invalid hash format: {}", e))?;

    // Use IntegrityChecker for efficient streaming hash computation
    // Note: IntegrityChecker is from adapteros-model-hub which may not be available here
    // Fallback to manual streaming implementation
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut hasher = blake3::Hasher::new();

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if n == 0 {
            break;
        }

        hasher.update(&buffer[..n]);
    }

    let computed = B3Hash::from_bytes(*hasher.finalize().as_bytes());
    Ok(computed == expected)
}

/// Batch insert file records to reduce database transaction overhead
/// Reserved for future optimized bulk insert operations
#[allow(dead_code)]
async fn batch_add_files(
    state: &AppState,
    dataset_id: &str,
    files: &[DatasetFile],
) -> Result<(), String> {
    for batch in files.chunks(VALIDATION_BATCH_SIZE) {
        for file in batch {
            state
                .db
                .add_dataset_file(
                    dataset_id,
                    &file.file_name,
                    &file.file_path,
                    file.size_bytes,
                    &file.hash_b3,
                    file.mime_type.as_deref(),
                )
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;
        }
    }
    Ok(())
}

/// Stream file preview without loading entire file into memory
async fn stream_preview_file(
    file_path: &std::path::Path,
    format: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut examples = Vec::new();
    let mut count = 0;

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if n == 0 {
            break;
        }

        if count >= limit {
            break;
        }

        let text = String::from_utf8_lossy(&buffer[..n]);
        for line in text.lines() {
            if count >= limit {
                break;
            }

            match format {
                "jsonl" => {
                    if !line.trim().is_empty() {
                        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
                            examples.push(json_value);
                            count += 1;
                        }
                    }
                }
                "json" => {
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(array) = json_value.as_array() {
                            for item in array.iter().take(limit - count) {
                                examples.push(item.clone());
                                count += 1;
                            }
                        } else {
                            examples.push(json_value);
                            count += 1;
                        }
                    }
                }
                "txt" | "text" => {
                    examples.push(serde_json::json!({ "text": line }));
                    count += 1;
                }
                _ => {
                    examples.push(serde_json::json!({ "content": line }));
                    count += 1;
                }
            }
        }
    }

    Ok(examples)
}

// ===== Chunked Upload Handlers =====

/// Upload a single chunk for a chunked upload session
///
/// This endpoint receives a single chunk of data for an ongoing chunked upload.
/// Chunks can be uploaded in any order and the system will track which chunks
/// have been received. The session must have been initiated first with the
/// initiate_chunked_upload endpoint.
///
/// ## Error Cases
/// - 404: Session not found or expired (sessions expire after 24 hours)
/// - 400: Invalid chunk index (negative or exceeds expected chunks)
/// - 409: Chunk already uploaded (duplicate chunk index)
/// - 413: Chunk size exceeds the session's configured chunk size
/// - 500: Failed to write chunk to disk
#[utoipa::path(
    post,
    path = "/v1/datasets/chunked-upload/{session_id}/chunk",
    params(
        ("session_id" = String, Path, description = "Upload session ID"),
        UploadChunkQuery,
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Chunk uploaded successfully", body = UploadChunkResponse),
        (status = 400, description = "Invalid chunk index or data"),
        (status = 404, description = "Session not found or expired"),
        (status = 409, description = "Chunk already uploaded"),
        (status = 413, description = "Chunk too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn upload_chunk(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Query(query): Query<UploadChunkQuery>,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    let chunk_index = query.chunk_index;

    let (session, expected_chunks, chunk_hash, chunks_received, is_complete, resume_token) =
        persist_chunk(&state, &session_id, chunk_index, &body).await?;

    // Send progress event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &session_id,
        "upload",
        Some(session.file_name.clone()),
        (chunks_received as f32 / expected_chunks as f32) * 100.0,
        format!(
            "Uploaded chunk {}/{} for {}",
            chunk_index + 1,
            expected_chunks,
            session.file_name
        ),
        Some(expected_chunks as i32),
        Some(chunks_received as i32),
    );

    info!(
        "Uploaded chunk {}/{} for session {} ({} bytes, hash: {})",
        chunk_index + 1,
        expected_chunks,
        session_id,
        body.len(),
        chunk_hash
    );

    Ok(Json(UploadChunkResponse {
        session_id,
        chunk_index,
        chunk_hash,
        chunks_received,
        expected_chunks,
        is_complete,
        resume_token,
    }))
}

/// Complete a chunked upload and create the dataset
///
/// This endpoint assembles all uploaded chunks into the final file and creates
/// a dataset entry in the database. All chunks must have been uploaded before
/// calling this endpoint.
///
/// ## Cleanup Strategy
/// - On success: Temporary chunk files are deleted during assembly
/// - On failure: Temporary files remain for retry; session expires after 24 hours
/// - Abandoned sessions: Background cleanup runs every hour to remove expired sessions
///   and their temporary files (see UPLOAD_TIMEOUT_SECS in chunked_upload.rs)
#[utoipa::path(
    post,
    path = "/v1/datasets/chunked-upload/{session_id}/complete",
    params(
        ("session_id" = String, Path, description = "Upload session ID"),
    ),
    request_body = CompleteChunkedUploadRequest,
    responses(
        (status = 200, description = "Dataset created successfully", body = CompleteChunkedUploadResponse),
        (status = 400, description = "Upload not complete or validation failed"),
        (status = 404, description = "Session not found or expired"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn complete_chunked_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(request): Json<CompleteChunkedUploadRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    let paths = DatasetPaths::new(resolve_dataset_root(&state));

    // Get session
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;

    // Verify upload is complete
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(&session_id)
        .await
        .map_err(internal_error)?;

    if !is_complete {
        let expected_chunks = ((session.total_size + (session.chunk_size as u64 - 1))
            / (session.chunk_size as u64)) as usize;
        let received = session.received_chunks.len();

        // Find missing chunks for error message
        let missing: Vec<usize> = (0..expected_chunks)
            .filter(|i| !session.received_chunks.contains_key(i))
            .take(10)
            .collect();

        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!(
                    "Upload not complete. Received {}/{} chunks. Missing chunks: {:?}{}",
                    received,
                    expected_chunks,
                    missing,
                    if missing.len() < expected_chunks - received {
                        "..."
                    } else {
                        ""
                    }
                ),
                code: "UPLOAD_INCOMPLETE".to_string(),
                details: None,
            }),
        ));
    }

    ensure_dirs([
        paths.files.as_path(),
        paths.temp.as_path(),
        paths.chunked.as_path(),
        paths.logs.as_path(),
    ])
    .await?;

    let dataset_id = Uuid::now_v7().to_string();
    let dataset_path = paths.dataset_dir(&dataset_id);
    ensure_dirs([dataset_path.as_path()]).await?;

    let output_path = dataset_path.join(&session.file_name);

    // Assemble chunks
    let (file_hash, total_bytes) = match assemble_chunks(&session, &output_path).await {
        Ok(res) => res,
        Err((status, Json(payload))) => {
            let error_msg = payload.error.clone();
            error!("Failed to assemble chunks: {}", error_msg);
            let db = state.db.clone();
            let claims_clone = claims.clone();
            let error_msg_clone = error_msg.clone();
            if let Err(e) =
                spawn_deterministic(format!("audit-log:dataset-upload-failure"), async move {
                    let _ = log_failure(
                        &db,
                        &claims_clone,
                        actions::DATASET_UPLOAD,
                        resources::DATASET,
                        None,
                        &error_msg_clone,
                    )
                    .await;
                })
            {
                let db_fallback = state.db.clone();
                let claims_fallback = claims.clone();
                let error_msg_fallback = error_msg.clone();
                let _ = tokio::spawn(async move {
                    let _ = log_failure(
                        &db_fallback,
                        &claims_fallback,
                        actions::DATASET_UPLOAD,
                        resources::DATASET,
                        None,
                        &error_msg_fallback,
                    )
                    .await;
                });
            }
            return Err((status, Json(payload)));
        }
    };

    // Validate file format if requested
    if let Err(e) =
        FileValidator::quick_validate(&output_path, &request.format, STREAM_BUFFER_SIZE).await
    {
        warn!("File format validation warning: {}", e);
        // Continue anyway - validation is advisory for chunked uploads
    }

    // Determine dataset name
    let dataset_name = request.name.unwrap_or_else(|| session.file_name.clone());

    // Create dataset in database
    let _dataset_db_id = state
        .db
        .create_training_dataset(
            &dataset_name,
            request.description.as_deref(),
            &request.format,
            &file_hash,
            &dataset_path.to_string_lossy(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| {
            error!("Failed to create dataset record: {}", e);
            db_error(format!("Failed to create dataset record: {}", e))
        })?;

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await?;

    // Add file record
    state
        .db
        .add_dataset_file(
            &dataset_id,
            &session.file_name,
            &output_path.to_string_lossy(),
            total_bytes as i64,
            &file_hash,
            Some(&session.content_type),
        )
        .await
        .map_err(|e| {
            error!("Failed to add file record: {}", e);
            db_error(format!("Failed to add file record: {}", e))
        })?;

    // Clean up session
    let _ = state
        .upload_session_manager
        .remove_session(&session_id)
        .await;

    // Clean up temp directory
    clean_temp(&session.temp_dir).await;

    // Log audit success
    let _ = log_success(
        &state.db,
        &claims,
        actions::DATASET_UPLOAD,
        resources::DATASET,
        Some(&dataset_id),
    )
    .await;

    // Build citation index for training files (best-effort)
    if let Err(e) = build_dataset_index(&state, &dataset_id, &claims.tenant_id).await {
        warn!(
            dataset_id = %dataset_id,
            error = %e,
            "Failed to build dataset citation index (chunked upload)"
        );
    }

    // Send completion event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &dataset_id,
        "upload",
        Some(session.file_name.clone()),
        100.0,
        format!(
            "Completed chunked upload for {} ({} bytes)",
            session.file_name, total_bytes
        ),
        Some(1),
        Some(1),
    );

    info!(
        "Completed chunked upload for session {}. Created dataset {} with {} bytes",
        session_id, dataset_id, total_bytes
    );

    Ok(Json(CompleteChunkedUploadResponse {
        dataset_id,
        name: dataset_name,
        hash: file_hash,
        total_size_bytes: total_bytes as i64,
        storage_path: dataset_path.to_string_lossy().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get the status of an upload session
///
/// Returns information about an ongoing chunked upload session, including
/// which chunks have been received and whether the upload is complete.
#[utoipa::path(
    get,
    path = "/v1/datasets/chunked-upload/{session_id}/status",
    params(
        ("session_id" = String, Path, description = "Upload session ID"),
    ),
    responses(
        (status = 200, description = "Session status", body = UploadSessionStatusResponse),
        (status = 404, description = "Session not found or expired"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_upload_session_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;

    let expected_chunks = expected_chunks(session.total_size, session.chunk_size);

    let chunks_received = session.received_chunks.len();
    let received_chunk_indices: Vec<usize> = session.received_chunks.keys().cloned().collect();
    let is_complete = chunks_received == expected_chunks;

    let created_at = session
        .created_at
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .ok()
        .flatten()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    Ok(Json(UploadSessionStatusResponse {
        session_id,
        file_name: session.file_name,
        total_size: session.total_size,
        chunk_size: session.chunk_size,
        expected_chunks,
        chunks_received,
        received_chunk_indices,
        is_complete,
        created_at,
        compression_format: format!("{:?}", session.compression),
    }))
}

/// Cancel and cleanup an upload session
///
/// Cancels an ongoing chunked upload and removes all temporary files.
/// Use this if the client decides to abort an upload.
#[utoipa::path(
    delete,
    path = "/v1/datasets/chunked-upload/{session_id}",
    params(
        ("session_id" = String, Path, description = "Upload session ID"),
    ),
    responses(
        (status = 204, description = "Session cancelled successfully"),
        (status = 404, description = "Session not found or expired"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn cancel_chunked_upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Get session to find temp dir
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;

    // Remove session from manager
    state
        .upload_session_manager
        .remove_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to remove session: {}", e);
            internal_error(format!("Failed to remove session: {}", e))
        })?;

    // Clean up temp directory
    clean_temp(&session.temp_dir).await;

    info!("Cancelled chunked upload session {}", session_id);

    // Audit log: chunked upload cancelled
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_CHUNKED_UPLOAD_CANCEL,
        crate::audit_helper::resources::DATASET,
        Some(&session_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Request to create a dataset from existing documents or a collection
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDatasetFromDocumentsRequest {
    /// Single document ID (mutually exclusive with collection_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Collection ID to convert (mutually exclusive with document_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Name for the new dataset (auto-generated if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Create a training dataset from existing documents or a document collection
///
/// Converts RAG documents into JSONL training format. Either `document_id` or
/// `collection_id` must be provided (mutually exclusive). The resulting dataset
/// is immediately marked as valid since the source documents are already indexed.
///
/// The JSONL format is: `{"text": "<chunk_text>"}` for each chunk, ordered
/// deterministically by (document_id ASC, chunk_index ASC) for reproducibility.
#[utoipa::path(
    post,
    path = "/v1/datasets/from-documents",
    request_body = CreateDatasetFromDocumentsRequest,
    responses(
        (status = 200, description = "Dataset created successfully", body = DatasetResponse),
        (status = 400, description = "Invalid request - must provide exactly one of document_id or collection_id"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document or collection not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_from_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateDatasetFromDocumentsRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate: exactly one of document_id or collection_id must be provided
    let (document_ids, source_name) = match (&request.document_id, &request.collection_id) {
        (Some(doc_id), None) => {
            // Single document mode
            let doc = state
                .db
                .get_document(&claims.tenant_id, doc_id)
                .await
                .map_err(|e| db_error(format!("Failed to get document: {}", e)))?
                .ok_or_else(|| not_found("Document"))?;

            // Tenant isolation check
            validate_tenant_isolation(&claims, &doc.tenant_id)?;

            // Check document is indexed (has chunks available)
            if doc.status != "indexed" {
                return Err(bad_request(format!(
                    "Document must be indexed before conversion. Current status: {}",
                    doc.status
                )));
            }

            (
                vec![doc.id.clone()],
                format!("Training from doc: {}", doc.name),
            )
        }
        (None, Some(col_id)) => {
            // Collection mode
            let collection = state
                .db
                .get_collection(&claims.tenant_id, col_id)
                .await
                .map_err(|e| db_error(format!("Failed to get collection: {}", e)))?
                .ok_or_else(|| not_found("Collection"))?;

            // Tenant isolation check
            validate_tenant_isolation(&claims, &collection.tenant_id)?;

            // Get documents in collection
            let docs = state
                .db
                .get_collection_documents(&claims.tenant_id, col_id)
                .await
                .map_err(|e| db_error(format!("Failed to get collection documents: {}", e)))?;

            if docs.is_empty() {
                return Err(bad_request("Collection is empty - no documents to convert"));
            }

            // Filter to indexed documents only, deterministic order
            let mut indexed_docs: Vec<_> =
                docs.into_iter().filter(|d| d.status == "indexed").collect();
            indexed_docs.sort_by(|a, b| a.id.cmp(&b.id));

            if indexed_docs.is_empty() {
                return Err(bad_request(
                    "No indexed documents in collection. Documents must be indexed before conversion.",
                ));
            }

            let doc_ids = indexed_docs.iter().map(|d| d.id.clone()).collect();
            (
                doc_ids,
                format!("Training from collection: {}", collection.name),
            )
        }
        (Some(_), Some(_)) => {
            return Err(bad_request(
                "Cannot specify both document_id and collection_id. Provide exactly one.",
            ));
        }
        (None, None) => {
            return Err(bad_request(
                "Must provide either document_id or collection_id",
            ));
        }
    };

    // Safety limits
    const MAX_CHUNKS: usize = 50_000; // Max chunks to prevent massive datasets
    const MAX_FILE_SIZE: i64 = 100 * 1024 * 1024; // 100MB max JSONL file

    // Get chunks for all documents with deterministic ordering
    // SECURITY: tenant_id enforced at DB level
    let chunks = state
        .db
        .get_chunks_for_documents(&claims.tenant_id, &document_ids)
        .await
        .map_err(|e| db_error(format!("Failed to get document chunks: {}", e)))?;

    if chunks.is_empty() {
        return Err(bad_request(
            "No text chunks found in the selected documents",
        ));
    }

    if chunks.len() > MAX_CHUNKS {
        return Err(bad_request(format!(
            "Too many chunks ({}). Maximum allowed is {}. Try selecting fewer documents.",
            chunks.len(),
            MAX_CHUNKS
        )));
    }

    // Generate JSONL content: {"text": "<chunk_text>"} per chunk
    let mut jsonl_lines: Vec<String> = Vec::with_capacity(chunks.len());
    for chunk in &chunks {
        if let Some(text) = &chunk.text_preview {
            if !text.trim().is_empty() {
                // Escape for JSON and create line
                let json_obj = serde_json::json!({ "text": text });
                jsonl_lines.push(json_obj.to_string());
            }
        }
    }

    if jsonl_lines.is_empty() {
        return Err(bad_request(
            "No non-empty text chunks found in the selected documents",
        ));
    }

    let jsonl_content = jsonl_lines.join("\n");
    let content_bytes = jsonl_content.as_bytes();
    let file_size = content_bytes.len() as i64;

    if file_size > MAX_FILE_SIZE {
        return Err(bad_request(format!(
            "Generated dataset too large ({} bytes). Maximum allowed is {} bytes.",
            file_size, MAX_FILE_SIZE
        )));
    }

    // Compute BLAKE3 hash
    let content_hash = hash_file(content_bytes);

    let dataset_name = request.name.unwrap_or(source_name);

    let dataset_paths = DatasetPaths::new(resolve_dataset_root(&state));
    ensure_dirs([
        dataset_paths.files.as_path(),
        dataset_paths.temp.as_path(),
        dataset_paths.chunked.as_path(),
        dataset_paths.logs.as_path(),
    ])
    .await?;

    // Create dataset record first to get the canonical ID
    // Use a placeholder path initially, then update after directory creation
    let dataset_id = state
        .db
        .create_training_dataset(
            &dataset_name,
            request.description.as_deref(),
            "jsonl",
            &content_hash,
            "", // Placeholder - will update after creating directory
            Some(&claims.sub),
        )
        .await
        .map_err(|e| db_error(format!("Failed to create dataset record: {}", e)))?;

    // Now create directory with the canonical ID
    let dataset_path = dataset_paths.dataset_dir(&dataset_id);
    if let Err(e) = ensure_dirs([dataset_path.as_path()]).await {
        // Cleanup: delete DB record on failure
        if let Err(cleanup_err) = state.db.delete_training_dataset(&dataset_id).await {
            warn!(dataset_id = %dataset_id, error = %cleanup_err, "Failed to cleanup orphaned dataset record");
        }
        return Err(e);
    }

    // Write JSONL file
    let file_name = "training.jsonl";
    let file_path = dataset_path.join(file_name);
    if let Err(e) = fs::write(&file_path, content_bytes).await {
        // Cleanup both directory and DB record
        clean_dataset_dir(&dataset_path).await;
        if let Err(cleanup_err) = state.db.delete_training_dataset(&dataset_id).await {
            warn!(dataset_id = %dataset_id, error = %cleanup_err, "Failed to cleanup orphaned dataset record");
        }
        return Err(internal_error(format!(
            "Failed to write dataset file: {}",
            e
        )));
    }

    // Update storage path now that we have the real path
    if let Err(e) = state
        .db
        .update_dataset_storage_path(&dataset_id, &dataset_path.to_string_lossy())
        .await
    {
        clean_dataset_dir(&dataset_path).await;
        if let Err(cleanup_err) = state.db.delete_training_dataset(&dataset_id).await {
            warn!(dataset_id = %dataset_id, error = %cleanup_err, "Failed to cleanup orphaned dataset record");
        }
        return Err(db_error(format!("Failed to update storage path: {}", e)));
    }

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    if let Err(e) = bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await {
        clean_dataset_dir(&dataset_path).await;
        if let Err(cleanup_err) = state.db.delete_training_dataset(&dataset_id).await {
            warn!(dataset_id = %dataset_id, error = %cleanup_err, "Failed to cleanup orphaned dataset record");
        }
        return Err(e);
    }

    // Add file record
    if let Err(e) = state
        .db
        .add_dataset_file(
            &dataset_id,
            file_name,
            &file_path.to_string_lossy(),
            file_size,
            &content_hash,
            Some("application/jsonl"),
        )
        .await
    {
        clean_dataset_dir(&dataset_path).await;
        if let Err(cleanup_err) = state.db.delete_training_dataset(&dataset_id).await {
            warn!(dataset_id = %dataset_id, error = %cleanup_err, "Failed to cleanup orphaned dataset record");
        }
        return Err(db_error(format!("Failed to add file record: {}", e)));
    }

    // Run actual validation instead of hardcoding "valid"
    // The JSONL we generate should always be valid, but run through the same
    // validation pipeline for consistency
    let validation_result =
        FileValidator::quick_validate(&file_path, "jsonl", STREAM_BUFFER_SIZE).await;

    let (validation_status, validation_errors) = match validation_result {
        Ok(()) => ("valid".to_string(), None),
        Err(e) => ("invalid".to_string(), Some(e.to_string())),
    };

    state
        .db
        .update_dataset_validation(
            &dataset_id,
            &validation_status,
            validation_errors.as_deref(),
        )
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    let response_validation_status = map_validation_status(&validation_status);
    let response_validation_errors = map_validation_errors(validation_errors);

    let now = chrono::Utc::now().to_rfc3339();

    // Audit log
    let _ = log_success(
        &state.db,
        &claims,
        actions::DATASET_CREATE,
        resources::DATASET,
        Some(&dataset_id),
    )
    .await;

    // Build citation index for training files (best-effort)
    if let Err(e) = build_dataset_index(&state, &dataset_id, &claims.tenant_id).await {
        warn!(
            dataset_id = %dataset_id,
            error = %e,
            "Failed to build dataset citation index (document-derived)"
        );
    }

    info!(
        dataset_id = %dataset_id,
        name = %dataset_name,
        chunks = chunks.len(),
        lines = jsonl_lines.len(),
        size_bytes = file_size,
        "Created dataset from documents"
    );

    Ok(Json(DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        name: dataset_name,
        description: request.description,
        file_count: 1,
        total_size_bytes: file_size,
        format: "jsonl".to_string(),
        hash: content_hash,
        storage_path: dataset_path.to_string_lossy().to_string(),
        validation_status: response_validation_status,
        validation_errors: response_validation_errors,
        created_by: claims.sub.clone(),
        created_at: now.clone(),
        updated_at: now,
    }))
}
