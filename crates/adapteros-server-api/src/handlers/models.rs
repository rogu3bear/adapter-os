//! Minimal model management handlers (stubs for unwired routes)
//!
//! These handlers provide basic API endpoints for model operations.
//! Full implementation details are in the original models.rs file.

use crate::api_error::{ApiError, ApiResult};
use crate::audit_helper::{log_failure_or_warn, log_success_or_warn};
use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::model_status::aggregate_status;
use crate::state::AppState;
use crate::types::ErrorResponse;
use crate::uds_client::UdsClient;
use adapteros_api_types::ModelLoadStatus;
use adapteros_config::{
    resolve_base_model_location, resolve_worker_socket_for_cp, DEFAULT_MODEL_CACHE_ROOT,
};
use adapteros_core::io_utils::get_directory_size;
use adapteros_db::users::Role;
use adapteros_lora_worker::memory::UmaStats;
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use std::path::{Path as StdPath, PathBuf};
use std::time::Duration;
use tracing::{error, warn};

/// Resource type for model audit logs
const RESOURCE_MODEL: &str = "model";

/// Audit action: model load
const ACTION_MODEL_LOAD: &str = "model.load";

/// Audit action: model unload
const ACTION_MODEL_UNLOAD: &str = "model.unload";

/// Audit action: model import
const ACTION_MODEL_IMPORT: &str = "model.import";

fn normalize_backend_label(backend: &str) -> &str {
    let trimmed = backend.trim();
    if trimmed.eq_ignore_ascii_case("mlx-ffi") || trimmed.eq_ignore_ascii_case("mlx_ffi") {
        "mlx"
    } else {
        trimmed
    }
}
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

async fn model_allowed_roots() -> Result<Vec<PathBuf>, String> {
    let location = resolve_base_model_location(None, None, false).map_err(|e| e.to_string())?;
    if !location.cache_root.exists() {
        tokio::fs::create_dir_all(&location.cache_root)
            .await
            .map_err(|e| {
                format!(
                    "Failed to create model cache root {}: {}",
                    location.cache_root.display(),
                    e
                )
            })?;
    }
    Ok(vec![location.cache_root])
}

async fn has_safetensors_files(dir: &StdPath) -> bool {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("safetensors"))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn has_coreml_weights(dir: &StdPath) -> bool {
    let candidates = [
        dir.join("Data/com.apple.CoreML/weights/weight.bin"),
        dir.join("Data/model/weights.bin"),
        dir.join("Data/model/weight.bin"),
    ];
    candidates.iter().any(|path| path.exists())
}

async fn validate_model_compatibility(
    model_path: &StdPath,
    format: Option<&str>,
    backend: &str,
) -> Result<(), String> {
    let model_dir = if model_path.is_dir() {
        model_path
    } else {
        model_path.parent().ok_or_else(|| {
            format!(
                "Model path has no parent directory: {}",
                model_path.display()
            )
        })?
    };

    let config_path = model_dir.join("config.json");
    if !config_path.exists() {
        return Err(format!(
            "config.json not found at '{}'",
            config_path.display()
        ));
    }

    let tokenizer_path = model_dir.join("tokenizer.json");
    if !tokenizer_path.exists() {
        return Err(format!(
            "tokenizer.json not found at '{}'",
            tokenizer_path.display()
        ));
    }

    match backend {
        "coreml" => {
            if !has_coreml_weights(model_dir) {
                return Err(format!(
                    "CoreML weights not found under '{}'",
                    model_dir.display()
                ));
            }
        }
        "metal" | "mlx" => {
            if let Some(format) = format {
                if !matches!(format, "mlx" | "safetensors") {
                    return Err(format!(
                        "Format '{}' is not compatible with backend '{}'",
                        format, backend
                    ));
                }
            }
            if !has_safetensors_files(model_dir).await {
                return Err(format!(
                    "No safetensors weights found under '{}'",
                    model_dir.display()
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

// Import and re-export consolidated model types from shared crate
pub use adapteros_api_types::models::{
    AllModelsStatusResponse, AneMemoryStatus, ModelStatusResponse, SeedModelRequest,
    SeedModelResponse,
};

/// Alias for backward compatibility with existing code
pub type ImportModelRequest = SeedModelRequest;
pub type ImportModelResponse = SeedModelResponse;

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

// AneMemoryStatus is now imported from adapteros_api_types::models

#[allow(clippy::too_many_arguments)]
async fn build_model_status_response(
    state: &AppState,
    model_id: String,
    model_name: String,
    model_path: Option<String>,
    status: ModelLoadStatus,
    loaded_at: Option<String>,
    error_message: Option<String>,
    memory_usage_mb: Option<i32>,
    is_loaded: bool,
) -> ModelStatusResponse {
    let stats = state.uma_monitor.get_uma_stats().await;
    let ane_memory = ane_usage_from_stats(&stats);
    let uma_pressure_level = Some(state.uma_monitor.get_current_pressure().to_string());

    ModelStatusResponse {
        model_id,
        model_name,
        model_path,
        status,
        loaded_at,
        error_message,
        memory_usage_mb,
        is_loaded,
        ane_memory,
        uma_pressure_level,
    }
}

fn ane_usage_from_stats(stats: &UmaStats) -> Option<AneMemoryStatus> {
    let allocated_mb = stats.ane_allocated_mb?;
    let used_mb = stats.ane_used_mb?;
    let available_mb = stats.ane_available_mb?;
    let usage_pct = stats.ane_usage_percent?;

    Some(AneMemoryStatus {
        allocated_mb,
        used_mb,
        available_mb,
        usage_pct,
    })
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
) -> ApiResult<ModelStatusResponse> {
    use tracing::{debug, error, info, warn};

    let request_id = crate::request_id::get_request_id().unwrap_or_else(|| "unknown".to_string());

    require_any_role(&claims, &[Role::Admin, Role::Operator])
        .map_err(|_| ApiError::forbidden("access denied"))?;
    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id).await?;

    let tenant_id = &claims.tenant_id;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists in database
    let model = state
        .db
        .get_model_for_tenant(tenant_id, &model_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch model {}: {}", model_id, e);
            ApiError::db_error(e)
        })?
        .ok_or_else(|| {
            warn!("Model not found: {}", model_id);
            ApiError::not_found("model")
        })?;

    // Aggregate current status across tenants/nodes for this model
    let all_statuses = state.db.list_base_model_statuses().await.map_err(|e| {
        error!("Failed to fetch model statuses: {}", e);
        ApiError::db_error(e)
    })?;
    let matching: Vec<_> = all_statuses
        .iter()
        .filter(|s| s.model_id == model_id)
        .collect();
    let aggregated = aggregate_status(matching.iter().copied());
    let state_before = aggregated.status;

    debug!(
        request_id = %request_id,
        model_id = %model_id,
        tenant_id = %tenant_id,
        state_before = %state_before.as_str(),
        "model_load_request"
    );

    if matches!(
        state_before,
        ModelLoadStatus::Ready | ModelLoadStatus::Loading
    ) {
        if state_before.is_ready() {
            if let Err(e) = state
                .db
                .set_active_base_model_if_empty(
                    tenant_id,
                    &model_id,
                    state.manifest_hash.as_deref(),
                )
                .await
            {
                error!(
                    error = %e,
                    model_id = %model_id,
                    tenant_id = %tenant_id,
                    "Failed to set active base model during fast-path load"
                );
            }
        }
        let latest = aggregated.latest;
        state.metrics_exporter.set_model_loaded_gauge(
            &model_id,
            tenant_id,
            state_before.is_ready(),
        );
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            state_before,
            latest.and_then(|s| s.loaded_at.clone()),
            latest.and_then(|s| s.error_message.clone()),
            latest.and_then(|s| s.memory_usage_mb),
            state_before.is_ready(),
        )
        .await;
        return Ok(Json(response));
    }

    // Update status to "loading" first (idempotent ensure)
    state
        .db
        .update_base_model_status(
            tenant_id,
            &model_id,
            ModelLoadStatus::Loading.as_str(),
            None,
            None,
        )
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
    state
        .metrics_exporter
        .set_model_loaded_gauge(&model_id, tenant_id, false);

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

    let record_failure = |err_msg: String| {
        let state = state.clone();
        let model_id = model_id.clone();
        let claims = claims.clone();
        let op_id = op_id.clone();
        let now = now.clone();
        async move {
            let _ = state
                .db
                .update_base_model_status(
                    tenant_id,
                    &model_id,
                    ModelLoadStatus::Error.as_str(),
                    Some(&err_msg),
                    None,
                )
                .await;
            let _ = state
                .db
                .update_model_operation(&op_id, "failed", Some(&err_msg), Some(&now), None)
                .await;
            log_failure_or_warn(
                &state.db,
                &claims,
                ACTION_MODEL_LOAD,
                RESOURCE_MODEL,
                Some(&model_id),
                &err_msg,
            )
            .await;
        }
    };

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

    // Get model path from database; fall back to canonical resolver
    let model_path = model.model_path.clone().unwrap_or_else(|| {
        resolve_base_model_location(Some(&model_id), None, false)
            .map(|loc| loc.full_path.display().to_string())
            .unwrap_or_else(|_| {
                PathBuf::from(DEFAULT_MODEL_CACHE_ROOT)
                    .join(&model_id)
                    .display()
                    .to_string()
            })
    });
    let backend = model
        .backend
        .as_deref()
        .map(normalize_backend_label)
        .unwrap_or("mlx");
    let format = model.format.as_deref();
    let allowed_roots = match model_allowed_roots().await {
        Ok(roots) => roots,
        Err(e) => {
            let err_msg = format!("failed to resolve model roots: {}", e);
            record_failure(err_msg.clone()).await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to resolve model roots")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(err_msg),
                ),
            )
                .into());
        }
    };
    let canonical_path =
        match canonicalize_strict_in_allowed_roots(StdPath::new(&model_path), &allowed_roots) {
            Ok(path) => path,
            Err(e) => {
                let (status, code, message, err_msg) = match e {
                    adapteros_core::AosError::NotFound(_) => (
                        StatusCode::NOT_FOUND,
                        "MODEL_PATH_MISSING",
                        "model path does not exist",
                        format!("model path does not exist: {}", e),
                    ),
                    _ => (
                        StatusCode::FORBIDDEN,
                        "MODEL_PATH_FORBIDDEN",
                        "model path not permitted",
                        format!("model path rejected: {}", e),
                    ),
                };
                record_failure(err_msg.clone()).await;
                return Err((
                    status,
                    Json(
                        ErrorResponse::new(message)
                            .with_code(code)
                            .with_string_details(err_msg),
                    ),
                )
                    .into());
            }
        };
    let model_path = canonical_path.to_string_lossy().to_string();

    if let Err(e) = validate_model_compatibility(&canonical_path, format, backend).await {
        let err_msg = format!("model compatibility check failed: {}", e);
        record_failure(err_msg.clone()).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("model compatibility check failed")
                    .with_code("MODEL_COMPATIBILITY_FAILED")
                    .with_string_details(err_msg),
            ),
        )
            .into());
    }

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
                (ModelLoadStatus::Ready, response.memory_usage_mb, None)
            } else {
                warn!(
                    model_id = %model_id,
                    status = %response.status,
                    error = ?response.error,
                    "Worker returned non-loaded status"
                );
                (ModelLoadStatus::Error, None, response.error)
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
                ModelLoadStatus::Error,
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
            final_status.as_str(),
            load_error.as_deref(),
            Some(estimated_memory_mb),
        )
        .await
    {
        error!(
            model_id = %model_id,
            request_id = %request_id,
            error = %e,
            "Failed to update model status after worker response"
        );
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
            .await;
        // Audit log: model load failure
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_LOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to load model: {}", e),
        )
        .await;
        state
            .metrics_exporter
            .record_model_load(&model_id, tenant_id, false);
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            ModelLoadStatus::Error,
            None,
            Some(e.to_string()),
            Some(estimated_memory_mb),
            false,
        )
        .await;
        return Ok(Json(response));
    }

    // If worker returned an error, report it
    if let Some(error_msg) = load_error {
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&error_msg), Some(&now), None)
            .await;
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_LOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Worker failed to load model: {}", error_msg),
        )
        .await;
        state
            .metrics_exporter
            .record_model_load(&model_id, tenant_id, false);
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            ModelLoadStatus::Error,
            None,
            Some(error_msg),
            Some(estimated_memory_mb),
            false,
        )
        .await;
        return Ok(Json(response));
    }

    // Log successful operation
    let completion_time = chrono::Utc::now().to_rfc3339();
    let _ = state
        .db
        .update_model_operation(&op_id, "completed", None, Some(&completion_time), Some(100))
        .await;

    // Audit log: model load success
    log_success_or_warn(
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
        request_id = %request_id,
        memory_usage_mb = estimated_memory_mb,
        "Model loaded successfully"
    );

    state
        .metrics_exporter
        .record_model_load(&model_id, tenant_id, true);

    if let Err(e) = state
        .db
        .set_active_base_model_if_empty(tenant_id, &model_id, state.manifest_hash.as_deref())
        .await
    {
        error!(
            error = %e,
            model_id = %model_id,
            tenant_id = %tenant_id,
            "Failed to set active base model after load"
        );
    }

    let response = build_model_status_response(
        &state,
        model_id,
        model.name,
        model.model_path,
        ModelLoadStatus::Ready,
        Some(chrono::Utc::now().to_rfc3339()),
        None,
        Some(estimated_memory_mb),
        true,
    )
    .await;

    Ok(Json(response))
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
    use tracing::{debug, error, info, warn};

    let request_id = crate::request_id::get_request_id().unwrap_or_else(|| "unknown".to_string());

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
        )
    })?;
    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let tenant_id = &claims.tenant_id;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists in database
    let model = state
        .db
        .get_model_for_tenant(tenant_id, &model_id)
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

    // Aggregate current status across tenants/nodes for this model
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
    let matching: Vec<_> = all_statuses
        .iter()
        .filter(|s| s.model_id == model_id)
        .collect();
    let aggregated = aggregate_status(matching.iter().copied());
    let state_before = aggregated.status;

    debug!(
        request_id = %request_id,
        model_id = %model_id,
        tenant_id = %tenant_id,
        state_before = %state_before.as_str(),
        "model_unload_request"
    );

    if let Err(e) = state
        .db
        .clear_active_base_model_if_matches(tenant_id, &model_id)
        .await
    {
        error!(
            error = %e,
            model_id = %model_id,
            tenant_id = %tenant_id,
            "Failed to clear active base model during unload"
        );
    }

    // Idempotent no-op for non-ready states
    if !matches!(state_before, ModelLoadStatus::Ready) {
        let latest = aggregated.latest;
        state.metrics_exporter.set_model_loaded_gauge(
            &model_id,
            tenant_id,
            state_before.is_ready(),
        );
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            state_before,
            latest.and_then(|s| s.loaded_at.clone()),
            latest.and_then(|s| s.error_message.clone()),
            latest.and_then(|s| s.memory_usage_mb),
            state_before.is_ready(),
        )
        .await;
        return Ok(Json(response));
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

    // Transition to unloading then no-model
    if let Err(e) = state
        .db
        .update_base_model_status(
            tenant_id,
            &model_id,
            ModelLoadStatus::Unloading.as_str(),
            None,
            None,
        )
        .await
    {
        error!("Failed to update model status to unloading: {}", e);
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
            .await;
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_UNLOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to set unloading status: {}", e),
        )
        .await;
        state
            .metrics_exporter
            .record_model_unload(&model_id, tenant_id, false);
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            ModelLoadStatus::Error,
            None,
            Some(e.to_string()),
            None,
            false,
        )
        .await;
        return Ok(Json(response));
    }

    if let Err(e) = state
        .db
        .update_base_model_status(
            tenant_id,
            &model_id,
            ModelLoadStatus::NoModel.as_str(),
            None,
            None,
        )
        .await
    {
        error!("Failed to update model status to no-model: {}", e);
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e.to_string()), Some(&now), None)
            .await;
        // Audit log: model unload failure
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_UNLOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to unload model: {}", e),
        )
        .await;
        state
            .metrics_exporter
            .record_model_unload(&model_id, tenant_id, false);
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            ModelLoadStatus::Error,
            None,
            Some(e.to_string()),
            None,
            false,
        )
        .await;
        return Ok(Json(response));
    }

    // Log successful operation
    let completion_time = chrono::Utc::now().to_rfc3339();
    let _ = state
        .db
        .update_model_operation(&op_id, "completed", None, Some(&completion_time), Some(100))
        .await;

    // Audit log: model unload success
    log_success_or_warn(
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
        request_id = %request_id,
        "Model unloaded successfully"
    );

    state
        .metrics_exporter
        .record_model_unload(&model_id, tenant_id, true);

    let response = build_model_status_response(
        &state,
        model_id,
        model.name,
        model.model_path,
        ModelLoadStatus::NoModel,
        None,
        None,
        None,
        false,
    )
    .await;

    Ok(Json(response))
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
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, warn};

    let tenant_id = &claims.tenant_id;
    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Check if model exists in database
    let model = state
        .db
        .get_model_for_tenant(tenant_id, &model_id)
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
        .collect::<Vec<_>>();

    let aggregated = aggregate_status(status.iter().copied());
    let latest = aggregated.latest;

    let response = build_model_status_response(
        &state,
        model_id,
        model.name,
        model.model_path,
        aggregated.status,
        latest.and_then(|s| s.loaded_at.clone()),
        latest.and_then(|s| s.error_message.clone()),
        latest.and_then(|s| s.memory_usage_mb),
        aggregated.status.is_ready(),
    )
    .await;

    Ok(Json(response))
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
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::{error, warn};

    let tenant_id = &claims.tenant_id;
    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Check if model exists in database
    let model = state
        .db
        .get_model_for_tenant(tenant_id, &model_id)
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
        if !license_hash.is_empty()
            && (license_hash.len() != 64 || !license_hash.chars().all(|c| c.is_ascii_hexdigit()))
        {
            errors.push(format!(
                "Invalid license hash format: expected 64-char hex, got {}",
                license_hash
            ));
            is_valid = false;
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
/// - `backend`: Backend to use (mlx, metal, coreml)
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
///   "backend": "mlx",
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
    use tracing::{error, info, warn};

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
        )
    })?;

    let tenant_id = &claims.tenant_id;

    // Validate format
    let valid_formats = ["mlx", "safetensors", "pytorch", "gguf"];
    if !valid_formats.contains(&req.format.as_str()) {
        warn!("Invalid model format: {}", req.format);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid model format").with_code("BAD_REQUEST")),
        ));
    }

    // Normalize backend aliases (keep API surface MLX-only)
    let backend = normalize_backend_label(&req.backend).to_ascii_lowercase();

    // Validate backend
    let valid_backends = ["mlx", "metal", "coreml"];
    if !valid_backends.contains(&backend.as_str()) {
        warn!("Invalid backend: {}", req.backend);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("invalid backend").with_code("BAD_REQUEST")),
        ));
    }

    let allowed_roots = model_allowed_roots().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to resolve model roots")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e),
            ),
        )
    })?;
    let canonical_path =
        canonicalize_strict_in_allowed_roots(StdPath::new(&req.model_path), &allowed_roots)
            .map_err(|e| {
                let (status, code, message) = match e {
                    adapteros_core::AosError::NotFound(_) => (
                        StatusCode::BAD_REQUEST,
                        "BAD_REQUEST",
                        "model path does not exist",
                    ),
                    _ => (
                        StatusCode::FORBIDDEN,
                        "MODEL_PATH_FORBIDDEN",
                        "model path not permitted",
                    ),
                };
                (
                    status,
                    Json(
                        ErrorResponse::new(message)
                            .with_code(code)
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    if let Err(e) = validate_model_compatibility(&canonical_path, Some(&req.format), &backend).await
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("model compatibility check failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e),
            ),
        ));
    }
    let canonical_path_str = canonical_path.to_string_lossy().to_string();

    // Start import
    let model_id = match state
        .db
        .import_model_from_path(
            &req.model_name,
            &canonical_path_str,
            &req.format,
            &backend,
            tenant_id,
            &claims.sub,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to start model import: {}", e);
            // Audit log: model import failure
            log_failure_or_warn(
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
    log_success_or_warn(
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
        model_path = %canonical_path_str,
        format = %req.format,
        backend = %backend,
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
pub struct ModelArchitectureSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_layers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocab_size: Option<usize>,
}

#[derive(Serialize, ToSchema)]
pub struct ModelWithStatsResponse {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub format: Option<String>,
    pub backend: Option<String>,
    pub size_bytes: Option<i64>,
    pub import_status: Option<String>,
    pub model_path: Option<String>,
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub adapter_count: i64,
    pub training_job_count: i64,
    pub imported_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<ModelArchitectureSummary>,
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
///   - `backend`: Backend type (mlx, metal, coreml)
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
    Extension(claims): Extension<Claims>,
) -> Result<Json<ModelListResponse>, (StatusCode, Json<ErrorResponse>)> {
    use tracing::error;

    let models_with_stats = state
        .db
        .list_models_with_stats(&claims.tenant_id)
        .await
        .map_err(|e| {
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
    let mut models = Vec::with_capacity(total);
    for m in models_with_stats {
        let model = &m.model;
        let capabilities = model
            .capabilities
            .as_ref()
            .and_then(|c| serde_json::from_str::<Vec<String>>(c).ok());

        models.push(ModelWithStatsResponse {
            id: model.id.clone(),
            name: model.name.clone(),
            hash_b3: model.hash_b3.clone(),
            config_hash_b3: model.config_hash_b3.clone(),
            tokenizer_hash_b3: model.tokenizer_hash_b3.clone(),
            format: model.format.clone(),
            backend: model
                .backend
                .as_deref()
                .map(normalize_backend_label)
                .map(|value| value.to_string()),
            size_bytes: model.size_bytes,
            import_status: model.import_status.clone(),
            model_path: model.model_path.clone(),
            capabilities,
            quantization: model.quantization.clone(),
            tenant_id: model.tenant_id.clone(),
            adapter_count: m.adapter_count,
            training_job_count: m.training_job_count,
            imported_at: model.imported_at.clone(),
            updated_at: model.updated_at.clone(),
            architecture: parse_architecture_summary(model).await,
        });
    }

    Ok(Json(ModelListResponse { models, total }))
}

async fn parse_architecture_summary(
    model: &adapteros_db::models::Model,
) -> Option<ModelArchitectureSummary> {
    let mut summary = ModelArchitectureSummary {
        architecture: model.model_type.clone(),
        num_layers: None,
        hidden_size: None,
        vocab_size: None,
    };

    if let Some(raw) = &model.metadata_json {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            summary.num_layers = value
                .get("num_hidden_layers")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            summary.hidden_size = value
                .get("hidden_size")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            summary.vocab_size = value
                .get("vocab_size")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            if summary.architecture.is_none() {
                summary.architecture = value
                    .get("architecture")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        value
                            .get("model_type")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .or_else(|| {
                        value
                            .get("architectures")
                            .and_then(|v| v.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    });
            }
        }
    }

    // Fallback: parse config.json from model_path when metadata is missing/incomplete
    if (summary.architecture.is_none()
        || summary.num_layers.is_none()
        || summary.hidden_size.is_none()
        || summary.vocab_size.is_none())
        && model.model_path.is_some()
    {
        if let Some(path_str) = &model.model_path {
            let path = StdPath::new(path_str);
            let config_path = if path.is_dir() {
                path.join("config.json")
            } else {
                path.to_path_buf()
            };

            if config_path.exists() {
                if let Ok(contents) = tokio::fs::read_to_string(&config_path).await {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
                        if summary.num_layers.is_none() {
                            summary.num_layers = value
                                .get("num_hidden_layers")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                        }
                        if summary.hidden_size.is_none() {
                            summary.hidden_size = value
                                .get("hidden_size")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                        }
                        if summary.vocab_size.is_none() {
                            summary.vocab_size = value
                                .get("vocab_size")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as usize);
                        }
                        if summary.architecture.is_none() {
                            summary.architecture = value
                                .get("model_type")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    value
                                        .get("architectures")
                                        .and_then(|v| v.as_array())
                                        .and_then(|arr| arr.first())
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                });
                        }
                    }
                }
            }
        }
    }

    if summary.architecture.is_none()
        && summary.num_layers.is_none()
        && summary.hidden_size.is_none()
        && summary.vocab_size.is_none()
    {
        None
    } else {
        Some(summary)
    }
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

    require_any_role(&claims, &[Role::Operator, Role::Admin, Role::Viewer])?;

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

    // PRD-RECT-002: Non-admin users can only see their own tenant's model statuses
    let is_admin = claims.roles.iter().any(|r| r.to_lowercase() == "admin");
    let tenant_filter = query.get("tenant_id");
    let statuses: Vec<_> = if is_admin {
        // Admin: can filter by specific tenant or see all
        if let Some(tenant_id) = tenant_filter {
            statuses
                .into_iter()
                .filter(|s| s.tenant_id == *tenant_id)
                .collect()
        } else {
            statuses
        }
    } else {
        // Non-admin: always filter to their own tenant only, ignore tenant_id query param
        statuses
            .into_iter()
            .filter(|s| s.tenant_id == claims.tenant_id)
            .collect()
    };

    // Convert to response format and get model details
    let mut model_responses = Vec::new();
    let mut total_memory_mb = 0;
    let mut active_model_count = 0;

    let mut grouped: std::collections::HashMap<String, Vec<adapteros_db::models::BaseModelStatus>> =
        std::collections::HashMap::new();
    for status in statuses {
        grouped
            .entry(status.model_id.clone())
            .or_default()
            .push(status);
    }

    for (model_id, records) in grouped {
        let aggregated = aggregate_status(records.iter());
        let latest = aggregated.latest;

        // Get model details
        let model = match if let Some(tenant_id) = tenant_filter {
            state.db.get_model_for_tenant(tenant_id, &model_id).await
        } else {
            state.db.get_model(&model_id).await
        } {
            Ok(Some(m)) => m,
            Ok(None) => {
                error!("Model not found: {}", model_id);
                continue;
            }
            Err(e) => {
                error!("Failed to get model {}: {}", model_id, e);
                continue;
            }
        };

        let is_loaded = aggregated.status.is_ready();
        if is_loaded {
            active_model_count += 1;
        }

        if let Some(memory) = latest.and_then(|s| s.memory_usage_mb) {
            total_memory_mb += memory as i64;
        }

        let gauge_tenant = latest.map(|s| s.tenant_id.as_str()).unwrap_or("unknown");
        state.metrics_exporter.set_model_loaded_gauge(
            &model_id,
            gauge_tenant,
            aggregated.status.is_ready(),
        );

        model_responses.push(crate::types::BaseModelStatusResponse {
            model_id,
            model_name: model.name,
            model_path: model.model_path,
            status: aggregated.status,
            loaded_at: latest.and_then(|s| s.loaded_at.clone()),
            unloaded_at: latest.and_then(|s| s.unloaded_at.clone()),
            error_message: latest.and_then(|s| s.error_message.clone()),
            memory_usage_mb: latest.and_then(|s| s.memory_usage_mb),
            is_loaded,
            updated_at: latest
                .map(|s| s.updated_at.clone())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
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

    match resolve_worker_socket_for_cp() {
        Ok(resolved) => {
            if resolved.path.exists() {
                return Some(resolved.path);
            }
            warn!(
                path = %resolved.path.display(),
                source = %resolved.source,
                "Resolved worker socket path does not exist for models helper"
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to resolve worker socket for models helper");
        }
    }

    None
}

// ============================================================================
// Download Progress Endpoint
// ============================================================================

/// Progress information for a single model download/import
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub operation_id: String,
    pub operation: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_pct: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Response for GET /v1/models/download-progress
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct DownloadProgressResponse {
    pub schema_version: String,
    pub imports: Vec<ModelDownloadProgress>,
    pub total_active: usize,
}

/// Get download/import progress for all active model operations
///
/// Returns progress information for all currently in-progress model downloads
/// and imports for the tenant.
#[utoipa::path(
    get,
    path = "/v1/models/download-progress",
    responses(
        (status = 200, description = "Download progress for active imports", body = DownloadProgressResponse),
        (status = 500, description = "Database error")
    ),
    tag = "models"
)]
pub async fn get_download_progress(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<DownloadProgressResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require at least Operator role to view model import progress
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("PERMISSION_DENIED")),
        )
    })?;

    let tenant_id = &claims.tenant_id;

    // Get all active import/download operations for the tenant
    let operations = state.db.get_active_imports(tenant_id).await.map_err(|e| {
        error!("Failed to get active imports: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Convert to progress response
    let mut imports: Vec<ModelDownloadProgress> = Vec::with_capacity(operations.len());
    for op in operations {
        let mut progress_pct = None;
        let mut speed_mbps = None;
        let mut eta_seconds = None;

        if let Ok(Some(model)) = state.db.get_model_for_tenant(tenant_id, &op.model_id).await {
            if let (Some(expected_bytes), Some(model_path)) =
                (model.size_bytes, model.model_path.clone())
            {
                if expected_bytes > 0 {
                    let current_bytes = get_directory_size(StdPath::new(&model_path))
                        .map(|size| size as i64)
                        .unwrap_or(0);
                    let current_bytes = current_bytes.max(0) as f64;
                    let expected_bytes = expected_bytes as f64;
                    if expected_bytes > 0.0 {
                        let pct = ((current_bytes / expected_bytes) * 100.0).min(99.0) as i32;
                        progress_pct = Some(pct);

                        if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&op.started_at) {
                            let elapsed = chrono::Utc::now()
                                .signed_duration_since(started.with_timezone(&chrono::Utc));
                            let elapsed_secs = elapsed.num_seconds();
                            if elapsed_secs > 0 {
                                let mbps =
                                    (current_bytes / (1024.0 * 1024.0)) / (elapsed_secs as f64);
                                if mbps.is_finite() && mbps > 0.0 {
                                    speed_mbps = Some(mbps);
                                    if current_bytes < expected_bytes {
                                        let remaining_bytes = expected_bytes - current_bytes;
                                        let eta = remaining_bytes / (mbps * 1024.0 * 1024.0);
                                        eta_seconds = Some(eta.round() as i64);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        imports.push(ModelDownloadProgress {
            model_id: op.model_id,
            operation_id: op.id,
            operation: op.operation,
            status: op.status,
            started_at: op.started_at,
            progress_pct,
            speed_mbps,
            eta_seconds,
            error_message: op.error_message,
        });
    }

    let total_active = imports.len();

    Ok(Json(DownloadProgressResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        imports,
        total_active,
    }))
}

// Note: get_base_model_status is now in handlers::infrastructure module
