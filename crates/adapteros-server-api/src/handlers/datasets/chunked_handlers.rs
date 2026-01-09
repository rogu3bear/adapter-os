//! Chunked upload handlers for large dataset files.

use super::chunked::{assemble_chunks, expected_chunk_size, expected_chunks, persist_chunk};
use super::fs_utils::{clean_temp, ensure_dirs};
use super::hashing::{hash_dataset_manifest, DatasetHashInput};
use super::helpers::{dataset_quota_limits, quota_error, STREAM_BUFFER_SIZE};
use super::paths::{resolve_dataset_root, DatasetPaths};
use super::progress::emit_progress;
use super::tenant::bind_dataset_to_tenant;
use super::types::{
    CompleteChunkedUploadRequest, CompleteChunkedUploadResponse, ListUploadSessionsResponse,
    RetryChunkQuery, RetryChunkResponse, UploadChunkQuery, UploadChunkResponse,
    UploadSessionStatusResponse, UploadSessionSummary,
};
use super::upload::build_training_rows_from_jsonl_bytes;
use super::validation::{
    CompositeValidator, FileExistsRule, FileExtensionRule, FileSizeRule, ValidationConfig,
};
use crate::api_error::ApiError;
use crate::audit_helper::{actions, log_failure_or_warn, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::citations::build_dataset_index;
use crate::error_helpers::{bad_request, db_error, forbidden, internal_error, not_found};
use crate::handlers::chunked_upload::{ChunkWriter, FileValidator, UploadSessionManager};
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::storage_usage::compute_tenant_storage_usage;
use crate::types::ErrorResponse;
use adapteros_db::training_datasets::{build_codebase_rows_from_jsonl_bytes, CreateDatasetParams};
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde_json::{Map, Value};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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

    let (session, total_chunks, chunk_hash, chunks_received, is_complete, resume_token) =
        persist_chunk(&state, &session_id, chunk_index, &body).await?;

    // Send progress event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &session_id,
        "upload",
        Some(session.file_name.clone()),
        (chunks_received as f32 / total_chunks as f32) * 100.0,
        format!(
            "Uploaded chunk {}/{} for {}",
            chunk_index + 1,
            total_chunks,
            session.file_name
        ),
        Some(total_chunks as i32),
        Some(chunks_received as i32),
    );

    info!(
        "Uploaded chunk {}/{} for session {} ({} bytes, hash: {})",
        chunk_index + 1,
        total_chunks,
        session_id,
        body.len(),
        chunk_hash
    );

    Ok(Json(UploadChunkResponse {
        session_id,
        chunk_index,
        chunk_hash,
        chunks_received,
        expected_chunks: total_chunks,
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
    let paths = DatasetPaths::new(dataset_root.clone());
    let allowed_roots = [paths.root().to_path_buf()];

    // Get session
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;
    if UploadSessionManager::is_session_expired(&session) {
        return Err(not_found("Upload session"));
    }

    // Validate workspace_id consistency between session and request for tenant isolation
    if let Some(ref session_workspace_id) = session.workspace_id {
        if let Some(ref request_workspace_id) = request.workspace_id {
            if session_workspace_id != request_workspace_id {
                return Err(ApiError::bad_request(
                    "Workspace ID mismatch: session was created for a different workspace",
                )
                .into());
            }
        }
    }

    // Determine workspace_id: prefer request, fall back to session, fall back to tenant
    let workspace_id = request
        .workspace_id
        .clone()
        .or_else(|| session.workspace_id.clone());

    // Validate workspace access if workspace_id is provided
    if let Some(ref ws_id) = workspace_id {
        let access = state
            .db
            .check_workspace_access_with_admin(
                ws_id,
                &claims.sub,
                &claims.tenant_id,
                &claims.admin_tenants,
            )
            .await
            .map_err(|e| db_error(format!("Failed to check workspace access: {}", e)))?;
        if access.is_none() {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    ErrorResponse::new("Access denied: you are not a member of this workspace")
                        .with_code("WORKSPACE_ACCESS_DENIED"),
                ),
            ));
        }
    }

    // Use workspace_id for storage, falling back to tenant_id
    let storage_workspace = workspace_id
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    let resolved_workspace_id = Some(storage_workspace.clone());

    // Verify upload is complete
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(&session_id)
        .await
        .map_err(internal_error)?;

    if !is_complete {
        let total_chunks = session.total_size.div_ceil(session.chunk_size as u64) as usize;
        let received = session.received_chunks.len();

        // Find missing chunks for error message
        let missing: Vec<usize> = (0..total_chunks)
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
                    total_chunks,
                    missing,
                    if missing.len() < total_chunks - received {
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
    let storage = FsByteStorage::new(dataset_root, adapters_root.into());

    let dataset_id = Uuid::now_v7().to_string();
    let storage_key = StorageKey::dataset_file(
        Some(storage_workspace.clone()),
        &dataset_id,
        None,
        &session.file_name,
    );
    let output_path = storage.path_for(&storage_key).map_err(|e| {
        internal_error(format!(
            "Failed to resolve storage path for dataset {}: {}",
            dataset_id, e
        ))
    })?;
    let dataset_path = output_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| paths.dataset_dir(&storage_workspace, &dataset_id));
    ensure_dirs([dataset_path.as_path()]).await?;
    canonicalize_strict_in_allowed_roots(&dataset_path, &allowed_roots).map_err(|e| {
        forbidden(&format!("Dataset storage path rejected: {}", e))
    })?;

    // Assemble chunks
    let (file_hash, total_bytes) = match assemble_chunks(&session, &output_path).await {
        Ok(res) => res,
        Err((status, Json(payload))) => {
            let error_msg = payload.error.clone();
            error!("Failed to assemble chunks: {}", error_msg);
            let db = state.db.clone();
            let claims_clone = claims.clone();
            let error_msg_clone = error_msg.clone();
            if let Err(_e) =
                spawn_deterministic("audit-log:dataset-upload-failure".to_string(), async move {
                    log_failure_or_warn(
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
                tokio::spawn(async move {
                    log_failure_or_warn(
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
    if let Err(e) = canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots) {
        return Err(forbidden(&format!(
            "Dataset output path escapes dataset root: {}",
            e
        )));
    }
    if total_bytes != session.total_size {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!(
                    "Assembled size {} does not match expected size {}",
                    total_bytes, session.total_size
                ),
                code: "SIZE_MISMATCH".to_string(),
                failure_code: None,
                details: None,
            }),
        ));
    }

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

    let content_validation = if matches!(request.format.as_str(), "parquet" | "custom") {
        Ok(())
    } else {
        FileValidator::quick_validate(&output_path, &request.format, STREAM_BUFFER_SIZE).await
    };
    if let Err(ref e) = content_validation {
        warn!("File format validation warning: {}", e);
    }

    let mut validation_config = match request.format.as_str() {
        "jsonl" => ValidationConfig::for_training_jsonl(),
        "json" => ValidationConfig::for_json(),
        _ => ValidationConfig::default(),
    };
    if !validation_config
        .allowed_extensions
        .contains(&request.format)
    {
        validation_config
            .allowed_extensions
            .insert(request.format.clone());
    }
    if request.format == "custom" {
        if let Some(ext) = std::path::Path::new(&session.file_name)
            .extension()
            .and_then(|e| e.to_str())
        {
            validation_config
                .allowed_extensions
                .insert(ext.to_ascii_lowercase());
        }
    }
    let validator = if matches!(request.format.as_str(), "parquet" | "custom") {
        let mut validator = CompositeValidator::new(validation_config);
        validator.add_rule(Box::new(FileExistsRule));
        validator.add_rule(Box::new(FileSizeRule));
        validator.add_rule(Box::new(FileExtensionRule));
        validator
    } else {
        CompositeValidator::quick_validator(validation_config)
    };
    let validation_result = validator.validate_files(&[output_path.as_path()]).await;
    let mut validation_messages: Vec<String> = Vec::new();
    if let Err(e) = content_validation {
        validation_messages.push(e.to_string());
    }
    validation_messages.extend(validation_result.errors.iter().map(|err| err.to_string()));
    let validation_status = if validation_result.is_valid && validation_messages.is_empty() {
        "valid"
    } else {
        "invalid"
    };
    let validation_errors = if validation_messages.is_empty() {
        None
    } else {
        Some(validation_messages.join("; "))
    };

    // Determine dataset name
    let dataset_name = request.name.unwrap_or_else(|| session.file_name.clone());

    let dataset_hash = hash_dataset_manifest(&[DatasetHashInput {
        file_name: session.file_name.clone(),
        size_bytes: total_bytes,
        file_hash_b3: file_hash.clone(),
    }]);

    let mut jsonl_bytes: Option<Vec<u8>> = None;
    let mut is_codebase_jsonl = false;
    let mut codebase_context = CodebaseUploadContext::default();
    if request.format == "jsonl" {
        match tokio::fs::read(&output_path).await {
            Ok(bytes) => {
                is_codebase_jsonl = detect_codebase_jsonl(&bytes);
                if is_codebase_jsonl {
                    codebase_context = extract_codebase_context(&bytes);
                }
                jsonl_bytes = Some(bytes);
            }
            Err(e) => {
                warn!(
                    error = %e,
                    dataset_id = %dataset_id,
                    "Failed to read uploaded dataset for JSONL inspection"
                );
            }
        }
    }

    let dataset_type = if is_codebase_jsonl {
        "codebase"
    } else {
        "training"
    };
    let dataset_category = if is_codebase_jsonl {
        "codebase"
    } else {
        "upload"
    };

    let mut dataset_builder = CreateDatasetParams::builder()
        .id(&dataset_id)
        .name(&dataset_name)
        .format(&request.format)
        .hash_b3(&dataset_hash)
        .dataset_hash_b3(&dataset_hash)
        .storage_path(dataset_path.to_string_lossy().to_string())
        .status("ready")
        .created_by(&claims.sub)
        .tenant_id(&claims.tenant_id)
        .workspace_id(&storage_workspace)
        .dataset_type(dataset_type)
        .collection_method("upload")
        .category(dataset_category);

    if let Some(ref desc) = request.description {
        if !desc.trim().is_empty() {
            dataset_builder = dataset_builder.description(desc);
        }
    }

    if is_codebase_jsonl {
        if let Some(ref repo_slug) = codebase_context.repo_slug {
            dataset_builder = dataset_builder.repo_slug(repo_slug);
        }
        if let Some(ref commit) = codebase_context.commit_sha {
            dataset_builder = dataset_builder.commit_sha(commit);
        }
        if let Some(ref branch) = codebase_context.branch {
            dataset_builder = dataset_builder.branch(branch);
        }
        if let Some(ref location) = codebase_context.source_location {
            dataset_builder = dataset_builder.source_location(location);
        }
    }

    let dataset_params = dataset_builder.build().map_err(|e| {
        error!("Failed to build dataset params: {}", e);
        bad_request(format!("Invalid dataset parameters: {}", e))
    })?;

    state
        .db
        .create_training_dataset_from_params(&dataset_params)
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

    state
        .db
        .update_dataset_validation(&dataset_id, validation_status, validation_errors.as_deref())
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    let version_id = if is_codebase_jsonl {
        if let Some(bytes) = jsonl_bytes.as_deref() {
            let (rows, parse_errors, dropped) =
                build_codebase_rows_from_jsonl_bytes(&session.file_name, bytes);
            if !rows.is_empty() {
                match state
                    .db
                    .insert_codebase_dataset_rows_for_run(
                        &dataset_id,
                        None,
                        Some(&session_id),
                        codebase_context.repo_name.as_deref(),
                        codebase_context.repo_slug.as_deref(),
                        codebase_context.repo_identifier.as_deref(),
                        codebase_context.project_name.as_deref(),
                        codebase_context.commit_sha.as_deref(),
                        &rows,
                        Some(&claims.tenant_id),
                    )
                    .await
                {
                    Ok(inserted) => {
                        info!(
                            dataset_id = %dataset_id,
                            inserted,
                            parse_errors,
                            dropped,
                            "Inserted codebase dataset rows from chunked upload"
                        );
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            dataset_id = %dataset_id,
                            "Failed to insert codebase dataset rows (non-blocking)"
                        );
                    }
                }
            } else if parse_errors > 0 || dropped > 0 {
                warn!(
                    dataset_id = %dataset_id,
                    parse_errors,
                    dropped,
                    "No codebase dataset rows created from chunked upload"
                );
            }
        } else {
            warn!(
                dataset_id = %dataset_id,
                "No JSONL bytes available for codebase row ingestion"
            );
        }

        state
            .db
            .record_dataset_version_for_run(&dataset_id, &session_id, None, None, Some(&claims.sub))
            .await
            .map_err(|e| db_error(format!("Failed to record dataset version: {}", e)))?
    } else {
        state
            .db
            .ensure_dataset_version_exists(&dataset_id)
            .await
            .map_err(|e| db_error(format!("Failed to ensure dataset version: {}", e)))?
    };

    if let Err(e) = state
        .db
        .update_dataset_version_structural_validation(
            &version_id,
            validation_status,
            validation_errors.as_deref(),
        )
        .await
    {
        warn!(
            error = %e,
            dataset_id = %dataset_id,
            dataset_version_id = %version_id,
            "Failed to update dataset version validation status"
        );
    }

    if request.format == "jsonl" {
        if is_codebase_jsonl {
            debug!(
                dataset_id = %dataset_id,
                "Skipping training row creation for codebase JSONL upload"
            );
        } else if let Some(bytes) = jsonl_bytes.as_deref() {
            let (rows, parse_errors, dropped) = build_training_rows_from_jsonl_bytes(
                &session.file_name,
                bytes,
                &dataset_id,
                &version_id,
                &claims.tenant_id,
                Some(&claims.sub),
            );
            if !rows.is_empty() {
                match state.db.bulk_insert_training_dataset_rows(&rows).await {
                    Ok(inserted) => {
                        info!(
                            dataset_id = %dataset_id,
                            dataset_version_id = %version_id,
                            inserted,
                            parse_errors,
                            dropped,
                            "Inserted training dataset rows from chunked upload"
                        );
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            dataset_id = %dataset_id,
                            dataset_version_id = %version_id,
                            "Failed to insert training dataset rows (non-blocking)"
                        );
                    }
                }
            } else if parse_errors > 0 || dropped > 0 {
                warn!(
                    dataset_id = %dataset_id,
                    dataset_version_id = %version_id,
                    parse_errors,
                    dropped,
                    "No training dataset rows created from chunked upload"
                );
            }
        } else {
            warn!(
                dataset_id = %dataset_id,
                "Failed to read uploaded dataset for row creation (non-blocking)"
            );
        }
    } else {
        debug!(
            dataset_id = %dataset_id,
            format = %request.format,
            "Skipping dataset row creation for non-jsonl chunked upload"
        );
    }

    if let Err(e) = state
        .db
        .sync_dataset_to_kv(&claims.tenant_id, &dataset_id)
        .await
    {
        warn!(
            error = %e,
            dataset_id = %dataset_id,
            "Failed to sync dataset to KV store after chunked upload"
        );
    }

    // Clean up session
    let _ = state
        .upload_session_manager
        .remove_session(&session_id)
        .await;

    // Clean up temp directory
    clean_temp(&session.temp_dir).await;

    // Log audit success
    log_success_or_warn(
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
        "Completed chunked upload for session {}. Created dataset {} with {} bytes (workspace: {:?})",
        session_id, dataset_id, total_bytes, resolved_workspace_id
    );

    Ok(Json(CompleteChunkedUploadResponse {
        dataset_id,
        dataset_version_id: Some(version_id),
        name: dataset_name,
        hash: dataset_hash.clone(),
        total_size_bytes: total_bytes as i64,
        storage_path: dataset_path.to_string_lossy().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        workspace_id: resolved_workspace_id,
    }))
}

#[derive(Debug, Default)]
struct CodebaseUploadContext {
    repo_name: Option<String>,
    repo_slug: Option<String>,
    repo_identifier: Option<String>,
    project_name: Option<String>,
    commit_sha: Option<String>,
    branch: Option<String>,
    source_location: Option<String>,
}

fn detect_codebase_jsonl(data: &[u8]) -> bool {
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => return false,
    };

    for line in text.lines().take(200) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let Some(object) = value.as_object() else {
            continue;
        };

        let metadata = object
            .get("metadata")
            .or_else(|| object.get("meta"))
            .and_then(|val| val.as_object());

        if has_codebase_signal(object, metadata) {
            return true;
        }
    }

    false
}

fn extract_codebase_context(data: &[u8]) -> CodebaseUploadContext {
    let mut context = CodebaseUploadContext::default();
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(_) => return context,
    };

    for line in text.lines().take(500) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let Some(object) = value.as_object() else {
            continue;
        };

        let metadata = object
            .get("metadata")
            .or_else(|| object.get("meta"))
            .and_then(|val| val.as_object());

        context.repo_slug = context
            .repo_slug
            .or_else(|| extract_json_string(object, metadata, &["repo_slug", "scope_repo_slug"]));
        context.repo_identifier = context.repo_identifier.or_else(|| {
            extract_json_string(
                object,
                metadata,
                &[
                    "repo_identifier",
                    "scope_repo_id",
                    "repo_id",
                    "codebase_scope",
                ],
            )
        });
        context.repo_name = context
            .repo_name
            .or_else(|| extract_json_string(object, metadata, &["repo_name"]));
        context.project_name = context
            .project_name
            .or_else(|| extract_json_string(object, metadata, &["project_name", "project"]));
        context.commit_sha = context.commit_sha.or_else(|| {
            extract_json_string(
                object,
                metadata,
                &["repo_commit", "commit_sha", "commit_hash", "commit"],
            )
        });
        context.branch = context.branch.or_else(|| {
            extract_json_string(object, metadata, &["repo_branch", "branch", "git_branch"])
        });
        context.source_location = context.source_location.or_else(|| {
            extract_json_string(
                object,
                metadata,
                &[
                    "repo_root_path",
                    "repo_path",
                    "scan_root_path",
                    "repo_remote",
                    "remote_url",
                    "repo_url",
                ],
            )
        });
    }

    if context.source_location.is_none() {
        context.source_location = context.repo_identifier.clone();
    }

    context
}

fn has_codebase_signal(object: &Map<String, Value>, metadata: Option<&Map<String, Value>>) -> bool {
    const SIGNAL_KEYS: &[&str] = &[
        "symbol_kind",
        "qualified_name",
        "file_path",
        "start_line",
        "end_line",
        "repo_slug",
        "repo_identifier",
        "scope_repo_id",
        "repo_id",
        "repo_commit",
        "repo_name",
        "repo_path",
        "codebase_scope",
    ];

    SIGNAL_KEYS.iter().any(|key| {
        object.contains_key(*key)
            || metadata
                .map(|meta| meta.contains_key(*key))
                .unwrap_or(false)
    })
}

fn extract_json_string(
    object: &Map<String, Value>,
    metadata: Option<&Map<String, Value>>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key).and_then(|val| val.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(meta) = metadata {
            if let Some(value) = meta.get(*key).and_then(|val| val.as_str()) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    None
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

    let total_chunks = expected_chunks(session.total_size, session.chunk_size);

    let chunks_received = session.received_chunks.len();
    let received_chunk_indices: Vec<usize> = session.received_chunks.keys().cloned().collect();
    let is_complete = chunks_received == total_chunks;

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
        expected_chunks: total_chunks,
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

/// Retry uploading a specific chunk (for failed or corrupted chunks)
///
/// This endpoint allows re-uploading a chunk that may have failed or been corrupted.
/// Unlike the regular upload_chunk endpoint, this allows overwriting an existing chunk.
/// Optionally, the client can provide an expected hash for validation.
///
/// ## Use Cases
/// - Network failure during chunk upload
/// - Chunk corruption detected during validation
/// - Resume after partial failure
///
/// ## Error Cases
/// - 404: Session not found or expired
/// - 400: Invalid chunk index or hash mismatch
/// - 413: Chunk size exceeds the session's configured chunk size
/// - 500: Failed to write chunk to disk
#[utoipa::path(
    put,
    path = "/v1/datasets/chunked-upload/{session_id}/chunk",
    params(
        ("session_id" = String, Path, description = "Upload session ID"),
        RetryChunkQuery,
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Chunk retried successfully", body = RetryChunkResponse),
        (status = 400, description = "Invalid chunk index, data, or hash mismatch"),
        (status = 404, description = "Session not found or expired"),
        (status = 413, description = "Chunk too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn retry_chunk(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Query(query): Query<RetryChunkQuery>,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    let chunk_index = query.chunk_index;

    // Get session to validate chunk index
    let session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;

    let total_chunks = expected_chunks(session.total_size, session.chunk_size);
    if chunk_index >= total_chunks {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!(
                    "Invalid chunk index {}. Expected 0-{} for {} total chunks",
                    chunk_index,
                    total_chunks - 1,
                    total_chunks
                ),
                code: "INVALID_CHUNK_INDEX".to_string(),
                failure_code: None,
                details: None,
            }),
        ));
    }

    // Validate chunk size
    let expected_size = expected_chunk_size(session.total_size, session.chunk_size, chunk_index);
    if body.len() != expected_size {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!(
                    "Invalid chunk size {}. Expected {} bytes for chunk {}",
                    body.len(),
                    expected_size,
                    chunk_index
                ),
                code: "INVALID_CHUNK_SIZE".to_string(),
                failure_code: None,
                details: None,
            }),
        ));
    }

    // Check if this is actually a retry (chunk already exists)
    let was_retry = session.received_chunks.contains_key(&chunk_index);

    // Write chunk to disk
    let chunk_path = session.temp_dir.join(format!("chunk_{:08}", chunk_index));
    let mut writer = ChunkWriter::new(&chunk_path).await.map_err(|e| {
        error!("Failed to create chunk writer: {}", e);
        internal_error(format!("Failed to create chunk file: {}", e))
    })?;

    writer.write_chunk(&body).await.map_err(|e| {
        error!("Failed to write chunk data: {}", e);
        internal_error(format!("Failed to write chunk: {}", e))
    })?;

    let chunk_hash = writer.finalize().await.map_err(|e| {
        error!("Failed to finalize chunk: {}", e);
        internal_error(format!("Failed to finalize chunk: {}", e))
    })?;

    // Validate expected hash if provided
    if let Some(ref exp_hash) = query.expected_hash {
        if &chunk_hash != exp_hash {
            // Remove the corrupted chunk file
            let _ = tokio::fs::remove_file(&chunk_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                    error: format!(
                        "Chunk hash mismatch. Expected {}, got {}",
                        exp_hash, chunk_hash
                    ),
                    code: "HASH_MISMATCH".to_string(),
                    failure_code: None,
                    details: None,
                }),
            ));
        }
    }

    // Update session with retried chunk
    let previous_hash = state
        .upload_session_manager
        .retry_chunk(&session_id, chunk_index, chunk_hash.clone())
        .await
        .map_err(|e| {
            error!("Failed to update session: {}", e);
            internal_error(format!("Failed to update session: {}", e))
        })?;

    // Get updated session state
    let updated_session = state
        .upload_session_manager
        .get_session(&session_id)
        .await
        .map_err(internal_error)?;

    let chunks_received = updated_session.received_chunks.len();
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(&session_id)
        .await
        .unwrap_or(false);

    // Send progress event
    emit_progress(
        state.dataset_progress_tx.as_ref(),
        &session_id,
        "upload",
        Some(session.file_name.clone()),
        (chunks_received as f32 / total_chunks as f32) * 100.0,
        format!(
            "Retried chunk {}/{} for {}",
            chunk_index + 1,
            total_chunks,
            session.file_name
        ),
        Some(total_chunks as i32),
        Some(chunks_received as i32),
    );

    info!(
        "Retried chunk {}/{} for session {} ({} bytes, hash: {}, was_retry: {})",
        chunk_index + 1,
        total_chunks,
        session_id,
        body.len(),
        chunk_hash,
        was_retry
    );

    Ok(Json(RetryChunkResponse {
        session_id,
        chunk_index,
        chunk_hash,
        previous_hash,
        chunks_received,
        expected_chunks: total_chunks,
        is_complete,
        was_retry,
    }))
}

/// List all active chunked upload sessions
///
/// Returns a summary of all active upload sessions. This is useful for
/// monitoring upload progress and identifying stale/abandoned sessions.
/// Requires DatasetView permission.
///
/// ## Response
/// - List of session summaries with progress information
/// - Total count and maximum allowed sessions
/// - Expired session indicators
#[utoipa::path(
    get,
    path = "/v1/datasets/chunked-upload/sessions",
    responses(
        (status = 200, description = "List of active upload sessions", body = ListUploadSessionsResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_upload_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission - require at least DatasetView
    require_permission(&claims, Permission::DatasetView)?;

    let sessions = state.upload_session_manager.list_sessions().await;
    let max_sessions = state.upload_session_manager.max_sessions();

    let summaries: Vec<UploadSessionSummary> = sessions
        .into_iter()
        .map(|session| {
            let total_chunks = expected_chunks(session.total_size, session.chunk_size);
            let chunks_received = session.received_chunks.len();
            let progress_percent = if total_chunks > 0 {
                (chunks_received as f32 / total_chunks as f32) * 100.0
            } else {
                0.0
            };

            let age_seconds = UploadSessionManager::get_session_age(&session);
            let is_expired = UploadSessionManager::is_session_expired(&session);

            let created_at = session
                .created_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .ok()
                .flatten()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            UploadSessionSummary {
                session_id: session.session_id,
                file_name: session.file_name,
                total_size: session.total_size,
                chunks_received,
                expected_chunks: total_chunks,
                progress_percent,
                created_at,
                age_seconds,
                is_expired,
            }
        })
        .collect();

    let total_count = summaries.len();

    info!("Listed {} active upload sessions", total_count);

    Ok(Json(ListUploadSessionsResponse {
        sessions: summaries,
        total_count,
        max_sessions,
    }))
}

/// Trigger cleanup of expired upload sessions
///
/// Manually triggers the cleanup of expired upload sessions and their
/// temporary files. This is normally done automatically by a background
/// task every hour, but can be triggered manually for immediate cleanup.
///
/// Requires admin permission.
#[utoipa::path(
    post,
    path = "/v1/datasets/chunked-upload/cleanup",
    responses(
        (status = 200, description = "Cleanup completed", body = serde_json::Value),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn cleanup_expired_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Require admin permission for manual cleanup
    require_permission(&claims, Permission::DatasetDelete)?;

    let cleaned_count = state
        .upload_session_manager
        .cleanup_expired()
        .await
        .map_err(|e| {
            error!("Failed to cleanup expired sessions: {}", e);
            internal_error(format!("Failed to cleanup expired sessions: {}", e))
        })?;

    info!(
        "Manual cleanup triggered by {}: removed {} expired sessions",
        claims.sub, cleaned_count
    );

    // Audit log
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::DATASET_CHUNKED_UPLOAD_CLEANUP,
        crate::audit_helper::resources::DATASET,
        None,
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(serde_json::json!({
        "cleaned_count": cleaned_count,
        "message": format!("Cleaned up {} expired upload sessions", cleaned_count)
    })))
}
