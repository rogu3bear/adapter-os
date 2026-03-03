//! Minimal model management handlers (stubs for unwired routes)
//!
//! These handlers provide basic API endpoints for model operations.
//! Full implementation details are in the original models.rs file.

use crate::api_error::{ApiError, ApiResult};
use crate::audit_helper::{log_failure_or_warn, log_success_or_warn};
use crate::auth::Claims;
use crate::control_plane::model_worker_lifecycle_reducer::{
    ModelWorkerLifecycleEvent, ModelWorkerLifecycleReducer,
};
use crate::ip_extraction::ClientIp;
use crate::middleware::require_any_role;
use crate::model_roots::resolve_model_allowed_roots;
use crate::model_status::aggregate_status;
use crate::state::AppState;
use crate::types::ErrorResponse;
use crate::uds_client::UdsClient;
use adapteros_api_types::ModelLoadStatus;
use adapteros_config::{
    resolve_base_model_location, resolve_worker_socket_for_cp, DEFAULT_MODEL_CACHE_ROOT,
};
use adapteros_core::io_utils::get_directory_size;
use adapteros_core::WorkerStatus;
use adapteros_db::users::Role;
use adapteros_lora_worker::memory::UmaStats;
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use dashmap::DashMap;
use std::collections::HashSet;
use std::path::{Path as StdPath, PathBuf};
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};
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
const ACTION_MODEL_REGISTER_SAFE: &str = "model.register_safe";
const MODEL_LOAD_STALE_AFTER_SECS: i64 = 180;
const MODEL_LOAD_ATTEMPTS_PER_WORKER: usize = 2;
static MODEL_LOAD_LOCKS: LazyLock<DashMap<String, Arc<tokio::sync::Mutex<()>>>> =
    LazyLock::new(DashMap::new);

struct CompatibilityRule {
    backend: &'static str,
    formats: &'static [&'static str],
    required_files: &'static [&'static str],
}

const MODEL_COMPATIBILITY_MATRIX: &[CompatibilityRule] = &[
    CompatibilityRule {
        backend: "mlx",
        formats: &["mlx", "safetensors"],
        required_files: &["config.json", "tokenizer.json"],
    },
    CompatibilityRule {
        backend: "metal",
        formats: &["mlx", "safetensors"],
        required_files: &["config.json", "tokenizer.json"],
    },
    CompatibilityRule {
        backend: "coreml",
        formats: &["mlx", "safetensors", "coreml"],
        required_files: &["config.json", "tokenizer.json"],
    },
];

/// Normalize backend label strings to canonical form via `BackendKind` parsing.
pub fn normalize_backend_label(backend: &str) -> &str {
    use std::str::FromStr;
    adapteros_core::BackendKind::from_str(backend)
        .map(|k| k.as_str())
        .unwrap_or(backend.trim())
}
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn model_lifecycle_reducer(state: &AppState) -> ModelWorkerLifecycleReducer {
    ModelWorkerLifecycleReducer::from_env(state.db.clone())
}

async fn reduce_model_lifecycle_event(
    state: &AppState,
    event: ModelWorkerLifecycleEvent,
) -> Result<(), String> {
    model_lifecycle_reducer(state)
        .reduce(event)
        .await
        .map_err(|e| format!("lifecycle reducer error: {}", e))
}

async fn model_allowed_roots(state: &AppState) -> Result<Vec<PathBuf>, String> {
    resolve_model_allowed_roots(Some(&state.db)).await
}

async fn acquire_model_load_guard(
    tenant_id: &str,
    model_id: &str,
) -> tokio::sync::OwnedMutexGuard<()> {
    let key = format!("{}::{}", tenant_id, model_id);
    let lock = MODEL_LOAD_LOCKS
        .entry(key)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    lock.lock_owned().await
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

fn loading_state_is_stale(updated_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> bool {
    let Some(raw_timestamp) = updated_at else {
        // Missing updated_at means we cannot reason about this loading state safely.
        return true;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw_timestamp) else {
        // Parse failure suggests bad/stale metadata; allow recovery instead of blocking.
        return true;
    };
    let updated = parsed.with_timezone(&chrono::Utc);
    now.signed_duration_since(updated).num_seconds() >= MODEL_LOAD_STALE_AFTER_SECS
}

fn worker_status_priority(status: &str) -> Option<u8> {
    match WorkerStatus::from_str(status).ok() {
        Some(WorkerStatus::Healthy) => Some(0),
        Some(WorkerStatus::Registered) => Some(1),
        Some(WorkerStatus::Created) => Some(2),
        Some(WorkerStatus::Pending) => Some(3),
        Some(WorkerStatus::Draining) => Some(4),
        Some(WorkerStatus::Stopped | WorkerStatus::Error) => None,
        None => None,
    }
}

fn worker_socket_candidates_from_records(
    workers: Vec<adapteros_db::models::Worker>,
) -> Vec<(PathBuf, String)> {
    let mut workers = workers;
    workers.sort_by(|a, b| {
        worker_status_priority(&a.status)
            .unwrap_or(u8::MAX)
            .cmp(&worker_status_priority(&b.status).unwrap_or(u8::MAX))
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for worker in workers {
        if worker_status_priority(&worker.status).is_none() {
            continue;
        }
        let normalized = worker.uds_path.replace("/./", "/");
        if !seen.insert(normalized.clone()) {
            continue;
        }
        let path = PathBuf::from(&normalized);
        if path.exists() {
            candidates.push((path, worker.id));
        }
    }

    candidates
}

fn validate_compatibility_matrix(format: Option<&str>, backend: &str) -> Result<(), String> {
    let backend = backend.trim().to_ascii_lowercase();
    let rule = MODEL_COMPATIBILITY_MATRIX
        .iter()
        .find(|candidate| candidate.backend == backend)
        .ok_or_else(|| format!("No compatibility rule found for backend '{}'", backend))?;

    if let Some(format) = format {
        if !rule
            .formats
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(format))
        {
            return Err(format!(
                "Compatibility matrix rejected format '{}' for backend '{}' (allowed: {})",
                format,
                backend,
                rule.formats.join(", ")
            ));
        }
    }

    Ok(())
}

async fn enforce_hot_swap_gate(
    state: &AppState,
    tenant_id: &str,
    incoming_model_id: &str,
) -> Result<(), String> {
    // Check actual inference operations in flight, not raw HTTP request count.
    // The raw `in_flight_requests` counter includes SSE streams, health polls,
    // and UI asset requests — none of which conflict with model loads.
    // Only active inference operations (running/paused) are unsafe to interrupt.
    if let Some(ref tracker) = state.inference_state_tracker {
        let active = tracker.count_active();
        if active > 0 {
            return Err(format!(
                "Hot-swap gate closed: {} active inference(s); retry after they complete",
                active
            ));
        }
    }

    let active_state = state
        .db
        .get_workspace_active_state(tenant_id)
        .await
        .map_err(|e| format!("Failed to check active workspace state: {}", e))?;

    if let Some(active_state) = active_state {
        if let Some(active_model_id) = active_state.active_base_model_id {
            if active_model_id != incoming_model_id {
                if let Some(status) = state
                    .db
                    .get_base_model_status_for_model(tenant_id, &active_model_id)
                    .await
                    .map_err(|e| format!("Failed to check base model status: {}", e))?
                {
                    if matches!(status.status.as_str(), "loading" | "unloading") {
                        return Err(format!(
                            "Hot-swap gate closed: active model transition '{}' in progress",
                            status.status
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

async fn fallback_to_last_known_good_base_model(
    state: &AppState,
    tenant_id: &str,
    failed_model_id: &str,
    worker_candidates: &[(PathBuf, String)],
    uds_client: &UdsClient,
) -> Result<Option<String>, String> {
    let active = state
        .db
        .get_workspace_active_state(tenant_id)
        .await
        .map_err(|e| format!("Failed to fetch active workspace state: {}", e))?;
    let fallback_model_id = active
        .and_then(|value| value.active_base_model_id)
        .filter(|id| id != failed_model_id);
    let Some(fallback_model_id) = fallback_model_id else {
        return Ok(None);
    };

    let fallback_model = state
        .db
        .get_model_for_tenant(tenant_id, &fallback_model_id)
        .await
        .map_err(|e| {
            format!(
                "Failed to fetch fallback model '{}': {}",
                fallback_model_id, e
            )
        })?
        .ok_or_else(|| {
            format!(
                "Fallback model '{}' is no longer available in tenant '{}'",
                fallback_model_id, tenant_id
            )
        })?;

    let fallback_path = fallback_model.model_path.clone().unwrap_or_else(|| {
        resolve_base_model_location(Some(&fallback_model_id), None, false)
            .map(|loc| loc.full_path.display().to_string())
            .unwrap_or_else(|_| {
                PathBuf::from(DEFAULT_MODEL_CACHE_ROOT)
                    .join(&fallback_model_id)
                    .display()
                    .to_string()
            })
    });

    let allowed_roots = model_allowed_roots(state).await?;
    let canonical_path =
        canonicalize_strict_in_allowed_roots(StdPath::new(&fallback_path), &allowed_roots)
            .map_err(|e| format!("Fallback path rejected: {}", e))?;
    let backend = fallback_model
        .backend
        .as_deref()
        .map(normalize_backend_label)
        .unwrap_or("mlx");
    validate_model_compatibility(&canonical_path, fallback_model.format.as_deref(), backend)
        .await?;

    let fallback_path = canonical_path.to_string_lossy().to_string();
    let mut last_error = None;
    for (socket, worker_id) in worker_candidates {
        match uds_client
            .load_model(socket, &fallback_model_id, &fallback_path)
            .await
        {
            Ok(resp) if resp.status == "loaded" || resp.status == "already_loaded" => {
                let memory_mb = resp.memory_usage_mb.unwrap_or(4096);
                reduce_model_lifecycle_event(
                    state,
                    ModelWorkerLifecycleEvent::ModelSwitchResult {
                        tenant_id: tenant_id.to_string(),
                        worker_id: Some(worker_id.clone()),
                        from_model_id: None,
                        to_model_id: Some(fallback_model_id.clone()),
                        to_model_hash_b3: None,
                        success: true,
                        error: None,
                        memory_usage_mb: Some(memory_mb),
                        reason: "fallback model restored after switch failure".to_string(),
                    },
                )
                .await
                .map_err(|e| format!("Failed to update fallback model status: {}", e))?;
                state
                    .metrics_exporter
                    .set_model_loaded_gauge(&fallback_model_id, tenant_id, true);
                tracing::warn!(
                    failed_model_id = %failed_model_id,
                    fallback_model_id = %fallback_model_id,
                    worker_id = %worker_id,
                    "Recovered model load failure by restoring last-known-good base model"
                );
                return Ok(Some(fallback_model_id));
            }
            Ok(resp) => {
                last_error = Some(format!(
                    "worker={} status={} error={}",
                    worker_id,
                    resp.status,
                    resp.error.unwrap_or_default()
                ));
            }
            Err(err) => {
                last_error = Some(format!("worker={} error={}", worker_id, err));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        format!(
            "Fallback model '{}' could not be loaded on any worker",
            fallback_model_id
        )
    }))
}

async fn validate_model_compatibility(
    model_path: &StdPath,
    format: Option<&str>,
    backend: &str,
) -> Result<(), String> {
    let backend = normalize_backend_label(backend).to_ascii_lowercase();
    validate_compatibility_matrix(format, &backend)?;

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

    if let Some(rule) = MODEL_COMPATIBILITY_MATRIX
        .iter()
        .find(|candidate| candidate.backend == backend.as_str())
    {
        for required in rule.required_files {
            let required_path = model_dir.join(required);
            if !required_path.exists() {
                return Err(format!(
                    "{} not found at '{}'",
                    required,
                    required_path.display()
                ));
            }
        }
    }

    match backend.as_str() {
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
    AllModelsStatusResponse, AneMemoryStatus, ModelStatusResponse, PatchModelRequest,
    SeedModelRequest, SeedModelResponse,
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
    Extension(client_ip): Extension<ClientIp>,
    Path(model_id): Path<String>,
) -> ApiResult<ModelStatusResponse> {
    use tracing::{debug, error, info, warn};
    let model_load_started = std::time::Instant::now();

    let request_id = crate::request_id::get_request_id().unwrap_or_else(|| "unknown".to_string());

    require_any_role(&claims, &[Role::Admin, Role::Operator])
        .map_err(|_| ApiError::forbidden("access denied"))?;
    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id).await?;

    let tenant_id = &claims.tenant_id;
    let _model_load_guard = acquire_model_load_guard(tenant_id, &model_id).await;
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

    if let Err(gate_error) = enforce_hot_swap_gate(&state, tenant_id, &model_id).await {
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_LOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Hot-swap gate denied model load: {}", gate_error),
            Some(client_ip.0.as_str()),
        )
        .await;
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("hot-swap gate denied model load")
                    .with_code("HOT_SWAP_GATED")
                    .with_string_details(gate_error),
            ),
        )
            .into());
    }

    // Aggregate current status across tenants/nodes for this model
    let all_statuses = state.db.list_base_model_statuses().await.map_err(|e| {
        error!("Failed to fetch model statuses: {}", e);
        ApiError::db_error(e)
    })?;
    let matching: Vec<_> = all_statuses
        .iter()
        .filter(|s| s.tenant_id == tenant_id.as_str() && s.model_id == model_id)
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

    if state_before.is_ready() {
        // Unconditionally set the active model when load succeeds.
        // `set_active_base_model_if_empty` would skip if a stale model ID is
        // already set, leaving the workspace pointing at a model that is no
        // longer loaded — which permanently blocks inference readiness.
        if let Err(e) = state
            .db
            .set_active_base_model(tenant_id, &model_id, state.manifest_hash.as_deref())
            .await
        {
            error!(
                error = %e,
                model_id = %model_id,
                tenant_id = %tenant_id,
                "Failed to set active base model during fast-path load"
            );
        }
        let latest = aggregated.latest;
        state
            .metrics_exporter
            .set_model_loaded_gauge(&model_id, tenant_id, true);
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            state_before,
            latest.and_then(|s| s.loaded_at.clone()),
            latest.and_then(|s| s.error_message.clone()),
            latest.and_then(|s| s.memory_usage_mb),
            true,
        )
        .await;
        return Ok(Json(response));
    }

    if matches!(state_before, ModelLoadStatus::Loading) {
        let latest = aggregated.latest;
        let tenant_updated_at = match state
            .db
            .get_base_model_status_for_model(tenant_id, &model_id)
            .await
        {
            Ok(status) => status.map(|s| s.updated_at),
            Err(e) => {
                warn!(
                    model_id = %model_id,
                    tenant_id = %tenant_id,
                    error = %e,
                    "Failed to read tenant-scoped model loading timestamp; falling back to aggregated timestamp"
                );
                None
            }
        };
        let stale = loading_state_is_stale(
            tenant_updated_at
                .as_deref()
                .or_else(|| latest.map(|s| s.updated_at.as_str())),
            chrono::Utc::now(),
        );
        if !stale {
            state
                .metrics_exporter
                .set_model_loaded_gauge(&model_id, tenant_id, false);
            let response = build_model_status_response(
                &state,
                model_id,
                model.name,
                model.model_path,
                state_before,
                latest.and_then(|s| s.loaded_at.clone()),
                latest.and_then(|s| s.error_message.clone()),
                latest.and_then(|s| s.memory_usage_mb),
                false,
            )
            .await;
            return Ok(Json(response));
        }
        warn!(
            model_id = %model_id,
            tenant_id = %tenant_id,
            stale_after_secs = MODEL_LOAD_STALE_AFTER_SECS,
            "Detected stale model loading state; forcing recovery load attempt"
        );
    }

    // Update status to "loading" first (idempotent ensure) via reducer.
    reduce_model_lifecycle_event(
        &state,
        ModelWorkerLifecycleEvent::ModelLoadRequested {
            tenant_id: tenant_id.to_string(),
            model_id: model_id.clone(),
            worker_id: None,
            model_hash_b3: Some(model.hash_b3.clone()),
            reason: "control-plane model load requested".to_string(),
        },
    )
    .await
    .map_err(|e| {
        error!("Failed to transition model status to loading: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update model status")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e),
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
        let client_ip = client_ip.clone();
        async move {
            let _ = reduce_model_lifecycle_event(
                &state,
                ModelWorkerLifecycleEvent::ModelSwitchResult {
                    tenant_id: tenant_id.to_string(),
                    worker_id: None,
                    from_model_id: None,
                    to_model_id: Some(model_id.clone()),
                    to_model_hash_b3: None,
                    success: false,
                    error: Some(err_msg.clone()),
                    memory_usage_mb: None,
                    reason: "model load failed before worker switch completed".to_string(),
                },
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
                Some(client_ip.0.as_str()),
            )
            .await;
        }
    };

    // Get worker socket paths - try tenant workers first, then global workers, then env fallback
    let worker_candidates = get_worker_socket_paths(&state, tenant_id).await;
    if worker_candidates.is_empty() {
        let err_msg =
            "No worker is available to load the model (all candidate sockets missing/unhealthy)"
                .to_string();
        error!(
            model_id = %model_id,
            tenant_id = %tenant_id,
            "No worker available for model loading"
        );
        record_failure(err_msg.clone()).await;
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no worker available")
                    .with_code("WORKER_UNAVAILABLE")
                    .with_string_details(err_msg),
            ),
        )
            .into());
    }

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
    let allowed_roots = match model_allowed_roots(&state).await {
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

    // Call worker(s) via UDS to actually load the model. Try multiple workers and retry once
    // per worker to improve recovery from transient UDS/socket issues.
    let uds_client = UdsClient::new(Duration::from_secs(120)); // Model loading can take time
    let mut worker_failures: Vec<String> = Vec::new();
    let mut final_status = ModelLoadStatus::Error;
    let mut memory_mb: Option<i32> = None;
    let mut load_error: Option<String> = None;
    let mut loaded_via_worker_id: Option<String> = None;

    for (uds_path, worker_id) in &worker_candidates {
        let mut worker_response = None;
        let mut last_error = None;
        for attempt in 1..=MODEL_LOAD_ATTEMPTS_PER_WORKER {
            match uds_client
                .load_model(uds_path, &model_id, &model_path)
                .await
            {
                Ok(response) => {
                    worker_response = Some(response);
                    break;
                }
                Err(e) => {
                    let err = e.to_string();
                    last_error = Some(err.clone());
                    if attempt < MODEL_LOAD_ATTEMPTS_PER_WORKER {
                        warn!(
                            model_id = %model_id,
                            worker_id = %worker_id,
                            uds_path = %uds_path.display(),
                            attempt = attempt,
                            max_attempts = MODEL_LOAD_ATTEMPTS_PER_WORKER,
                            error = %err,
                            "Model load attempt failed on worker, retrying once"
                        );
                    }
                }
            }
        }

        if let Some(response) = worker_response {
            if response.status == "loaded" || response.status == "already_loaded" {
                final_status = ModelLoadStatus::Ready;
                memory_mb = response.memory_usage_mb;
                load_error = None;
                loaded_via_worker_id = Some(worker_id.clone());
                break;
            }

            let err = response.error.unwrap_or_else(|| {
                format!("Worker returned non-loaded status '{}'", response.status)
            });
            worker_failures.push(format!(
                "worker={} path={} error={}",
                worker_id,
                uds_path.display(),
                err
            ));
            continue;
        }

        if let Some(err) = last_error {
            worker_failures.push(format!(
                "worker={} path={} error={}",
                worker_id,
                uds_path.display(),
                err
            ));
        }
    }

    let mut fallback_recovery_note: Option<String> = None;
    if !matches!(final_status, ModelLoadStatus::Ready) {
        load_error = Some(if worker_failures.is_empty() {
            "Model load failed: no worker attempts were recorded".to_string()
        } else {
            format!(
                "Model load failed across {} worker candidate(s): {}",
                worker_failures.len(),
                worker_failures.join(" | ")
            )
        });

        match fallback_to_last_known_good_base_model(
            &state,
            tenant_id,
            &model_id,
            &worker_candidates,
            &uds_client,
        )
        .await
        {
            Ok(Some(restored_model_id)) => {
                fallback_recovery_note = Some(format!(
                    "Recovered by restoring last-known-good model '{}'",
                    restored_model_id
                ));
            }
            Ok(None) => {}
            Err(fallback_error) => {
                if let Some(error) = load_error.as_mut() {
                    error.push_str(&format!(" | fallback failed: {}", fallback_error));
                }
            }
        }
    } else if let Some(ref worker_id) = loaded_via_worker_id {
        info!(
            model_id = %model_id,
            worker_id = %worker_id,
            "Worker confirmed model is loaded"
        );
    }

    let estimated_memory_mb = memory_mb.unwrap_or(4096);

    // Update status based on worker response via reducer.
    let switch_event = ModelWorkerLifecycleEvent::ModelSwitchResult {
        tenant_id: tenant_id.to_string(),
        worker_id: loaded_via_worker_id.clone(),
        from_model_id: None,
        to_model_id: Some(model_id.clone()),
        to_model_hash_b3: Some(model.hash_b3.clone()),
        success: matches!(final_status, ModelLoadStatus::Ready),
        error: load_error.clone(),
        memory_usage_mb: Some(estimated_memory_mb),
        reason: "worker model switch result".to_string(),
    };
    if let Err(e) = reduce_model_lifecycle_event(&state, switch_event).await {
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
            Some(client_ip.0.as_str()),
        )
        .await;
        state
            .metrics_exporter
            .record_model_load(&model_id, tenant_id, false);
        state.metrics_exporter.record_model_load_duration(
            &model_id,
            tenant_id,
            model_load_started.elapsed().as_secs_f64(),
        );
        let response = build_model_status_response(
            &state,
            model_id,
            model.name,
            model.model_path,
            ModelLoadStatus::Error,
            None,
            Some(e),
            Some(estimated_memory_mb),
            false,
        )
        .await;
        return Ok(Json(response));
    }

    // If worker returned an error, report it
    if let Some(error_msg) = load_error {
        let error_msg = if let Some(note) = fallback_recovery_note.as_ref() {
            format!("{} | {}", error_msg, note)
        } else {
            error_msg
        };
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
            Some(client_ip.0.as_str()),
        )
        .await;
        state
            .metrics_exporter
            .record_model_load(&model_id, tenant_id, false);
        state.metrics_exporter.record_model_load_duration(
            &model_id,
            tenant_id,
            model_load_started.elapsed().as_secs_f64(),
        );
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
        Some(client_ip.0.as_str()),
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
    state.metrics_exporter.record_model_load_duration(
        &model_id,
        tenant_id,
        model_load_started.elapsed().as_secs_f64(),
    );

    if let Err(e) = state
        .db
        .set_active_base_model(tenant_id, &model_id, state.manifest_hash.as_deref())
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
    Extension(client_ip): Extension<ClientIp>,
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
        .filter(|s| s.tenant_id == tenant_id.as_str() && s.model_id == model_id)
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

    // Transition to unloading then no-model via reducer.
    if let Err(e) = reduce_model_lifecycle_event(
        &state,
        ModelWorkerLifecycleEvent::ModelUnloadRequested {
            tenant_id: tenant_id.to_string(),
            model_id: model_id.clone(),
            worker_id: None,
            reason: "control-plane model unload requested".to_string(),
        },
    )
    .await
    {
        error!("Failed to update model status to unloading: {}", e);
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e), Some(&now), None)
            .await;
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_UNLOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to set unloading status: {}", e),
            Some(client_ip.0.as_str()),
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
            Some(e),
            None,
            false,
        )
        .await;
        return Ok(Json(response));
    }

    let worker_candidates = get_worker_socket_paths(&state, tenant_id).await;
    if worker_candidates.is_empty() {
        let err_msg = "No worker available to unload model".to_string();
        let _ = reduce_model_lifecycle_event(
            &state,
            ModelWorkerLifecycleEvent::ModelSwitchResult {
                tenant_id: tenant_id.to_string(),
                worker_id: None,
                from_model_id: Some(model_id.clone()),
                to_model_id: None,
                to_model_hash_b3: None,
                success: false,
                error: Some(err_msg.clone()),
                memory_usage_mb: None,
                reason: "model unload failed: no worker available".to_string(),
            },
        )
        .await;
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&err_msg), Some(&now), None)
            .await;
        return Ok(Json(
            build_model_status_response(
                &state,
                model_id,
                model.name,
                model.model_path,
                ModelLoadStatus::Error,
                None,
                Some(err_msg),
                None,
                false,
            )
            .await,
        ));
    }

    let uds_client = UdsClient::new(Duration::from_secs(60));
    let mut unload_success = false;
    let mut unload_error: Option<String> = None;
    for (uds_path, worker_id) in &worker_candidates {
        match uds_client.unload_model(uds_path).await {
            Ok(resp)
                if matches!(
                    resp.status.as_str(),
                    "unloaded" | "already_unloaded" | "no-model"
                ) =>
            {
                unload_success = true;
                let _ = reduce_model_lifecycle_event(
                    &state,
                    ModelWorkerLifecycleEvent::ModelSwitchResult {
                        tenant_id: tenant_id.to_string(),
                        worker_id: Some(worker_id.clone()),
                        from_model_id: Some(model_id.clone()),
                        to_model_id: None,
                        to_model_hash_b3: None,
                        success: true,
                        error: None,
                        memory_usage_mb: resp.memory_usage_mb,
                        reason: "worker model unload completed".to_string(),
                    },
                )
                .await;
                break;
            }
            Ok(resp) => {
                unload_error = Some(format!(
                    "worker={} status={} error={}",
                    worker_id,
                    resp.status,
                    resp.error.unwrap_or_default()
                ));
            }
            Err(err) => {
                unload_error = Some(format!("worker={} error={}", worker_id, err));
            }
        }
    }

    if !unload_success {
        let err_msg =
            unload_error.unwrap_or_else(|| "Model unload failed on all workers".to_string());
        let _ = reduce_model_lifecycle_event(
            &state,
            ModelWorkerLifecycleEvent::ModelSwitchResult {
                tenant_id: tenant_id.to_string(),
                worker_id: None,
                from_model_id: Some(model_id.clone()),
                to_model_id: None,
                to_model_hash_b3: None,
                success: false,
                error: Some(err_msg.clone()),
                memory_usage_mb: None,
                reason: "worker model unload failed".to_string(),
            },
        )
        .await;
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&err_msg), Some(&now), None)
            .await;
        return Ok(Json(
            build_model_status_response(
                &state,
                model_id,
                model.name,
                model.model_path,
                ModelLoadStatus::Error,
                None,
                Some(err_msg),
                None,
                false,
            )
            .await,
        ));
    }

    if let Err(e) = reduce_model_lifecycle_event(
        &state,
        ModelWorkerLifecycleEvent::ModelSwitchResult {
            tenant_id: tenant_id.to_string(),
            worker_id: None,
            from_model_id: Some(model_id.clone()),
            to_model_id: None,
            to_model_hash_b3: None,
            success: true,
            error: None,
            memory_usage_mb: None,
            reason: "model unload completed".to_string(),
        },
    )
    .await
    {
        error!("Failed to update model status to no-model: {}", e);
        // Log operation failure
        let _ = state
            .db
            .update_model_operation(&op_id, "failed", Some(&e), Some(&now), None)
            .await;
        // Audit log: model unload failure
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_UNLOAD,
            RESOURCE_MODEL,
            Some(&model_id),
            &format!("Failed to unload model: {}", e),
            Some(client_ip.0.as_str()),
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
            Some(e),
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
        Some(client_ip.0.as_str()),
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
        .filter(|s| s.tenant_id == tenant_id.as_str() && s.model_id == model_id)
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
    Extension(client_ip): Extension<ClientIp>,
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

    let allowed_roots = model_allowed_roots(&state).await.map_err(|e| {
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
    if let Err(gate_error) = enforce_hot_swap_gate(&state, tenant_id, &req.model_name).await {
        log_failure_or_warn(
            &state.db,
            &claims,
            ACTION_MODEL_REGISTER_SAFE,
            RESOURCE_MODEL,
            None,
            &format!("Safe registration blocked by hot-swap gate: {}", gate_error),
            Some(client_ip.0.as_str()),
        )
        .await;
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("safe registration blocked by hot-swap gate")
                    .with_code("HOT_SWAP_GATED")
                    .with_string_details(gate_error),
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
            adapteros_core::ModelImportStatus::Available,
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
                Some(client_ip.0.as_str()),
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

    // Audit log: model import success
    log_success_or_warn(
        &state.db,
        &claims,
        ACTION_MODEL_REGISTER_SAFE,
        RESOURCE_MODEL,
        Some(&model_id),
        Some(client_ip.0.as_str()),
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

/// Helper function to get worker socket paths
///
/// Tries to get the socket path from:
/// 1. Tenant workers in database (preferred)
/// 2. Global workers in database
/// 3. AOS_WORKER_SOCKET environment variable (development fallback)
/// 4. Default path var/run/worker.sock (local development)
async fn get_worker_socket_paths(state: &AppState, tenant_id: &str) -> Vec<(PathBuf, String)> {
    // Prefer tenant-local workers first, then fall back to global workers.
    let mut candidates = Vec::new();
    match state.db.list_workers_by_tenant(tenant_id).await {
        Ok(mut tenant_workers) => candidates.append(&mut tenant_workers),
        Err(e) => {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to list tenant workers for model loading"
            );
        }
    }
    match state.db.list_all_workers().await {
        Ok(all_workers) => {
            candidates.extend(all_workers.into_iter().filter(|w| w.tenant_id != tenant_id))
        }
        Err(e) => {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to list global workers for model loading"
            );
        }
    }

    let mut paths = worker_socket_candidates_from_records(candidates);
    if !paths.is_empty() {
        return paths;
    }

    match resolve_worker_socket_for_cp() {
        Ok(resolved) => {
            if resolved.path.exists() {
                paths.push((resolved.path, "env-fallback".to_string()));
                return paths;
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

    paths
}

#[cfg(test)]
mod model_load_recovery_tests {
    use super::*;
    use adapteros_db::models::Worker;

    fn mk_worker(
        id: &str,
        tenant_id: &str,
        status: &str,
        uds_path: String,
        last_seen_at: Option<&str>,
    ) -> Worker {
        Worker {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            node_id: "node-1".to_string(),
            plan_id: "plan-1".to_string(),
            uds_path,
            pid: Some(1234),
            status: status.to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            last_seen_at: last_seen_at.map(|s| s.to_string()),
            manifest_hash_b3: Some("manifest".to_string()),
            backend: Some("mlx".to_string()),
            model_hash_b3: None,
            capabilities_json: None,
            tokenizer_hash_b3: None,
            tokenizer_vocab_size: None,
        }
    }

    #[test]
    fn loading_state_staleness_threshold() {
        let now = chrono::Utc::now();
        let fresh = (now - chrono::Duration::seconds(MODEL_LOAD_STALE_AFTER_SECS - 5)).to_rfc3339();
        let stale = (now - chrono::Duration::seconds(MODEL_LOAD_STALE_AFTER_SECS + 1)).to_rfc3339();

        assert!(
            !loading_state_is_stale(Some(&fresh), now),
            "fresh loading state should not be treated as stale"
        );
        assert!(
            loading_state_is_stale(Some(&stale), now),
            "expired loading state should be treated as stale"
        );
        assert!(
            loading_state_is_stale(None, now),
            "missing timestamp should be treated as stale to unblock recovery"
        );
        assert!(
            loading_state_is_stale(Some("not-a-timestamp"), now),
            "invalid timestamp should be treated as stale to unblock recovery"
        );
    }

    #[test]
    fn worker_candidate_selection_prioritizes_and_dedupes() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let existing_a = manifest_dir.to_string_lossy().to_string();
        let existing_b = manifest_dir.join("src").to_string_lossy().to_string();

        let workers = vec![
            mk_worker(
                "w-registered-dup",
                "tenant-a",
                "registered",
                existing_a.clone(),
                Some("2026-01-01T00:00:00Z"),
            ),
            mk_worker(
                "w-healthy-dup",
                "tenant-a",
                "healthy",
                existing_a,
                Some("2026-01-01T00:00:10Z"),
            ),
            mk_worker(
                "w-stopped",
                "tenant-a",
                "stopped",
                existing_b.clone(),
                Some("2026-01-01T00:00:11Z"),
            ),
            mk_worker(
                "w-healthy-b",
                "tenant-a",
                "healthy",
                existing_b,
                Some("2026-01-01T00:00:09Z"),
            ),
        ];

        let selected = worker_socket_candidates_from_records(workers);
        assert_eq!(selected.len(), 2, "should dedupe by socket path");
        assert_eq!(selected[0].1, "w-healthy-dup");
        assert_eq!(selected[1].1, "w-healthy-b");
    }
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

/// Rename a model
///
/// Updates the registry name only; model files are not moved on disk.
///
/// # Endpoint
/// PATCH /v1/models/{model_id}
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `BAD_REQUEST` (400): Invalid model name
/// - `NOT_FOUND` (404): Model does not exist or is not visible to tenant
/// - `CONFLICT` (409): Model name already exists
/// - `INTERNAL_ERROR` (500): Database error
#[utoipa::path(
    patch,
    path = "/v1/models/{model_id}",
    request_body = PatchModelRequest,
    params(
        ("model_id" = String, Path, description = "Model ID (or name) to rename")
    ),
    responses(
        (status = 204, description = "Model renamed"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Model not found"),
        (status = 409, description = "Name conflict"),
        (status = 500, description = "Internal error")
    ),
    tag = "models"
)]
pub async fn patch_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
    Json(req): Json<PatchModelRequest>,
) -> Result<StatusCode, ApiError> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])
        .map_err(|_| ApiError::forbidden("access denied"))?;

    let requested_model = crate::id_resolver::resolve_any_id(&state.db, &model_id).await?;
    let tenant_id = &claims.tenant_id;
    let new_name = req.name.trim();
    if new_name.is_empty() {
        return Err(ApiError::bad_request("model name cannot be empty"));
    }

    let model = match state
        .db
        .get_model_for_tenant(tenant_id, &requested_model)
        .await
        .map_err(ApiError::db_error)?
    {
        Some(model) => Some(model),
        None => state
            .db
            .get_model_by_name_for_tenant(tenant_id, &requested_model)
            .await
            .map_err(ApiError::db_error)?,
    }
    .ok_or_else(|| ApiError::not_found("model"))?;

    state
        .db
        .update_model_name_for_tenant(tenant_id, &model.id, new_name)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("Model not found") || msg.contains("model not found") {
                ApiError::not_found("model")
            } else if msg.contains("cannot be empty") {
                ApiError::bad_request(msg)
            } else if msg.contains("UNIQUE constraint failed: models.name") {
                ApiError::conflict("model name already exists")
            } else {
                ApiError::db_error(e)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Delete a model
///
/// Rejects deletion if the model is currently loaded or referenced by adapters.
///
/// # Endpoint
/// DELETE /v1/models/{model_id}
///
/// # Errors
/// - `FORBIDDEN` (403): User lacks required role
/// - `NOT_FOUND` (404): Model does not exist
/// - `CONFLICT` (409): Model is loaded or referenced by adapters
/// - `INTERNAL_ERROR` (500): Database error
#[utoipa::path(
    delete,
    path = "/v1/models/{model_id}",
    params(
        ("model_id" = String, Path, description = "Model ID to delete")
    ),
    responses(
        (status = 204, description = "Model deleted"),
        (status = 404, description = "Model not found"),
        (status = 409, description = "Model in use"),
        (status = 500, description = "Internal error")
    ),
    tag = "models"
)]
pub async fn delete_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_any_role(&claims, &[Role::Admin]).map_err(|_| ApiError::forbidden("access denied"))?;

    let model_id = crate::id_resolver::resolve_any_id(&state.db, &model_id).await?;

    match state.db.delete_model(&model_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("Not found") {
                Err(ApiError::not_found("Model"))
            } else if msg.contains("Cannot delete") {
                Err(ApiError::conflict(msg))
            } else {
                Err(ApiError::db_error(e))
            }
        }
    }
}

// Note: get_base_model_status is now in handlers::infrastructure module
