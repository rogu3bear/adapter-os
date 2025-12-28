//! Dataset upload handlers.

use super::chunked::{expected_chunks, prepare_session};
use super::fs_utils::ensure_dirs;
use super::hashing::{hash_dataset_manifest, hash_file, DatasetHashInput};
use super::helpers::{dataset_quota_limits, quota_error, MAX_FILE_SIZE, MAX_TOTAL_SIZE};
use super::paths::{resolve_dataset_root, DatasetPaths};
use super::progress::emit_progress;
use super::tenant::bind_dataset_to_tenant;
use super::types::{InitiateChunkedUploadRequest, InitiateChunkedUploadResponse};
use crate::auth::Claims;
use crate::citations::build_dataset_index;
use crate::error_helpers::{bad_request, db_error, forbidden, internal_error, payload_too_large};
use crate::handlers::chunked_upload::{
    CompressionFormat, DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::storage_usage::compute_tenant_storage_usage;
use crate::types::{ErrorResponse, UploadDatasetResponse};
use adapteros_db::training_datasets::DatasetFile;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey, StorageKind};
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Upload files to create a new dataset
#[utoipa::path(
    post,
    path = "/v1/datasets",
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

    struct PendingFile {
        file_name: String,
        mime_type: Option<String>,
        data: bytes::Bytes,
        file_hash: String,
    }

    let mut pending_files: Vec<PendingFile> = Vec::new();
    let mut total_size = 0usize;
    let mut dataset_name = String::new();
    let mut dataset_description = String::new();
    let mut dataset_format = "jsonl".to_string();
    let mut file_count = 0;
    let mut workspace_id: Option<String> = None;

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
            "workspace_id" => {
                let ws = field.text().await.map_err(|e| {
                    bad_request(format!("Failed to read workspace_id field: {}", e))
                })?;
                workspace_id = Some(ws);
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

                if file_size == 0 {
                    return Err(bad_request(format!(
                        "Unsupported file {}: empty uploads are not allowed",
                        file_name
                    )));
                }

                if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
                    return Err(bad_request(format!(
                        "Unsupported file name '{}': path separators are not allowed",
                        file_name
                    )));
                }

                // Check file size limits
                if file_size > MAX_FILE_SIZE {
                    return Err(payload_too_large(&format!(
                        "File {} exceeds maximum size of {}MB",
                        file_name,
                        MAX_FILE_SIZE / 1024 / 1024
                    )));
                }

                total_size += file_size;
                if total_size > MAX_TOTAL_SIZE {
                    return Err(payload_too_large(&format!(
                        "Total upload size exceeds maximum of {}MB",
                        MAX_TOTAL_SIZE / 1024 / 1024
                    )));
                }

                let predicted_usage = current_usage + total_size as u64;
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

                // Compute hash using B3Hash
                let file_hash = hash_file(&data);

                file_count += 1;

                pending_files.push(PendingFile {
                    file_name: file_name.clone(),
                    mime_type: Some(content_type),
                    data,
                    file_hash,
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

    if pending_files.is_empty() {
        return Err(bad_request("No files uploaded"));
    }

    if dataset_name.is_empty() {
        dataset_name = format!("Dataset {}", &dataset_id[0..8]);
    }

    let workspace_id_opt = workspace_id;
    // Ensure caller can access the workspace when provided
    if let Some(ref ws_id) = workspace_id_opt {
        let access = state
            .db
            .check_workspace_access(ws_id, &claims.sub, &claims.tenant_id)
            .await
            .map_err(|e| db_error(format!("Failed to check workspace access: {}", e)))?;
        if access.is_none() {
            return Err(forbidden(
                "Access denied: you are not a member of this workspace",
            ));
        }
    }

    let storage_workspace = workspace_id_opt
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    let resolved_workspace_id = Some(storage_workspace.clone());

    let dataset_path = paths.dataset_dir(&storage_workspace, &dataset_id);
    let temp_path = paths.dataset_temp_dir(&storage_workspace, &dataset_id);
    ensure_dirs([dataset_path.as_path(), temp_path.as_path()]).await?;

    // Deterministic dataset hash based on manifest
    let manifest: Vec<DatasetHashInput> = pending_files
        .iter()
        .map(|f| DatasetHashInput {
            file_name: f.file_name.clone(),
            size_bytes: f.data.len() as u64,
            file_hash_b3: f.file_hash.clone(),
        })
        .collect();
    let dataset_hash = hash_dataset_manifest(&manifest);

    // Deduplicate by dataset hash within workspace (same tenant or admin only)
    if let Some(ref ws_id) = workspace_id_opt {
        if let Some(existing) = state
            .db
            .get_dataset_by_hash_and_workspace(&dataset_hash, ws_id)
            .await
            .map_err(|e| db_error(format!("Failed to check existing datasets: {}", e)))?
        {
            if existing
                .tenant_id
                .as_deref()
                .map(|t| t == claims.tenant_id)
                .unwrap_or(false)
                || claims.role == "admin"
            {
                info!(
                    dataset_id = %existing.id,
                    workspace_id = %ws_id,
                    "Reusing existing dataset with identical hash"
                );
                return Ok(Json(UploadDatasetResponse {
                    schema_version: "1.0".to_string(),
                    dataset_id: existing.id,
                    name: existing.name,
                    description: existing.description,
                    file_count: existing.file_count,
                    total_size_bytes: existing.total_size_bytes,
                    format: existing.format,
                    hash: existing.hash_b3.clone(),
                    dataset_hash_b3: Some(existing.dataset_hash_b3.clone()),
                    status: Some(existing.status.clone()),
                    workspace_id: existing
                        .workspace_id
                        .clone()
                        .or_else(|| resolved_workspace_id.clone()),
                    reused: true,
                    created_at: existing.created_at,
                }));
            }
        }
    }

    // Persist files now that deduplication is done
    let total_files = file_count.max(1);
    let mut uploaded_files = Vec::new();
    for (idx, pending) in pending_files.into_iter().enumerate() {
        let key = StorageKey {
            tenant_id: Some(claims.tenant_id.clone()),
            object_id: format!("{}/{}", storage_workspace, dataset_id),
            version_id: None,
            file_name: pending.file_name.clone(),
            kind: StorageKind::DatasetFile,
        };
        let location = storage
            .store_bytes(&key, &pending.data)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.to_ascii_lowercase().contains("insufficient disk space") {
                    (
                        StatusCode::INSUFFICIENT_STORAGE,
                        Json(ErrorResponse::new(msg.clone()).with_code("INSUFFICIENT_STORAGE")),
                    )
                } else {
                    internal_error(format!("Failed to store dataset file: {}", msg))
                }
            })?;

        emit_progress(
            state.dataset_progress_tx.as_ref(),
            &dataset_id,
            "upload",
            Some(pending.file_name.clone()),
            ((idx + 1) as f32 / total_files as f32) * 100.0,
            format!(
                "Stored {} ({} bytes)",
                pending.file_name,
                pending.data.len()
            ),
            None,
            Some(total_files),
        );

        uploaded_files.push(DatasetFile {
            id: Uuid::now_v7().to_string(),
            dataset_id: dataset_id.clone(),
            file_name: pending.file_name.clone(),
            file_path: location.path.to_string_lossy().to_string(),
            size_bytes: pending.data.len() as i64,
            hash_b3: pending.file_hash.clone(),
            mime_type: pending.mime_type,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

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
            resolved_workspace_id.as_deref(),
            Some("ready"),
            Some(&dataset_hash),
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
                tracing::error!("Failed to add file record: {}", e);
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
        hash: dataset_hash.clone(),
        dataset_hash_b3: Some(dataset_hash),
        status: Some("ready".to_string()),
        workspace_id: resolved_workspace_id,
        reused: false,
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
    let chunk_size = chunk_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE);

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
