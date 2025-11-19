use crate::handlers::chunked_upload::{
    ChunkAssembler, ChunkUploadResult, ChunkWriter, CompressionFormat, CompressionHandler,
    FileValidator, ResumeToken, UploadSession, UploadSessionManager, DEFAULT_CHUNK_SIZE,
    MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::state::{AppState, DatasetProgressEvent};
use crate::types::*;
use adapteros_api_types::training::*;
use adapteros_db::training_datasets::{
    DatasetFile, DatasetStatistics, TrainingDataset,
};
use adapteros_orchestrator::training_dataset_integration::TrainingDatasetManager;
use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{multipart::Multipart, Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive},
        Sse, IntoResponse,
    },
    Json,
};
use blake3::Hasher;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use sqlx;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};
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
#[derive(Deserialize)]
pub struct ListDatasetsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub format: Option<String>,
    pub validation_status: Option<String>,
}

/// Request to initiate a chunked upload
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Upload files to create a new dataset
#[utoipa::path(
    post,
    path = "/v1/datasets/upload",
    request_body(content = Multipart),
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
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dataset_id = Uuid::now_v7().to_string();
    let storage_root = std::env::var("DATASET_STORAGE_PATH")
        .unwrap_or_else(|_| DEFAULT_DATASET_STORAGE.to_string());

    // Create dataset directory structure
    let dataset_path = PathBuf::from(&storage_root).join(&dataset_id);
    let files_path = dataset_path.join("files");
    let temp_path = PathBuf::from(&storage_root).join("temp").join(&dataset_id);

    fs::create_dir_all(&files_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create dataset directory: {}", e)))?;

    fs::create_dir_all(&temp_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp directory: {}", e)))?;

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
        (StatusCode::BAD_REQUEST, format!("Failed to read multipart field: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                dataset_name = field.text().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("Failed to read name field: {}", e))
                })?;
            }
            "description" => {
                dataset_description = field.text().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("Failed to read description field: {}", e))
                })?;
            }
            "format" => {
                dataset_format = field.text().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("Failed to read format field: {}", e))
                })?;
            }
            "file" | "files" => {
                let file_name = field.file_name()
                    .ok_or((StatusCode::BAD_REQUEST, "File must have a name".to_string()))?
                    .to_string();

                let content_type = field.content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                // Stream file to temporary location
                let temp_file_path = temp_path.join(&file_name);
                let mut temp_file = fs::File::create(&temp_file_path).await.map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp file: {}", e))
                })?;

                let mut hasher = Hasher::new();
                let mut file_size = 0usize;
                let data = field.bytes().await.map_err(|e| {
                    (StatusCode::BAD_REQUEST, format!("Failed to read file data: {}", e))
                })?;

                file_size = data.len();

                // Check file size limits
                if file_size > MAX_FILE_SIZE {
                    fs::remove_dir_all(&temp_path).await.ok();
                    return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("File {} exceeds maximum size of {}MB", file_name, MAX_FILE_SIZE / 1024 / 1024)));
                }

                total_size += file_size;
                if total_size > MAX_TOTAL_SIZE {
                    fs::remove_dir_all(&temp_path).await.ok();
                    return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("Total upload size exceeds maximum of {}MB", MAX_TOTAL_SIZE / 1024 / 1024)));
                }

                // Write and hash file
                hasher.update(&data);
                temp_file.write_all(&data).await.map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write file: {}", e))
                })?;
                temp_file.flush().await.map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to flush file: {}", e))
                })?;

                let file_hash = hasher.finalize().to_hex().to_string();

                // Move file to permanent location
                let permanent_path = files_path.join(&file_name);
                fs::rename(&temp_file_path, &permanent_path).await.map_err(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to move file to permanent location: {}", e))
                })?;

                file_count += 1;

                // Send progress event for this file
                send_progress_event(
                    state.dataset_progress_tx.as_ref(),
                    DatasetProgressEvent {
                        dataset_id: dataset_id.clone(),
                        event_type: "upload".to_string(),
                        current_file: Some(file_name.clone()),
                        percentage_complete: if file_count > 0 { (file_count as f32 / 10.0).min(100.0) } else { 0.0 },
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

                info!("Uploaded file {} ({} bytes) for dataset {}", file_name, file_size, dataset_id);
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

    // Store in database
    let dataset_id_result = state.db.create_training_dataset(
        &dataset_name,
        if dataset_description.is_empty() { None } else { Some(&dataset_description) },
        &dataset_format,
        &dataset_hash,
        &dataset_path.to_string_lossy(),
        Some("system"),  // TODO: Get from auth context
    ).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create dataset record: {}", e))
    })?;

    // Add file records to database
    for file in &uploaded_files {
        state.db.add_dataset_file(
            &dataset_id,
            &file.file_name,
            &file.file_path,
            file.size_bytes,
            &file.hash_b3,
            file.mime_type.as_deref(),
        ).await.map_err(|e| {
            error!("Failed to add file record: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add file record: {}", e))
        })?;
    }

    info!("Created dataset {} with {} files, total size {} bytes",
          dataset_id, uploaded_files.len(), total_size);

    Ok(Json(UploadDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: dataset_id.clone(),
        name: dataset_name,
        description: if dataset_description.is_empty() { None } else { Some(dataset_description) },
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
        return Err((StatusCode::BAD_REQUEST, "File size must be greater than 0".to_string()));
    }

    if request.total_size > MAX_TOTAL_SIZE as u64 {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("File size exceeds maximum of {}MB", MAX_TOTAL_SIZE / 1024 / 1024)));
    }

    // Determine chunk size
    let chunk_size = request.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);
    let chunk_size = chunk_size.max(MIN_CHUNK_SIZE).min(MAX_CHUNK_SIZE);

    // Calculate expected chunks
    let expected_chunks = ((request.total_size + (chunk_size as u64 - 1)) / (chunk_size as u64)) as usize;

    // Detect compression
    let content_type = request.content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let compression = CompressionFormat::from_content_type(&content_type);

    // Create upload session
    let storage_root = std::env::var("DATASET_STORAGE_PATH")
        .unwrap_or_else(|_| DEFAULT_DATASET_STORAGE.to_string());
    let temp_base = PathBuf::from(&storage_root).join("chunked");

    fs::create_dir_all(&temp_base)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create temp directory: {}", e)))?;

    let manager = UploadSessionManager::new(1000); // Max 1000 concurrent sessions
    let session = manager.create_session(
        request.file_name.clone(),
        request.total_size,
        content_type.clone(),
        chunk_size,
        &temp_base,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!("Initiated chunked upload session {} for file {} ({} bytes, {} chunks)",
          session.session_id, request.file_name, request.total_size, expected_chunks);

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
    Query(params): Query<ListDatasetsQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(100);
    let _offset = params.offset.unwrap_or(0);

    let datasets = state.db.list_training_datasets(limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list datasets: {}", e)))?;

    let responses: Vec<DatasetResponse> = datasets.into_iter().map(|d| DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: d.id,
        name: d.name,
        description: d.description,
        file_count: d.file_count,
        total_size_bytes: d.total_size_bytes,
        format: d.format,
        hash: d.hash_b3,
        storage_path: d.storage_path,
        validation_status: d.validation_status,
        validation_errors: d.validation_errors,
        created_by: d.created_by.unwrap_or_else(|| "system".to_string()),
        created_at: d.created_at,
        updated_at: d.updated_at,
    }).collect();

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
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset(
    State(state): State<AppState>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (StatusCode::NOT_FOUND, "Dataset not found".to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e))
        })?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

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
        validation_status: dataset.validation_status,
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
    let _dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let files = state.db.get_dataset_files(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset files: {}", e)))?;

    let responses: Vec<DatasetFileResponse> = files.into_iter().map(|f| DatasetFileResponse {
        schema_version: "1.0".to_string(),
        file_id: f.id,
        file_name: f.file_name,
        file_path: f.file_path,
        size_bytes: f.size_bytes,
        hash: f.hash_b3,
        mime_type: f.mime_type,
        created_at: f.created_at,
    }).collect();

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
    let _dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let stats = state.db.get_dataset_statistics(&dataset_id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (StatusCode::NOT_FOUND, "Statistics not computed for this dataset".to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get statistics: {}", e))
        })?;

    Ok(Json(DatasetStatisticsResponse {
        schema_version: "1.0".to_string(),
        dataset_id: stats.dataset_id,
        num_examples: stats.num_examples,
        avg_input_length: stats.avg_input_length,
        avg_target_length: stats.avg_target_length,
        language_distribution: stats.language_distribution.and_then(|s| serde_json::from_str(&s).ok()),
        file_type_distribution: stats.file_type_distribution.and_then(|s| serde_json::from_str(&s).ok()),
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
    let dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

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
    let files = state.db.get_dataset_files(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset files: {}", e)))?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;
    let total_files = files.len() as f32;
    let mut processed_files = 0;

    // Validate each file
    for file in &files {
        // Check file exists
        if !tokio::fs::try_exists(&file.file_path).await.unwrap_or(false) {
            validation_errors.push(format!("File {} does not exist at path {}", file.file_name, file.file_path));
            is_valid = false;
            processed_files += 1;
            send_progress_event(
                state.dataset_progress_tx.as_ref(),
                DatasetProgressEvent {
                    dataset_id: dataset_id.clone(),
                    event_type: "validation".to_string(),
                    current_file: Some(file.file_name.clone()),
                    percentage_complete: if total_files > 0.0 { (processed_files as f32 / total_files) * 100.0 } else { 0.0 },
                    total_files: Some(files.len() as i32),
                    files_processed: Some(processed_files as i32),
                    message: format!("Validating {}", file.file_name),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            );
            continue;
        }

        // Verify file hash with streaming to avoid loading entire file
        match validate_file_hash_streaming(&file.file_path, &file.hash_b3).await {
            Ok(matches) => {
                if !matches {
                    validation_errors.push(format!(
                        "File {} hash mismatch",
                        file.file_name
                    ));
                    is_valid = false;
                }
            }
            Err(e) => {
                validation_errors.push(format!("Failed to validate file {}: {}", file.file_name, e));
                is_valid = false;
                continue;
            }
        }

        // Format-specific validation with quick checks
        if request.check_format.unwrap_or(true) {
            if let Err(e) = FileValidator::quick_validate(&file.file_path, &dataset.format, STREAM_BUFFER_SIZE).await {
                validation_errors.push(format!("File {} format validation failed: {}", file.file_name, e));
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
                percentage_complete: if total_files > 0.0 { (processed_files as f32 / total_files) * 100.0 } else { 0.0 },
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

    state.db.update_dataset_validation(
        &dataset_id,
        validation_status,
        validation_errors_str.as_deref(),
    ).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update validation status: {}", e))
    })?;

    Ok(Json(ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid,
        validation_status: validation_status.to_string(),
        errors: if validation_errors.is_empty() { None } else { Some(validation_errors) },
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
    let limit = params.get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);

    let dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    let files = state.db.get_dataset_files(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset files: {}", e)))?;

    let mut examples = Vec::new();
    let mut count = 0;

    // Stream read files for memory efficiency
    for file in files {
        if count >= limit {
            break;
        }

        match stream_preview_file(&file.file_path, &dataset.format, limit - count).await {
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
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn delete_dataset(
    State(state): State<AppState>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Get dataset to find storage path
    let dataset = state.db.get_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get dataset: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Dataset not found".to_string()))?;

    // Delete from database (cascades to files and statistics)
    state.db.delete_training_dataset(&dataset_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to delete dataset: {}", e)))?;

    // Delete files from filesystem
    if tokio::fs::try_exists(&dataset.storage_path).await.unwrap_or(false) {
        tokio::fs::remove_dir_all(&dataset.storage_path)
            .await
            .map_err(|e| {
                error!("Failed to delete dataset files at {}: {}", dataset.storage_path, e);
                // Don't fail the request if filesystem cleanup fails
                e
            }).ok();
    }

    info!("Deleted dataset {} and its files", dataset_id);

    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for progress stream
#[derive(Deserialize)]
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
async fn validate_file_hash_streaming(file_path: &std::path::Path, expected_hash: &str) -> Result<bool, String> {
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];

    loop {
        let n = file.read(&mut buffer)
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
            state.db.add_dataset_file(
                dataset_id,
                &file.file_name,
                &file.file_path,
                file.size_bytes,
                &file.hash_b3,
                file.mime_type.as_deref(),
            ).await.map_err(|e| format!("Failed to add file record: {}", e))?;
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
        let n = file.read(&mut buffer)
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