//! Dataset file handlers.
//!
//! This module provides handlers for dataset file operations including:
//! - Listing files within a dataset
//! - Retrieving individual file content
//! - Getting dataset statistics
//! - Validating individual files for format, structure, and content compliance

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{DatasetFileResponse, DatasetStatisticsResponse};
use adapteros_core::B3Hash;
use adapteros_db::training_datasets::{CreateDatasetFileParams, DatasetFile, TrainingDataset};
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path as StdPath, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, warn};
use utoipa::ToSchema;

use super::helpers::{ensure_dataset_file_within_root, MAX_FILE_SIZE, STREAM_BUFFER_SIZE};
use super::validation::{
    deep_validate_file, quick_validate_file, validate_file_integrity, DatasetValidationResult,
    ValidationCategory, ValidationConfig, ValidationError, ValidationSeverity,
};
use super::{resolve_dataset_root, DatasetPaths, VERSIONS_DIR_NAME};

fn resolve_dataset_storage(state: &AppState) -> Result<FsByteStorage, ApiError> {
    let datasets_root = resolve_dataset_root(state)
        .map_err(|e| ApiError::db_error(format!("Failed to resolve datasets root: {}", e)))?;
    let adapters_root = state
        .config
        .read()
        .map_err(|_| ApiError::db_error("Config lock poisoned".to_string()))?
        .paths
        .adapters_root
        .clone();
    Ok(FsByteStorage::new(datasets_root, adapters_root.into()))
}

fn resolve_dataset_file_path(
    storage: &FsByteStorage,
    dataset: &TrainingDataset,
    file: &DatasetFile,
    workspace_override: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let workspace_id = workspace_override
        .or(dataset.workspace_id.as_deref())
        .or(dataset.tenant_id.as_deref());
    let key = StorageKey::dataset_file(
        workspace_id.map(|id| id.to_string()),
        &file.dataset_id,
        None,
        &file.file_name,
    );
    let expected_path = storage
        .path_for(&key)
        .map_err(|e| ApiError::db_error(format!("Failed to resolve dataset file path: {}", e)))?;

    if file.file_path.is_empty() {
        return Ok(expected_path);
    }

    let reported_path = PathBuf::from(&file.file_path);
    if reported_path == expected_path {
        return Ok(expected_path);
    }

    let legacy_key = StorageKey::dataset_file(None, &file.dataset_id, None, &file.file_name);
    let legacy_path = storage.path_for(&legacy_key).map_err(|e| {
        ApiError::db_error(format!("Failed to resolve legacy dataset file path: {}", e))
    })?;
    if reported_path == legacy_path {
        return Ok(legacy_path);
    }

    let dataset_storage_path = PathBuf::from(&dataset.storage_path);
    if !dataset.storage_path.is_empty() && reported_path.starts_with(&dataset_storage_path) {
        return Ok(reported_path);
    }

    Err(ApiError::db_error(format!(
        "Dataset file path does not match storage layout for {}",
        file.file_name
    )))
}

async fn load_dataset_files(
    state: &AppState,
    dataset: &TrainingDataset,
) -> Result<Vec<DatasetFile>, ApiError> {
    if let Some(workspace_id) = dataset.workspace_id.as_deref() {
        return state
            .db
            .get_dataset_files_for_workspace(workspace_id, &dataset.id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to get dataset files: {}", e)));
    }

    state
        .db
        .get_dataset_files(&dataset.id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset files: {}", e)))
}

async fn load_dataset_file(
    state: &AppState,
    dataset: &TrainingDataset,
    file_id: &str,
) -> Result<DatasetFile, ApiError> {
    if let Some(workspace_id) = dataset.workspace_id.as_deref() {
        return state
            .db
            .get_dataset_file_for_workspace(workspace_id, &dataset.id, file_id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to get dataset file: {}", e)))?
            .ok_or_else(|| ApiError::not_found("File"));
    }

    state
        .db
        .get_dataset_file(&dataset.id, file_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset file: {}", e)))?
        .ok_or_else(|| ApiError::not_found("File"))
}

async fn hash_file_streaming(path: &StdPath) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path).await?;
    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut hasher = blake3::Hasher::new();

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(B3Hash::from_bytes(*hasher.finalize().as_bytes()).to_hex())
}

async fn collect_dataset_file_paths(root: &StdPath) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                if entry
                    .file_name()
                    .to_str()
                    .map(|name| name == VERSIONS_DIR_NAME)
                    .unwrap_or(false)
                {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file() {
                files.push(path);
            }
        }
    }

    Ok(files)
}

async fn populate_dataset_files_from_storage(
    state: &AppState,
    dataset: &TrainingDataset,
) -> Result<(), ApiError> {
    if dataset.tenant_id.is_none() {
        debug!(
            dataset_id = %dataset.id,
            "Skipping dataset file backfill (dataset has no tenant_id)"
        );
        return Ok(());
    }

    let mut roots: Vec<PathBuf> = Vec::new();

    match DatasetPaths::from_state(state) {
        Ok(paths) => {
            let workspace_id = dataset
                .workspace_id
                .as_deref()
                .or(dataset.tenant_id.as_deref());
            let dataset_dir = match workspace_id {
                Some(id) => paths.dataset_dir(id, &dataset.id),
                #[allow(deprecated)]
                None => paths.dataset_dir_unscoped(&dataset.id),
            };
            roots.push(dataset_dir);
        }
        Err(err) => {
            warn!(
                dataset_id = %dataset.id,
                error = %err,
                "Failed to resolve dataset root for file backfill"
            );
        }
    }

    let storage_path = dataset.storage_path.trim();
    if !storage_path.is_empty() {
        match ensure_dataset_file_within_root(state, StdPath::new(storage_path)).await {
            Ok(path) => {
                if !roots.iter().any(|root| root == &path) {
                    roots.push(path);
                }
            }
            Err(_) => {
                warn!(
                    dataset_id = %dataset.id,
                    path = %storage_path,
                    "Dataset storage path is outside the configured dataset root"
                );
            }
        }
    }

    if roots.is_empty() {
        return Ok(());
    }

    let mut file_records = Vec::new();
    let mut seen_names = HashSet::new();

    for root in roots {
        let root_metadata = match fs::metadata(&root).await {
            Ok(metadata) => metadata,
            Err(err) => {
                debug!(
                    dataset_id = %dataset.id,
                    path = %root.display(),
                    error = %err,
                    "Dataset storage path unavailable"
                );
                continue;
            }
        };

        let root_is_dir = root_metadata.is_dir();
        let mut file_paths = if root_metadata.is_file() {
            vec![root.clone()]
        } else {
            collect_dataset_file_paths(&root)
                .await
                .map_err(|e| ApiError::db_error(format!("Failed to list dataset files: {}", e)))?
        };

        file_paths.sort();

        for path in file_paths {
            let metadata = fs::metadata(&path)
                .await
                .map_err(|e| ApiError::db_error(format!("Failed to stat dataset file: {}", e)))?;
            if !metadata.is_file() {
                continue;
            }

            let file_name = if root_is_dir {
                path.strip_prefix(&root)
                    .ok()
                    .and_then(|rel| {
                        if rel.as_os_str().is_empty() {
                            None
                        } else {
                            Some(rel.to_string_lossy().to_string())
                        }
                    })
                    .or_else(|| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name.to_string())
                    })
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            } else {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            };

            if file_name.trim().is_empty() || !seen_names.insert(file_name.clone()) {
                continue;
            }

            let hash_b3 = hash_file_streaming(&path)
                .await
                .map_err(|e| ApiError::db_error(format!("Failed to hash dataset file: {}", e)))?;

            file_records.push(CreateDatasetFileParams {
                file_name,
                file_path: path.to_string_lossy().to_string(),
                size_bytes: metadata.len() as i64,
                hash_b3,
                mime_type: None,
            });
        }
    }

    if file_records.is_empty() {
        return Ok(());
    }

    state
        .db
        .insert_dataset_files(&dataset.id, &file_records)
        .await
        .map_err(|e| {
            ApiError::db_error(format!("Failed to insert dataset file metadata: {}", e))
        })?;

    Ok(())
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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let mut files = load_dataset_files(&state, &dataset).await?;
    if files.is_empty() && dataset.file_count == 0 && dataset.total_size_bytes == 0 {
        populate_dataset_files_from_storage(&state, &dataset).await?;
        files = load_dataset_files(&state, &dataset).await?;
    }

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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    let stats = state
        .db
        .get_dataset_statistics(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get statistics: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Statistics for this dataset"))?;

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
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists and check tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&path.dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        // Datasets without tenant_id are only accessible to admins
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the specific file
    let file = load_dataset_file(&state, &dataset, &path.file_id).await?;

    let storage = resolve_dataset_storage(&state)?;
    let resolved_path = resolve_dataset_file_path(&storage, &dataset, &file, None)?;
    let safe_path = ensure_dataset_file_within_root(&state, &resolved_path).await?;

    let metadata = fs::metadata(&safe_path)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to stat file: {}", e)))?;
    if metadata.len() > MAX_FILE_SIZE as u64 {
        return Err(ApiError::payload_too_large(format!(
            "File exceeds maximum size of {}MB",
            MAX_FILE_SIZE / 1024 / 1024
        )));
    }

    // Read file content
    let file_data = fs::read(&safe_path)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to read file: {}", e)))?;

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

// ============================================================================
// File Validation Types
// ============================================================================

/// Request parameters for file validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateFileRequest {
    /// Validation mode: "quick" for fast checks, "deep" for comprehensive validation
    #[serde(default = "default_validation_mode")]
    pub mode: String,
    /// Whether to check required fields for JSONL training format
    #[serde(default)]
    pub check_training_format: bool,
    /// Custom required fields to validate (overrides defaults when specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_fields: Option<Vec<String>>,
}

fn default_validation_mode() -> String {
    "quick".to_string()
}

impl Default for ValidateFileRequest {
    fn default() -> Self {
        Self {
            mode: default_validation_mode(),
            check_training_format: false,
            required_fields: None,
        }
    }
}

/// Response from file validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateFileResponse {
    /// Schema version for API compatibility
    pub schema_version: String,
    /// File ID that was validated
    pub file_id: String,
    /// File name
    pub file_name: String,
    /// Whether the file passed all error-level validations
    pub is_valid: bool,
    /// Validation mode used ("quick" or "deep")
    pub validation_mode: String,
    /// Total number of errors found
    pub error_count: usize,
    /// Total number of warnings found
    pub warning_count: usize,
    /// Total number of info messages
    pub info_count: usize,
    /// Number of entries validated (for JSONL/JSON files)
    pub entries_validated: usize,
    /// Validation duration in milliseconds
    pub duration_ms: u64,
    /// Detailed validation errors, warnings, and info messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FileValidationError>>,
    /// Timestamp when validation was performed (RFC3339)
    pub validated_at: String,
}

/// Detailed file validation error
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileValidationError {
    /// Error severity: "error", "warning", or "info"
    pub severity: String,
    /// Error category: "structure", "format", "schema", "size", "file_type", "integrity", "encoding", "content"
    pub category: String,
    /// Human-readable error message
    pub message: String,
    /// Error code for programmatic handling
    pub code: String,
    /// Line number where error occurred (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    /// Column number where error occurred (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_number: Option<usize>,
    /// Field name that caused the error (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Suggested fix for the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl From<&super::validation::ValidationError> for FileValidationError {
    fn from(err: &super::validation::ValidationError) -> Self {
        Self {
            severity: err.severity.to_string(),
            category: err.category.to_string(),
            message: err.message.clone(),
            code: err.code.clone(),
            line_number: err.line_number,
            column_number: err.column_number,
            field_name: err.field_name.clone(),
            suggestion: err.suggestion.clone(),
        }
    }
}

/// Convert internal validation result to API response
fn validation_result_to_response(
    file_id: &str,
    file_name: &str,
    result: DatasetValidationResult,
) -> ValidateFileResponse {
    let errors = if result.errors.is_empty() {
        None
    } else {
        Some(
            result
                .errors
                .iter()
                .map(FileValidationError::from)
                .collect(),
        )
    };

    ValidateFileResponse {
        schema_version: "1.0".to_string(),
        file_id: file_id.to_string(),
        file_name: file_name.to_string(),
        is_valid: result.is_valid,
        validation_mode: result.mode.to_string(),
        error_count: result.error_count,
        warning_count: result.warning_count,
        info_count: result.info_count,
        entries_validated: result.entries_validated,
        duration_ms: result.duration_ms,
        errors,
        validated_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ============================================================================
// File Validation Handlers
// ============================================================================

/// Validate a specific file within a dataset
///
/// Performs format and content validation on a single file. Supports two validation modes:
/// - **quick**: Fast checks including file existence, size limits, extension, and encoding
/// - **deep**: Comprehensive validation including JSON/JSONL parsing, required field checks,
///   duplicate detection, and content quality analysis
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/files/{file_id}/validate",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("file_id" = String, Path, description = "File ID")
    ),
    request_body = ValidateFileRequest,
    responses(
        (status = 200, description = "File validation result", body = ValidateFileResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset or file not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn validate_dataset_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(path): Path<DatasetFileContentPath>,
    Json(request): Json<ValidateFileRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Verify dataset exists and check tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&path.dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation - non-admin users can only validate their own tenant's datasets
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get the specific file
    let file = load_dataset_file(&state, &dataset, &path.file_id).await?;

    let storage = resolve_dataset_storage(&state)?;
    let resolved_path = resolve_dataset_file_path(&storage, &dataset, &file, None)?;
    let safe_path = ensure_dataset_file_within_root(&state, &resolved_path).await?;

    // Build validation config
    let config = if request.check_training_format {
        let mut cfg = ValidationConfig::for_training_jsonl();
        if let Some(ref fields) = request.required_fields {
            cfg.required_fields = fields.clone();
        }
        cfg
    } else {
        let mut cfg = ValidationConfig::default();
        if let Some(ref fields) = request.required_fields {
            cfg.required_fields = fields.clone();
        }
        cfg
    };

    // Perform validation based on mode
    let file_path = StdPath::new(&safe_path);
    let mut result = match request.mode.to_lowercase().as_str() {
        "deep" => deep_validate_file(file_path, Some(config)).await,
        _ => quick_validate_file(file_path, Some(config)).await,
    };
    let expected_hash = file.hash_b3.trim();
    if expected_hash.is_empty() {
        result.add_error(
            ValidationError::new(
                ValidationSeverity::Warning,
                ValidationCategory::Integrity,
                "File hash is missing; integrity check skipped",
                "HASH_MISSING",
            )
            .with_file(safe_path.to_string_lossy().to_string())
            .with_suggestion("Re-upload the file to compute and store its hash"),
        );
    } else {
        let expected_hash = expected_hash.strip_prefix("b3:").unwrap_or(expected_hash);
        let integrity = validate_file_integrity(file_path, expected_hash).await;
        result.duration_ms += integrity.duration_ms;
        result.merge(integrity);
    }

    let response = validation_result_to_response(&file.id, &file.file_name, result);
    Ok(Json(response))
}

/// Validate all files in a dataset
///
/// Performs batch validation on all files within a dataset. Returns aggregated results
/// with per-file error details.
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/files/validate",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    request_body = ValidateFileRequest,
    responses(
        (status = 200, description = "Batch file validation result", body = ValidateAllFilesResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Dataset not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn validate_all_dataset_files(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(request): Json<ValidateFileRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetValidate)?;

    // Verify dataset exists and check tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // CRITICAL: Validate tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Get all files in dataset
    let files = load_dataset_files(&state, &dataset).await?;

    let storage = resolve_dataset_storage(&state)?;

    // Build validation config
    let config = if request.check_training_format {
        let mut cfg = ValidationConfig::for_training_jsonl();
        if let Some(ref fields) = request.required_fields {
            cfg.required_fields = fields.clone();
        }
        cfg
    } else {
        let mut cfg = ValidationConfig::default();
        if let Some(ref fields) = request.required_fields {
            cfg.required_fields = fields.clone();
        }
        cfg
    };

    let start = std::time::Instant::now();
    let mut file_results = Vec::with_capacity(files.len());
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut total_entries = 0;
    let mut all_valid = true;

    // Validate each file
    for file in &files {
        let resolved_path = resolve_dataset_file_path(&storage, &dataset, file, None)?;
        let safe_path = ensure_dataset_file_within_root(&state, &resolved_path).await?;
        let file_path = StdPath::new(&safe_path);
        let mut result = match request.mode.to_lowercase().as_str() {
            "deep" => deep_validate_file(file_path, Some(config.clone())).await,
            _ => quick_validate_file(file_path, Some(config.clone())).await,
        };
        let expected_hash = file.hash_b3.trim();
        if expected_hash.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationSeverity::Warning,
                    ValidationCategory::Integrity,
                    "File hash is missing; integrity check skipped",
                    "HASH_MISSING",
                )
                .with_file(safe_path.to_string_lossy().to_string())
                .with_suggestion("Re-upload the file to compute and store its hash"),
            );
        } else {
            let expected_hash = expected_hash.strip_prefix("b3:").unwrap_or(expected_hash);
            let integrity = validate_file_integrity(file_path, expected_hash).await;
            result.duration_ms += integrity.duration_ms;
            result.merge(integrity);
        }

        if !result.is_valid {
            all_valid = false;
        }
        total_errors += result.error_count;
        total_warnings += result.warning_count;
        total_entries += result.entries_validated;

        file_results.push(validation_result_to_response(
            &file.id,
            &file.file_name,
            result,
        ));
    }

    let response = ValidateAllFilesResponse {
        schema_version: "1.0".to_string(),
        dataset_id,
        is_valid: all_valid,
        validation_mode: request.mode.clone(),
        files_validated: files.len(),
        total_error_count: total_errors,
        total_warning_count: total_warnings,
        total_entries_validated: total_entries,
        duration_ms: start.elapsed().as_millis() as u64,
        file_results,
        validated_at: chrono::Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
}

/// Response from validating all files in a dataset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ValidateAllFilesResponse {
    /// Schema version for API compatibility
    pub schema_version: String,
    /// Dataset ID
    pub dataset_id: String,
    /// Whether all files passed validation
    pub is_valid: bool,
    /// Validation mode used
    pub validation_mode: String,
    /// Number of files validated
    pub files_validated: usize,
    /// Total error count across all files
    pub total_error_count: usize,
    /// Total warning count across all files
    pub total_warning_count: usize,
    /// Total entries validated across all files
    pub total_entries_validated: usize,
    /// Total validation duration in milliseconds
    pub duration_ms: u64,
    /// Per-file validation results
    pub file_results: Vec<ValidateFileResponse>,
    /// Timestamp when validation was performed (RFC3339)
    pub validated_at: String,
}

// ============================================================================
// Scope-Aware File Listing Endpoints
// ============================================================================

/// Path parameters for workspace-scoped file listing
#[derive(Debug, Deserialize)]
pub struct WorkspaceDatasetFilesPath {
    pub workspace_id: String,
    pub dataset_id: String,
}

/// Path parameters for workspace-scoped file content
#[derive(Debug, Deserialize)]
pub struct WorkspaceDatasetFileContentPath {
    pub workspace_id: String,
    pub dataset_id: String,
    pub file_id: String,
}

/// Query parameters for workspace file listing
#[derive(Debug, Deserialize)]
pub struct WorkspaceFilesQuery {
    /// Maximum number of files to return (default: 100)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// Response for workspace-scoped file listing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceFilesResponse {
    /// Schema version for API compatibility
    pub schema_version: String,
    /// Workspace ID
    pub workspace_id: String,
    /// Total number of files in the workspace
    pub total_count: i64,
    /// Files in this response
    pub files: Vec<DatasetFileResponse>,
}

/// Get files in a dataset scoped to a workspace
///
/// This endpoint provides workspace-level isolation for dataset file listing.
/// Only files belonging to datasets within the specified workspace are returned.
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/datasets/{dataset_id}/files",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "List of files in dataset", body = Vec<DatasetFileResponse>),
        (status = 403, description = "Workspace access denied"),
        (status = 404, description = "Dataset not found in workspace"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_files_for_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(path): Path<WorkspaceDatasetFilesPath>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists and belongs to the workspace
    let dataset = state
        .db
        .get_training_dataset(&path.dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Verify workspace match
    if dataset.workspace_id.as_deref() != Some(&path.workspace_id) {
        return Err(ApiError::not_found("Dataset not found in this workspace"));
    }

    // CRITICAL: Validate tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Use the workspace-scoped query for additional safety
    let mut files = state
        .db
        .get_dataset_files_for_workspace(&path.workspace_id, &path.dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset files: {}", e)))?;
    if files.is_empty() && dataset.file_count == 0 && dataset.total_size_bytes == 0 {
        populate_dataset_files_from_storage(&state, &dataset).await?;
        files = state
            .db
            .get_dataset_files_for_workspace(&path.workspace_id, &path.dataset_id)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to get dataset files: {}", e)))?;
    }

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

/// Get file content scoped to a workspace
///
/// This endpoint provides workspace-level isolation for file content access.
/// Only files belonging to datasets within the specified workspace can be accessed.
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/datasets/{dataset_id}/files/{file_id}/content",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("dataset_id" = String, Path, description = "Dataset ID"),
        ("file_id" = String, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "File content streamed successfully"),
        (status = 403, description = "Workspace access denied"),
        (status = 404, description = "File not found in workspace"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_dataset_file_content_for_workspace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(path): Path<WorkspaceDatasetFileContentPath>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // Verify dataset exists and belongs to the workspace
    let dataset = state
        .db
        .get_training_dataset(&path.dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Verify workspace match
    if dataset.workspace_id.as_deref() != Some(&path.workspace_id) {
        return Err(ApiError::not_found("Dataset not found in this workspace"));
    }

    // CRITICAL: Validate tenant isolation
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Use the workspace-scoped query for additional safety
    let file = state
        .db
        .get_dataset_file_for_workspace(&path.workspace_id, &path.dataset_id, &path.file_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset file: {}", e)))?
        .ok_or_else(|| ApiError::not_found("File not found in this workspace"))?;

    let storage = resolve_dataset_storage(&state)?;
    let resolved_path =
        resolve_dataset_file_path(&storage, &dataset, &file, Some(&path.workspace_id))?;
    let safe_path = ensure_dataset_file_within_root(&state, &resolved_path).await?;

    // Read file content
    let file_data = fs::read(&safe_path)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to read file: {}", e)))?;

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

/// List all files across all datasets in a workspace
///
/// This endpoint returns all files from all datasets within the specified workspace,
/// useful for workspace-wide file inventory and management operations.
#[utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/files",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of files to return (default: 100)")
    ),
    responses(
        (status = 200, description = "List of all files in workspace", body = WorkspaceFilesResponse),
        (status = 403, description = "Workspace access denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn list_workspace_files(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<WorkspaceFilesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_permission(&claims, Permission::DatasetView)?;

    // For workspace-level access, verify the user has access to this workspace
    // by checking if they have any datasets in this workspace (non-admin check)
    if claims.role != "admin" {
        // Verify the workspace belongs to the user's tenant
        let datasets = state
            .db
            .list_training_datasets_for_workspace(&claims.tenant_id, &workspace_id, 1)
            .await
            .map_err(|e| ApiError::db_error(format!("Failed to verify workspace access: {}", e)))?;

        if datasets.is_empty() {
            // Check if workspace exists but user has no access, or workspace doesn't exist
            return Err(ApiError::forbidden(
                "Access denied: no datasets found in this workspace for your tenant",
            ));
        }
    }

    // Get total count
    let total_count = state
        .db
        .count_files_for_workspace(&workspace_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to count workspace files: {}", e)))?;

    // Get files
    let files = state
        .db
        .list_all_files_for_workspace(&workspace_id, query.limit)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to list workspace files: {}", e)))?;

    let file_responses: Vec<DatasetFileResponse> = files
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

    Ok(Json(WorkspaceFilesResponse {
        schema_version: "1.0".to_string(),
        workspace_id,
        total_count,
        files: file_responses,
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod file_validation_tests {
    use super::super::validation::ValidationMode;
    use super::*;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    async fn create_test_file(dir: &StdPath, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).await.unwrap();
        file.write_all(content.as_bytes()).await.unwrap();
        path
    }

    #[tokio::test]
    async fn test_file_validation_error_conversion() {
        let err = super::super::validation::ValidationError::new(
            super::super::validation::ValidationSeverity::Error,
            super::super::validation::ValidationCategory::Format,
            "Test error message",
            "TEST_ERROR",
        )
        .with_line(10)
        .with_column(5)
        .with_field("test_field")
        .with_suggestion("Fix this issue");

        let converted = FileValidationError::from(&err);

        assert_eq!(converted.severity, "error");
        assert_eq!(converted.category, "format");
        assert_eq!(converted.message, "Test error message");
        assert_eq!(converted.code, "TEST_ERROR");
        assert_eq!(converted.line_number, Some(10));
        assert_eq!(converted.column_number, Some(5));
        assert_eq!(converted.field_name, Some("test_field".to_string()));
        assert_eq!(converted.suggestion, Some("Fix this issue".to_string()));
    }

    #[tokio::test]
    async fn test_validate_file_request_defaults() {
        let request = ValidateFileRequest::default();
        assert_eq!(request.mode, "quick");
        assert!(!request.check_training_format);
        assert!(request.required_fields.is_none());
    }

    #[tokio::test]
    async fn test_quick_validation_valid_jsonl() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"data": "test"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let result = quick_validate_file(&path, None).await;

        assert!(result.is_valid);
        assert_eq!(result.mode, ValidationMode::Quick);
        assert_eq!(result.error_count, 0);
    }

    #[tokio::test]
    async fn test_deep_validation_with_training_format() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"prompt": "Hello", "completion": "World"}"#;
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let config = ValidationConfig::for_training_jsonl();
        let result = deep_validate_file(&path, Some(config)).await;

        assert!(result.is_valid);
        assert_eq!(result.mode, ValidationMode::Deep);
    }

    #[tokio::test]
    async fn test_deep_validation_missing_required_field() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let dir = tempdir().unwrap();
        let content = r#"{"prompt": "Hello"}"#; // missing completion
        let path = create_test_file(dir.path(), "test.jsonl", content).await;

        let config = ValidationConfig::for_training_jsonl();
        let result = deep_validate_file(&path, Some(config)).await;

        assert!(!result.is_valid);
        assert!(result.error_count > 0);
        assert!(result.errors.iter().any(|e| e.code == "JSONL_SCHEMA_ERROR"));
    }

    #[tokio::test]
    async fn test_validation_result_to_response() {
        let mut result = DatasetValidationResult::new(ValidationMode::Quick);
        result.is_valid = true;
        result.files_validated = 1;
        result.entries_validated = 10;
        result.duration_ms = 50;

        let response = validation_result_to_response("file-123", "test.jsonl", result);

        assert_eq!(response.file_id, "file-123");
        assert_eq!(response.file_name, "test.jsonl");
        assert!(response.is_valid);
        assert_eq!(response.validation_mode, "quick");
        assert_eq!(response.entries_validated, 10);
        assert!(response.errors.is_none());
    }
}
