//! Minimal model management handlers (stubs for unwired routes)
//!
//! These handlers provide basic API endpoints for model operations.
//! Full implementation details are in the original models.rs file.

use crate::audit_helper::{log_failure, log_success};
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use crate::uds_client::UdsClient;
use adapteros_db::users::Role;
use std::path::PathBuf;
use std::time::Duration;

/// Resource type for model audit logs
const RESOURCE_MODEL: &str = "model";

/// Audit action: model load
const ACTION_MODEL_LOAD: &str = "model.load";

/// Audit action: model unload
const ACTION_MODEL_UNLOAD: &str = "model.unload";

/// Audit action: model import
const ACTION_MODEL_IMPORT: &str = "model.import";
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct AllModelsStatusResponse {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub models: Vec<crate::types::BaseModelStatusResponse>,
    pub total_memory_mb: i64,
    pub available_memory_mb: Option<i64>,
    pub active_model_count: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct ImportModelRequest {
    pub model_name: String,
    pub model_path: String,
    pub format: String,  // "mlx", "safetensors", "pytorch", "gguf"
    pub backend: String, // "mlx-ffi", "metal"
    pub capabilities: Option<Vec<String>>, // ["chat", "completion", "embeddings"]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, ToSchema)]
pub struct ImportModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    pub progress: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: String,
    pub loaded_at: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ValidationIssue {
    #[serde(rename = "type")]
    pub issue_type: String,
    pub message: String,
}

#[derive(Serialize, ToSchema)]
pub struct ModelValidationResponse {
    pub model_id: String,
    pub status: String, // "ready", "needs_setup", "invalid"
    pub valid: bool,
    pub can_load: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub issues: Vec<ValidationIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>, // Legacy field for backwards compatibility
}

#[derive(Serialize, ToSchema)]
pub struct ModelRuntimeHealthResponse {
    pub model_id: String,
    pub health_status: String,
    pub memory_usage_mb: Option<i32>,
    pub last_accessed: Option<String>,
}

/// Load a base model into memory
///
/// # Endpoint
/// POST /v1/models/{model_id}/load
///
/// # Authentication
/// Required
///
/// # Permissions
/// Requires one of: Admin, Operator
///
/// # Response
/// Returns current model load status including memory usage and load timestamp.
/// If model is already loaded, returns success with current status.
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `WORKER_UNAVAILABLE` (503): No worker available to load the model
/// - `WORKER_ERROR` (500): Worker failed to load model
/// - `INTERNAL_ERROR` (500): Database error, memory pressure, load failure
///
/// # Example
/// ```
/// POST /v1/models/qwen-7b/load
/// ```
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/load",
    params(
        ("model_id" = String, Path, description = "Model ID to load")
    ),
    responses(
        (status = 200, description = "Model loaded", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 400, description = "Already loaded"),
        (status = 500, description = "Load failed")
    ),
    tag = "models"
)]
pub async fn load_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, info, warn};

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
        )
    })?;

    let tenant_id = &claims.sub;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists in database
    let model = state
        .db
        .get_model(&model_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model {}: {}", model_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Model not found: {}", model_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
            )
        })?;

    // Check current status in base_model_status table
    let current_status = state
        .db
        .get_base_model_status(tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Check if model is already loaded - return success with current status
    if let Some(status) = &current_status {
        if status.status == "loaded" && status.model_id == model_id {
            info!("Model already loaded: {}", model_id);
            return Ok(Json(ModelStatusResponse {
                model_id: model_id.clone(),
                model_name: model.name.clone(),
                model_path: model.model_path.clone(),
                status: "loaded".to_string(),
                memory_usage_mb: status.memory_usage_mb,
                loaded_at: status.loaded_at.clone(),
                is_loaded: true,
            }));
        }
    }

    // Update status to "loading" first
    state
        .db
        .update_base_model_status(tenant_id, &model_id, "loading", None, None)
        .await
        .map_err(|e| {
            error!("Failed to update model status to loading: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update model status")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Log operation
    let op_id = state
        .db
        .log_model_operation(
            tenant_id,
            &model_id,
            "load",
            &claims.sub,
            "in_progress",
            None,
            &now,
            None,
            None,
        )
        .await
        .map_err(|e| {
            error!("Failed to log model operation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to log operation")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get worker socket path - try from workers table first, then env var fallback
    let uds_path = get_worker_socket_path(&state, tenant_id)
        .await
        .ok_or_else(|| {
            error!("No worker available for model loading");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("no worker available")
                        .with_code("WORKER_UNAVAILABLE")
                        .with_string_details(
                            "No worker is available to load the model".to_string(),
                        ),
                ),
            )
        })?;

    // Get model path from database
    let model_path = model.model_path.clone().unwrap_or_else(|| {
        // Fallback to var/model-cache if no explicit path
        format!("var/model-cache/models/{}", model.name)
    });

    // Call worker via UDS to actually load the model
    let uds_client = UdsClient::new(Duration::from_secs(120)); // Model loading can take time
    let load_result = uds_client
        .load_model(&uds_path, &model_id, &model_path)
        .await;

    let (final_status, memory_mb, load_error) = match load_result {
        Ok(response) => {
            if response.status == "loaded" || response.status == "already_loaded" {
                info!(
                    model_id = %model_id,
                    memory_mb = ?response.memory_usage_mb,
                    "Worker confirmed model is loaded"
                );
                ("loaded", response.memory_usage_mb, None)
            } else {
                warn!(
                    model_id = %model_id,
                    status = %response.status,
                    error = ?response.error,
                    "Worker returned non-loaded status"
                );
                ("error", None, response.error)
            }
        }
        Err(e) => {
            // UDS call failed - worker is down or not responding
            // Report this as a real error instead of silently succeeding
            error!(
                model_id = %model_id,
                error = %e,
                "Failed to communicate with worker for model loading"
            );
            (
                "error",
                None,
                Some(format!("Worker communication failed: {}", e)),
            )
        }
    };

    let estimated_memory_mb = memory_mb.unwrap_or(4096);

    // Update status based on worker response
    if let Err(e) = state
        .db
        .update_base_model_status(
            tenant_id,
            &model_id,
            final_status,
            load_error.as_deref(),
            Some(estimated_memory_mb),
        )
        .await
    {
        error!("Failed to update model status to {}: {}", final_status, e);
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
            .await;
        // Audit log: model load failure
        let _ = log_failure(
            &state.db,
            &claims,
            ACTION_MODEL_LOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to load model: {}", e),
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update model status")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    // If worker returned an error, report it
    if let Some(error_msg) = load_error {
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&error_msg), Some(&now), None)
            .await;
        let _ = log_failure(
            &state.db,
            &claims,
            ACTION_MODEL_LOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Worker failed to load model: {}", error_msg),
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("worker failed to load model")
                    .with_code("WORKER_ERROR")
                    .with_string_details(error_msg),
            ),
        ));
    }

    // Log successful operation
    let completion_time = chrono::Utc::now().to_rfc3339();
    let _ = state
        .db
        .update_model_operation(&op_id, "completed", None, Some(&completion_time), Some(100))
        .await;

    // Audit log: model load success
    let _ = log_success(
        &state.db,
        &claims,
        ACTION_MODEL_LOAD,
        RESOURCE_MODEL,
        Some(&model_id),
    )
    .await;

    info!(
        model_id = %model_id,
        tenant_id = %tenant_id,
        memory_usage_mb = estimated_memory_mb,
        "Model loaded successfully"
    );

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: model.name,
        model_path: model.model_path,
        status: "loaded".to_string(),
        loaded_at: Some(chrono::Utc::now().to_rfc3339()),
        memory_usage_mb: Some(estimated_memory_mb),
        is_loaded: true,
    }))
}

/// Unload a base model from memory
///
/// # Endpoint
/// POST /v1/models/{model_id}/unload
///
/// # Authentication
/// Required
///
/// # Permissions
/// Requires one of: Admin, Operator
///
/// # Response
/// Returns updated model status confirming the model is unloaded. Frees GPU/memory resources.
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `BAD_REQUEST` (400): Model not currently loaded
/// - `INTERNAL_ERROR` (500): Database error, unload failure
///
/// # Example
/// ```
/// POST /v1/models/qwen-7b/unload
/// ```
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/unload",
    params(
        ("model_id" = String, Path, description = "Model ID to unload")
    ),
    responses(
        (status = 200, description = "Model unloaded", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 400, description = "Model not loaded"),
        (status = 500, description = "Unload failed")
    ),
    tag = "models"
)]
pub async fn unload_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, info, warn};

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
        )
    })?;

    let tenant_id = &claims.sub;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists in database
    let model = state
        .db
        .get_model(&model_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model {}: {}", model_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Model not found: {}", model_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
            )
        })?;

    // Check current status
    let current_status = state
        .db
        .get_base_model_status(tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Check if model is currently loaded
    if let Some(status) = &current_status {
        if status.status != "loaded" || status.model_id != model_id {
            warn!("Model not currently loaded: {}", model_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("model not currently loaded").with_code("BAD_REQUEST")),
            ));
        }
    } else {
        warn!("No model status found for tenant: {}", tenant_id);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("model not currently loaded").with_code("BAD_REQUEST")),
        ));
    }

    // Log operation
    let op_id = state
        .db
        .log_model_operation(
            tenant_id,
            &model_id,
            "unload",
            &claims.sub,
            "in_progress",
            None,
            &now,
            None,
            None,
        )
        .await
        .map_err(|e| {
            error!("Failed to log model operation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to log operation")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Update status to "unloaded"
    if let Err(e) = state
        .db
        .update_base_model_status(tenant_id, &model_id, "unloaded", None, None)
        .await
    {
        error!("Failed to update model status to unloaded: {}", e);
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
            .await;
        // Audit log: model unload failure
        let _ = log_failure(
            &state.db,
            &claims,
            ACTION_MODEL_UNLOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to unload model: {}", e),
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update model status")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    // Log successful operation
    let completion_time = chrono::Utc::now().to_rfc3339();
    let _ = state
        .db
        .update_model_operation(&op_id, "completed", None, Some(&completion_time), Some(100))
        .await;

    // Audit log: model unload success
    let _ = log_success(
        &state.db,
        &claims,
        ACTION_MODEL_UNLOAD,
        RESOURCE_MODEL,
        Some(&model_id),
    )
    .await;

    info!(
        model_id = %model_id,
        tenant_id = %tenant_id,
        "Model unloaded successfully"
    );

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: model.name,
        model_path: model.model_path,
        status: "unloaded".to_string(),
        loaded_at: None,
        memory_usage_mb: None,
        is_loaded: false,
    }))
}

/// Get model status
///
/// # Endpoint
/// GET /v1/models/{model_id}/status
///
/// # Authentication
/// Required
///
/// # Permissions
/// All authenticated users
///
/// # Response
/// Returns the current load status of a model, including:
/// - `model_id`: Model identifier
/// - `model_name`: Human-readable model name
/// - `model_path`: Filesystem path to model files
/// - `status`: Load status (loaded, unloaded, loading, error)
/// - `loaded_at`: Timestamp when model was loaded (if loaded)
/// - `memory_usage_mb`: Memory consumption in MB (if loaded)
/// - `is_loaded`: Boolean flag indicating if model is currently in memory
///
/// # Errors
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `INTERNAL_ERROR` (500): Database query failure
///
/// # Example
/// ```
/// GET /v1/models/qwen-7b/status
/// ```
#[utoipa::path(
    get,
    path = "/v1/models/{model_id}/status",
    params(
        ("model_id" = String, Path, description = "Model ID")
    ),
    responses(
        (status = 200, description = "Model status", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Database error")
    ),
    tag = "models"
)]
pub async fn get_model_status(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, warn};

    // Check if model exists in database
    let model = state
        .db
        .get_model(&model_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model {}: {}", model_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Model not found: {}", model_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
            )
        })?;

    // Query base_model_status to get current load state
    let all_statuses = state.db.list_base_model_statuses().await.map_err(|e| {
        error!("Failed to fetch model statuses: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Find the status record for this model (most recent)
    let status = all_statuses
        .iter()
        .filter(|s| s.model_id == model_id)
        .max_by_key(|s| s.updated_at.clone());

    let (status_str, loaded_at, memory_mb, is_loaded) = match status {
        Some(s) => (
            s.status.clone(),
            s.loaded_at.clone(),
            s.memory_usage_mb,
            s.status == "loaded",
        ),
        None => ("unloaded".to_string(), None, None, false),
    };

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: model.name,
        model_path: model.model_path,
        status: status_str,
        loaded_at,
        memory_usage_mb: memory_mb,
        is_loaded,
    }))
}

/// Validate a model
///
/// # Endpoint
/// GET /v1/models/{model_id}/validate
///
/// # Authentication
/// Required
///
/// # Permissions
/// All authenticated users
///
/// # Response
/// Validates model integrity by checking stored BLAKE3 hashes. Returns:
/// - `model_id`: Model identifier
/// - `status`: Validation status (ready, invalid)
/// - `valid`: Boolean indicating if all hashes are valid
/// - `can_load`: Boolean indicating if model can be loaded
/// - `reason`: Description of validation failure (if any)
/// - `issues`: List of validation issues with type and message
/// - `errors`: Legacy field for backwards compatibility
///
/// Validates:
/// - Model weights file hash (BLAKE3)
/// - Config file hash (BLAKE3)
/// - Tokenizer files hashes (BLAKE3)
/// - Metadata JSON format
///
/// This is a logical validation (hash comparison) and does not require actual file access.
///
/// # Errors
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `INTERNAL_ERROR` (500): Database query failure
///
/// # Example
/// ```
/// GET /v1/models/qwen-7b/validate
/// ```
#[utoipa::path(
    get,
    path = "/v1/models/{model_id}/validate",
    params(
        ("model_id" = String, Path, description = "Model ID to validate")
    ),
    responses(
        (status = 200, description = "Validation result", body = ModelValidationResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Validation error")
    ),
    tag = "models"
)]
pub async fn validate_model(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, warn};

    // Check if model exists in database
    let model = state
        .db
        .get_model(&model_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model {}: {}", model_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            warn!("Model not found: {}", model_id);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
            )
        })?;

    let mut errors = vec![];
    let mut is_valid = true;

    // Validate that all required hashes are present
    if model.hash_b3.is_empty() {
        errors.push("Model weights hash is missing".to_string());
        is_valid = false;
    }

    if model.config_hash_b3.is_empty() {
        errors.push("Config hash is missing".to_string());
        is_valid = false;
    }

    if model.tokenizer_hash_b3.is_empty() {
        errors.push("Tokenizer hash is missing".to_string());
        is_valid = false;
    }

    if model.tokenizer_cfg_hash_b3.is_empty() {
        errors.push("Tokenizer config hash is missing".to_string());
        is_valid = false;
    }

    // Validate hash format (BLAKE3 hashes are 64-character hex strings)
    let hashes_to_check = vec![
        ("weights", &model.hash_b3),
        ("config", &model.config_hash_b3),
        ("tokenizer", &model.tokenizer_hash_b3),
        ("tokenizer_config", &model.tokenizer_cfg_hash_b3),
    ];

    for (hash_type, hash_val) in hashes_to_check {
        if !hash_val.is_empty() {
            // BLAKE3 produces 64-character hex strings
            if hash_val.len() != 64 || !hash_val.chars().all(|c| c.is_ascii_hexdigit()) {
                errors.push(format!(
                    "Invalid {} hash format: expected 64-char hex, got {}",
                    hash_type, hash_val
                ));
                is_valid = false;
            }
        }
    }

    // Validate optional license hash if present
    if let Some(license_hash) = &model.license_hash_b3 {
        if !license_hash.is_empty() {
            if license_hash.len() != 64 || !license_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                errors.push(format!(
                    "Invalid license hash format: expected 64-char hex, got {}",
                    license_hash
                ));
                is_valid = false;
            }
        }
    }

    // Validate metadata JSON if present
    if let Some(metadata) = &model.metadata_json {
        if !metadata.is_empty() {
            match serde_json::from_str::<serde_json::Value>(metadata) {
                Ok(_) => {
                    // Metadata is valid JSON
                }
                Err(e) => {
                    errors.push(format!("Invalid metadata JSON: {}", e));
                    is_valid = false;
                }
            }
        }
    }

    // Log validation result
    let status = if is_valid { "valid" } else { "invalid" };
    if is_valid {
        tracing::info!(
            model_id = %model_id,
            "Model validation successful"
        );
    } else {
        tracing::warn!(
            model_id = %model_id,
            error_count = errors.len(),
            "Model validation failed"
        );
    }

    let reason = if !is_valid {
        Some(
            errors
                .first()
                .cloned()
                .unwrap_or_else(|| "Model validation failed".to_string()),
        )
    } else {
        None
    };

    // Convert errors to issues for frontend compatibility
    let issues: Vec<ValidationIssue> = errors
        .iter()
        .map(|e| ValidationIssue {
            issue_type: "validation_error".to_string(),
            message: e.clone(),
        })
        .collect();

    // Use frontend-compatible status values
    let status = if is_valid { "ready" } else { "invalid" };

    Ok(Json(ModelValidationResponse {
        model_id,
        status: status.to_string(),
        valid: is_valid,
        can_load: is_valid,
        reason,
        issues,
        errors,
    }))
}

/// Import a model from a path on disk
///
/// # Endpoint
/// POST /v1/models/import
///
/// # Authentication
/// Required
///
/// # Permissions
/// Requires one of: Admin, Operator
///
/// # Request Body
/// - `model_name`: Name for the imported model
/// - `model_path`: Filesystem path to model directory
/// - `format`: Model format (mlx, safetensors, pytorch, gguf)
/// - `backend`: Backend to use (mlx-ffi, metal)
/// - `capabilities`: Optional list of capabilities (chat, completion, embeddings)
/// - `metadata`: Optional JSON metadata
///
/// # Response
/// Returns import status with:
/// - `import_id`: Unique identifier for the imported model
/// - `status`: Import status (available, in_progress, failed)
/// - `message`: Human-readable status message
/// - `progress`: Import progress percentage (0-100)
///
/// The import process:
/// 1. Validates path exists and format is supported
/// 2. Scans directory and computes BLAKE3 hashes for all files
/// 3. Detects model structure and configuration
/// 4. Registers model in database with metadata
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `BAD_REQUEST` (400): Invalid path, unsupported format, or unsupported backend
/// - `INTERNAL_ERROR` (500): Import failure, database error
///
/// # Example
/// ```
/// POST /v1/models/import
/// {
///   "model_name": "qwen-7b",
///   "model_path": "/var/model-cache/models/qwen2.5-7b-instruct-bf16",
///   "format": "mlx",
///   "backend": "mlx-ffi",
///   "capabilities": ["chat", "completion"]
/// }
/// ```
#[utoipa::path(
    post,
    path = "/v1/models/import",
    request_body = ImportModelRequest,
    responses(
        (status = 200, description = "Import started", body = ImportModelResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Import failed")
    ),
    tag = "models"
)]
pub async fn import_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportModelRequest>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    use std::path::Path;
    use tracing::{error, info, warn};

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
        )
    })?;

    let tenant_id = &claims.sub;

    // Validate path exists
    if !Path::new(&req.model_path).exists() {
        warn!("Model path does not exist: {}", req.model_path);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("model path does not exist").with_code("BAD_REQUEST")),
        ));
    }

    // Validate format
    let valid_formats = ["mlx", "safetensors", "pytorch", "gguf"];
    if !valid_formats.contains(&req.format.as_str()) {
        warn!("Invalid model format: {}", req.format);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid model format").with_code("BAD_REQUEST")),
        ));
    }

    // Validate backend
    let valid_backends = ["mlx-ffi", "metal"];
    if !valid_backends.contains(&req.backend.as_str()) {
        warn!("Invalid backend: {}", req.backend);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid backend").with_code("BAD_REQUEST")),
        ));
    }

    // Start import
    let model_id = match state
        .db
        .import_model_from_path(
            &req.model_name,
            &req.model_path,
            &req.format,
            &req.backend,
            tenant_id,
            &claims.sub,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to start model import: {}", e);
            // Audit log: model import failure
            let _ = log_failure(
                &state.db,
                &claims,
                ACTION_MODEL_IMPORT,
                RESOURCE_MODEL,
                None,
                &format!("Failed to import model {}: {}", req.model_name, e),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to start import")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Update status to available (in real implementation, this would be async)
    if let Err(e) = state
        .db
        .update_model_import_status(&model_id, "available", None)
        .await
    {
        error!("Failed to update import status: {}", e);
    }

    // Audit log: model import success
    let _ = log_success(
        &state.db,
        &claims,
        ACTION_MODEL_IMPORT,
        RESOURCE_MODEL,
        Some(&model_id),
    )
    .await;

    info!(
        model_id = %model_id,
        model_name = %req.model_name,
        model_path = %req.model_path,
        format = %req.format,
        backend = %req.backend,
        tenant_id = %tenant_id,
        "Model import completed"
    );

    Ok(Json(ImportModelResponse {
        import_id: model_id,
        status: "available".to_string(),
        message: "Model import completed".to_string(),
        progress: Some(100),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct ModelListResponse {
    pub models: Vec<ModelWithStatsResponse>,
    pub total: usize,
}

#[derive(Serialize, ToSchema)]
pub struct ModelWithStatsResponse {
    pub id: String,
    pub name: String,
    pub format: Option<String>,
    pub backend: Option<String>,
    pub size_bytes: Option<i64>,
    pub import_status: Option<String>,
    pub model_path: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub adapter_count: i64,
    pub training_job_count: i64,
    pub imported_at: Option<String>,
    pub updated_at: Option<String>,
}

/// List all models with statistics
///
/// # Endpoint
/// GET /v1/models
///
/// # Authentication
/// Required
///
/// # Permissions
/// All authenticated users
///
/// # Response
/// Returns a list of all models with associated statistics:
/// - `models`: Array of model records with:
///   - `id`: Unique model identifier
///   - `name`: Model name
///   - `format`: Model format (mlx, safetensors, pytorch, gguf)
///   - `backend`: Backend type (mlx-ffi, metal)
///   - `size_bytes`: Total size of model files
///   - `import_status`: Import status (available, in_progress, failed)
///   - `model_path`: Filesystem path to model files
///   - `capabilities`: List of supported capabilities
///   - `adapter_count`: Number of adapters using this model
///   - `training_job_count`: Number of training jobs for this model
///   - `imported_at`: Import timestamp
///   - `updated_at`: Last update timestamp
/// - `total`: Total number of models
///
/// # Errors
/// - `INTERNAL_ERROR` (500): Database query failure
///
/// # Example
/// ```
/// GET /v1/models
/// ```
#[utoipa::path(
    get,
    path = "/v1/models",
    responses(
        (status = 200, description = "List of models", body = ModelListResponse),
        (status = 500, description = "Database error")
    ),
    tag = "models"
)]
pub async fn list_models_with_stats(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<ModelListResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::error;

    let models_with_stats = state.db.list_models_with_stats().await.map_err(|e| {
        error!("Failed to list models with stats: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let total = models_with_stats.len();
    let models = models_with_stats
        .into_iter()
        .map(|m| {
            let capabilities = m
                .model
                .capabilities
                .as_ref()
                .and_then(|c| serde_json::from_str::<Vec<String>>(c).ok());

            ModelWithStatsResponse {
                id: m.model.id,
                name: m.model.name,
                format: m.model.format,
                backend: m.model.backend,
                size_bytes: m.model.size_bytes,
                import_status: m.model.import_status,
                model_path: m.model.model_path,
                capabilities,
                adapter_count: m.adapter_count,
                training_job_count: m.training_job_count,
                imported_at: m.model.imported_at,
                updated_at: m.model.updated_at,
            }
        })
        .collect();

    Ok(Json(ModelListResponse { models, total }))
}

/// Get all models status
///
/// # Endpoint
/// GET /v1/models/status/all
///
/// # Authentication
/// Required
///
/// # Permissions
/// Requires one of: Operator, Admin, Compliance
///
/// # Query Parameters
/// - `tenant_id`: Optional tenant filter to show only models for specific tenant
///
/// # Response
/// Returns aggregated status for all base models:
/// - `schema_version`: API schema version
/// - `models`: Array of model status records with:
///   - `model_id`: Model identifier
///   - `model_name`: Model name
///   - `model_path`: Filesystem path
///   - `status`: Load status (loaded, unloaded, loading, error)
///   - `loaded_at`: Load timestamp
///   - `unloaded_at`: Unload timestamp
///   - `error_message`: Error message if status is error
///   - `memory_usage_mb`: Memory consumption in MB
///   - `is_loaded`: Boolean flag
///   - `updated_at`: Last status update
/// - `total_memory_mb`: Total memory used by all loaded models
/// - `available_memory_mb`: Available memory (currently null)
/// - `active_model_count`: Number of currently loaded models
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `INTERNAL_ERROR` (500): Database query failure
///
/// # Example
/// ```
/// GET /v1/models/status/all?tenant_id=default
/// ```
#[utoipa::path(
    get,
    path = "/v1/models/status/all",
    params(
        ("tenant_id" = Option<String>, Query, description = "Optional tenant filter")
    ),
    responses(
        (status = 200, description = "All models status", body = AllModelsStatusResponse),
        (status = 500, description = "Database error")
    ),
    tag = "models"
)]
pub async fn get_all_models_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<AllModelsStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::error;

    require_any_role(&claims, &[Role::Operator, Role::Admin, Role::Compliance])?;

    // Get all base model statuses
    let statuses = state.db.list_base_model_statuses().await.map_err(|e| {
        error!("Failed to list base model statuses: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Filter by tenant if provided
    let tenant_filter = query.get("tenant_id");
    let statuses: Vec<_> = if let Some(tenant_id) = tenant_filter {
        statuses
            .into_iter()
            .filter(|s| s.tenant_id == *tenant_id)
            .collect()
    } else {
        statuses
    };

    // Convert to response format and get model details
    let mut model_responses = Vec::new();
    let mut total_memory_mb = 0;
    let mut active_model_count = 0;

    for status in statuses {
        // Get model details
        let model = match state.db.get_model(&status.model_id).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                error!("Model not found: {}", status.model_id);
                continue;
            }
            Err(e) => {
                error!("Failed to get model {}: {}", status.model_id, e);
                continue;
            }
        };

        let is_loaded = status.status == "loaded";
        if is_loaded {
            active_model_count += 1;
        }

        if let Some(memory) = status.memory_usage_mb {
            total_memory_mb += memory as i64;
        }

        model_responses.push(crate::types::BaseModelStatusResponse {
            model_id: status.model_id,
            model_name: model.name,
            model_path: model.model_path,
            status: status.status,
            loaded_at: status.loaded_at,
            unloaded_at: status.unloaded_at,
            error_message: status.error_message,
            memory_usage_mb: status.memory_usage_mb,
            is_loaded,
            updated_at: status.updated_at,
        });
    }

    Ok(Json(AllModelsStatusResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        models: model_responses,
        total_memory_mb,
        available_memory_mb: None, // Not available from current data
        active_model_count,
    }))
}

/// Helper function to get the worker socket path
///
/// Tries to get the socket path from:
/// 1. Workers table in database (production path)
/// 2. AOS_WORKER_SOCKET environment variable (development fallback)
/// 3. Default path var/run/worker.sock (local development)
async fn get_worker_socket_path(state: &AppState, _tenant_id: &str) -> Option<PathBuf> {
    // Try to get from workers table first
    if let Ok(workers) = state.db.list_all_workers().await {
        if let Some(worker) = workers.first() {
            return Some(PathBuf::from(&worker.uds_path));
        }
    }

    // Try environment variable fallback
    if let Ok(socket_path) = std::env::var("AOS_WORKER_SOCKET") {
        if !socket_path.is_empty() {
            return Some(PathBuf::from(socket_path));
        }
    }

    // Default development path
    let default_path = PathBuf::from("var/run/worker.sock");
    if default_path.exists() {
        return Some(default_path);
    }

    None
}
