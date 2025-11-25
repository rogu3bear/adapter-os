use super::chunked_upload::{
    ChunkAssembler, ChunkWriter, CompressionFormat, FileValidator, ResumeToken, DEFAULT_CHUNK_SIZE,
    MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::audit_helper::{actions, log_failure, log_success, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::{AppState, DatasetProgressEvent};
use crate::types::*;
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
use blake3::Hasher;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Default dataset storage root if not configured
const DEFAULT_DATASET_STORAGE: &str = "var/datasets";

/// Maximum file size (100MB)
const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Maximum total upload size (500MB)
const MAX_TOTAL_SIZE: usize = 500 * 1024 * 1024;

/// Buffer size for streaming operations (64KB)
const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Validation batch size to reduce database transaction overhead
const VALIDATION_BATCH_SIZE: usize = 10;

/// Map validation status: 'pending' → 'draft' for API responses
fn map_validation_status(status: &str) -> String {
    match status {
        "pending" => "draft".to_string(),
        other => other.to_string(),
    }
}

/// Helper function to send progress events
fn send_progress_event(
    tx: Option<&Arc<tokio::sync::broadcast::Sender<DatasetProgressEvent>>>,
    event: DatasetProgressEvent,
) {
    if let Some(sender) = tx {
        let _ = sender.send(event);
    }
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dataset_id = Uuid::now_v7().to_string();
    let storage_root = std::env::var("DATASET_STORAGE_PATH")
        .unwrap_or_else(|_| DEFAULT_DATASET_STORAGE.to_string());

    // Create dataset directory structure
    let dataset_path = PathBuf::from(&storage_root).join(&dataset_id);
    let files_path = dataset_path.join("files");
    let temp_path = PathBuf::from(&storage_root).join("temp").join(&dataset_id);

    fs::create_dir_all(&files_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create dataset directory: {}", e),
        )
    })?;

    fs::create_dir_all(&temp_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create temp directory: {}", e),
        )
    })?;

    // Send initial progress event
    send_progress_event(
        state.dataset_progress_tx.as_ref(),
        DatasetProgressEvent {
            dataset_id: dataset_id.clone(),
            event_type: "upload".to_string(),
            current_file: None,
            percentage_complete: 0.0,
            total_files: None,
            files_processed: Some(0),
            message: "Starting dataset upload...".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    let mut uploaded_files = Vec::new();
    let mut total_size = 0usize;
    let mut dataset_name = String::new();
    let mut dataset_description = String::new();
    let mut dataset_format = "jsonl".to_string();
    let mut file_count = 0;

    // Process multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart field: {}", e),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                dataset_name = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read name field: {}", e),
                    )
                })?;
            }
            "description" => {
                dataset_description = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read description field: {}", e),
                    )
                })?;
            }
            "format" => {
                dataset_format = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read format field: {}", e),
                    )
                })?;
            }
            "file" | "files" => {
                let file_name = field
                    .file_name()
                    .ok_or((StatusCode::BAD_REQUEST, "File must have a name".to_string()))?
                    .to_string();

                let content_type = field
                    .content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                // Stream file to temporary location
                let temp_file_path = temp_path.join(&file_name);
                let mut temp_file = fs::File::create(&temp_file_path).await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to create temp file: {}", e),
                    )
                })?;

                let mut hasher = Hasher::new();
                let data = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read file data: {}", e),
                    )
                })?;

                let file_size = data.len();

                // Check file size limits
                if file_size > MAX_FILE_SIZE {
                    fs::remove_dir_all(&temp_path).await.ok();
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "File {} exceeds maximum size of {}MB",
                            file_name,
                            MAX_FILE_SIZE / 1024 / 1024
                        ),
                    ));
                }

                total_size += file_size;
                if total_size > MAX_TOTAL_SIZE {
                    fs::remove_dir_all(&temp_path).await.ok();
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "Total upload size exceeds maximum of {}MB",
                            MAX_TOTAL_SIZE / 1024 / 1024
                        ),
                    ));
                }

                // Write and hash file
                hasher.update(&data);
                temp_file.write_all(&data).await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to write file: {}", e),
                    )
                })?;
                temp_file.flush().await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to flush file: {}", e),
                    )
                })?;

                let file_hash = hasher.finalize().to_hex().to_string();

                // Move file to permanent location
                let permanent_path = files_path.join(&file_name);
                fs::rename(&temp_file_path, &permanent_path)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to move file to permanent location: {}", e),
                        )
                    })?;

                file_count += 1;

                // Send progress event for this file
                send_progress_event(
                    state.dataset_progress_tx.as_ref(),
                    DatasetProgressEvent {
                        dataset_id: dataset_id.clone(),
                        event_type: "upload".to_string(),
                        current_file: Some(file_name.clone()),
                        percentage_complete: if file_count > 0 {
                            (file_count as f32 / 10.0).min(100.0)
                        } else {
                            0.0
                        },
                        total_files: None,
                        files_processed: Some(file_count),
                        message: format!("Uploaded {} ({} bytes)", file_name, file_size),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
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
    fs::remove_dir_all(&temp_path).await.ok();

    if uploaded_files.is_empty() {
        fs::remove_dir_all(&dataset_path).await.ok();
        return Err((StatusCode::BAD_REQUEST, "No files uploaded".to_string()));
    }

    if dataset_name.is_empty() {
        dataset_name = format!("Dataset {}", &dataset_id[0..8]);
    }

    // Compute dataset hash from all file hashes
    let mut dataset_hasher = Hasher::new();
    for file in &uploaded_files {
        dataset_hasher.update(file.hash_b3.as_bytes());
    }
    let dataset_hash = dataset_hasher.finalize().to_hex().to_string();

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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create dataset record: {}", e),
            )
        })?;

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    state
        .db
        .update_dataset_extended_fields(
            &dataset_id,
            None,
            None,
            None,
            None,
            None,
            Some(&claims.tenant_id),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set dataset tenant: {}", e),
            )
        })?;

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
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to add file record: {}", e),
                )
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
    Json(request): Json<InitiateChunkedUploadRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Validate total size
    if request.total_size == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "File size must be greater than 0".to_string(),
        ));
    }

    if request.total_size > MAX_TOTAL_SIZE as u64 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "File size exceeds maximum of {}MB",
                MAX_TOTAL_SIZE / 1024 / 1024
            ),
        ));
    }

    // Determine chunk size
    let chunk_size = request.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);
    let chunk_size = chunk_size.max(MIN_CHUNK_SIZE).min(MAX_CHUNK_SIZE);

    // Calculate expected chunks
    let expected_chunks =
        ((request.total_size + (chunk_size as u64 - 1)) / (chunk_size as u64)) as usize;

    // Detect compression
    let content_type = request
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let compression = CompressionFormat::from_content_type(&content_type);

    // Create upload session using the shared session manager
    let storage_root = std::env::var("DATASET_STORAGE_PATH")
        .unwrap_or_else(|_| DEFAULT_DATASET_STORAGE.to_string());
    let temp_base = PathBuf::from(&storage_root).join("chunked");

    fs::create_dir_all(&temp_base).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create temp directory: {}", e),
        )
    })?;

    // Use shared session manager from AppState
    let session = state
        .upload_session_manager
        .create_session(
            request.file_name.clone(),
            request.total_size,
            content_type.clone(),
            chunk_size,
            &temp_base,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(100);
    let _offset = params.offset.unwrap_or(0);

    let datasets = state.db.list_training_datasets(limit).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list datasets: {}", e),
        )
    })?;

    // CRITICAL: Tenant isolation - filter datasets to only show those belonging to the user's tenant
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
            validation_errors: d.validation_errors,
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)
            .map_err(|(code, json_err)| (code, json_err.0.error))?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err((
            StatusCode::FORBIDDEN,
            "Access denied: dataset has no tenant association".to_string(),
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
        validation_errors: dataset.validation_errors,
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
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Verify dataset exists
    let _dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let files = state.db.get_dataset_files(&dataset_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get dataset files: {}", e),
        )
    })?;

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
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Verify dataset exists
    let _dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let stats = state
        .db
        .get_dataset_statistics(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get statistics: {}", e),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Statistics not computed for this dataset".to_string(),
        ))?;

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
    Path(dataset_id): Path<String>,
    Json(request): Json<ValidateDatasetRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    // Set status to 'validating' at start
    state
        .db
        .update_dataset_validation(&dataset_id, "validating", None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update validation status: {}", e),
            )
        })?;

    // Send initial validation event
    send_progress_event(
        state.dataset_progress_tx.as_ref(),
        DatasetProgressEvent {
            dataset_id: dataset_id.clone(),
            event_type: "validation".to_string(),
            current_file: None,
            percentage_complete: 0.0,
            total_files: Some(dataset.file_count as i32),
            files_processed: Some(0),
            message: "Starting dataset validation...".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    // Get dataset files
    let files = state.db.get_dataset_files(&dataset_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get dataset files: {}", e),
        )
    })?;

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
            send_progress_event(
                state.dataset_progress_tx.as_ref(),
                DatasetProgressEvent {
                    dataset_id: dataset_id.clone(),
                    event_type: "validation".to_string(),
                    current_file: Some(file.file_name.clone()),
                    percentage_complete: if total_files > 0.0 {
                        (processed_files as f32 / total_files) * 100.0
                    } else {
                        0.0
                    },
                    total_files: Some(files.len() as i32),
                    files_processed: Some(processed_files as i32),
                    message: format!("Validating {}", file.file_name),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
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
        send_progress_event(
            state.dataset_progress_tx.as_ref(),
            DatasetProgressEvent {
                dataset_id: dataset_id.clone(),
                event_type: "validation".to_string(),
                current_file: Some(file.file_name.clone()),
                percentage_complete: if total_files > 0.0 {
                    (processed_files as f32 / total_files) * 100.0
                } else {
                    0.0
                },
                total_files: Some(files.len() as i32),
                files_processed: Some(processed_files as i32),
                message: format!("Validated {}", file.file_name),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        );
    }

    // Update validation status in database
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update validation status: {}", e),
            )
        })?;

    Ok(Json(ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid,
        validation_status: validation_status.to_string(),
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
    Path(dataset_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);

    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let files = state.db.get_dataset_files(&dataset_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get dataset files: {}", e),
        )
    })?;

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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Get dataset to find storage path
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get dataset: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    // CRITICAL: Validate tenant isolation before deletion - non-admin users can only delete their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)
            .map_err(|(code, json_err)| (code, json_err.0.error))?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id can only be deleted by admins
        return Err((
            StatusCode::FORBIDDEN,
            "Access denied: dataset has no tenant association".to_string(),
        ));
    }

    // Delete from database (cascades to files and statistics)
    state
        .db
        .delete_training_dataset(&dataset_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete dataset: {}", e),
            )
        })?;

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
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    // Get progress broadcast channel from state
    let rx = state
        .dataset_progress_tx
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Dataset progress streaming not available".to_string(),
            )
        })?
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
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];

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

    let computed_hash = hasher.finalize().to_hex().to_string();
    Ok(computed_hash == expected_hash)
}

/// Batch insert file records to reduce database transaction overhead
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload).map_err(|e| (e.0, e.1.error.clone()))?;

    let chunk_index = query.chunk_index;

    // Get session from manager
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Calculate expected chunks
    let expected_chunks = ((session.total_size + (session.chunk_size as u64 - 1))
        / (session.chunk_size as u64)) as usize;

    // Validate chunk index
    if chunk_index >= expected_chunks {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid chunk index {}. Expected 0-{} for {} total chunks",
                chunk_index,
                expected_chunks - 1,
                expected_chunks
            ),
        ));
    }

    // Check for duplicate chunk
    if session.received_chunks.contains_key(&chunk_index) {
        return Err((
            StatusCode::CONFLICT,
            format!("Chunk {} has already been uploaded", chunk_index),
        ));
    }

    // Validate chunk size (last chunk may be smaller)
    let is_last_chunk = chunk_index == expected_chunks - 1;
    let expected_chunk_size = if is_last_chunk {
        let remainder = session.total_size % (session.chunk_size as u64);
        if remainder == 0 {
            session.chunk_size
        } else {
            remainder as usize
        }
    } else {
        session.chunk_size
    };

    if body.len() > session.chunk_size {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Chunk size {} exceeds maximum chunk size {}",
                body.len(),
                session.chunk_size
            ),
        ));
    }

    // Write chunk to temp directory
    let chunk_path = session.temp_dir.join(format!("chunk_{:08}", chunk_index));
    let mut writer = ChunkWriter::new(&chunk_path).await.map_err(|e| {
        error!("Failed to create chunk writer: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create chunk file: {}", e),
        )
    })?;

    writer.write_chunk(&body).await.map_err(|e| {
        error!("Failed to write chunk data: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write chunk: {}", e),
        )
    })?;

    let chunk_hash = writer.finalize().await.map_err(|e| {
        error!("Failed to finalize chunk: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to finalize chunk: {}", e),
        )
    })?;

    // Update session with received chunk
    state
        .upload_session_manager
        .add_chunk(&session_id, chunk_index, chunk_hash.clone())
        .await
        .map_err(|e| {
            error!("Failed to update session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update session: {}", e),
            )
        })?;

    // Check if upload is complete
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(&session_id)
        .await
        .unwrap_or(false);

    // Get updated session for chunk count
    let updated_session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let chunks_received = updated_session.received_chunks.len();

    // Generate resume token if not complete
    let resume_token = if !is_complete {
        // Find next missing chunk
        let next_chunk = (0..expected_chunks)
            .find(|i| !updated_session.received_chunks.contains_key(i))
            .unwrap_or(expected_chunks);

        Some(
            serde_json::to_string(&ResumeToken {
                session_id: session_id.clone(),
                next_chunk,
                hash_state: chunk_hash.clone(),
            })
            .unwrap_or_default(),
        )
    } else {
        None
    };

    // Send progress event
    send_progress_event(
        state.dataset_progress_tx.as_ref(),
        DatasetProgressEvent {
            dataset_id: session_id.clone(),
            event_type: "upload".to_string(),
            current_file: Some(session.file_name.clone()),
            percentage_complete: (chunks_received as f32 / expected_chunks as f32) * 100.0,
            total_files: Some(expected_chunks as i32),
            files_processed: Some(chunks_received as i32),
            message: format!(
                "Uploaded chunk {}/{} for {}",
                chunk_index + 1,
                expected_chunks,
                session.file_name
            ),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload).map_err(|e| (e.0, e.1.error.clone()))?;

    // Get session
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Verify upload is complete
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(&session_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
            format!(
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
        ));
    }

    // Prepare output paths
    let storage_root = std::env::var("DATASET_STORAGE_PATH")
        .unwrap_or_else(|_| DEFAULT_DATASET_STORAGE.to_string());
    let dataset_id = Uuid::now_v7().to_string();
    let dataset_path = PathBuf::from(&storage_root).join(&dataset_id);
    let files_path = dataset_path.join("files");

    fs::create_dir_all(&files_path).await.map_err(|e| {
        error!("Failed to create dataset directory: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create dataset directory: {}", e),
        )
    })?;

    let output_path = files_path.join(&session.file_name);

    // Assemble chunks
    let assembler = ChunkAssembler::new(
        output_path.clone(),
        session.temp_dir.clone(),
        session.chunk_size,
        session.total_size,
        session.compression.clone(),
    );

    let (file_hash, total_bytes) = assembler.assemble().await.map_err(|e| {
        let error_msg = e.to_string();
        error!("Failed to assemble chunks: {}", error_msg);
        // Log audit failure (background task - acceptable as tokio::spawn per CLAUDE.md,
        // but using deterministic spawn for consistency)
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
            // Fallback: use tokio::spawn if deterministic executor not available
            // Audit logging is acceptable as background task per CLAUDE.md
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to assemble file: {}", error_msg),
        )
    })?;

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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create dataset record: {}", e),
            )
        })?;

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    state
        .db
        .update_dataset_extended_fields(
            &dataset_id,
            None,
            None,
            None,
            None,
            None,
            Some(&claims.tenant_id),
        )
        .await
        .map_err(|e| {
            error!("Failed to set dataset tenant: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set dataset tenant: {}", e),
            )
        })?;

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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to add file record: {}", e),
            )
        })?;

    // Clean up session
    let _ = state
        .upload_session_manager
        .remove_session(&session_id)
        .await;

    // Clean up temp directory
    let _ = fs::remove_dir_all(&session.temp_dir).await;

    // Log audit success
    let _ = log_success(
        &state.db,
        &claims,
        actions::DATASET_UPLOAD,
        resources::DATASET,
        Some(&dataset_id),
    )
    .await;

    // Send completion event
    send_progress_event(
        state.dataset_progress_tx.as_ref(),
        DatasetProgressEvent {
            dataset_id: dataset_id.clone(),
            event_type: "upload".to_string(),
            current_file: Some(session.file_name.clone()),
            percentage_complete: 100.0,
            total_files: Some(1),
            files_processed: Some(1),
            message: format!(
                "Completed chunked upload for {} ({} bytes)",
                session.file_name, total_bytes
            ),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView).map_err(|e| (e.0, e.1.error.clone()))?;

    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let expected_chunks = ((session.total_size + (session.chunk_size as u64 - 1))
        / (session.chunk_size as u64)) as usize;

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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload).map_err(|e| (e.0, e.1.error.clone()))?;

    // Get session to find temp dir
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Remove session from manager
    state
        .upload_session_manager
        .remove_session(&session_id)
        .await
        .map_err(|e| {
            error!("Failed to remove session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to remove session: {}", e),
            )
        })?;

    // Clean up temp directory
    if let Err(e) = fs::remove_dir_all(&session.temp_dir).await {
        warn!(
            "Failed to cleanup temp directory for session {}: {}",
            session_id, e
        );
    }

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
