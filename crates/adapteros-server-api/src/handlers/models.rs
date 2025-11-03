//! Base Model Management Handlers
//!
//! Provides API endpoints for model import, loading, and status management.
//!
//! # Citations
//! - CONTRIBUTING.md L123: Use `tracing` for logging
//! - Policy Pack #9 (Telemetry): Emit structured JSON events
//! - Policy Pack #8 (Isolation): Per-tenant operations with UID/GID checks
//! - Handler pattern from handlers.rs L4567-4597

use crate::{
    auth::Claims,
    state::AppState,
    types::{ErrorResponse, ModelValidationResponse},
};
use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportModelRequest {
    pub model_name: String,
    pub weights_path: String,
    pub config_path: String,
    pub tokenizer_path: String,
    pub tokenizer_config_path: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    pub progress: Option<i32>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    pub status: String,
    pub loaded_at: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CursorConfigResponse {
    pub api_endpoint: String,
    pub model_name: String,
    pub model_id: String,
    pub is_ready: bool,
    pub setup_instructions: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelDownloadArtifact {
    pub artifact: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: Option<u64>,
    pub download_url: String,
    pub expires_at: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelDownloadResponse {
    pub model_id: String,
    pub model_name: String,
    pub artifacts: Vec<ModelDownloadArtifact>,
}

/// Import a new base model
///
/// # Citation
/// - Handler pattern from handlers.rs L4567-4597
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/import",
    request_body = ImportModelRequest,
    responses(
        (status = 200, description = "Import started", body = ImportModelResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "models"
))]
pub async fn import_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportModelRequest>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for model import
    // Citation: CONTRIBUTING.md L132 - Security-sensitive code requires review
    if claims.role != "admin" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;
    let import_id = Uuid::new_v4().to_string();

    // Validate file paths exist
    let weights_exists = std::path::Path::new(&req.weights_path).exists();
    let config_exists = std::path::Path::new(&req.config_path).exists();
    let tokenizer_exists = std::path::Path::new(&req.tokenizer_path).exists();

    if !weights_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("weights file not found").with_code("INVALID_PATH")),
        ));
    }
    if !config_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("config file not found").with_code("INVALID_PATH")),
        ));
    }
    if !tokenizer_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("tokenizer file not found").with_code("INVALID_PATH")),
        ));
    }

    // Create import record
    let now = chrono::Utc::now().to_rfc3339();
    let metadata_str = req.metadata.as_ref().map(|m| m.to_string());
    let tokenizer_config = req.tokenizer_config_path.as_deref();

    sqlx::query!(
        r#"
        INSERT INTO base_model_imports 
        (id, tenant_id, model_name, weights_path, config_path, tokenizer_path, 
         tokenizer_config_path, status, started_at, created_by, metadata_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, 'validating', ?, ?, ?)
        "#,
        import_id,
        tenant_id,
        req.model_name,
        req.weights_path,
        req.config_path,
        req.tokenizer_path,
        tokenizer_config,
        now,
        claims.sub,
        metadata_str
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to create import record: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    // Note: The import process should eventually:
    // 1. Validate files and compute hashes
    // 2. Register model in 'models' table using db.register_model()
    // 3. Create base_model_status record with status 'unloaded' for the tenant
    // 4. Update import status to 'completed'
    //
    // For now, we check if a model with this name already exists and ensure
    // base_model_status record exists. This ensures models can be loaded even
    // if import completion logic runs elsewhere.
    let existing_model = sqlx::query!(
        "SELECT id FROM models WHERE name = ? LIMIT 1",
        req.model_name
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        warn!("Failed to check for existing model: {}", e);
        // Don't fail import if this check fails
    })
    .ok()
    .flatten();

    if let Some(model) = existing_model {
        // Ensure base_model_status record exists for this tenant
        let status_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM base_model_status WHERE model_id = ? AND tenant_id = ?",
        )
        .bind(&model.id)
        .bind(tenant_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            warn!("Failed to check base_model_status: {}", e);
        })
        .unwrap_or(0);

        if status_exists == 0 {
            // Create base_model_status record
            let _ = sqlx::query!(
                "INSERT INTO base_model_status (tenant_id, model_id, status, import_id) VALUES (?, ?, 'unloaded', ?)",
                tenant_id,
                model.id,
                import_id
            )
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                warn!("Failed to create base_model_status record: {}", e);
            });
        }
    }

    // Emit telemetry event
    // Citation: Policy Pack #9 (Telemetry)
    info!(
        event = "model.import.started",
        import_id = %import_id,
        model_name = %req.model_name,
        tenant_id = %tenant_id,
        user_id = %claims.sub,
        "Model import started"
    );

    // Track onboarding journey step
    let _ = track_journey_step(&state, tenant_id, &claims.sub, "model_imported").await;

    Ok(Json(ImportModelResponse {
        import_id,
        status: "validating".to_string(),
        message: format!("Import started for model: {}", req.model_name),
        progress: Some(0),
    }))
}

/// Load a base model into memory
///
/// # Citation
/// - Pattern from handlers.rs L4567-4630 (load_adapter)
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/{model_id}/load",
    params(
        ("model_id" = String, Path, description = "Model ID to load")
    ),
    responses(
        (status = 200, description = "Model loaded", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Load failed")
    ),
    tag = "models"
))]
pub async fn load_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Check if model exists
    let model_check = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name FROM base_model_status bms
         JOIN models m ON bms.model_id = m.id
         WHERE bms.model_id = ? AND bms.tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model: {}", e);
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    let model_name = if let Some(row) = model_check {
        row.model_name
    } else {
        let technical_msg = format!("Model '{}' not found in database for tenant '{}'. Import the model first using POST /v1/models/import", model_id, tenant_id);
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", &technical_msg)),
        ));
    };

    // Start operation tracking
    state.operation_tracker.start_operation(&model_id, tenant_id, crate::operation_tracker::OperationType::Model(crate::operation_tracker::ModelOperationType::Load)).await
        .map_err(|e| {
            error!("Failed to start operation tracking: {:?}", e);
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse::new_user_friendly("OPERATION_IN_PROGRESS", "Another operation is already in progress for this model")),
            )
        })?;

    // Update status to loading
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE base_model_status SET status = 'loading', updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        now,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update model status: {}", e);
        // Note: Operation tracking cleanup skipped in error handler (already async context)
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    // Load model into runtime (if available)
    let load_result = if let Some(rt) = &state.model_runtime {
        #[cfg(feature = "mlx-ffi-backend")]
        {
            match std::env::var("AOS_MLX_FFI_MODEL") {
                Ok(model_path) => {
                    if !std::path::Path::new(&model_path).exists() {
                        Err(format!(
                            "AOS_MLX_FFI_MODEL path does not exist: {}. Verify the path is correct.",
                            model_path
                        ))
                    } else {
                        // Add retry logic for transient failures during model loading
                        let mut attempts = 0;
                        let max_attempts = 3;
                        let base_delay = std::time::Duration::from_millis(500);

                        loop {
                            attempts += 1;

                            let mut guard = rt.lock().await;
                            match guard.load_model(tenant_id, &model_id, &model_path) {
                                Ok(()) => break Ok(()),
                                Err(e) => {
                                    if attempts >= max_attempts {
                                        let technical_msg = format!("Model loading failed after {} attempts: {}", max_attempts, e);
                                        break Err(technical_msg);
                                    }

                                    // Check if this is a retryable error
                                    if e.contains("temporarily") || e.contains("timeout") || e.contains("busy") {
                                        warn!("Model loading attempt {} failed, retrying in {:?}: {}", attempts, base_delay, e);
                                        tokio::time::sleep(base_delay).await;
                                        continue;
                                    } else {
                                        // Not a retryable error, fail immediately
                                        break Err(e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => Err(
                    "AOS_MLX_FFI_MODEL environment variable not set. Set this to the path of your MLX model directory.".to_string()
                ),
            }
        }
        #[cfg(not(feature = "mlx-ffi-backend"))]
        {
            Err("mlx-ffi-backend feature not enabled. Rebuild with --features mlx-ffi-backend to enable model loading.".to_string())
        }
    } else {
        Err(
            "Model runtime not available. This should not happen - please report this error."
                .to_string(),
        )
    };

    // Handle load result - only mark as loaded if successful
    match load_result {
        Ok(()) => {
            // Update to loaded state
            let loaded_at = chrono::Utc::now().to_rfc3339();
            let memory_mb: i32 = 8192; // TODO: Get actual memory usage

            sqlx::query!(
                "UPDATE base_model_status SET status = 'loaded', loaded_at = ?, memory_usage_mb = ?, updated_at = ? WHERE model_id = ? AND tenant_id = ?",
                loaded_at,
                memory_mb,
                loaded_at,
                model_id,
                tenant_id
            )
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                error!("Failed to update loaded status: {}", e);
                let technical_msg = format!("{}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
                )
            })?;

            // Complete operation tracking
            let _ = state.operation_tracker.complete_operation(&model_id, tenant_id, crate::operation_tracker::OperationType::Model(crate::operation_tracker::ModelOperationType::Load), true).await;

            // Emit telemetry
            info!(
                event = "model.load",
                model_id = %model_id,
                tenant_id = %tenant_id,
                memory_mb = %memory_mb,
                "Base model loaded"
            );

            // Track onboarding journey step
            let _ = track_journey_step(&state, tenant_id, &claims.sub, "model_loaded").await;

            Ok(Json(ModelStatusResponse {
                model_id: model_id.clone(),
                model_name,
                status: "loaded".to_string(),
                loaded_at: Some(loaded_at),
                memory_usage_mb: Some(memory_mb),
                is_loaded: true,
            }))
        }
        Err(e) => {
            // Mark as error state
            // Complete operation tracking with failure
            let _ = state.operation_tracker.complete_operation(&model_id, tenant_id, crate::operation_tracker::OperationType::Model(crate::operation_tracker::ModelOperationType::Load), false).await;

            error!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                error = %e,
                "Model load failed"
            );

            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query!(
                "UPDATE base_model_status SET status = 'error', updated_at = ? WHERE model_id = ? AND tenant_id = ?",
                now,
                model_id,
                tenant_id
            )
            .execute(state.db.pool())
            .await
            .map_err(|db_err| {
                error!("Failed to update error status: {}", db_err);
                let technical_msg = format!("{}", db_err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
                )
            })?;

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new_user_friendly("LOAD_FAILED", &e)),
            ))
        }
    }
}

/// Unload a base model from memory
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/{model_id}/unload",
    params(
        ("model_id" = String, Path, description = "Model ID to unload")
    ),
    responses(
        (status = 200, description = "Model unloaded"),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Unload failed")
    ),
    tag = "models"
))]
pub async fn unload_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists
    let exists = sqlx::query!(
        "SELECT id FROM base_model_status WHERE model_id = ? AND tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model: {}", e);
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    if exists.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", "Model not found")),
        ));
    }

    // Start operation tracking
    state.operation_tracker.start_operation(&model_id, tenant_id, crate::operation_tracker::OperationType::Model(crate::operation_tracker::ModelOperationType::Unload)).await
        .map_err(|e| {
            error!("Failed to start operation tracking: {:?}", e);
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse::new_user_friendly("OPERATION_IN_PROGRESS", "Another operation is already in progress for this model")),
            )
        })?;

    // Update to unloading state
    sqlx::query!(
        "UPDATE base_model_status SET status = 'unloading', updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        now,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update status: {}", e);
        // Note: Operation tracking cleanup skipped in error handler (already async context)
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    // Unload from runtime (if available)
    if let Some(rt) = &state.model_runtime {
        let mut guard = rt.lock().await;
        if let Err(e) = guard.unload_model(tenant_id, &model_id) {
            warn!("Runtime unload failed: {}", e);
        }
    }

    // Update to unloaded
    let unloaded_at = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE base_model_status SET status = 'unloaded', unloaded_at = ?, loaded_at = NULL, memory_usage_mb = NULL, updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        unloaded_at,
        unloaded_at,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update unloaded status: {}", e);
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    // Complete operation tracking
    let _ = state.operation_tracker.complete_operation(&model_id, tenant_id, crate::operation_tracker::OperationType::Model(crate::operation_tracker::ModelOperationType::Unload), true).await;

    info!(
        event = "model.unload",
        model_id = %model_id,
        tenant_id = %tenant_id,
        "Base model unloaded"
    );

    Ok(StatusCode::OK)
}

/// Cancel a model operation (load/unload)
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/{model_id}/cancel",
    params(
        ("model_id" = String, Path, description = "Model ID to cancel operation for")
    ),
    responses(
        (status = 200, description = "Operation cancelled successfully"),
        (status = 404, description = "No ongoing operation found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "models"
))]
pub async fn cancel_model_operation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Attempt to cancel the operation
    match state.operation_tracker.cancel_model_operation(&model_id, tenant_id).await {
        Ok(()) => {
            info!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                "Successfully cancelled model operation"
            );
            Ok(StatusCode::OK)
        }
        Err(crate::operation_tracker::OperationCancellationError::OperationNotFound) => {
            let technical_msg = format!("No ongoing operation found for model '{}' in tenant '{}'", model_id, tenant_id);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new_user_friendly("NOT_FOUND", &technical_msg)),
            ))
        }
        Err(_) => {
            let technical_msg = "Failed to cancel operation";
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new_user_friendly("INTERNAL_ERROR", technical_msg)),
            ))
        }
    }
}

/// Get import status
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/imports/{import_id}",
    params(
        ("import_id" = String, Path, description = "Import ID")
    ),
    responses(
        (status = 200, description = "Import status", body = ImportModelResponse),
        (status = 404, description = "Import not found")
    ),
    tag = "models"
))]
pub async fn get_import_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(import_id): Path<String>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    let import = sqlx::query!(
        "SELECT status, model_name, progress, error_message FROM base_model_imports WHERE id = ? AND tenant_id = ?",
        import_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to get import status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    let import = import.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("import not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(ImportModelResponse {
        import_id,
        status: import.status,
        message: import
            .error_message
            .unwrap_or_else(|| format!("Import in progress for {}", import.model_name)),
        progress: import.progress.map(|p| p as i32),
    }))
}

/// Get status of a specific model
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/{model_id}/status",
    params(
        ("model_id" = String, Path, description = "Model ID to get status for")
    ),
    responses(
        (status = 200, description = "Model status", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "models"
))]
pub async fn get_model_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Check if model exists and get its status
    let model_status = sqlx::query!(
        "SELECT bms.status, bms.loaded_at, bms.memory_usage_mb, m.name as model_name
         FROM base_model_status bms
         JOIN models m ON bms.model_id = m.id
         WHERE bms.model_id = ? AND bms.tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to get model status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    match model_status {
        Some(row) => {
            let is_loaded = row.status == "loaded";
            Ok(Json(ModelStatusResponse {
                model_id: model_id.clone(),
                model_name: row.model_name,
                status: row.status,
                loaded_at: row.loaded_at,
                memory_usage_mb: row.memory_usage_mb.map(|v| v as i32),
                is_loaded,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", "model not found or not loaded")),
        )),
    }
}

/// Download model artifacts
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/{model_id}/download",
    params(
        ("model_id" = String, Path, description = "Model ID to download")
    ),
    responses(
        (status = 200, description = "Model download info", body = ModelDownloadResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Download failed")
    ),
    tag = "models"
))]
pub async fn download_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelDownloadResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for downloads
    if claims.role != "admin" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Ensure tenant has access to the requested model
    let model_info = sqlx::query!(
        "SELECT m.name FROM base_model_status bms
         JOIN models m ON bms.model_id = m.id
         WHERE bms.model_id = ? AND bms.tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to get model info: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    let model_info = model_info.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", "model not found or access denied")),
        )
    })?;

    // Locate the latest completed import for this model
    let import_record = sqlx::query!(
        r#"
        SELECT id, weights_path, config_path, tokenizer_path, tokenizer_config_path
        FROM base_model_imports
        WHERE tenant_id = ? AND model_name = ? AND status = 'completed'
        ORDER BY completed_at DESC, started_at DESC
        LIMIT 1
        "#,
        tenant_id,
        model_info.name
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to fetch model import record: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    let import_record = import_record.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", "no completed imports found for model")),
        )
    })?;

    let mut artifacts = Vec::new();

    if let Some(artifact) = prepare_artifact_descriptor(
        &state,
        &claims,
        &model_id,
        tenant_id,
        "weights",
        Some(&import_record.weights_path),
    )
    .await?
    {
        artifacts.push(artifact);
    }

    if let Some(artifact) = prepare_artifact_descriptor(
        &state,
        &claims,
        &model_id,
        tenant_id,
        "config",
        Some(&import_record.config_path),
    )
    .await?
    {
        artifacts.push(artifact);
    }

    if let Some(artifact) = prepare_artifact_descriptor(
        &state,
        &claims,
        &model_id,
        tenant_id,
        "tokenizer",
        Some(&import_record.tokenizer_path),
    )
    .await?
    {
        artifacts.push(artifact);
    }

    if let Some(tokenizer_cfg_path) = import_record.tokenizer_config_path.as_ref() {
        if let Some(artifact) = prepare_artifact_descriptor(
            &state,
            &claims,
            &model_id,
            tenant_id,
            "tokenizer_config",
            Some(tokenizer_cfg_path),
        )
        .await?
        {
            artifacts.push(artifact);
        }
    }

    if artifacts.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new_user_friendly("NOT_FOUND", "no downloadable artifacts available")),
        ));
    }

    info!(
        event = "model.download.requested",
        model_id = %model_id,
        tenant_id = %tenant_id,
        user_id = %claims.sub,
        artifact_count = artifacts.len(),
        "Model download descriptors generated"
    );

    Ok(Json(ModelDownloadResponse {
        model_id,
        model_name: model_info.name,
        artifacts,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelArtifactTokenClaims {
    sub: String,
    tenant_id: String,
    model_id: String,
    artifact: String,
    path: String,
    filename: String,
    content_type: String,
    exp: usize,
    iat: usize,
}

async fn prepare_artifact_descriptor(
    state: &AppState,
    claims: &Claims,
    model_id: &str,
    tenant_id: &str,
    artifact: &str,
    path: Option<&str>,
) -> Result<Option<ModelDownloadArtifact>, (StatusCode, Json<ErrorResponse>)> {
    let path = match path {
        Some(path) if !path.trim().is_empty() => path,
        _ => return Ok(None),
    };

    let canonical_path = match fs::canonicalize(path).await {
        Ok(p) => p,
        Err(e) => {
            warn!(
                error = %e,
                artifact,
                model_id,
                tenant_id,
                "Skipping artifact download descriptor; canonicalize failed"
            );
            return Ok(None);
        }
    };

    let metadata = match fs::metadata(&canonical_path).await {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => {
            warn!(
                artifact,
                model_id, tenant_id, "Skipping artifact download descriptor; path is not a file"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(
                error = %e,
                artifact,
                model_id,
                tenant_id,
                "Skipping artifact download descriptor; metadata lookup failed"
            );
            return Ok(None);
        }
    };

    let filename = canonical_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{artifact}-{model_id}.bin"));

    let content_type = infer_artifact_content_type(artifact);
    let canonical_str = canonical_path.to_string_lossy().to_string();

    let (token, expires_at) = generate_download_token(
        state,
        claims,
        tenant_id,
        model_id,
        artifact,
        &canonical_str,
        &filename,
        content_type,
    )?;

    Ok(Some(ModelDownloadArtifact {
        artifact: artifact.to_string(),
        filename,
        content_type: content_type.to_string(),
        size_bytes: Some(metadata.len()),
        download_url: format!("/v1/models/download/{}", token),
        expires_at,
    }))
}

fn infer_artifact_content_type(artifact: &str) -> &'static str {
    match artifact {
        "config" | "tokenizer" | "tokenizer_config" => "application/json",
        _ => "application/octet-stream",
    }
}

fn generate_download_token(
    state: &AppState,
    claims: &Claims,
    tenant_id: &str,
    model_id: &str,
    artifact: &str,
    path: &str,
    filename: &str,
    content_type: &str,
) -> Result<(String, String), (StatusCode, Json<ErrorResponse>)> {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::minutes(10);

    let token_claims = ModelArtifactTokenClaims {
        sub: claims.sub.clone(),
        tenant_id: tenant_id.to_string(),
        model_id: model_id.to_string(),
        artifact: artifact.to_string(),
        path: path.to_string(),
        filename: filename.to_string(),
        content_type: content_type.to_string(),
        iat: issued_at.timestamp() as usize,
        exp: expires_at.timestamp() as usize,
    };

    let mut header = Header::new(Algorithm::HS256);
    header.typ = Some("JWT".to_string());

    let token = encode(
        &header,
        &token_claims,
        &EncodingKey::from_secret(state.jwt_secret.as_slice()),
    )
    .map_err(|e| {
        error!(
            error = %e,
            artifact,
            model_id,
            tenant_id,
            "Failed to generate model artifact download token"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("TOKEN_ERROR", "failed to generate download token")),
        )
    })?;

    Ok((token, expires_at.to_rfc3339()))
}

/// Stream a model artifact referenced by a signed token
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/download/{token}",
    params(("token" = String, Path, description = "Signed artifact download token")),
    responses(
        (status = 200, description = "Artifact stream"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Artifact not found")
    ),
    tag = "models"
))]
pub async fn download_model_artifact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(token): Path<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "admin role required")),
        ));
    }

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_nbf = false;

    let decoded = decode::<ModelArtifactTokenClaims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_slice()),
        &validation,
    )
    .map_err(|e| {
        warn!(error = %e, "Invalid or expired model artifact token");
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("invalid download token").with_code("UNAUTHORIZED")),
        )
    })?;

    let token_claims = decoded.claims;

    if token_claims.tenant_id != claims.tenant_id || token_claims.sub != claims.sub {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("token does not match requester").with_code("UNAUTHORIZED")),
        ));
    }

    let metadata = fs::metadata(&token_claims.path).await.map_err(|e| {
        warn!(
            error = %e,
            path = %token_claims.path,
            "Artifact metadata lookup failed"
        );
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("artifact not found").with_code("NOT_FOUND")),
        )
    })?;

    if !metadata.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("artifact not found").with_code("NOT_FOUND")),
        ));
    }

    let file = fs::File::open(&token_claims.path).await.map_err(|e| {
        warn!(
            error = %e,
            path = %token_claims.path,
            "Artifact open failed"
        );
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("artifact not found").with_code("NOT_FOUND")),
        )
    })?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let safe_filename = token_claims.filename.replace('"', "'");
    let content_disposition = format!("attachment; filename=\"{}\"", safe_filename);
    let disposition_header = HeaderValue::from_str(&content_disposition)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));

    let content_type = HeaderValue::from_str(&token_claims.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

    let content_length = HeaderValue::from_str(&metadata.len().to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("0"));

    let response = (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition_header),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::CONTENT_LENGTH, content_length),
        ],
        body,
    )
        .into_response();

    info!(
        event = "model.download.stream",
        model_id = %token_claims.model_id,
        tenant_id = %token_claims.tenant_id,
        user_id = %claims.sub,
        artifact = %token_claims.artifact,
        "Streaming model artifact"
    );

    Ok(response)
}

/// Get model diagnostics for troubleshooting
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/diagnostics",
    responses(
        (status = 200, description = "Model diagnostics", body = crate::types::ModelDiagnosticsResponse),
        (status = 500, description = "Failed to get diagnostics")
    ),
    tag = "models"
))]
pub async fn get_model_diagnostics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<crate::types::ModelDiagnosticsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    // Check feature flag
    let mlx_ffi_backend_enabled = cfg!(feature = "mlx-ffi-backend");

    // Check environment variable
    let aos_mlx_ffi_model_env = std::env::var("AOS_MLX_FFI_MODEL").ok();
    let aos_mlx_ffi_model_path_exists = aos_mlx_ffi_model_env
        .as_ref()
        .map(|p| std::path::Path::new(p).exists());

    // Check model runtime availability
    let model_runtime_available = state.model_runtime.is_some();

    // Query database for models
    let (database_models_count, database_model_ids) = sqlx::query!(
        "SELECT bms.model_id FROM base_model_status bms WHERE bms.tenant_id = ?",
        tenant_id
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to query database models: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })
    .map(|rows| {
        let ids: Vec<String> = rows.into_iter().map(|r| r.model_id).collect();
        (ids.len() as i64, ids)
    })?;

    // Generate summary
    let mut issues = Vec::new();
    let mut ok_items = Vec::new();

    if !mlx_ffi_backend_enabled {
        issues.push("mlx-ffi-backend feature not enabled".to_string());
    } else {
        ok_items.push("mlx-ffi-backend feature enabled".to_string());
    }

    if aos_mlx_ffi_model_env.is_none() {
        issues.push("AOS_MLX_FFI_MODEL environment variable not set".to_string());
    } else {
        ok_items.push("AOS_MLX_FFI_MODEL environment variable set".to_string());
        if let Some(exists) = aos_mlx_ffi_model_path_exists {
            if !exists {
                issues.push("AOS_MLX_FFI_MODEL path does not exist".to_string());
            } else {
                ok_items.push("AOS_MLX_FFI_MODEL path exists".to_string());
            }
        }
    }

    if !model_runtime_available {
        issues.push("Model runtime not available".to_string());
    } else {
        ok_items.push("Model runtime available".to_string());
    }

    if database_models_count == 0 {
        issues.push("No models found in database - import a model first".to_string());
    } else {
        ok_items.push(format!(
            "{} model(s) found in database",
            database_models_count
        ));
    }

    let summary = if issues.is_empty() {
        format!("All checks passed. {}", ok_items.join(", "))
    } else {
        format!(
            "Issues found: {}. {}",
            issues.join(", "),
            ok_items.join(", ")
        )
    };

    Ok(Json(crate::types::ModelDiagnosticsResponse {
        mlx_ffi_backend_enabled,
        aos_mlx_ffi_model_env,
        aos_mlx_ffi_model_path_exists,
        model_runtime_available,
        database_models_count,
        database_model_ids,
        summary,
    }))
}

/// Validate if a model can be loaded without actually loading it
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/{model_id}/validate",
    params(
        ("model_id" = String, Path, description = "Model ID to validate")
    ),
    responses(
        (status = 200, description = "Model validation result", body = ModelValidationResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Validation failed")
    ),
    tag = "models"
))]
pub async fn validate_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    // Check if model exists in database
    let model_check = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name FROM base_model_status bms
         JOIN models m ON bms.model_id = m.id
         WHERE bms.model_id = ? AND bms.tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    let (model_name, can_load, reason, download_commands) = if let Some(row) = model_check {
        // Model exists in database, now check if it can be loaded
        // Check if model runtime is available
        let (can_load, reason, download_commands) = if state.model_runtime.is_none() {
            (
                false,
                Some("Model runtime not available".to_string()),
                Some(vec![
                    "cargo build --release --features mlx-ffi-backend".to_string(),
                    "export AOS_MLX_FFI_MODEL=/path/to/your/model".to_string(),
                ]),
            )
        } else {
            // Check if MLX model path exists
            #[cfg(feature = "mlx-ffi-backend")]
            {
                match std::env::var("AOS_MLX_FFI_MODEL") {
                    Ok(model_path) => {
                        if !std::path::Path::new(&model_path).exists() {
                            (
                                false,
                                Some(format!("Model path does not exist: {}", model_path)),
                                Some(vec![
                                    format!(
                                        "mkdir -p {}",
                                        std::path::Path::new(&model_path)
                                            .parent()
                                            .unwrap_or(std::path::Path::new("/tmp"))
                                            .display()
                                    ),
                                    format!("# Download your model to: {}", model_path),
                                    "# For example, using huggingface-hub:".to_string(),
                                    format!(
                                        "huggingface-cli download {} --local-dir {}",
                                        row.model_name, model_path
                                    ),
                                    "# Or using git-lfs for large models:".to_string(),
                                    format!(
                                        "git lfs clone https://huggingface.co/{} {}",
                                        row.model_name, model_path
                                    ),
                                ]),
                            )
                        } else {
                            (true, None, None)
                        }
                    }
                    Err(_) => (
                        false,
                        Some("AOS_MLX_FFI_MODEL environment variable not set".to_string()),
                        Some(vec![
                            "export AOS_MLX_FFI_MODEL=/path/to/your/model/directory".to_string(),
                            "# Example: export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b".to_string(),
                        ]),
                    ),
                }
            }
            #[cfg(not(feature = "mlx-ffi-backend"))]
            {
                (
                    false,
                    Some("mlx-ffi-backend feature not enabled".to_string()),
                    Some(vec![
                        "cargo build --release --features mlx-ffi-backend".to_string()
                    ]),
                )
            }
        };

        (row.model_name, can_load, reason, download_commands)
    } else {
        // Model doesn't exist in database
        (format!("unknown-model-{}", model_id), false, Some(format!("Model '{}' not found in database for tenant '{}'", model_id, tenant_id)), Some(vec![
            format!("# Import the model first using the web UI or API"),
            format!("POST /v1/models/import with model_name: {}", model_id),
            "# Or use the command line:".to_string(),
            format!("curl -X POST http://localhost:8080/api/v1/models/import -H 'Content-Type: application/json' -d '{{\"model_name\":\"{}\"}}'", model_id),
        ]))
    };

    Ok(Json(ModelValidationResponse {
        model_id: model_id.clone(),
        model_name,
        can_load,
        reason,
        download_commands,
    }))
}

/// Get Cursor IDE configuration
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/cursor-config",
    responses(
        (status = 200, description = "Cursor configuration", body = CursorConfigResponse),
        (status = 500, description = "Failed to get config")
    ),
    tag = "models"
))]
pub async fn get_cursor_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CursorConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    // Check if model is loaded
    let status = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name, bms.status FROM base_model_status bms 
         JOIN models m ON bms.model_id = m.id 
         WHERE bms.tenant_id = ? ORDER BY bms.updated_at DESC LIMIT 1",
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &e.to_string())),
        )
    })?;

    let (model_id, model_name, is_ready) = if let Some(row) = status {
        (row.model_id, row.model_name, row.status == "loaded")
    } else {
        ("unknown".to_string(), "no-model".to_string(), false)
    };

    let api_endpoint = "http://127.0.0.1:8080/api/v1/chat/completions".to_string();
    let model_display_name = format!("adapteros-{}", model_name);

    Ok(Json(CursorConfigResponse {
        api_endpoint: api_endpoint.clone(),
        model_name: model_display_name.clone(),
        model_id: model_id.clone(),
        is_ready,
        setup_instructions: vec![
            "1. Open Cursor IDE Settings (Cmd+, or Ctrl+,)".to_string(),
            "2. Navigate to the 'Models' section".to_string(),
            format!("3. Add custom endpoint: {}", api_endpoint),
            format!("4. Set model name: {}", model_display_name),
            "5. Save settings and test the connection".to_string(),
            "6. Try a code completion or chat to verify".to_string(),
        ],
    }))
}

// Helper function to track onboarding journey
async fn track_journey_step(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    step: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query!(
        r#"
        INSERT INTO onboarding_journeys (id, tenant_id, user_id, journey_type, step_completed, completed_at)
        VALUES (?, ?, ?, 'cursor_integration', ?, ?)
        "#,
        id,
        tenant_id,
        user_id,
        step,
        now
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        warn!("Failed to track journey step: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("journey tracking failed").with_code("TRACKING_ERROR")),
        )
    })?;

    info!(
        event = "journey.step_completed",
        tenant_id = %tenant_id,
        user_id = %user_id,
        step = %step,
        "Onboarding journey step completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_request_serialization() {
        let req = ImportModelRequest {
            model_name: "test-model".to_string(),
            weights_path: "/path/to/weights.safetensors".to_string(),
            config_path: "/path/to/config.json".to_string(),
            tokenizer_path: "/path/to/tokenizer.json".to_string(),
            tokenizer_config_path: None,
            metadata: None,
        };

        assert_eq!(req.model_name, "test-model");
    }

/// Cancel a model operation (load/unload)
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/cancel",
    params(
        ("model_id" = String, Path, description = "Model ID")
    ),
    responses(
        (status = 200, description = "Operation cancelled successfully"),
        (status = 404, description = "Operation not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "models"
)]
pub async fn cancel_model_operation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Use operation tracker to cancel the operation
    match state.operation_tracker.cancel_model_operation(&model_id, tenant_id).await {
        Ok(()) => Ok(StatusCode::OK),
        Err(crate::operation_tracker::OperationCancellationError::NotFound) => {
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("operation not found").with_code("NOT_FOUND")),
            ))
        }
        Err(crate::operation_tracker::OperationCancellationError::NotCancellable) => {
            Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse::new("operation cannot be cancelled").with_code("NOT_CANCELLABLE")),
            ))
        }
        Err(_) => {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("failed to cancel operation").with_code("INTERNAL_ERROR")),
            ))
        }
    }
}

/// Get model runtime health status
#[utoipa::path(
    get,
    path = "/v1/models/health",
    responses(
        (status = 200, description = "Health check response", body = ModelRuntimeHealthResponse),
        (status = 500, description = "Health check failed")
    ),
    tag = "models"
)]
pub async fn model_runtime_health(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ModelRuntimeHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new_user_friendly("UNAUTHORIZED", "operator or admin role required")),
        ));
    }

    let Some(rt) = &state.model_runtime else {
        return Ok(Json(ModelRuntimeHealthResponse {
            status: "unhealthy".to_string(),
            total_models: 0,
            loaded_count: 0,
            inconsistencies: vec![],
            checked_at: chrono::Utc::now().to_rfc3339(),
        }));
    };

    // Get all models from database
    let db_models = sqlx::query!(
        "SELECT tenant_id, model_id, status FROM base_model_status"
    )
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to query model states: {}", e);
        let technical_msg = format!("{}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new_user_friendly("DB_ERROR", &technical_msg)),
        )
    })?;

    // Get all loaded models from runtime
    let guard = rt.lock().await;
    let runtime_models = guard.get_all_loaded_models();
    drop(guard);

    let mut inconsistencies = Vec::new();
    let mut runtime_model_set: std::collections::HashSet<(String, String)> = runtime_models.iter().cloned().collect();

    // Check each DB model
    for db_model in &db_models {
        let tenant_id = &db_model.tenant_id;
        let model_id = &db_model.model_id;
        let db_status = &db_model.status;

        let runtime_loaded = runtime_model_set.contains(&(tenant_id.clone(), model_id.clone()));

        match db_status.as_str() {
            "active" => {
                if !runtime_loaded {
                    inconsistencies.push(ModelInconsistency {
                        model_id: model_id.clone(),
                        tenant_id: tenant_id.clone(),
                        issue: "Model marked active in DB but not loaded in runtime".to_string(),
                        runtime_status: "not_loaded".to_string(),
                    });
                }
            }
            "inactive" | "failed" => {
                if runtime_loaded {
                    inconsistencies.push(ModelInconsistency {
                        model_id: model_id.clone(),
                        tenant_id: tenant_id.clone(),
                        issue: "Model marked inactive/failed in DB but loaded in runtime".to_string(),
                        runtime_status: "loaded".to_string(),
                    });
                }
            }
            _ => {
                inconsistencies.push(ModelInconsistency {
                    model_id: model_id.clone(),
                    tenant_id: tenant_id.clone(),
                    issue: format!("Unknown model status: {}", db_status),
                    runtime_status: if runtime_loaded { "loaded" } else { "not_loaded" }.to_string(),
                });
            }
        }
    }

    // Check for models loaded in runtime but not in database
    for (tenant_id, model_id) in &runtime_models {
        let in_db = db_models.iter().any(|db| &db.tenant_id == tenant_id && &db.model_id == model_id);
        if !in_db {
            inconsistencies.push(ModelInconsistency {
                model_id: model_id.clone(),
                tenant_id: tenant_id.clone(),
                issue: "Model loaded in runtime but not found in database".to_string(),
                runtime_status: "loaded".to_string(),
            });
        }
    }

    let status = if inconsistencies.is_empty() { "healthy" } else { "unhealthy" };

    Ok(Json(ModelRuntimeHealthResponse {
        status: status.to_string(),
        total_models: db_models.len() as i32,
        loaded_count: runtime_models.len() as i32,
        inconsistencies,
        checked_at: chrono::Utc::now().to_rfc3339(),
    }))
}

    #[test]
    fn test_cursor_config_instructions() {
        let config = CursorConfigResponse {
            api_endpoint: "http://localhost:8080/api/v1/chat/completions".to_string(),
            model_name: "adapteros-test".to_string(),
            model_id: "test-123".to_string(),
            is_ready: true,
            setup_instructions: vec!["Step 1".to_string()],
        };

        assert!(config.is_ready);
        assert_eq!(config.setup_instructions.len(), 1);
    }
}
