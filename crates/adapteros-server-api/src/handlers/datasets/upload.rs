//! Dataset upload handlers.

use super::chunked::{expected_chunks, prepare_session_with_workspace};
use super::fs_utils::ensure_dirs;
use super::hashing::{hash_dataset_manifest, hash_file, normalize_filename, DatasetHashInput};
use super::helpers::{
    build_validation_error_payload, dataset_quota_limits, path_policy_error, quota_error,
    MAX_FILE_COUNT, MAX_FILE_SIZE, MAX_TOTAL_SIZE,
};
use super::paths::{resolve_dataset_root, DatasetPaths};
use super::progress::emit_progress;
use super::tenant::bind_dataset_to_tenant;
use super::types::{InitiateChunkedUploadRequest, InitiateChunkedUploadResponse};
use super::upload_sessions::{
    build_session_key, fetch_session_by_key, insert_session, validate_idempotency_key,
    UploadSessionRecord, UPLOAD_SESSION_DB_SCHEMA_VERSION,
};
use super::validation::{
    validation_error_response, CompositeValidator, FileExistsRule, FileExtensionRule, FileSizeRule,
    ValidationConfig,
};
use crate::auth::Claims;
use crate::citations::build_dataset_index;
use crate::error_helpers::{bad_request, db_error, forbidden, internal_error, payload_too_large};
use crate::handlers::chunked_upload::{
    CompressionFormat, DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE, MIN_CHUNK_SIZE,
};
use crate::middleware::request_id::RequestId;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::storage_usage::{compute_tenant_storage_usage, compute_workspace_storage_usage};
use crate::types::{ErrorResponse, UploadDatasetResponse};
use adapteros_db::training_datasets::{
    validate_format, validate_hash_b3, CreateDatasetParams, CreateTrainingDatasetRowParams,
    DatasetFile,
};
use adapteros_secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use adapteros_storage::{ByteStorage, DatasetCategory, FsByteStorage, StorageKey};
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use bytes::Bytes;
use serde_json::{Map, Value};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use uuid::Uuid;

const METRIC_CHUNKED_SESSIONS_CREATED: &str = "chunked_upload_sessions_created";
const METRIC_CHUNKED_SESSIONS_REUSED: &str = "chunked_upload_reused";
const METRIC_CHUNKED_CONFLICT_HASH_MISMATCH: &str = "chunked_upload_conflict_hash_mismatch";

struct PendingFile {
    file_name: String,
    mime_type: Option<String>,
    data: Bytes,
    file_hash: String,
}

pub(crate) fn build_training_rows_from_jsonl_bytes(
    file_name: &str,
    data: &[u8],
    dataset_id: &str,
    dataset_version_id: &str,
    tenant_id: &str,
    created_by: Option<&str>,
) -> (Vec<CreateTrainingDatasetRowParams>, usize, usize) {
    adapteros_db::training_datasets::build_training_rows_from_jsonl_bytes(
        file_name,
        data,
        dataset_id,
        dataset_version_id,
        Some(tenant_id),
        created_by,
        Some("upload"),
    )
}

fn build_training_rows_from_jsonl(
    pending_files: &[PendingFile],
    dataset_id: &str,
    dataset_version_id: &str,
    tenant_id: &str,
    created_by: Option<&str>,
) -> (Vec<CreateTrainingDatasetRowParams>, usize, usize) {
    let mut rows = Vec::new();
    let mut parse_errors = 0usize;
    let mut dropped = 0usize;

    let mut ordered_files: Vec<(String, &PendingFile)> = pending_files
        .iter()
        .map(|file| (normalize_filename(&file.file_name), file))
        .collect();
    ordered_files.sort_by(|(a_norm, a), (b_norm, b)| {
        a_norm
            .cmp(b_norm)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });

    for (_, file) in ordered_files {
        let (mut file_rows, file_errors, file_dropped) = build_training_rows_from_jsonl_bytes(
            &file.file_name,
            &file.data,
            dataset_id,
            dataset_version_id,
            tenant_id,
            created_by,
        );
        rows.append(&mut file_rows);
        parse_errors += file_errors;
        dropped += file_dropped;
    }

    (rows, parse_errors, dropped)
}

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
    request_id: Option<Extension<RequestId>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    let dataset_id = Uuid::now_v7().to_string();
    let correlation_id = request_id
        .map(|r| r.0 .0)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let dataset_root = resolve_dataset_root(&state).map_err(internal_error)?;
    let paths = DatasetPaths::new(dataset_root.clone());
    let allowed_roots = [paths.root().to_path_buf()];
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
    ensure_dirs([
        paths.files.as_path(),
        paths.temp.as_path(),
        paths.chunked.as_path(),
        paths.logs.as_path(),
    ])
    .await?;

    let storage = FsByteStorage::new(dataset_root, adapters_root.into());

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

    let mut pending_files: Vec<PendingFile> = Vec::new();
    let mut total_size = 0usize;
    let mut dataset_name = String::new();
    let mut dataset_description = String::new();
    let mut dataset_format = "jsonl".to_string();
    let mut file_count = 0;
    let mut workspace_id: Option<String> = None;
    let mut source_type: Option<String> = None;
    let mut language: Option<String> = None;
    let mut framework: Option<String> = None;
    let mut repository_url: Option<String> = None;
    let mut repo_slug: Option<String> = None;
    let mut repo_branch: Option<String> = None;
    let mut repo_commit: Option<String> = None;
    let mut unknown_fields: Vec<String> = Vec::new();

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
            "source_type" => {
                let source = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read source_type field: {}", e)))?;
                let trimmed = source.trim();
                if !trimmed.is_empty() {
                    source_type = Some(trimmed.to_string());
                }
            }
            "description" => {
                dataset_description = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read description field: {}", e)))?;
            }
            "language" => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read language field: {}", e)))?;
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    language = Some(trimmed.to_string());
                }
            }
            "framework" => {
                let value = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read framework field: {}", e)))?;
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    framework = Some(trimmed.to_string());
                }
            }
            "format" => {
                let format = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read format field: {}", e)))?;
                dataset_format = format.trim().to_ascii_lowercase();
            }
            "workspace_id" => {
                let ws = field.text().await.map_err(|e| {
                    bad_request(format!("Failed to read workspace_id field: {}", e))
                })?;
                workspace_id = Some(ws);
            }
            "repo_slug" => {
                let slug = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read repo_slug field: {}", e)))?;
                let trimmed = slug.trim();
                if !trimmed.is_empty() {
                    repo_slug = Some(trimmed.to_string());
                }
            }
            "branch" | "repo_branch" => {
                let branch = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read branch field: {}", e)))?;
                let trimmed = branch.trim();
                if !trimmed.is_empty() {
                    repo_branch = Some(trimmed.to_string());
                }
            }
            "commit_sha" | "commit_hash" | "repo_commit" | "commit" => {
                let commit = field
                    .text()
                    .await
                    .map_err(|e| bad_request(format!("Failed to read commit field: {}", e)))?;
                let trimmed = commit.trim();
                if !trimmed.is_empty() {
                    repo_commit = Some(trimmed.to_string());
                }
            }
            "repository_url" | "repo_url" => {
                let value = field.text().await.map_err(|e| {
                    bad_request(format!("Failed to read repository_url field: {}", e))
                })?;
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    repository_url = Some(trimmed.to_string());
                }
            }
            "file" | "files" | "files[]" => {
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
                if file_count > MAX_FILE_COUNT {
                    return Err(payload_too_large(&format!(
                        "Upload exceeds maximum file count of {}",
                        MAX_FILE_COUNT
                    )));
                }

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
                if !name.trim().is_empty() {
                    unknown_fields.push(name);
                }
            }
        }
    }

    if !unknown_fields.is_empty() {
        return Err(bad_request(format!(
            "Unsupported upload fields: {}",
            unknown_fields.join(", ")
        )));
    }

    if pending_files.is_empty() {
        return Err(bad_request("No files uploaded"));
    }

    if dataset_name.is_empty() {
        dataset_name = format!("Dataset {}", &dataset_id[0..8]);
    }

    validate_format(&dataset_format).map_err(|e| bad_request(e.to_string()))?;

    let workspace_id_opt = workspace_id;
    // Ensure caller can access the workspace when provided
    if let Some(ref ws_id) = workspace_id_opt {
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
            return Err(forbidden(
                "Access denied: you are not a member of this workspace",
            ));
        }
    }

    let storage_workspace = workspace_id_opt
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    let resolved_workspace_id = Some(storage_workspace.clone());

    let temp_path = paths.dataset_temp_dir(&storage_workspace, &dataset_id);
    ensure_dirs([temp_path.as_path()]).await?;

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

    let dataset_category = match source_type.as_deref() {
        Some("code_repo") => DatasetCategory::Codebase,
        Some("generated") => DatasetCategory::Synthetic,
        _ => DatasetCategory::Upload,
    };
    let canonical_dir = storage
        .layout()
        .canonical_dir_path_with_tenant(&dataset_category, &dataset_hash, Some(&claims.tenant_id))
        .map_err(|e| internal_error(format!("Failed to resolve canonical dataset path: {}", e)))?;
    ensure_dirs([canonical_dir.as_path()]).await?;
    let canonical_dir = canonicalize_strict_in_allowed_roots(&canonical_dir, &allowed_roots)
        .map_err(|e| {
            tracing::warn!(error = %e, path = %canonical_dir.display(), "Path policy rejection");
            path_policy_error(&canonical_dir, e)
        })?;
    let storage_path = canonical_dir.to_string_lossy().to_string();

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
                state
                    .db
                    .ensure_dataset_version_exists(&existing.id)
                    .await
                    .map_err(|e| {
                        db_error(format!(
                            "Failed to ensure dataset version for reused dataset: {}",
                            e
                        ))
                    })?;
                // Fetch the latest version ID for reused dataset
                let reused_version_id = state
                    .db
                    .get_latest_trusted_dataset_version_for_dataset(&existing.id)
                    .await
                    .map_err(|e| {
                        db_error(format!("Failed to get version for reused dataset: {}", e))
                    })?
                    .map(|(v, _trust)| v.id);
                info!(
                    dataset_id = %existing.id,
                    workspace_id = %ws_id,
                    "Reusing existing dataset with identical hash"
                );
                return Ok(Json(UploadDatasetResponse {
                    schema_version: "1.0".to_string(),
                    dataset_id: existing.id,
                    dataset_version_id: reused_version_id,
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
    for (idx, pending) in pending_files.iter().enumerate() {
        let key = StorageKey::canonical_dataset_with_tenant(
            claims.tenant_id.clone(),
            dataset_hash.clone(),
            dataset_category.clone(),
            None,
            &pending.file_name,
        );
        let candidate_path = storage
            .path_for(&key)
            .map_err(|e| internal_error(format!("Failed to resolve storage path: {}", e)))?;
        let parent = candidate_path.parent().ok_or_else(|| {
            internal_error(format!(
                "Dataset storage path has no parent: {}",
                candidate_path.display()
            ))
        })?;
        canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
            .map_err(|e| path_policy_error(parent, e))?;
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
        if let Err(e) = canonicalize_strict_in_allowed_roots(&location.path, &allowed_roots) {
            let _ = storage.delete(&key).await;
            return Err(path_policy_error(&location.path, e));
        }

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
            Some(total_files as i32),
        );

        uploaded_files.push(DatasetFile {
            id: Uuid::now_v7().to_string(),
            dataset_id: dataset_id.clone(),
            file_name: pending.file_name.clone(),
            file_path: location.path.to_string_lossy().to_string(),
            size_bytes: pending.data.len() as i64,
            hash_b3: pending.file_hash.clone(),
            mime_type: pending.mime_type.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    // Store in database - associate dataset with the user's tenant
    let dataset_status = if dataset_format == "jsonl" {
        "processing"
    } else {
        "ready"
    };
    let mut dataset_builder = CreateDatasetParams::builder()
        .id(&dataset_id)
        .name(&dataset_name)
        .format(&dataset_format)
        .hash_b3(&dataset_hash)
        .dataset_hash_b3(&dataset_hash)
        .storage_path(&storage_path)
        .status(dataset_status)
        .created_by(&claims.sub)
        .tenant_id(&claims.tenant_id)
        .workspace_id(&storage_workspace)
        .dataset_type("training")
        .collection_method("api")
        .category(dataset_category.as_dir_name())
        .correlation_id(&correlation_id);

    if !dataset_description.is_empty() {
        dataset_builder = dataset_builder.description(&dataset_description);
    }
    if let Some(ref slug) = repo_slug {
        dataset_builder = dataset_builder.repo_slug(slug);
    }
    if let Some(ref branch) = repo_branch {
        dataset_builder = dataset_builder.branch(branch);
    }
    if let Some(ref commit) = repo_commit {
        dataset_builder = dataset_builder.commit_sha(commit);
    }
    if let Some(ref url) = repository_url {
        dataset_builder = dataset_builder.source_location(url);
    }

    let mut upload_files: Vec<(String, String, u64, String, Option<String>)> = pending_files
        .iter()
        .map(|file| {
            (
                normalize_filename(&file.file_name),
                file.file_name.clone(),
                file.data.len() as u64,
                file.file_hash.clone(),
                file.mime_type.clone(),
            )
        })
        .collect();
    upload_files.sort_by(|(a_norm, a_name, ..), (b_norm, b_name, ..)| {
        a_norm.cmp(b_norm).then_with(|| a_name.cmp(b_name))
    });
    let upload_files_value: Vec<serde_json::Value> = upload_files
        .into_iter()
        .map(|(normalized, name, size_bytes, hash_b3, mime_type)| {
            serde_json::json!({
                "name": name,
                "normalized_name": normalized,
                "size_bytes": size_bytes,
                "hash_b3": hash_b3,
                "mime_type": mime_type,
            })
        })
        .collect();

    let mut metadata = Map::new();
    if let Some(value) = source_type {
        metadata.insert("source_type".to_string(), Value::String(value));
    }
    if let Some(value) = language {
        metadata.insert("language".to_string(), Value::String(value));
    }
    if let Some(value) = framework {
        metadata.insert("framework".to_string(), Value::String(value));
    }
    if let Some(value) = repository_url {
        metadata.insert("repository_url".to_string(), Value::String(value));
    }
    metadata.insert(
        "upload".to_string(),
        serde_json::json!({
            "dataset_hash_b3": dataset_hash.clone(),
            "file_count": pending_files.len(),
            "total_size_bytes": total_size as u64,
            "files": upload_files_value,
        }),
    );
    if !metadata.is_empty() {
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| bad_request(format!("Invalid metadata_json: {}", e)))?;
        dataset_builder = dataset_builder.metadata_json(metadata_json);
    }

    let dataset_params = dataset_builder
        .build()
        .map_err(|e| bad_request(format!("Invalid dataset parameters: {}", e)))?;

    let (_, dataset_version_id) = state
        .db
        .create_training_dataset_from_params_with_version(
            &dataset_params,
            None,
            &storage_path,
            &dataset_hash,
            None,
            None,
        )
        .await
        .map_err(|e| db_error(format!("Failed to create dataset record: {}", e)))?;

    // CRITICAL: Associate dataset with user's tenant for tenant isolation
    bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await?;
    info!(
        dataset_id = %dataset_id,
        correlation_id = %correlation_id,
        "Dataset upload recorded"
    );

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

    let mut validation_config = match dataset_format.as_str() {
        "jsonl" => ValidationConfig::for_training_jsonl(),
        "json" => ValidationConfig::for_json(),
        _ => ValidationConfig::default(),
    };
    if !validation_config
        .allowed_extensions
        .contains(&dataset_format)
    {
        validation_config
            .allowed_extensions
            .insert(dataset_format.clone());
    }
    if dataset_format == "custom" {
        for file in &uploaded_files {
            if let Some(ext) = std::path::Path::new(&file.file_name)
                .extension()
                .and_then(|e| e.to_str())
            {
                validation_config
                    .allowed_extensions
                    .insert(ext.to_ascii_lowercase());
            }
        }
    }
    let validator = if matches!(dataset_format.as_str(), "parquet" | "custom") {
        let mut validator = CompositeValidator::new(validation_config);
        validator.add_rule(Box::new(FileExistsRule));
        validator.add_rule(Box::new(FileSizeRule));
        validator.add_rule(Box::new(FileExtensionRule));
        validator
    } else {
        CompositeValidator::quick_validator(validation_config)
    };
    let file_paths: Vec<&std::path::Path> = uploaded_files
        .iter()
        .map(|file| std::path::Path::new(&file.file_path))
        .collect();
    let validation_result = validator.validate_files(&file_paths).await;
    let validation_status = if validation_result.is_valid {
        "valid"
    } else {
        "invalid"
    };
    let validation_errors = build_validation_error_payload(&validation_result.errors);

    state
        .db
        .update_dataset_validation(&dataset_id, validation_status, validation_errors.as_deref())
        .await
        .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

    if let Err(e) = state
        .db
        .update_dataset_version_structural_validation(
            &dataset_version_id,
            validation_status,
            validation_errors.as_deref(),
        )
        .await
    {
        warn!(
            error = %e,
            dataset_id = %dataset_id,
            dataset_version_id = %dataset_version_id,
            "Failed to update dataset version validation status"
        );
    }

    if !validation_result.is_valid {
        let _ = state.db.update_dataset_status(&dataset_id, "failed").await;
        return Err(validation_error_response(
            "Dataset validation failed",
            &validation_result.errors,
        ));
    }

    // Sync dataset to KV store after file counts update (non-blocking)
    if let Err(e) = state
        .db
        .sync_dataset_to_kv(&claims.tenant_id, &dataset_id)
        .await
    {
        tracing::warn!(
            error = %e,
            dataset_id = %dataset_id,
            "Failed to sync dataset to KV store after upload"
        );
    }

    if dataset_format == "jsonl" {
        let (rows, parse_errors, dropped) = build_training_rows_from_jsonl(
            &pending_files,
            &dataset_id,
            &dataset_version_id,
            &claims.tenant_id,
            Some(&claims.sub),
        );

        if rows.is_empty() {
            warn!(
                dataset_id = %dataset_id,
                dataset_version_id = %dataset_version_id,
                parse_errors,
                dropped,
                "No training dataset rows created from upload"
            );
            let _ = state.db.update_dataset_status(&dataset_id, "failed").await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Dataset contains no valid training rows")
                        .with_code("DATASET_EMPTY"),
                ),
            ));
        }

        let inserted = match state.db.bulk_insert_training_dataset_rows(&rows).await {
            Ok(inserted) => inserted,
            Err(e) => {
                warn!(
                    error = %e,
                    dataset_id = %dataset_id,
                    dataset_version_id = %dataset_version_id,
                    "Failed to insert training dataset rows"
                );
                let _ = state.db.update_dataset_status(&dataset_id, "failed").await;
                return Err(db_error(format!(
                    "Failed to insert training dataset rows: {}",
                    e
                )));
            }
        };

        if inserted == 0 {
            let _ = state.db.update_dataset_status(&dataset_id, "failed").await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Dataset contains no valid training rows")
                        .with_code("DATASET_EMPTY"),
                ),
            ));
        }

        if let Err(e) = state.db.update_dataset_status(&dataset_id, "ready").await {
            return Err(db_error(format!("Failed to update dataset status: {}", e)));
        }

        info!(
            dataset_id = %dataset_id,
            dataset_version_id = %dataset_version_id,
            inserted,
            parse_errors,
            dropped,
            "Inserted training dataset rows from upload"
        );
    } else {
        debug!(
            dataset_id = %dataset_id,
            format = %dataset_format,
            "Skipping training dataset row creation for non-jsonl upload"
        );
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
        dataset_version_id: Some(dataset_version_id),
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

    // PRD-4.2: Check quota at initiation (not completion) to fail fast
    let (soft_quota, hard_quota) = dataset_quota_limits();
    let usage = compute_tenant_storage_usage(&state, &claims.tenant_id)
        .await
        .map_err(|e| internal_error(format!("Failed to compute storage usage: {}", e)))?;
    let predicted_usage = usage.total_bytes() + request.total_size;
    if predicted_usage > hard_quota {
        return Err(quota_error(format!(
            "Dataset storage quota would be exceeded: predicted {} > {} bytes",
            predicted_usage, hard_quota
        )));
    }
    if predicted_usage > soft_quota {
        warn!(
            tenant_id = %claims.tenant_id,
            predicted_usage,
            soft_quota,
            "Chunked upload initiation: soft quota will be exceeded"
        );
    }

    // Validate workspace access if provided
    if let Some(ref ws_id) = request.workspace_id {
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
            return Err(forbidden(
                "Access denied: you are not a member of this workspace",
            ));
        }

        // Check workspace quota if configured
        let uploads_cfg = adapteros_config::effective_config()
            .map(|cfg| cfg.uploads.clone())
            .ok();
        if let Some(cfg) = uploads_cfg {
            if cfg.workspace_hard_quota_bytes > 0 {
                let ws_usage = compute_workspace_storage_usage(&state, ws_id)
                    .await
                    .map_err(|e| {
                        internal_error(format!("Failed to compute workspace usage: {}", e))
                    })?;
                let predicted_ws_usage = ws_usage.total_bytes() + request.total_size;

                if predicted_ws_usage > cfg.workspace_hard_quota_bytes {
                    return Err(quota_error(format!(
                        "Workspace storage quota exceeded: predicted {} > {} bytes",
                        predicted_ws_usage, cfg.workspace_hard_quota_bytes
                    )));
                }
                if predicted_ws_usage > cfg.workspace_soft_quota_bytes {
                    warn!(
                        workspace_id = %ws_id,
                        predicted_usage = predicted_ws_usage,
                        soft_quota = cfg.workspace_soft_quota_bytes,
                        "Chunked upload initiation: workspace soft quota will be exceeded"
                    );
                }
            }
        }
    }

    let file_name = request.file_name.trim();
    if file_name.is_empty() {
        return Err(bad_request("File name must not be empty"));
    }
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err(bad_request(format!(
            "Unsupported file name '{}': path separators are not allowed",
            file_name
        )));
    }

    let idempotency_key = match request.idempotency_key.as_deref() {
        Some(value) => Some(validate_idempotency_key(value)?),
        None => None,
    };

    let expected_file_hash_b3 = match request.expected_file_hash_b3.as_deref() {
        Some(value) => {
            validate_hash_b3(value)
                .map_err(|e| bad_request(format!("Invalid expected file hash (BLAKE3): {}", e)))?;
            Some(value.to_string())
        }
        None => None,
    };

    // Determine chunk size - reject explicit out-of-bounds values instead of silent clamping
    let chunk_size = match request.chunk_size {
        Some(size) if size < MIN_CHUNK_SIZE => {
            return Err(bad_request(format!(
                "Chunk size {} is below minimum of {} bytes",
                size, MIN_CHUNK_SIZE
            )));
        }
        Some(size) if size > MAX_CHUNK_SIZE => {
            return Err(bad_request(format!(
                "Chunk size {} exceeds maximum of {} bytes",
                size, MAX_CHUNK_SIZE
            )));
        }
        Some(size) => size,
        None => DEFAULT_CHUNK_SIZE,
    };

    // Calculate expected chunks
    let expected_chunk_count = expected_chunks(request.total_size, chunk_size);

    // Detect compression
    let content_type = request
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let compression = CompressionFormat::from_content_type(&content_type);

    let dataset_root = resolve_dataset_root(&state).map_err(internal_error)?;
    let paths = DatasetPaths::new(dataset_root);

    let storage_workspace = request
        .workspace_id
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    let normalized_file_name = normalize_filename(file_name);
    let session_key = build_session_key(
        idempotency_key.as_deref(),
        expected_file_hash_b3.as_deref(),
        &claims.tenant_id,
        &storage_workspace,
        &normalized_file_name,
        request.total_size,
        chunk_size,
        &content_type,
    );

    if let Some(existing) = fetch_session_by_key(
        &state.db,
        &claims.tenant_id,
        &storage_workspace,
        &session_key,
    )
    .await?
    {
        if existing.status == "failed" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(format!(
                        "Upload session {} previously failed: {}",
                        existing.session_id,
                        existing
                            .error_message
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string())
                    ))
                    .with_code("UPLOAD_SESSION_FAILED"),
                ),
            ));
        }

        if existing.normalized_file_name != normalized_file_name
            || existing.total_size_bytes != request.total_size
            || existing.chunk_size_bytes != chunk_size
            || existing.content_type != content_type
        {
            return Err((
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::new(
                        "Idempotency key already used with different upload parameters",
                    )
                    .with_code("IDEMPOTENCY_CONFLICT"),
                ),
            ));
        }

        if let (Some(existing_hash), Some(request_hash)) = (
            existing.expected_file_hash_b3.as_deref(),
            expected_file_hash_b3.as_deref(),
        ) {
            if existing_hash != request_hash {
                state
                    .metrics_registry
                    .record_metric(METRIC_CHUNKED_CONFLICT_HASH_MISMATCH.to_string(), 1.0)
                    .await;
                return Err((
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new(
                            "Idempotency key already used with a different expected hash",
                        )
                        .with_code("IDEMPOTENCY_CONFLICT"),
                    ),
                ));
            }
        }

        info!(
            "Reusing chunked upload session {} for file {} ({} bytes, {} chunks, workspace: {:?})",
            existing.session_id,
            existing.file_name,
            existing.total_size_bytes,
            expected_chunks(existing.total_size_bytes, existing.chunk_size_bytes),
            request.workspace_id
        );
        state
            .metrics_registry
            .record_metric(METRIC_CHUNKED_SESSIONS_REUSED.to_string(), 1.0)
            .await;

        return Ok(Json(InitiateChunkedUploadResponse {
            session_id: existing.session_id,
            chunk_size: existing.chunk_size_bytes,
            expected_chunks: expected_chunks(existing.total_size_bytes, existing.chunk_size_bytes),
            compression_format: format!(
                "{:?}",
                CompressionFormat::from_content_type(&content_type)
            ),
        }));
    }

    // Use shared session manager from AppState with workspace isolation
    let session = prepare_session_with_workspace(
        &state,
        &paths,
        file_name,
        request.total_size,
        &content_type,
        chunk_size,
        compression.clone(),
        Some(storage_workspace.clone()),
    )
    .await?
    .0;

    let dataset_id = Uuid::now_v7().to_string();
    let session_record = UploadSessionRecord {
        schema_version: UPLOAD_SESSION_DB_SCHEMA_VERSION,
        session_id: session.session_id.clone(),
        session_key,
        tenant_id: claims.tenant_id.clone(),
        workspace_id: storage_workspace.clone(),
        dataset_id: dataset_id.clone(),
        file_name: session.file_name.clone(),
        normalized_file_name,
        total_size_bytes: session.total_size,
        chunk_size_bytes: session.chunk_size,
        content_type: session.content_type.clone(),
        expected_file_hash_b3: expected_file_hash_b3.clone(),
        actual_file_hash_b3: None,
        received_chunks: HashMap::new(),
        status: "initiated".to_string(),
        error_message: None,
        temp_dir: session.temp_dir.clone(),
        created_at: String::new(),
        updated_at: String::new(),
    };

    insert_session(&state.db, &session_record).await?;
    state
        .metrics_registry
        .record_metric(METRIC_CHUNKED_SESSIONS_CREATED.to_string(), 1.0)
        .await;

    info!(
        "Initiated chunked upload session {} for file {} ({} bytes, {} chunks, workspace: {:?})",
        session.session_id,
        request.file_name,
        request.total_size,
        expected_chunk_count,
        request.workspace_id
    );

    Ok(Json(InitiateChunkedUploadResponse {
        session_id: session.session_id,
        chunk_size,
        expected_chunks: expected_chunk_count,
        compression_format: format!("{:?}", compression),
    }))
}
