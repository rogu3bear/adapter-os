mod chunked;
mod fs_utils;
mod hashing;
mod paths;
mod progress;
mod tenant;

// Re-export selected helpers for services and shared consumers
pub use self::fs_utils::{clean_dataset_dir, ensure_dirs};
pub use self::hashing::hash_file;
pub use self::paths::{resolve_dataset_root, DatasetPaths};
pub use self::tenant::bind_dataset_to_tenant;

use self::chunked::{assemble_chunks, expected_chunks, persist_chunk, prepare_session};
use self::fs_utils::clean_temp;
use self::hashing::hash_multi;
use self::progress::emit_progress;
use super::chunked_upload::{
    CompressionFormat, FileValidator, DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::citations::build_dataset_index;
use crate::error_helpers::{
    bad_request, db_error, forbidden, internal_error, not_found, payload_too_large,
};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::services::{
    CanonicalRow, DatasetFromCollectionParams, DatasetFromDocumentIdsParams,
    DefaultTrainingDatasetService, TrainingDatasetService,
};
use crate::state::AppState;
use crate::storage_usage::compute_tenant_storage_usage;
use crate::types::*;
use adapteros_core::B3Hash;
use adapteros_db::training_datasets::DatasetFile;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey, StorageKind};
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
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Maximum file size (100MB)
const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Maximum total upload size (500MB)
const MAX_TOTAL_SIZE: usize = 500 * 1024 * 1024;

const DEFAULT_DATASET_HARD_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
const DEFAULT_SOFT_PCT: f64 = 0.8;

/// Buffer size for streaming operations (64KB)
pub(crate) const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Validation batch size to reduce database transaction overhead
const VALIDATION_BATCH_SIZE: usize = 10;

fn dataset_quota_limits() -> (u64, u64) {
    let hard = std::env::var("AOS_DATASET_HARD_QUOTA_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DATASET_HARD_QUOTA_BYTES);
    let soft = std::env::var("AOS_DATASET_SOFT_QUOTA_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_else(|| (hard as f64 * DEFAULT_SOFT_PCT) as u64);
    (soft, hard)
}

fn quota_error(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(message.into()).with_code("DATASET_QUOTA_EXCEEDED".to_string())),
    )
}

/// Map validation status: 'pending' → 'pending' for API responses
pub(crate) fn map_validation_status(status: &str) -> DatasetValidationStatus {
    match status {
        "validating" => DatasetValidationStatus::Validating,
        "valid" => DatasetValidationStatus::Valid,
        "invalid" => DatasetValidationStatus::Invalid,
        "failed" => DatasetValidationStatus::Invalid,
        "pending" => DatasetValidationStatus::Pending,
        "skipped" => DatasetValidationStatus::Skipped,
        _ => DatasetValidationStatus::Pending,
    }
}

pub(crate) fn map_validation_errors(errors: Option<String>) -> Option<Vec<String>> {
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

/// Request to apply an admin trust override to a dataset version
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetTrustOverrideRequest {
    /// Override state: allowed | allowed_with_warning | blocked | needs_approval
    pub override_state: String,
    /// Optional human-readable reason for auditability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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
    let dataset_root = resolve_dataset_root(&state).map_err(internal_error)?;
    let paths = DatasetPaths::new(dataset_root);
    let adapters_root = {
        let cfg = state.config.read().map_err(|_| {
            tracing::error!("Config lock poisoned");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE")),
            )
        })?;
        cfg.paths.adapters_root.clone()
    };
    let storage = FsByteStorage::new(paths.files.clone(), adapters_root.into());
    ensure_dirs([
        paths.files.as_path(),
        paths.temp.as_path(),
        paths.chunked.as_path(),
        paths.logs.as_path(),
    ])
    .await?;

    let adapters_root = {
        let cfg = state.config.read().map_err(|_| {
            tracing::error!("Config lock poisoned");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE")),
            )
        })?;
        cfg.paths.adapters_root.clone()
    };
    let storage = FsByteStorage::new(paths.files.clone(), adapters_root.into());

    let dataset_path = paths.dataset_dir(&dataset_id);
    let temp_path = paths.dataset_temp_dir(&dataset_id);

    ensure_dirs([dataset_path.as_path(), temp_path.as_path()]).await?;

    let (soft_quota, hard_quota) = dataset_quota_limits();
    let usage = compute_tenant_storage_usage(&state, &claims.tenant_id)
        .await
        .map_err(|e| internal_error(format!("Failed to compute storage usage: {}", e)))?;
    let mut current_usage = usage.total_bytes();

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

                let predicted_usage = current_usage + total_size as u64;
                if predicted_usage > hard_quota {
                    clean_temp(&temp_path).await;
                    return Err(quota_error(format!(
                        "Dataset storage quota exceeded: {} > {} bytes",
                        predicted_usage, hard_quota
                    )));
                }
                if predicted_usage > soft_quota {
                    warn!(
                        tenant_id = %claims.tenant_id,
                        predicted_usage,
                        soft_quota,
                        "Dataset storage soft quota exceeded"
                    );
                }

                // Compute hash using B3Hash
                let file_hash = hash_file(&data);

                let key = StorageKey {
                    tenant_id: Some(claims.tenant_id.clone()),
                    object_id: dataset_id.clone(),
                    version_id: None,
                    file_name: file_name.clone(),
                    kind: StorageKind::DatasetFile,
                };
                let location = storage
                    .store_bytes(&key, &data)
                    .await
                    .map_err(|e| internal_error(format!("Failed to store dataset file: {}", e)))?;
                let permanent_path = location.path;

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
                current_usage += file_size as u64;

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
    state
        .db
        .create_training_dataset_with_id(
            &dataset_id,
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
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_UPLOAD,
        crate::audit_helper::resources::DATASET,
        Some(&dataset_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

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

    let dataset_root = resolve_dataset_root(&state).map_err(internal_error)?;
    let paths = DatasetPaths::new(dataset_root);

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
            .map_err(|e| db_error(format!("Failed to load dataset versions: {}", e)))?;
        let (dataset_version_id, trust_state) = latest_trusted
            .map(|(v, trust)| (Some(v.id), Some(trust)))
            .unwrap_or((None, None));

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
            storage_path: d.storage_path,
            validation_status: map_validation_status(&d.validation_status),
            validation_errors: map_validation_errors(d.validation_errors),
            trust_state,
            created_by: d.created_by.unwrap_or_else(|| "system".to_string()),
            created_at: d.created_at,
            updated_at: d.updated_at,
        });
    }

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

    let latest_trusted = state
        .db
        .get_latest_trusted_dataset_version_for_dataset(&dataset.id)
        .await
        .map_err(|e| db_error(format!("Failed to load dataset versions: {}", e)))?;
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
        storage_path: dataset.storage_path,
        validation_status: map_validation_status(&dataset.validation_status),
        validation_errors: map_validation_errors(dataset.validation_errors),
        trust_state,
        created_by: dataset.created_by.unwrap_or_else(|| "system".to_string()),
        created_at: dataset.created_at,
        updated_at: dataset.updated_at,
    }))
}

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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDatasetVersionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_json: Option<Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateDatasetVersionResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub version_number: i64,
    pub trust_state: String,
    pub created_at: String,
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
        .ok_or_else(|| internal_error("Dataset version was created but not found"))?;

    Ok(Json(CreateDatasetVersionResponse {
        dataset_id,
        dataset_version_id: version_id,
        version_number: version.version_number,
        trust_state: version.trust_state,
        created_at: version.created_at,
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

    // Mirror structural validation into dataset version trust pipeline
    if let Ok(version_id) = state.db.ensure_dataset_version_exists(&dataset_id).await {
        let _ = state
            .db
            .update_dataset_version_structural_validation(
                &version_id,
                validation_status,
                validation_errors_str.as_deref(),
            )
            .await;
        // Kick off tier2 safety validation asynchronously (stub pipeline)
        spawn_tier2_safety_validation(state.clone(), version_id.clone(), claims.sub.clone());
        let _ = state
            .db
            .record_dataset_version_validation_run(
                &version_id,
                "tier1_structural",
                if is_valid { "valid" } else { "invalid" },
                Some("structural"),
                validation_errors_str.as_deref(),
                None,
                Some(claims.sub.as_str()),
            )
            .await;
    }

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

/// Update semantic/safety statuses for a dataset version (Tier 2).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateDatasetSafetyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pii_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toxicity_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leak_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anomaly_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpdateDatasetSafetyResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub trust_state: String,
    pub overall_safety_status: String,
}

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
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TrustOverrideRequest {
    pub trust_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrustOverrideResponse {
    pub dataset_id: String,
    pub dataset_version_id: String,
    pub trust_state: String,
}

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

/// Spawn asynchronous tier2 safety validation (heuristic scan with trust gating).
fn spawn_tier2_safety_validation(state: AppState, dataset_version_id: String, actor: String) {
    tokio::spawn(async move {
        // Record pending safety validation
        let _ = state
            .db
            .record_dataset_version_validation_run(
                &dataset_version_id,
                "tier2_safety",
                "pending",
                Some("safety"),
                None,
                None,
                Some(actor.as_str()),
            )
            .await;

        let version = match state
            .db
            .get_training_dataset_version(&dataset_version_id)
            .await
        {
            Ok(Some(v)) => v,
            Ok(None) => {
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some("Dataset version not found"),
                        None,
                        Some(actor.as_str()),
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
                return;
            }
            Err(e) => {
                let msg = format!("Failed to load dataset version: {}", e);
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some(msg.as_str()),
                        None,
                        Some(actor.as_str()),
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
                return;
            }
        };

        match run_tier2_safety_scan(&version.storage_path).await {
            Ok(outcome) => {
                let pii_status = outcome.pii.status();
                let toxicity_status = outcome.toxicity.status();
                let leak_status = outcome.leak.status();
                let anomaly_status = outcome.anomaly.status();

                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some(pii_status.as_str()),
                        Some(toxicity_status.as_str()),
                        Some(leak_status.as_str()),
                        Some(anomaly_status.as_str()),
                    )
                    .await;

                record_safety_validation_runs(
                    &state,
                    &dataset_version_id,
                    actor.as_str(),
                    &outcome,
                )
                .await;
            }
            Err(err) => {
                let msg = err;
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some(msg.as_str()),
                        None,
                        Some(actor.as_str()),
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
            }
        }
    });
}

/// Safety signal sample cap to avoid large audit payloads
const SAFETY_SAMPLE_LIMIT: usize = 5;
const PROMPT_WARN_LEN: usize = 4096;
const PROMPT_BLOCK_LEN: usize = 12000;

#[derive(Default)]
struct SignalAccumulator {
    warn: usize,
    block: usize,
    reasons: Vec<String>,
    sample_row_ids: Vec<String>,
}

impl SignalAccumulator {
    fn note_warn(&mut self, reason: impl Into<String>, row_id: Option<&str>) {
        self.warn += 1;
        self.reasons.push(reason.into());
        if let Some(id) = row_id {
            push_sample(&mut self.sample_row_ids, id);
        }
    }

    fn note_block(&mut self, reason: impl Into<String>, row_id: Option<&str>) {
        self.block += 1;
        self.reasons.push(reason.into());
        if let Some(id) = row_id {
            push_sample(&mut self.sample_row_ids, id);
        }
    }

    fn status(&self) -> String {
        if self.block > 0 {
            "block".to_string()
        } else if self.warn > 0 {
            "warn".to_string()
        } else {
            "clean".to_string()
        }
    }
}

#[derive(Default)]
struct SafetyScanOutcome {
    pii: SignalAccumulator,
    toxicity: SignalAccumulator,
    leak: SignalAccumulator,
    anomaly: SignalAccumulator,
}

fn push_sample(target: &mut Vec<String>, row_id: &str) {
    if target.len() < SAFETY_SAMPLE_LIMIT {
        target.push(row_id.to_string());
    }
}

fn has_email_like_token(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        let cleaned = token
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '@' && c != '.' && c != '-');
        let mut parts = cleaned.split('@');
        if let (Some(local), Some(domain)) = (parts.next(), parts.next()) {
            !local.is_empty() && domain.contains('.')
        } else {
            false
        }
    })
}

fn has_secret_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("aws_secret_access_key")
        || lower.contains("aws_access_key_id")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("-----begin")
}

fn has_toxic_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["hate", "kill", "bomb", "terror", "violent"]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn classify_row(
    row: &CanonicalRow,
    seen_ids: &mut HashSet<String>,
    outcome: &mut SafetyScanOutcome,
) {
    // Duplicate row_ids should not be allowed
    if !seen_ids.insert(row.row_id.clone()) {
        outcome
            .anomaly
            .note_block("duplicate_row_id", Some(&row.row_id));
    }

    // Length bounds
    let prompt_len = row.prompt.len();
    let response_len = row.response.len();
    if prompt_len > PROMPT_BLOCK_LEN || response_len > PROMPT_BLOCK_LEN {
        outcome
            .anomaly
            .note_block("text_too_long", Some(&row.row_id));
    } else if prompt_len > PROMPT_WARN_LEN || response_len > PROMPT_WARN_LEN {
        outcome
            .anomaly
            .note_warn("text_near_limit", Some(&row.row_id));
    }

    let combined = format!("{} {}", row.prompt, row.response);
    if has_email_like_token(&combined) {
        outcome
            .pii
            .note_warn("email_like_pattern", Some(&row.row_id));
    }
    if has_secret_marker(&combined) {
        outcome.leak.note_block("secret_marker", Some(&row.row_id));
    }
    if has_toxic_marker(&combined) {
        outcome
            .toxicity
            .note_warn("toxic_language", Some(&row.row_id));
    }
}

async fn run_tier2_safety_scan(path: &str) -> Result<SafetyScanOutcome, String> {
    let file = fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open dataset for safety scan: {}", e))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut outcome = SafetyScanOutcome::default();
    let mut seen_ids: HashSet<String> = HashSet::new();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read dataset line: {}", e))?
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<CanonicalRow>(trimmed) {
            Ok(row) => classify_row(&row, &mut seen_ids, &mut outcome),
            Err(e) => outcome
                .anomaly
                .note_warn(format!("row_parse_error:{e}"), None),
        }
    }

    Ok(outcome)
}

async fn record_safety_validation_runs(
    state: &AppState,
    dataset_version_id: &str,
    actor: &str,
    outcome: &SafetyScanOutcome,
) {
    let signals = [
        ("pii", &outcome.pii),
        ("toxicity", &outcome.toxicity),
        ("leak", &outcome.leak),
        ("anomaly", &outcome.anomaly),
    ];

    for (signal, acc) in signals {
        let status = match acc.status().as_str() {
            "block" => "block",
            "warn" => "warn",
            "clean" => "valid",
            _ => "pending",
        };
        let reasons_json = if acc.reasons.is_empty() {
            None
        } else {
            serde_json::to_string(&acc.reasons).ok()
        };
        let samples_json = if acc.sample_row_ids.is_empty() {
            None
        } else {
            serde_json::to_string(&acc.sample_row_ids).ok()
        };

        let _ = state
            .db
            .record_dataset_version_validation_run(
                dataset_version_id,
                "tier2_safety",
                status,
                Some(signal),
                reasons_json.as_deref(),
                samples_json.as_deref(),
                Some(actor),
            )
            .await;
    }
}

#[cfg(test)]
mod safety_scan_tests {
    use super::*;

    fn mk_row(prompt: &str, response: &str, row_id: &str) -> CanonicalRow {
        CanonicalRow {
            row_id: row_id.to_string(),
            split: "train".into(),
            prompt: prompt.into(),
            response: response.into(),
            weight: 1.0,
            metadata: Default::default(),
        }
    }

    #[test]
    fn detects_email_secret_and_duplicates() {
        let mut outcome = SafetyScanOutcome::default();
        let mut seen = HashSet::new();
        let row1 = mk_row("reach me at user@example.com", "ok", "row-1");
        let row2 = mk_row("api_key=SECRET", "body", "row-2");
        let row3 = mk_row("neutral", "text", "row-1"); // duplicate id

        classify_row(&row1, &mut seen, &mut outcome);
        classify_row(&row2, &mut seen, &mut outcome);
        classify_row(&row3, &mut seen, &mut outcome);

        assert_eq!(outcome.pii.status(), "warn");
        assert_eq!(outcome.leak.status(), "block");
        assert_eq!(outcome.anomaly.status(), "block");
        assert!(outcome.pii.sample_row_ids.contains(&row1.row_id));
        assert!(outcome.leak.sample_row_ids.contains(&row2.row_id));
    }

    #[tokio::test]
    async fn safety_scan_marks_parse_errors_as_anomaly() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let tmp = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        let path = tmp.path().join("data.jsonl");
        let row = mk_row("ok prompt", "resp", "row-1");
        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&row).unwrap(),
            "{invalid json"
        );
        fs::write(&path, content).await.unwrap();

        let outcome = run_tier2_safety_scan(path.to_str().unwrap()).await.unwrap();
        assert!(matches!(
            outcome.anomaly.status().as_str(),
            "warn" | "block"
        ));
    }
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

    let dataset_root = resolve_dataset_root(&state).map_err(internal_error)?;
    let paths = DatasetPaths::new(dataset_root);

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
                failure_code: None,
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

    let adapters_root = {
        let cfg = state.config.read().map_err(|_| {
            tracing::error!("Config lock poisoned");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE")),
            )
        })?;
        cfg.paths.adapters_root.clone()
    };
    let storage = FsByteStorage::new(paths.files.clone(), adapters_root.into());

    let dataset_id = Uuid::now_v7().to_string();
    let storage_key = StorageKey {
        tenant_id: Some(claims.tenant_id.clone()),
        object_id: dataset_id.clone(),
        version_id: None,
        file_name: session.file_name.clone(),
        kind: StorageKind::DatasetFile,
    };
    let output_path = storage.path_for(&storage_key).map_err(|e| {
        internal_error(format!(
            "Failed to resolve storage path for dataset {}: {}",
            dataset_id, e
        ))
    })?;
    let dataset_path = output_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| paths.dataset_dir(&dataset_id));
    ensure_dirs([dataset_path.as_path()]).await?;

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

    let (soft_quota, hard_quota) = dataset_quota_limits();
    let usage = compute_tenant_storage_usage(&state, &claims.tenant_id)
        .await
        .map_err(|e| internal_error(format!("Failed to compute storage usage: {}", e)))?;
    let predicted_usage = usage.total_bytes() + total_bytes;
    if predicted_usage > hard_quota {
        return Err(quota_error(format!(
            "Dataset storage quota exceeded: {} > {} bytes",
            predicted_usage, hard_quota
        )));
    }
    if predicted_usage > soft_quota {
        warn!(
            tenant_id = %claims.tenant_id,
            predicted_usage,
            soft_quota,
            "Dataset storage soft quota exceeded"
        );
    }

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
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_CHUNKED_UPLOAD_CANCEL,
        crate::audit_helper::resources::DATASET,
        Some(&session_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Request to create a dataset from existing documents or a collection
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDatasetFromDocumentsRequest {
    /// Single document ID (mutually exclusive with collection_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    /// Multiple document IDs (mutually exclusive with collection_id)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_ids: Option<Vec<String>>,
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

    // Validate exclusivity: only one source allowed
    let multiple_sources = request.collection_id.is_some()
        && (request.document_id.is_some() || request.document_ids.is_some());
    if multiple_sources {
        return Err(bad_request(
            "Cannot specify both document_id/document_ids and collection_id. Provide exactly one.",
        ));
    }

    let service = DefaultTrainingDatasetService::new(Arc::new(state.clone()));

    let dataset = match (
        request.document_ids,
        request.document_id,
        request.collection_id,
    ) {
        (Some(document_ids), None, None) => {
            service
                .create_from_document_ids(
                    &claims,
                    DatasetFromDocumentIdsParams {
                        document_ids,
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, Some(document_id), None) => {
            service
                .create_from_document_ids(
                    &claims,
                    DatasetFromDocumentIdsParams {
                        document_ids: vec![document_id],
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, None, Some(collection_id)) => {
            service
                .create_from_collection(
                    &claims,
                    DatasetFromCollectionParams {
                        collection_id,
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, None, None) => {
            return Err(bad_request(
                "Must provide either document_id, document_ids, or collection_id",
            ));
        }
        _ => {
            return Err(bad_request(
                "Cannot specify both document_id/document_ids and collection_id. Provide exactly one.",
            ));
        }
    };

    Ok(Json(dataset))
}
