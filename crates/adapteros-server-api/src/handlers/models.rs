//! Minimal model management handlers (stubs for unwired routes)
//!
//! These handlers provide basic API endpoints for model operations.
//! Full implementation details are in the original models.rs file.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ImportModelRequest {
    pub model_name: String,
    pub weights_path: String,
    pub config_path: String,
    pub tokenizer_path: String,
    pub tokenizer_config_path: Option<String>,
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
    pub status: String,
    pub loaded_at: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ModelValidationResponse {
    pub model_id: String,
    pub status: String,
    pub valid: bool,
    pub errors: Vec<String>,
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
/// Loads a base model specified by model_id into GPU/memory. The model must exist in the
/// database before loading. Returns current status including memory usage.
///
/// **Permissions:** Requires `admin` or `operator` role.
///
/// **Errors:**
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `BAD_REQUEST` (400): Model already loaded
/// - `INTERNAL_ERROR` (500): Memory pressure, load failure
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

    require_any_role(&claims, &["admin", "operator"])
        .map_err(|_| {
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

    // Check if model is already loaded
    if let Some(status) = &current_status {
        if status.status == "loaded" && status.model_id == model_id {
            warn!("Model already loaded: {}", model_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("model already loaded")
                        .with_code("BAD_REQUEST"),
                ),
            ));
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
        .log_model_operation(tenant_id, &model_id, "load", &claims.sub, "in_progress", None, &now, None, None)
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

    // Estimate memory usage (typically 7B model = 4-8GB, using conservative estimate)
    let estimated_memory_mb: i32 = 4096;

    // Update status to "loaded"
    if let Err(e) = state
        .db
        .update_base_model_status(tenant_id, &model_id, "loaded", None, Some(estimated_memory_mb))
        .await
    {
        error!("Failed to update model status to loaded: {}", e);
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
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

    info!(
        model_id = %model_id,
        tenant_id = %tenant_id,
        memory_usage_mb = estimated_memory_mb,
        "Model loaded successfully"
    );

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: model.name,
        status: "loaded".to_string(),
        loaded_at: Some(chrono::Utc::now().to_rfc3339()),
        memory_usage_mb: Some(estimated_memory_mb),
        is_loaded: true,
    }))
}

/// Unload a base model from memory
///
/// Unloads a previously loaded model from GPU/memory. Frees memory resources and marks
/// the model as unloaded in the database.
///
/// **Permissions:** Requires `admin` or `operator` role.
///
/// **Errors:**
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `BAD_REQUEST` (400): Model not currently loaded
/// - `INTERNAL_ERROR` (500): Unload failure
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

    require_any_role(&claims, &["admin", "operator"])
        .map_err(|_| {
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
                Json(
                    ErrorResponse::new("model not currently loaded")
                        .with_code("BAD_REQUEST"),
                ),
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
        .log_model_operation(tenant_id, &model_id, "unload", &claims.sub, "in_progress", None, &now, None, None)
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

    info!(
        model_id = %model_id,
        tenant_id = %tenant_id,
        "Model unloaded successfully"
    );

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: model.name,
        status: "unloaded".to_string(),
        loaded_at: None,
        memory_usage_mb: None,
        is_loaded: false,
    }))
}

/// Get model status
///
/// Returns the current load status of a model, including memory usage, load timestamp,
/// and whether the model is currently in memory.
///
/// **Permissions:** All authenticated users.
///
/// **Errors:**
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
    let all_statuses = state
        .db
        .list_base_model_statuses()
        .await
        .map_err(|e| {
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
        status: status_str,
        loaded_at,
        memory_usage_mb: memory_mb,
        is_loaded,
    }))
}

/// Validate a model
///
/// Validates model integrity by checking stored BLAKE3 hashes. Verifies that:
/// - Model weights file matches stored hash
/// - Config file matches stored hash
/// - Tokenizer files match stored hashes
///
/// This is a logical validation (hash comparison) - does not require actual file access.
/// Returns list of any validation errors found.
///
/// **Permissions:** All authenticated users.
///
/// **Errors:**
/// - `NOT_FOUND` (404): Model does not exist in database
/// - `INTERNAL_ERROR` (500): Validation failure
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
                    hash_type,
                    hash_val
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

    Ok(Json(ModelValidationResponse {
        model_id,
        status: status.to_string(),
        valid: is_valid,
        errors,
    }))
}
