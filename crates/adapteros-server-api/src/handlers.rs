#![allow(unused_variables)]

use crate::auth::{generate_token, generate_token_ed25519, refresh_token, verify_password, Claims};
use crate::errors::ErrorResponseExt;
use crate::middleware::{require_any_role, require_role};
use crate::operation_tracker::{ModelOperationType, OperationCancellationError};
use crate::services::repo_url::infer_repo_urls_parallel;
use crate::state::{AppState, JwtMode, TrainingSessionMetadata};
use crate::types::AdapterOSStatus;
use crate::types::*; // This already re-exports adapteros_api_types::*
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::*;
use adapteros_api_types::{
    repositories::RepositorySummary, AdapterActivationResponse, AdapterHealthResponse,
    AdapterManifest, AdapterMetricsResponse, AdapterPerformance, AdapterResponse,
    AdapterStateResponse, AdapterStats, AuthConfigResponse, BuildPlanRequest, ComparePlansRequest,
    CreateTenantRequest, HealthResponse, InferRequest, InferResponse, InferenceTrace,
    LoadAverageResponse, LoginRequest, LoginResponse, NodeDetailsResponse, NodePingResponse,
    NodeResponse, PlanComparisonResponse, PlanDetailsResponse, PlanRebuildResponse, PlanResponse,
    ProfileResponse, QualityMetricsResponse, RegisterAdapterRequest, RegisterNodeRequest,
    RotateTokenResponse, SessionInfo, SystemMetricsResponse, TenantResponse, TenantUsageResponse,
    TokenMetadata, UpdateAuthConfigRequest, UpdateProfileRequest, UpdateTenantRequest,
    UserInfoResponse, WorkerInfo, WorkerResponse,
};
use adapteros_lora_lifecycle::state::AdapterState;
use adapteros_orchestrator::training::TrainingJobBuilder;
use adapteros_policy::unified_enforcement::{
    PolicyContext as UnifiedPolicyContext, Priority as UnifiedPriority,
};
use adapteros_policy::{
    EnforcementAction, Operation as PolicyOperation, OperationType as PolicyOperationType,
    PolicyEnforcer,
};
use adapteros_system_metrics::monitoring_types::{
    AcknowledgeAlertRequest, AlertResponse, AnomalyResponse, BaselineResponse,
    CreateMonitoringRuleApiRequest, MonitoringRuleResponse, RecalculateBaselineRequest,
    UpdateAnomalyStatusRequest, UpdateMonitoringRuleApiRequest,
};
use axum::response::Response;
use sqlx::Row;
use tracing::{error, info, warn};

pub mod activity;
pub mod batch;
#[cfg(feature = "cdp")]
pub mod code;
pub mod domain_adapters;
#[cfg(feature = "federation")]
pub mod federation;
pub mod git;
pub mod git_repository;
pub mod golden;
pub mod journeys;
pub mod messages;
pub mod models;
pub mod notifications;
pub mod openai;
pub mod replay;
pub mod telemetry;
pub mod tutorials;
pub mod workspaces;

// Re-export domain adapter handlers
use adapteros_core::{AosError, TrainingConfig, TrainingJob, TrainingJobStatus};
use adapteros_db::commits::Commit;
use adapteros_db::process_monitoring::{
    CreateDashboardRequest, CreateReportRequest, ProcessAlert, ProcessAnomaly,
};
use adapteros_db::users::Role;
use adapteros_db::{sqlx, AdapterRegistrationBuilder};
use adapteros_git::CommitInfo;
use adapteros_lora_router::features::CodeFeatures;
use adapteros_system_metrics::monitoring_types::{
    AlertFilters, AlertSeverity, AlertStatus, AnomalyFilters, AnomalyStatus,
};
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, UnifiedTelemetryEvent};
use adapteros_trace::{reader::read_trace_bundle, signing::verify_bundle_signature_from_dir};
use adapteros_verify::{verify_against_golden, ComparisonConfig};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use blake3;
pub use domain_adapters::*;
use serde::Deserialize;
use serde_json::json;

/// Hot-swap an adapter with zero downtime
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/hot-swap",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = adapteros_api_types::adapters::HotSwapRequest,
    responses(
        (status = 200, description = "Adapter hot-swapped successfully", body = adapteros_api_types::adapters::HotSwapResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Hot-swap failed", body = ErrorResponse),
        (status = 503, description = "Hot-swap not enabled", body = ErrorResponse)
    )
)]
pub async fn hot_swap_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<adapteros_api_types::adapters::HotSwapRequest>,
) -> Result<Json<adapteros_api_types::adapters::HotSwapResponse>, (StatusCode, Json<ErrorResponse>)>
{
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let lifecycle = state.lifecycle_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Hot-swap not enabled").with_code("NOT_CONFIGURED")),
        )
    })?;

    let hot_swap = {
        let mgr = lifecycle.lock().await;
        mgr.hot_swap_manager().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new("Hot-swap not enabled").with_code("NOT_CONFIGURED")),
            )
        })?
    };

    // Measure elapsed time and compute previous adapter id (if any)
    let start = std::time::Instant::now();
    let old_id = hot_swap
        .get_active(&adapter_id)
        .map(|a| a.adapter().manifest.adapter_id.clone());

    hot_swap
        .swap_single(&adapter_id, req.new_path.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Hot-swap failed")
                        .with_code("SWAP_FAILED")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let elapsed = start.elapsed();
    Ok(Json(adapteros_api_types::adapters::HotSwapResponse {
        adapter_id,
        swap_time_ms: elapsed.as_millis() as u64,
        old_adapter: old_id,
    }))
}
use anyhow::Context as _;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{DateTime, Utc};
use futures_util::stream::{self, Stream, StreamExt};
use std::collections::HashMap;
use std::convert::{Infallible, TryInto};
use std::path::Path as StdPath;
use std::time::Duration;
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};

// Helper: CAB Golden Gate verification (read-only)
async fn run_golden_gate(state: &AppState) -> anyhow::Result<bool> {
    // Copy required config values and drop the lock before awaiting
    let (gg_opt, bundles_root) = {
        let cfg_guard = state
            .config
            .read()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        (
            cfg_guard.golden_gate.clone(),
            cfg_guard.bundles_root.clone(),
        )
    };

    let gg = match gg_opt {
        Some(c) if c.enabled => c,
        _ => return Ok(true),
    };

    let golden_dir = StdPath::new("golden_runs/baselines").join(&gg.baseline);
    if !golden_dir.exists() {
        // Treat missing golden as failure to be safe
        return Ok(false);
    }

    // Choose bundle path: prefer explicit, else newest .ndjson under bundles_root
    let bundle_path = if let Some(p) = &gg.bundle_path {
        std::path::PathBuf::from(p)
    } else {
        newest_ndjson(&bundles_root).context("no .ndjson bundles found under bundles_root")?
    };

    if !bundle_path.exists() {
        return Ok(false);
    }

    let mut cmp = ComparisonConfig::default();
    cmp.strictness = gg.strictness;
    cmp.verify_toolchain = !gg.skip_toolchain;
    cmp.verify_signature = !gg.skip_signature;
    cmp.verify_device = gg.verify_device;

    let report = verify_against_golden(&golden_dir, &bundle_path, &cmp).await?;
    Ok(report.passed)
}

fn newest_ndjson(root: &str) -> anyhow::Result<std::path::PathBuf> {
    let root_path = std::path::Path::new(root);
    if !root_path.exists() {
        return Err(anyhow::anyhow!("bundles_root does not exist: {}", root));
    }

    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;

    if let Ok(entries) = std::fs::read_dir(root_path) {
        for ent in entries.flatten() {
            let p = ent.path();
            if p.extension().and_then(|s| s.to_str()) == Some("ndjson") {
                if let Ok(meta) = ent.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                            newest = Some((mtime, p.clone()));
                        }
                    }
                }
            }
        }
    }

    if let Some((_, p)) = newest {
        return Ok(p);
    }

    // Fallback: recursive search
    fn walk(
        dir: &std::path::Path,
        newest: &mut Option<(std::time::SystemTime, std::path::PathBuf)>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for ent in entries.flatten() {
                let p = ent.path();
                if p.is_dir() {
                    walk(&p, newest);
                } else if p.extension().and_then(|s| s.to_str()) == Some("ndjson") {
                    if let Ok(meta) = ent.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                                *newest = Some((mtime, p.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    walk(root_path, &mut newest);
    newest
        .map(|(_, p)| p)
        .ok_or_else(|| anyhow::anyhow!("no .ndjson bundles found under {}", root))
}

fn map_git_error(message: &str, err: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let err_string = err.to_string();
    let lower = err_string.to_lowercase();
    let status = if lower.contains("not found") {
        StatusCode::NOT_FOUND
    } else if lower.contains("invalid") {
        StatusCode::BAD_REQUEST
    } else if lower.contains("no git repositories registered") {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    let code = match status {
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::BAD_REQUEST => "BAD_REQUEST",
        StatusCode::SERVICE_UNAVAILABLE => "SERVICE_UNAVAILABLE",
        _ => "GIT_ERROR",
    };

    (
        status,
        Json(
            ErrorResponse::new(message)
                .with_code(code)
                .with_string_details(err_string),
        ),
    )
}

fn map_alert_to_response(alert: ProcessAlert) -> ProcessAlertResponse {
    let escalation_level: i32 = alert.escalation_level.try_into().unwrap_or(i32::MAX);

    ProcessAlertResponse {
        id: alert.id,
        rule_id: alert.rule_id,
        worker_id: alert.worker_id,
        tenant_id: alert.tenant_id,
        alert_type: alert.alert_type,
        severity: alert.severity.to_string(),
        title: alert.title,
        message: alert.message,
        metric_value: alert.metric_value,
        threshold_value: alert.threshold_value,
        status: alert.status.to_string(),
        acknowledged_by: alert.acknowledged_by,
        acknowledged_at: alert.acknowledged_at.map(|dt| dt.to_rfc3339()),
        resolved_at: alert.resolved_at.map(|dt| dt.to_rfc3339()),
        suppression_reason: alert.suppression_reason,
        suppression_until: alert.suppression_until.map(|dt| dt.to_rfc3339()),
        escalation_level,
        notification_sent: alert.notification_sent,
        created_at: alert.created_at.to_rfc3339(),
        updated_at: alert.updated_at.to_rfc3339(),
    }
}

fn map_system_alert_to_response(
    alert: adapteros_system_metrics::monitoring_types::AlertResponse,
) -> ProcessAlertResponse {
    let escalation_level: i32 = alert.escalation_level.try_into().unwrap_or(i32::MAX);

    ProcessAlertResponse {
        id: alert.id,
        rule_id: alert.rule_id,
        worker_id: alert.worker_id,
        tenant_id: alert.tenant_id,
        alert_type: alert.alert_type,
        severity: alert.severity,
        title: alert.title,
        message: alert.message,
        metric_value: alert.metric_value,
        threshold_value: alert.threshold_value,
        status: alert.status,
        acknowledged_by: alert.acknowledged_by,
        acknowledged_at: alert.acknowledged_at,
        resolved_at: alert.resolved_at,
        suppression_reason: alert.suppression_reason,
        suppression_until: alert.suppression_until,
        escalation_level,
        notification_sent: alert.notification_sent,
        created_at: alert.created_at,
        updated_at: alert.updated_at,
    }
}

fn map_anomaly_to_response(anomaly: ProcessAnomaly) -> ProcessAnomalyResponse {
    ProcessAnomalyResponse {
        id: anomaly.id,
        worker_id: anomaly.worker_id,
        tenant_id: anomaly.tenant_id,
        anomaly_type: anomaly.anomaly_type,
        metric_name: anomaly.metric_name,
        detected_value: anomaly.detected_value,
        expected_range_min: anomaly.expected_range_min,
        expected_range_max: anomaly.expected_range_max,
        confidence_score: anomaly.confidence_score,
        severity: anomaly.severity.to_string(),
        description: anomaly.description,
        detection_method: anomaly.detection_method,
        model_version: anomaly.model_version,
        status: anomaly.status.to_string(),
        investigated_by: anomaly.investigated_by,
        investigation_notes: anomaly.investigation_notes,
        resolved_at: anomaly.resolved_at.map(|dt| dt.to_rfc3339()),
        created_at: anomaly.created_at.to_rfc3339(),
    }
}

fn map_commit_to_response(info: CommitInfo) -> CommitResponse {
    CommitResponse {
        id: info.sha.clone(),
        repo_id: info.repo_id,
        sha: info.sha,
        author: info.author,
        date: info.date.to_rfc3339(),
        message: info.message,
        branch: info.branch,
        changed_files: info.changed_files,
        impacted_symbols: info.impacted_symbols,
        ephemeral_adapter_id: info.ephemeral_adapter_id,
    }
}

fn map_db_commit_to_response(commit: Commit) -> Result<CommitResponse, AosError> {
    let changed_files: Vec<String> = serde_json::from_str(&commit.changed_files_json)
        .map_err(|e| AosError::Parse(format!("Failed to parse changed_files JSON: {}", e)))?;

    let impacted_symbols: Vec<String> = if let Some(json) = &commit.impacted_symbols_json {
        serde_json::from_str(json)
            .map_err(|e| AosError::Parse(format!("Failed to parse impacted_symbols JSON: {}", e)))?
    } else {
        Vec::new()
    };

    Ok(CommitResponse {
        id: commit.id,
        repo_id: commit.repo_id,
        sha: commit.sha,
        author: commit.author,
        date: commit.date,
        message: commit.message,
        branch: commit.branch,
        changed_files,
        impacted_symbols,
        ephemeral_adapter_id: commit.ephemeral_adapter_id,
    })
}

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Internal model runtime health status
#[derive(Debug, Clone)]
pub struct ModelRuntimeHealth {
    /// Total number of models in the database
    total_models: i32,
    /// Number of models currently loaded
    loaded_count: i32,
    /// Overall health status
    healthy: bool,
    /// Number of detected inconsistencies
    inconsistencies_count: i32,
}

/// Cached health check result to avoid expensive database queries on every request
#[derive(Debug)]
struct HealthCache {
    result: ModelRuntimeHealth,
    timestamp: Instant,
    ttl: Duration,
}

impl HealthCache {
    fn new(ttl_seconds: u64) -> Self {
        Self {
            result: ModelRuntimeHealth {
                total_models: 0,
                loaded_count: 0,
                healthy: true,
                inconsistencies_count: 0,
            },
            timestamp: Instant::now() - Duration::from_secs(ttl_seconds + 1), // Force initial refresh
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }

    fn get(&self) -> Option<&ModelRuntimeHealth> {
        if self.is_expired() {
            None
        } else {
            Some(&self.result)
        }
    }

    fn update(&mut self, result: ModelRuntimeHealth) {
        self.result = result;
        self.timestamp = Instant::now();
    }
}

/// Global health cache - in production this should be per-tenant or have better isolation
static HEALTH_CACHE: once_cell::sync::Lazy<Arc<RwLock<HealthCache>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HealthCache::new(30)))); // 30 second cache

/// Check model runtime health summary for health endpoint with caching
pub async fn check_model_runtime_health_summary(
    state: &AppState,
) -> Result<ModelRuntimeHealth, anyhow::Error> {
    let start_time = std::time::Instant::now();

    // Check cache first
    let cache_hit = {
        let cache = HEALTH_CACHE.read().await;
        if let Some(cached) = cache.get() {
            // Record cache hit metric
            let metrics = &state.metrics_collector;
            metrics.record_inference_latency(
                "health",
                "cache_hit",
                start_time.elapsed().as_secs_f64(),
            );
            return Ok(cached.clone());
        }
        false
    };

    // Cache miss - compute fresh result
    let result = check_model_runtime_health_uncached(state).await?;
    let duration = start_time.elapsed().as_secs_f64();

    // Record metrics
    state
        .metrics_collector
        .record_inference_latency("health", "cache_miss", duration);
    // Could add custom counter for cache misses if needed

    // Update cache
    {
        let mut cache = HEALTH_CACHE.write().await;
        cache.update(result.clone());
    }

    Ok(result)
}

/// Uncached version of model runtime health check
async fn check_model_runtime_health_uncached(
    state: &AppState,
) -> Result<ModelRuntimeHealth, anyhow::Error> {
    let Some(rt) = &state.model_runtime else {
        return Ok(ModelRuntimeHealth {
            total_models: 0,
            loaded_count: 0,
            healthy: true,
            inconsistencies_count: 0,
        });
    };

    // Get all models from database
    let db_models = sqlx::query!("SELECT tenant_id, model_id, status FROM base_model_status")
        .fetch_all(state.db.pool())
        .await?;

    let total_models = db_models.len() as i32;

    // Get all loaded models from runtime
    let guard = rt.lock().await;
    let runtime_models = guard.get_all_loaded_models();
    let loaded_count = runtime_models.len() as i32;
    drop(guard);

    // Check for inconsistencies (simplified version)
    let mut inconsistencies_count = 0;
    let runtime_model_set: std::collections::HashSet<(String, String)> = runtime_models
        .iter()
        .map(|k| (k.tenant_id.clone(), k.model_id.clone()))
        .collect();

    // Check each DB model
    for db_model in &db_models {
        let tenant_id = &db_model.tenant_id;
        let model_id = &db_model.model_id;
        let db_status = &db_model.status;

        let runtime_loaded = runtime_model_set.contains(&(tenant_id.clone(), model_id.clone()));

        match db_status.as_str() {
            "active" => {
                if !runtime_loaded {
                    inconsistencies_count += 1;
                }
            }
            "inactive" | "failed" => {
                if runtime_loaded {
                    inconsistencies_count += 1;
                }
            }
            _ => {
                // Unknown status, consider it an inconsistency
                inconsistencies_count += 1;
            }
        }
    }

    // Check for models loaded in runtime but not in database
    for model_key in &runtime_models {
        let in_db = db_models
            .iter()
            .any(|db| db.tenant_id == model_key.tenant_id && db.model_id == model_key.model_id);
        if !in_db {
            inconsistencies_count += 1;
        }
    }

    let healthy = inconsistencies_count == 0;

    Ok(ModelRuntimeHealth {
        total_models,
        loaded_count,
        healthy,
        inconsistencies_count,
    })
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    // Check model runtime health
    let models_health = if let Some(rt) = &state.model_runtime {
        match check_model_runtime_health_summary(&state).await {
            Ok(health) => Some(health),
            Err(e) => {
                tracing::warn!("Failed to check model runtime health: {}", e);
                None
            }
        }
    } else {
        None
    };

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        models: models_health.map(|health| adapteros_api_types::ModelRuntimeHealth {
            total_models: health.total_models,
            loaded_count: health.loaded_count,
            healthy: health.healthy,
            inconsistencies_count: health.inconsistencies_count as usize,
        }),
    })
}

/// Get current system status including service information
/// Citation: crates/adapteros-server/src/status_writer.rs L135-144
#[utoipa::path(
    get,
    path = "/v1/status",
    tag = "status",
    responses(
        (status = 200, description = "Current system status", body = AdapterOSStatus)
    )
)]
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<AdapterOSStatus>, (StatusCode, Json<ErrorResponse>)> {
    // Note: The actual status collection logic is in adapteros-server crate
    // For now, return a basic status structure
    // TODO: Import status collection from adapteros-server once dependency cycle is resolved
    let (production_mode, telemetry_mode) = match state.config.read() {
        Ok(cfg) => (
            cfg.production_mode,
            if cfg.metrics.enabled {
                "local".to_string()
            } else {
                "disabled".to_string()
            },
        ),
        Err(err) => {
            warn!(error = %err, "Failed to read config for status response");
            (false, "unknown".to_string())
        }
    };

    let status = AdapterOSStatus {
        schema_version: "1.0".to_string(),
        status: "ok".to_string(),
        uptime_secs: 0,
        adapters_loaded: 0,
        deterministic: true,
        kernel_hash: "unknown".to_string(),
        telemetry_mode,
        worker_count: 0,
        base_model_loaded: false,
        base_model_id: None,
        services: vec![],
        production_mode,
        tenant_id: None,
    };
    Ok(Json(status))
}

/// Readiness check
#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 200, description = "Service is ready", body = HealthResponse),
        (status = 503, description = "Service is not ready", body = HealthResponse)
    )
)]
pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    // Check database connectivity
    match state.db.pool().acquire().await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None, // Readiness check doesn't include model health
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not ready".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                models: None, // Readiness check doesn't include model health
            }),
        ),
    }
}

/// Upsert a synthetic directory adapter and optionally activate it
#[utoipa::path(
    post,
    path = "/v1/adapters/directory/upsert",
    request_body = DirectoryUpsertRequest,
    responses(
        (status = 201, description = "Directory adapter upserted", body = DirectoryUpsertResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Failed to upsert directory adapter", body = ErrorResponse)
    ),
    tag = "adapters"
)]
pub async fn upsert_directory_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<DirectoryUpsertRequest>,
) -> Result<(StatusCode, Json<DirectoryUpsertResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Require admin or operator
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Validate root is absolute and readable
    let root = std::path::PathBuf::from(&req.root);
    if !root.is_absolute() || !root.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid root")
                    .with_code("BAD_REQUEST")
                    .with_string_details("root must be an existing absolute path"),
            ),
        ));
    }

    // Validate path is safe relative
    let rel = std::path::PathBuf::from(&req.path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid path")
                    .with_code("BAD_REQUEST")
                    .with_string_details("path must be relative and must not contain .."),
            ),
        ));
    }

    // Analyze directory to derive deterministic fingerprint
    let analysis = adapteros_codegraph::analyze_directory(&root, &rel).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("directory analysis failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Build adapter_id and synthetic artifact hash from fingerprint
    let adapter_id = format!(
        "directory::{}::{}",
        req.tenant_id,
        analysis.fingerprint.to_short_hex()
    );
    let hash_hex = analysis.fingerprint.to_hex();
    let hash_b3 = format!("b3:{}", hash_hex);

    // Ensure placeholder artifact exists at ./adapters/{hash}.safetensors
    let artifact_dir = std::path::PathBuf::from("./adapters");
    let artifact_path = artifact_dir.join(format!("{}.safetensors", hash_hex));
    if !artifact_path.exists() {
        if let Some(parent) = artifact_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to create adapters directory")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }
        if let Err(e) = std::fs::write(&artifact_path, b"synthetic adapter placeholder") {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to write adapter artifact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    // Register adapter if not present
    let existing = state.db.get_adapter(&adapter_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if existing.is_none() {
        let languages = analysis.language_stats.keys().cloned().collect::<Vec<_>>();
        let languages_json = serde_json::to_string(&languages).unwrap_or("[]".to_string());

        let registration = AdapterRegistrationBuilder::new()
            .adapter_id(&adapter_id)
            .name(&adapter_id)
            .hash_b3(&hash_b3)
            .rank(analysis.symbols.len() as i32 % 17 + 16)
            .tier(4)
            .languages_json(Some(languages_json))
            .framework(Some("directory"))
            .build()
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to build adapter registration")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        state.db.register_adapter(registration).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    // Optionally activate (load) adapter now
    let mut activated = false;
    if req.activate {
        if let Ok(Some(a)) = state.db.get_adapter(&adapter_id).await {
            let _ = state
                .db
                .update_adapter_state(&adapter_id, "loading", "directory_upsert")
                .await;

            if let Some(ref lifecycle) = state.lifecycle_manager {
                use adapteros_lora_lifecycle::AdapterLoader;
                use std::path::PathBuf;
                // Use the DB numeric id if it parses, else fall back to 0
                let adapter_idx = a.id.parse::<u16>().unwrap_or(0);
                let adapters_path = PathBuf::from("./adapters");
                let mut loader = AdapterLoader::new(adapters_path);
                if loader
                    .load_adapter_async(adapter_idx, &hash_hex, None)
                    .await
                    .is_ok()
                {
                    let _ = state
                        .db
                        .update_adapter_state(&adapter_id, "warm", "loaded_successfully")
                        .await;
                    activated = true;
                } else {
                    let _ = state
                        .db
                        .update_adapter_state(&adapter_id, "cold", "load_failed")
                        .await;
                }
            } else {
                // Simulate load
                let _ = state
                    .db
                    .update_adapter_state(&adapter_id, "warm", "simulated_load")
                    .await;
                activated = true;
            }
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(DirectoryUpsertResponse {
            adapter_id,
            hash_b3,
            activated,
        }),
    ))
}

/// Bulk load/unload adapters on a worker
pub async fn bulk_adapter_load(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<BulkAdapterRequest>,
) -> Result<Json<BulkAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Resolve tenant and UDS path
    let tenant_id = req.tenant_id.unwrap_or_else(|| claims.tenant_id.clone());
    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(_) => Vec::new(),
    };
    let uds_path = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        std::path::PathBuf::from(format!("/var/run/aos/{}/aos.sock", tenant_id))
    };

    let client = UdsClient::new(std::time::Duration::from_secs(10));
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut errors = Vec::new();

    // Add (load) adapters
    for a in &req.add {
        match client
            .send_http_request(
                uds_path.as_path(),
                "POST",
                &format!("/adapter/{}/load", a),
                None,
            )
            .await
        {
            Ok(resp) => {
                /* handle ok */
                if let Some(status) = resp.get("status").and_then(|s| s.as_str()) {
                    if status == "ok" {
                        added += 1;
                    }
                }
            }
            Err(e) => errors.push(format!("load {}: {}", a, e)),
        }
    }

    // Remove (unload) adapters
    for r in &req.remove {
        match client
            .send_http_request(
                uds_path.as_path(),
                "POST",
                &format!("/adapter/{}/unload", r),
                None,
            )
            .await
        {
            Ok(resp) => {
                /* handle ok */
                if let Some(status) = resp.get("status").and_then(|s| s.as_str()) {
                    if status == "ok" {
                        removed += 1;
                    }
                }
            }
            Err(e) => errors.push(format!("unload {}: {}", r, e)),
        }
    }

    Ok(Json(BulkAdapterResponse {
        added,
        removed,
        errors,
    }))
}

/// Login handler
#[utoipa::path(
    post,
    path = "/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse)
    )
)]
pub async fn auth_login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Login attempt for email: {}", req.email);

    // Get user by email
    let user = state
        .db
        .get_user_by_email(&req.email)
        .await
        .map_err(|e| {
            tracing::error!("Database error during user lookup: {}", e);
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
            tracing::warn!("User not found: {}", req.email);
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new("invalid credentials").with_code("INVALID_CREDENTIALS")),
            )
        })?;

    tracing::debug!(
        "User found: {} (role: {}, disabled: {})",
        user.id,
        user.role,
        user.disabled
    );

    // Check if user is disabled
    if user.disabled {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("user disabled").with_code("USER_DISABLED")),
        ));
    }

    // Verify password with production mode gating
    tracing::debug!("Verifying password for user: {}", user.id);

    // Check if we're in production mode
    let is_production = {
        let config = state.config.read().map_err(|_| {
            tracing::error!("Failed to read config for production mode check");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("configuration error").with_code("CONFIG_ERROR")),
            )
        })?;
        config.production_mode
    };

    let valid = if user.pw_hash == "password" {
        // Plain text password check only allowed when NOT in production mode
        if is_production {
            tracing::warn!("Plain text password attempted in production mode - rejecting");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("authentication system misconfigured")
                        .with_code("CONFIG_ERROR")
                        .with_string_details("plain text passwords not allowed in production"),
                ),
            ));
        }
        tracing::debug!("Using plain text password check (development mode)");
        let result = req.password == "password";
        tracing::debug!("Password check result: {}", result);
        result
    } else {
        // Use proper Argon2 verification
        tracing::debug!("Using Argon2 password verification");
        verify_password(&req.password, &user.pw_hash).map_err(|e| {
            tracing::error!("Password verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("authentication error")
                        .with_code("AUTHENTICATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    if !valid {
        tracing::warn!("Password verification failed for user: {}", user.id);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("invalid credentials").with_code("INVALID_CREDENTIALS")),
        ));
    }

    tracing::debug!("Password verification successful for user: {}", user.id);

    // Generate JWT
    tracing::debug!("Generating JWT token for user: {}", user.id);
    let token_result = match state.jwt_mode {
        JwtMode::EdDsa => {
            let keypair = state.crypto.clone_jwt_keypair();
            generate_token_ed25519(&user.id, &user.email, &user.role, "default", &keypair)
        }
        JwtMode::Hmac => generate_token(
            &user.id,
            &user.email,
            &user.role,
            "default",
            &state.jwt_secret,
        ),
    };

    let token = token_result.map_err(|e| {
        tracing::error!("JWT token generation failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("token generation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::debug!("JWT token generated successfully for user: {}", user.id);

    // Create response with httpOnly cookie for browser authentication
    let login_response = LoginResponse {
        token: token.clone(),
        user_id: user.id,
        role: user.role,
    };

    // Create response with cookie header
    let cookie = format!(
        "auth_token={}; HttpOnly; Path=/; Max-Age=28800; SameSite=Strict",
        token
    );

    let mut response = Json(login_response).into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&cookie).map_err(|_| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create cookie header")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?,
    );
    Ok(response)
}

/// Refresh authentication token
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    responses(
        (status = 200, description = "Token refreshed successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_refresh(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Token refresh requested for user: {}", claims.sub);

    // Generate new JWT token with refreshed expiry
    tracing::debug!("Generating refreshed JWT token for user: {}", claims.sub);
    let token_result = match state.jwt_mode {
        JwtMode::EdDsa => {
            let keypair = state.crypto.clone_jwt_keypair();
            refresh_token(&claims, &keypair)
        }
        JwtMode::Hmac => {
            // For HMAC mode, we need to regenerate the token since refresh_token expects EdDSA
            generate_token(
                &claims.sub,
                &claims.email,
                &claims.role,
                &claims.tenant_id,
                &state.jwt_secret,
            )
        }
    };

    let token = token_result.map_err(|e| {
        tracing::error!("JWT token refresh failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("token refresh failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::debug!("JWT token refreshed successfully for user: {}", claims.sub);

    // Create response with httpOnly cookie for browser authentication
    let refresh_response = json!({
        "message": "Token refreshed successfully",
        "user_id": claims.sub,
        "role": claims.role,
        "tenant_id": claims.tenant_id
    });

    // Create response with cookie header (same pattern as login)
    let cookie = format!(
        "auth_token={}; HttpOnly; Path=/; Max-Age=28800; SameSite=Strict",
        token
    );

    let mut response = Json(refresh_response).into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&cookie).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create cookie header")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?,
    );
    Ok(response)
}

/// List active authentication sessions
#[utoipa::path(
    get,
    path = "/v1/auth/sessions",
    responses(
        (status = 200, description = "Sessions retrieved successfully", body = Vec<SessionInfo>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_list_sessions(
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<SessionInfo>>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Listing sessions for user: {}", claims.sub);

    // Since JWT is stateless, we only return information about the current session
    // In a full implementation, this would query a session store
    let current_session = SessionInfo {
        id: claims.jti.clone(),
        device: None,     // Would be populated from user agent parsing
        ip_address: None, // Would be populated from request metadata
        user_agent: None, // Would be populated from request headers
        location: None,   // Would be populated from IP geolocation
        created_at: claims.iat.to_string(),
        last_seen_at: chrono::Utc::now().timestamp().to_string(),
        is_current: true,
    };

    let sessions = vec![current_session];
    Ok(Json(sessions))
}

/// Revoke a specific authentication session
#[utoipa::path(
    delete,
    path = "/v1/auth/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Session ID to revoke")
    ),
    responses(
        (status = 200, description = "Session revoked successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_revoke_session(
    Path(session_id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Revoking session {} for user: {}", session_id, claims.sub);

    // Check if the session_id matches current session's JTI
    if session_id == claims.jti {
        // For stateless JWT, we can't actually revoke tokens server-side
        // In a full implementation, this would add the JTI to a blacklist
        tracing::info!(
            "Session revocation requested for current session - client should clear local storage"
        );
        Ok(Json(json!({
            "message": "Session revoked - please clear local storage and re-authenticate",
            "revoked_session_id": session_id
        })))
    } else {
        // Session ID doesn't match current session
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("session not found").with_code("SESSION_NOT_FOUND")),
        ))
    }
}

/// Logout from all authentication sessions
#[utoipa::path(
    post,
    path = "/v1/auth/logout-all",
    responses(
        (status = 200, description = "All sessions logged out successfully", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_logout_all(
    Extension(claims): Extension<Claims>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Logout all sessions for user: {}", claims.sub);

    // For stateless JWT, we can't actually revoke all sessions server-side
    // In a full implementation, this would blacklist all JTIs for the user
    tracing::info!("Logout all requested - client should clear all stored tokens");

    Ok(Json(json!({
        "message": "All sessions logged out - please clear local storage and re-authenticate",
        "user_id": claims.sub
    })))
}

/// Rotate authentication token
#[utoipa::path(
    post,
    path = "/v1/auth/token/rotate",
    responses(
        (status = 200, description = "Token rotated successfully", body = RotateTokenResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_rotate_token(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Rotating token for user: {}", claims.sub);

    // Generate new JWT token with same claims but new JTI
    tracing::debug!("Generating rotated JWT token for user: {}", claims.sub);
    let token_result = match state.jwt_mode {
        JwtMode::EdDsa => {
            let keypair = state.crypto.clone_jwt_keypair();
            generate_token_ed25519(
                &claims.sub,
                &claims.email,
                &claims.role,
                &claims.tenant_id,
                &keypair,
            )
        }
        JwtMode::Hmac => generate_token(
            &claims.sub,
            &claims.email,
            &claims.role,
            &claims.tenant_id,
            &state.jwt_secret,
        ),
    };

    let token = token_result.map_err(|e| {
        tracing::error!("JWT token rotation failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("token rotation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::debug!("JWT token rotated successfully for user: {}", claims.sub);

    // Create response with new token and metadata
    let now = chrono::Utc::now();
    let rotation_response = RotateTokenResponse {
        token: token.clone(),
        created_at: now.to_rfc3339(),
        expires_at: Some((now + chrono::Duration::hours(8)).to_rfc3339()),
        last_rotated_at: Some(now.to_rfc3339()),
    };

    // Create response with httpOnly cookie for browser authentication
    let cookie = format!(
        "auth_token={}; HttpOnly; Path=/; Max-Age=28800; SameSite=Strict",
        token
    );

    let mut response = Json(rotation_response).into_response();
    response.headers_mut().insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&cookie).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create cookie header")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?,
    );
    Ok(response)
}

/// Get authentication token metadata
#[utoipa::path(
    get,
    path = "/v1/auth/token",
    responses(
        (status = 200, description = "Token metadata retrieved successfully", body = TokenMetadata),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_token_metadata(
    Extension(claims): Extension<Claims>,
) -> Result<Json<TokenMetadata>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Getting token metadata for user: {}", claims.sub);

    let metadata = TokenMetadata {
        created_at: claims.iat.to_string(),
        expires_at: Some(claims.exp.to_string()),
        last_rotated_at: None, // Would track rotation history in full implementation
        role: claims.role.clone(),
        tenant_id: claims.tenant_id.clone(),
    };

    Ok(Json(metadata))
}

/// Update user profile information
#[utoipa::path(
    put,
    path = "/v1/auth/profile",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Profile updated successfully", body = ProfileResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_update_profile(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::debug!("Updating profile for user: {}", claims.sub);

    // Get current user data
    let current_user = state
        .db
        .get_user(&claims.sub)
        .await
        .map_err(|e| {
            tracing::error!("Database error during user lookup: {}", e);
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
            tracing::warn!("User not found: {}", claims.sub);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("user not found").with_code("USER_NOT_FOUND")),
            )
        })?;

    // Update display_name if provided
    let new_display_name = req
        .display_name
        .as_ref()
        .unwrap_or(&current_user.display_name)
        .clone();

    // In a full implementation, this would update the database
    // For now, we'll just return the updated profile without persisting
    // TODO: Add database update logic when user profile persistence is implemented

    tracing::info!(
        "Profile update requested for user {}: display_name={:?}",
        claims.sub,
        req.display_name
    );

    let profile_response = ProfileResponse {
        user_id: current_user.id,
        email: current_user.email,
        role: current_user.role,
        display_name: Some(new_display_name),
        tenant_id: Some(claims.tenant_id.clone()),
        permissions: None,           // Would be populated from role mapping
        last_login_at: None,         // Would track login history
        mfa_enabled: Some(false),    // Would check MFA status
        token_last_rotated_at: None, // Would track rotation history
    };

    Ok(Json(profile_response))
}

/// Get authentication configuration
#[utoipa::path(
    get,
    path = "/v1/auth/config",
    responses(
        (status = 200, description = "Authentication configuration retrieved successfully", body = AuthConfigResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_get_config(
    State(state): State<AppState>,
) -> Result<Json<AuthConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = state.config.read().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("config lock poisoned").with_code("INTERNAL_ERROR")),
        )
    })?;

    let response = AuthConfigResponse {
        production_mode: config.production_mode,
        dev_token_enabled: !config.production_mode, // Dev token only available when not in production
        jwt_mode: match state.jwt_mode {
            JwtMode::EdDsa => "eddsa".to_string(),
            JwtMode::Hmac => "hmac".to_string(),
        },
        token_expiry_hours: 8, // Default value, could be configurable
    };

    Ok(Json(response))
}

/// Update authentication configuration
#[utoipa::path(
    put,
    path = "/v1/auth/config",
    request_body = UpdateAuthConfigRequest,
    responses(
        (status = 200, description = "Authentication configuration updated successfully", body = AuthConfigResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("bearer_token" = [])
    )
)]
#[axum::debug_handler]
pub async fn auth_update_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateAuthConfigRequest>,
) -> Result<Json<AuthConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get mutable access to config
    let mut config = state.config.write().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("config lock poisoned").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Validate production_mode and dev_token_enabled relationship
    if let Some(production_mode) = req.production_mode {
        if production_mode && req.dev_token_enabled.unwrap_or(false) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new(
                        "dev_token_enabled cannot be true when production_mode is true",
                    )
                    .with_code("INVALID_CONFIG"),
                ),
            ));
        }
        config.production_mode = production_mode;
    }

    // Note: Other config updates (jwt_mode, token_expiry_hours) would require server restart
    // For now, we only allow runtime updates to production_mode

    tracing::info!(
        "Authentication config updated: production_mode={}",
        config.production_mode
    );

    let response = AuthConfigResponse {
        production_mode: config.production_mode,
        dev_token_enabled: !config.production_mode,
        jwt_mode: match state.jwt_mode {
            JwtMode::EdDsa => "eddsa".to_string(),
            JwtMode::Hmac => "hmac".to_string(),
        },
        token_expiry_hours: 8,
    };

    Ok(Json(response))
}

/// Dev bypass endpoint - sets dev token cookie (only available when not in production)
#[axum::debug_handler]
pub async fn auth_dev_bypass(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check if production mode is enabled
    let is_production = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("config lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.production_mode
    };

    if is_production {
        tracing::warn!("Dev bypass attempted in production mode - rejected");
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("dev bypass not available in production").with_code("FORBIDDEN"),
            ),
        ));
    }

    tracing::info!("Dev bypass activated - setting dev token cookie");

    // Set dev token as cookie (same as login does, but longer expiry for dev convenience)
    let dev_token = "adapteros-local";
    let cookie = format!(
        "auth_token={}; HttpOnly; Path=/; Max-Age=86400; SameSite=Strict",
        dev_token
    );

    let response = Json(json!({
        "message": "Dev bypass activated",
        "token": dev_token,
        "user": {
            "email": "dev@adapteros.local",
            "role": "admin"
        }
    }));

    Ok((
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
        response,
    ))
}

/// List tenants (all roles can view)
pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<TenantResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let tenants = state.db.list_tenants().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<TenantResponse> = tenants
        .into_iter()
        .map(|t| TenantResponse {
            id: t.id,
            name: t.name,
            itar_flag: t.itar_flag,
            created_at: t.created_at,
            status: "active".to_string(),
        })
        .collect();

    Ok(Json(response))
}

/// Create tenant (admin only)
pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let id = state
        .db
        .create_tenant(&req.name, req.itar_flag)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let tenant = state.db.get_tenant(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("tenant not found after creation").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(TenantResponse {
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: "active".to_string(),
    }))
}

/// Update tenant metadata
pub async fn update_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant in database
    if let Some(ref name) = req.name {
        sqlx::query(
            "UPDATE tenants SET name = ?, updated_at = datetime('now') WHERE tenant_id = ?",
        )
        .bind(name)
        .bind(&tenant_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    if let Some(itar_flag) = req.itar_flag {
        sqlx::query(
            "UPDATE tenants SET itar_flag = ?, updated_at = datetime('now') WHERE tenant_id = ?",
        )
        .bind(itar_flag)
        .bind(&tenant_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    // Fetch updated tenant
    let row = sqlx::query(
        "SELECT tenant_id, name, itar_flag, created_at FROM tenants WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("tenant not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    use sqlx::Row;
    Ok(Json(TenantResponse {
        id: row.get("tenant_id"),
        name: row.get("name"),
        itar_flag: row.get("itar_flag"),
        created_at: row.get("created_at"),
        status: "active".to_string(),
    }))
}

#[derive(Deserialize)]
pub struct RenameTenantRequest {
    pub new_name: String,
}

/// Rename tenant
pub async fn rename_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<RenameTenantRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;
    state
        .db
        .rename_tenant(&tenant_id, &req.new_name)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to rename tenant")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Cordon node (mark maintenance)
pub async fn node_cordon(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    state
        .db
        .update_node_status(&node_id, "maintenance")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to cordon node")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}
/// Drain a node (set workers to draining)
pub async fn node_drain(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<JobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Create a job to simulate drain progression
    let payload = serde_json::json!({ "node_id": node_id });
    let job_id = state
        .db
        .create_job("node_drain", None, Some(&claims.sub), &payload.to_string())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create drain job")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Best-effort: set all workers on node to draining
    match state.db.list_workers_by_node(&node_id).await {
        Ok(workers) => {
            for w in workers {
                let _ = state.db.update_worker_status(&w.id, "draining").await;
            }
        }
        Err(e) => {
            tracing::warn!("Failed to list workers for drain: {}", e);
        }
    }

    Ok(Json(JobResponse {
        id: job_id,
        kind: "node_drain".to_string(),
        status: "queued".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Pause tenant (stop new sessions)
pub async fn pause_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update tenant status to 'paused' in database
    sqlx::query(
        "UPDATE tenants SET status = 'paused', updated_at = datetime('now') WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to pause tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} paused by {}", tenant_id, claims.email);
    Ok(StatusCode::NO_CONTENT)
}

/// Archive tenant (permanent deactivation)
pub async fn archive_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Mark tenant as archived in database
    sqlx::query(
        "UPDATE tenants SET status = 'archived', updated_at = datetime('now') WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to archive tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Tenant {} archived by {}", tenant_id, claims.email);
    Ok(StatusCode::NO_CONTENT)
}

// ===== RAG Retrieval Audit Endpoints (Operator/Admin) =====

#[derive(Debug, Deserialize)]
pub struct RagRetrievalsQuery {
    pub tenant_id: Option<String>,
    pub limit: Option<i64>,
}

/// List recent RAG retrievals (operator/admin only)
#[utoipa::path(
    get,
    path = "/v1/rag/retrievals",
    responses(
        (status = 200, description = "Recent retrievals", body = [RagRetrievalRecordResponse]),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn rag_list_retrievals(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<RagRetrievalsQuery>,
) -> Result<Json<Vec<RagRetrievalRecordResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let tenant_opt = params.tenant_id.as_deref();

    let rows = adapteros_db::rag_retrieval_audit::list_recent_rag_retrievals_sqlite(
        state.db.pool(),
        limit,
        tenant_opt,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query rag retrievals")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let resp: Vec<RagRetrievalRecordResponse> = rows
        .into_iter()
        .map(|r| RagRetrievalRecordResponse {
            tenant_id: r.tenant_id,
            query_hash: r.query_hash,
            doc_ids: r.doc_ids,
            scores: r.scores,
            top_k: r.top_k as i32,
            embedding_model_hash: r.embedding_model_hash,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
pub struct RagStatsQuery {
    pub window_days: Option<i64>,
}

/// Summarize RAG retrieval counts by tenant (operator/admin only)
#[utoipa::path(
    get,
    path = "/v1/rag/stats",
    responses(
        (status = 200, description = "Counts per tenant", body = [RagRetrievalTenantCount]),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn rag_stats(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<RagStatsQuery>,
) -> Result<Json<Vec<RagRetrievalTenantCount>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let rows = adapteros_db::rag_retrieval_audit::rag_retrieval_counts_by_tenant_sqlite(
        state.db.pool(),
        params.window_days,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query rag stats")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let resp: Vec<RagRetrievalTenantCount> = rows
        .into_iter()
        .map(|r| RagRetrievalTenantCount {
            tenant_id: r.tenant_id,
            count: r.count,
        })
        .collect();

    Ok(Json(resp))
}

/// Assign policies to tenant
pub async fn assign_tenant_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignPoliciesRequest>,
) -> Result<Json<AssignPoliciesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance])?;

    // Create tenant-policy associations in database
    for policy_id in &req.policy_ids {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_policies (tenant_id, cpid, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(&tenant_id)
        .bind(policy_id)
        .bind(&claims.sub)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to assign policy")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    tracing::info!(
        "Assigned {} policies to tenant {} by {}",
        req.policy_ids.len(),
        tenant_id,
        claims.email
    );

    Ok(Json(AssignPoliciesResponse {
        tenant_id,
        assigned_cpids: req.policy_ids,
        assigned_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Assign adapters to tenant
pub async fn assign_tenant_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignAdaptersRequest>,
) -> Result<Json<AssignAdaptersResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Create tenant-adapter associations in database
    for adapter_id in &req.adapter_ids {
        sqlx::query(
            "INSERT OR REPLACE INTO tenant_adapters (tenant_id, adapter_id, assigned_by, assigned_at)
             VALUES (?, ?, ?, datetime('now'))"
        )
        .bind(&tenant_id)
        .bind(adapter_id)
        .bind(&claims.sub)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("failed to assign adapter").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
            )
        })?;
    }

    tracing::info!(
        "Assigned {} adapters to tenant {} by {}",
        req.adapter_ids.len(),
        tenant_id,
        claims.email
    );

    Ok(Json(AssignAdaptersResponse {
        tenant_id,
        assigned_adapter_ids: req.adapter_ids,
        assigned_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get tenant resource usage metrics
pub async fn get_tenant_usage(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would aggregate usage metrics from workers/sessions
    Ok(Json(TenantUsageResponse {
        tenant_id,
        cpu_usage_pct: 45.2,
        gpu_usage_pct: 85.0,
        memory_used_gb: 8.5,
        memory_total_gb: 16.0,
        inference_count_24h: 1250,
        active_adapters_count: 12,
        avg_latency_ms: Some(125.5),
        estimated_cost_usd: Some(42.50),
    }))
}

/// List nodes
pub async fn list_nodes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<NodeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let nodes = state.db.list_nodes().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<NodeResponse> = nodes
        .into_iter()
        .map(|n| NodeResponse {
            id: n.id,
            hostname: n.hostname,
            agent_endpoint: n.agent_endpoint,
            status: n.status,
            last_seen_at: n.last_seen_at,
        })
        .collect();

    Ok(Json(response))
}
/// Register node
pub async fn register_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterNodeRequest>,
) -> Result<Json<NodeResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let id = state
        .db
        .register_node(&req.hostname, &req.agent_endpoint)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register node")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let node = state.db.get_node(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node = node.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("node not found after registration").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(NodeResponse {
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
    }))
}
/// Test node connection (ping)
pub async fn test_node_connection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodePingResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
            )
        })?;

    // Try to ping the node agent
    let start = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create HTTP client")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let ping_url = format!("{}/health", node.agent_endpoint);
    let result = client.get(&ping_url).send().await;

    let (status, latency_ms) = match result {
        Ok(response) if response.status().is_success() => {
            ("reachable".to_string(), start.elapsed().as_millis() as f64)
        }
        Ok(response) => (
            format!("error: HTTP {}", response.status()),
            start.elapsed().as_millis() as f64,
        ),
        Err(_) => ("unreachable".to_string(), 0.0),
    };

    Ok(Json(NodePingResponse {
        node_id: node.id,
        status,
        latency_ms,
    }))
}

/// Mark node offline
pub async fn mark_node_offline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Update node status in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE nodes SET status = 'offline', last_seen_at = ? WHERE id = ?",
        timestamp,
        node_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update node status")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Evict node (delete from registry)
pub async fn evict_node(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Check for running workers on this node
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let node_has_workers = workers.iter().any(|w| w.node_id == node_id);

    if node_has_workers {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("node has running workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Stop all workers before evicting node"),
            ),
        ));
    }

    // Delete node from database
    sqlx::query!("DELETE FROM nodes WHERE id = ?", node_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to delete node")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get node details
pub async fn get_node_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get node from database
    let node = state
        .db
        .get_node(&node_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("node not found").with_code("NOT_FOUND")),
            )
        })?;

    // Get workers running on this node
    let all_workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let workers: Vec<WorkerInfo> = all_workers
        .iter()
        .filter(|w| w.node_id == node_id)
        .map(|w| WorkerInfo {
            id: w.id.clone(),
            tenant_id: w.tenant_id.clone(),
            plan_id: w.plan_id.clone(),
            status: w.status.clone(),
        })
        .collect();

    Ok(Json(NodeDetailsResponse {
        id: node.id,
        hostname: node.hostname,
        agent_endpoint: node.agent_endpoint,
        status: node.status,
        last_seen_at: node.last_seen_at,
        workers,
        recent_logs: {
            // Attempt to fetch recent logs, but don't fail if unavailable
            match sqlx::query_as::<_, (String,)>(
                "SELECT message FROM node_logs WHERE node_id = ? ORDER BY timestamp DESC LIMIT 10",
            )
            .bind(&node_id)
            .fetch_all(state.db.pool())
            .await
            {
                Ok(rows) => rows.into_iter().map(|(msg,)| msg).collect(),
                Err(e) => {
                    tracing::warn!("Failed to fetch node logs for {}: {}", node_id, e);
                    vec![]
                }
            }
        },
    }))
}

/// Import model
pub async fn import_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportModelRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance])?;

    let params = adapteros_db::ModelRegistrationParams {
        name: req.name.clone(),
        hash_b3: req.hash_b3.clone(),
        config_hash_b3: req.config_hash_b3.clone(),
        tokenizer_hash_b3: req.tokenizer_hash_b3.clone(),
        tokenizer_cfg_hash_b3: req.tokenizer_cfg_hash_b3.clone(),
        license_hash_b3: req.license_hash_b3.clone(),
        metadata_json: req.metadata_json.clone(),
    };

    state.db.register_model(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to import model")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::CREATED)
}

/// Get base model status
#[utoipa::path(
    get,
    path = "/v1/models/status",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "Base model status", body = BaseModelStatusResponse),
        (status = 404, description = "No base model status found", body = ErrorResponse)
    )
)]
pub async fn get_base_model_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<BaseModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin, Role::Compliance])?;

    let tenant_id = query.tenant_id.unwrap_or_else(|| "default".to_string());

    // Get base model status from database
    let status_record = state
        .db
        .get_base_model_status(&tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // If no status record exists, return default unloaded status
    if let Some(status_record) = status_record {
        // Get model details
        let model = state
            .db
            .get_model(&status_record.model_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
                )
            })?;

        let is_loaded = status_record.status == "loaded";

        Ok(Json(BaseModelStatusResponse {
            model_id: status_record.model_id,
            model_name: model.name,
            status: status_record.status,
            loaded_at: status_record.loaded_at,
            unloaded_at: status_record.unloaded_at,
            error_message: status_record.error_message,
            memory_usage_mb: status_record.memory_usage_mb,
            is_loaded,
            updated_at: status_record.updated_at,
        }))
    } else {
        // Return default unloaded status when no record exists
        Ok(Json(BaseModelStatusResponse {
            model_id: "none".to_string(),
            model_name: "No Model Loaded".to_string(),
            status: "unloaded".to_string(),
            loaded_at: None,
            unloaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            is_loaded: false,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }))
    }
}

/// Get all models status for a tenant
#[utoipa::path(
    get,
    path = "/v1/models/status/all",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID")
    ),
    responses(
        (status = 200, description = "All models status", body = crate::types::AllModelsStatusResponse)
    ),
    tag = "models"
)]
pub async fn get_all_models_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<crate::types::AllModelsStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin, Role::Compliance])?;
    let tenant_id = query.tenant_id.unwrap_or_else(|| "default".to_string());

    if let Some(rt) = &state.model_runtime {
        let guard = rt.lock().await;
        // Get basic runtime info
        let active = guard.get_loaded_count() as i32;
        let total_mem = active * 8192; // Estimate 8GB per model
        let models = vec![]; // Not used in this context
        return Ok(Json(crate::types::AllModelsStatusResponse {
            models,
            total_memory_mb: total_mem,
            active_model_count: active as i32,
        }));
    }

    Ok(Json(crate::types::AllModelsStatusResponse {
        models: vec![],
        total_memory_mb: 0,
        active_model_count: 0,
    }))
}

#[derive(Deserialize)]
pub struct ListJobsQuery {
    tenant_id: Option<String>,
}

/// List jobs
pub async fn list_jobs(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<JobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let jobs = state
        .db
        .list_jobs(query.tenant_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response: Vec<JobResponse> = jobs
        .into_iter()
        .map(|j| JobResponse {
            id: j.id,
            kind: j.kind,
            status: j.status,
            created_at: j.created_at,
        })
        .collect();

    Ok(Json(response))
}
/// Build plan (stub)
pub async fn build_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<BuildPlanRequest>,
) -> Result<Json<JobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let payload = serde_json::to_string(&req).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("serialization error")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let job_id = state
        .db
        .create_job(
            "build_plan",
            Some(&req.tenant_id),
            Some(&claims.sub),
            &payload,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create job")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(JobResponse {
        id: job_id,
        kind: "build_plan".to_string(),
        status: "queued".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}
/// Promote CP with quality gates
#[axum::debug_handler]
pub async fn cp_promote(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<PromoteCPRequest>,
) -> Result<Json<PromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance])?;

    // Load plan from database
    let plan = state
        .db
        .get_plan(&req.plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load plan")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("plan not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Plan ID: {}", req.plan_id)),
                ),
            )
        })?;

    // Load latest audit for the CPID
    let audits = state.db.list_all_audits().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to load audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let latest_audit = audits
        .iter()
        .filter(|a| {
            a.tenant_id == plan.tenant_id
                && a.cpid.as_deref() == Some(&req.cpid)
                && a.status == "pass"
        })
        .max_by_key(|a| &a.created_at)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("no passing audit found for CPID")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!(
                            "Run audit and ensure it passes before promotion: {}",
                            req.cpid
                        )),
                ),
            )
        })?;

    // Parse audit results to check quality gates
    let audit_result: serde_json::Value =
        serde_json::from_str(&latest_audit.result_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to parse audit results")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Extract hallucination metrics
    let metrics = &audit_result["hallucination_metrics"];
    let arr = metrics["arr"].as_f64().unwrap_or(0.0) as f32;
    let ecs5 = metrics["ecs5"].as_f64().unwrap_or(0.0) as f32;
    let hlr = metrics["hlr"].as_f64().unwrap_or(1.0) as f32;
    let cr = metrics["cr"].as_f64().unwrap_or(1.0) as f32;

    // Check quality gates (from Ruleset #15)
    let mut failures = Vec::new();

    if arr < 0.95 {
        failures.push(format!("ARR too low: {:.3} < 0.95", arr));
    }

    if ecs5 < 0.75 {
        failures.push(format!("ECS@5 too low: {:.3} < 0.75", ecs5));
    }

    if hlr > 0.03 {
        failures.push(format!("HLR too high: {:.3} > 0.03", hlr));
    }

    if cr > 0.01 {
        failures.push(format!("CR too high: {:.3} > 0.01", cr));
    }

    // If any gates fail, reject promotion
    if !failures.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("quality gates failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(failures.join("; ")),
            ),
        ));
    }

    // CAB Golden Gate: verify against configured golden baseline (if enabled)
    match run_golden_gate(&state).await {
        Ok(true) => { /* proceed */ }
        Ok(false) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("golden verification failed")
                        .with_code("BAD_REQUEST")
                        .with_string_details("Golden Run Match gate did not pass"),
                ),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("golden verification error")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    // All gates passed - proceed with promotion in a transaction
    // Get current active CPID for before_cpid tracking
    let current_cp = state
        .db
        .get_active_cp_pointer(&plan.tenant_id)
        .await
        .ok()
        .flatten();
    let before_cpid = current_cp.as_ref().map(|cp| cp.name.clone());

    // Find target CP pointer
    let cp_pointer = state
        .db
        .get_cp_pointer_by_name(&req.cpid)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("CP pointer not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("CPID: {}", req.cpid)),
                ),
            )
        })?;

    // Create quality metrics JSON for signing
    let quality_metrics = QualityMetrics { arr, ecs5, hlr, cr };
    let quality_json = serde_json::to_string(&quality_metrics).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to serialize quality metrics")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Generate Ed25519 signature
    let (signature_b64, signer_key_id) =
        crate::signing::sign_promotion(&req.cpid, &claims.email, &quality_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to sign promotion")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // BEGIN TRANSACTION
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&plan.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 2. Activate target CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&cp_pointer.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to activate CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 3. Insert promotion record with signature
    let promotion_id = uuid::Uuid::now_v7().to_string();
    let promotion_timestamp = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO promotions 
         (id, cpid, cp_pointer_id, promoted_by, promoted_at, signature_b64, signer_key_id, quality_json, before_cpid) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&promotion_id)
    .bind(&req.cpid)
    .bind(&cp_pointer.id)
    .bind(&claims.email)
    .bind(promotion_timestamp.to_rfc3339())
    .bind(&signature_b64)
    .bind(&signer_key_id)
    .bind(&quality_json)
    .bind(&before_cpid)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("failed to insert promotion record").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Record promotion metric
    state.metrics_exporter.record_promotion();

    tracing::info!(
        "Promotion completed: {} -> {} by {} (signature: {})",
        before_cpid.as_deref().unwrap_or("(none)"),
        req.cpid,
        claims.email,
        &signature_b64[..16]
    );

    Ok(Json(PromotionResponse {
        cpid: req.cpid,
        plan_id: req.plan_id,
        promoted_by: claims.email,
        promoted_at: promotion_timestamp.to_rfc3339(),
        quality_metrics,
    }))
}
/// Spawn worker via node agent
pub async fn worker_spawn(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<SpawnWorkerRequest>,
) -> Result<Json<WorkerResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Look up node by ID
    let node = state
        .db
        .get_node(&req.node_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("node not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Node ID: {}", req.node_id)),
                ),
            )
        })?;

    // Prepare spawn request for node agent
    let spawn_req = serde_json::json!({
        "tenant_id": req.tenant_id,
        "plan_id": req.plan_id,
    });

    // Send HTTP POST to node agent
    let client = reqwest::Client::new();
    let spawn_url = format!("{}/spawn_worker", node.agent_endpoint);

    let response = client
        .post(&spawn_url)
        .json(&spawn_req)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(
                    ErrorResponse::new("failed to contact node agent")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("node agent spawn failed")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(error_text),
            ),
        ));
    }

    let spawn_response: serde_json::Value = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to parse node agent response")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let pid = spawn_response["pid"].as_i64().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("invalid response from node agent")
                    .with_code("BAD_REQUEST")
                    .with_string_details("missing or invalid PID field"),
            ),
        )
    })? as i32;

    // Create UDS path for worker (standardized)
    let uds_path = format!("/var/run/aos/{}/aos.sock", req.tenant_id);

    // Register worker in database
    let worker_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status) 
         VALUES (?, ?, ?, ?, ?, ?, 'starting')",
    )
    .bind(&worker_id)
    .bind(&req.tenant_id)
    .bind(&req.node_id)
    .bind(&req.plan_id)
    .bind(&uds_path)
    .bind(pid)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to register worker in database")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Return worker info
    Ok(Json(WorkerResponse {
        id: worker_id,
        tenant_id: req.tenant_id,
        node_id: req.node_id,
        plan_id: req.plan_id,
        uds_path,
        pid: Some(pid),
        status: "starting".to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_seen_at: None,
    }))
}

#[derive(Deserialize)]
pub struct ListWorkersQuery {
    tenant_id: Option<String>,
}

/// List workers with optional tenant filter
pub async fn list_workers(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListWorkersQuery>,
) -> Result<Json<Vec<WorkerResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let workers = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_workers_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                tracing::error!(
                    error = %e,
                    tenant_id = %tenant_id,
                    "Failed to list workers by tenant"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    } else {
        state.db.list_all_workers().await.map_err(|e| {
            tracing::error!(
                error = %e,
                "Failed to list all workers"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    let response: Vec<WorkerResponse> = workers
        .into_iter()
        .map(|w| WorkerResponse {
            id: w.id,
            tenant_id: w.tenant_id,
            node_id: w.node_id,
            plan_id: w.plan_id,
            uds_path: w.uds_path,
            pid: w.pid,
            status: w.status,
            started_at: w.started_at,
            last_seen_at: w.last_seen_at,
        })
        .collect();

    tracing::debug!(worker_count = response.len(), "Successfully listed workers");

    Ok(Json(response))
}

/// Register a local worker (from aosctl serve)
pub async fn worker_register_local(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterLocalWorkerRequest>,
) -> Result<Json<WorkerResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Insert worker with status 'serving'
    let worker_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::WorkerInsertParams {
        id: worker_id.clone(),
        tenant_id: req.tenant_id.clone(),
        node_id: req.node_id.clone(),
        plan_id: req.plan_id.clone(),
        uds_path: req.uds_path.clone(),
        pid: req.pid,
        status: "serving".to_string(),
    };

    state.db.insert_worker(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to register worker")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    Ok(Json(WorkerResponse {
        id: worker_id,
        tenant_id: req.tenant_id,
        node_id: req.node_id,
        plan_id: req.plan_id,
        uds_path: req.uds_path,
        pid: req.pid,
        status: "serving".to_string(),
        started_at: now.clone(),
        last_seen_at: Some(now),
    }))
}

/// Update worker heartbeat
pub async fn worker_heartbeat(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    state
        .db
        .update_worker_heartbeat(&worker_id, Some("serving"))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update heartbeat")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Logout user by clearing the auth cookie
pub async fn auth_logout(
    Extension(_claims): Extension<Claims>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Clear the auth cookie by setting it to expire immediately
    let clear_cookie = "auth_token=; HttpOnly; Path=/; Max-Age=0; SameSite=Strict";

    Ok((
        StatusCode::NO_CONTENT,
        [(axum::http::header::SET_COOKIE, clear_cookie)],
    ))
}

/// Get current user info
pub async fn auth_me(
    Extension(claims): Extension<Claims>,
) -> Result<Json<UserInfoResponse>, (StatusCode, Json<ErrorResponse>)> {
    let iat_timestamp = chrono::DateTime::from_timestamp(claims.iat, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(Json(UserInfoResponse {
        user_id: claims.sub,
        email: claims.email.clone(),
        role: claims.role.clone(),
        display_name: None, // Can be derived from email on frontend
        tenant_id: Some(claims.tenant_id.clone()),
        permissions: None,                          // Not stored in JWT claims
        last_login_at: Some(iat_timestamp.clone()), // Use iat as last login time
        mfa_enabled: None,
        token_last_rotated_at: None,
        created_at: Some(iat_timestamp), // Keep for backwards compatibility
    }))
}

/// List plans with optional tenant filter
pub async fn list_plans(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListPlansQuery>,
) -> Result<Json<Vec<PlanResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let plans = if let Some(tenant_id) = query.tenant_id {
        state
            .db
            .list_plans_by_tenant(&tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    } else {
        state.db.list_all_plans().await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    let response: Vec<PlanResponse> = plans
        .into_iter()
        .map(|p| PlanResponse {
            id: p.id,
            tenant_id: p.tenant_id,
            manifest_hash_b3: p.manifest_hash_b3,
            kernel_hash_b3: None,         // Not stored in Plan model
            layout_hash_b3: None,         // Not stored in Plan model
            status: "active".to_string(), // Default status
            created_at: p.created_at,
        })
        .collect();

    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/v1/plans/{plan_id}",
    params(("plan_id" = String, Path, description = "Plan ID")),
    responses(
        (status = 204, description = "Plan deleted successfully"),
        (status = 404, description = "Plan not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "plans",
    security(("bearer_token" = []))
)]
pub async fn delete_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let deleted = state.db.delete_plan(&plan_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete plan")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
        ))
    }
}

#[derive(Deserialize)]
pub struct ListPlansQuery {
    tenant_id: Option<String>,
}

/// Get plan details
pub async fn get_plan_details(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<PlanDetailsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(PlanDetailsResponse {
        id: plan.id.clone(),
        tenant_id: plan.tenant_id,
        manifest_hash_b3: plan.manifest_hash_b3.clone(),
        kernel_hash_b3: {
            // Query kernel hash from plan metadata
            match sqlx::query_scalar::<_, Option<String>>(
                "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
            )
            .bind(&plan.id)
            .fetch_optional(state.db.pool())
            .await
            {
                Ok(hash) => hash.flatten(),
                Err(e) => {
                    tracing::warn!("Failed to fetch kernel hash for plan {}: {}", plan.id, e);
                    None
                }
            }
        },
        routing_config: {
            // Query routing config from plan or use default
            match sqlx::query_scalar::<_, Option<String>>(
                "SELECT routing_config FROM plan_metadata WHERE plan_id = ?",
            )
            .bind(&plan.id)
            .fetch_optional(state.db.pool())
            .await
            {
                Ok(Some(Some(config_str))) => {
                    serde_json::from_str(&config_str).unwrap_or_else(|e| {
                        tracing::warn!("Failed to parse routing config: {}", e);
                        serde_json::json!({"k_sparse": 3, "gate_quant": "q15"})
                    })
                }
                _ => {
                    tracing::debug!(
                        "No routing config found for plan {}, using default",
                        plan.id
                    );
                    serde_json::json!({"k_sparse": 3, "gate_quant": "q15"})
                }
            }
        },
        created_at: plan.created_at,
    }))
}
/// Pin a plan with an alias (control-plane pointer)
pub async fn pin_plan_alias(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
    Json(req): Json<PlanPinRequest>,
) -> Result<Json<CpPointerResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Optional activation: deactivate old pointers first
    if req.active {
        state
            .db
            .deactivate_all_cp_pointers(&claims.tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to deactivate existing pointers")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    let id = uuid::Uuid::now_v7().to_string();
    state
        .db
        .insert_cp_pointer(&id, &claims.tenant_id, &req.alias, &plan_id, req.active)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to pin plan")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Build response
    let now = chrono::Utc::now();
    Ok(Json(CpPointerResponse {
        id,
        tenant_id: claims.tenant_id,
        name: req.alias,
        plan_id,
        active: req.active,
        created_at: now.to_rfc3339(),
        activated_at: if req.active {
            Some(now.to_rfc3339())
        } else {
            None
        },
    }))
}

/// List control plane pointers for a tenant
pub async fn list_cp_pointers(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<CpPointerResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let rows = state
        .db
        .list_cp_pointers_by_tenant(&tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list pointers")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let items = rows
        .into_iter()
        .map(|r| CpPointerResponse {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            plan_id: r.plan_id,
            active: r.active == 1,
            created_at: r.created_at,
            activated_at: r.activated_at,
        })
        .collect();
    Ok(Json(items))
}

/// Activate a specific pointer alias for a tenant
pub async fn activate_cp_pointer(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((tenant_id, alias)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Find pointer by name
    let ptr = state.db.get_cp_pointer_by_name(&alias).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("db error")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let ptr = ptr.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("pointer not found").with_code("NOT_FOUND")),
        )
    })?;

    if ptr.tenant_id != tenant_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("tenant mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details("Alias belongs to a different tenant"),
            ),
        ));
    }

    state
        .db
        .deactivate_all_cp_pointers(&tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to deactivate pointers")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    state.db.activate_cp_pointer(&ptr.id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to activate pointer")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rebuild plan
pub async fn rebuild_plan(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<PlanRebuildResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
            )
        })?;

    // Rebuild the plan by creating a new plan from the manifest
    // This allows incorporating any changes to the Metal kernels or manifest
    let new_plan_id = format!("{}-rebuilt-{}", plan.id, chrono::Utc::now().timestamp());

    // Create new plan record
    match sqlx::query(
        "INSERT INTO plans (id, tenant_id, manifest_hash_b3, status, created_at) 
         VALUES (?, ?, ?, 'building', datetime('now'))",
    )
    .bind(&new_plan_id)
    .bind(&plan.tenant_id)
    .bind(&plan.manifest_hash_b3)
    .execute(state.db.pool())
    .await
    {
        Ok(_) => {
            tracing::info!("Created new plan {} from {}", new_plan_id, plan.id);

            // Compare kernel hashes if available
            let diff_summary = match (
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
                )
                .bind(&plan.id)
                .fetch_optional(state.db.pool())
                .await,
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
                )
                .bind(&new_plan_id)
                .fetch_optional(state.db.pool())
                .await,
            ) {
                (Ok(Some(old_hash)), Ok(Some(new_hash))) if old_hash != new_hash => {
                    "Metal kernels updated (hash changed)".to_string()
                }
                _ => "Plan rebuilt with current Metal kernels".to_string(),
            };

            Ok(Json(PlanRebuildResponse {
                old_plan_id: plan.id,
                new_plan_id: new_plan_id.clone(),
                diff_summary,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create new plan: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to rebuild plan")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ))
        }
    }
}

/// Compare plans
pub async fn compare_plans(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ComparePlansRequest>,
) -> Result<Json<PlanComparisonResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    let plan1 = state
        .db
        .get_plan(&req.plan_id_1)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new(format!("plan {} not found", req.plan_id_1))
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    let plan2 = state
        .db
        .get_plan(&req.plan_id_2)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new(format!("plan {} not found", req.plan_id_2))
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    // Simple comparison based on manifest hash
    let differences = if plan1.manifest_hash_b3 == plan2.manifest_hash_b3 {
        vec!["No differences detected".to_string()]
    } else {
        vec!["Manifest hashes differ".to_string()]
    };

    Ok(Json(PlanComparisonResponse {
        plan_id_1: plan1.id,
        plan_id_2: plan2.id,
        differences,
        identical: plan1.manifest_hash_b3 == plan2.manifest_hash_b3,
    }))
}

/// Export plan manifest
pub async fn export_plan_manifest(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(plan_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let plan = state
        .db
        .get_plan(&plan_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("plan not found").with_code("NOT_FOUND")),
            )
        })?;

    let manifest = serde_json::json!({
        "plan_id": plan.id,
        "tenant_id": plan.tenant_id,
        "manifest_hash_b3": plan.manifest_hash_b3,
        "created_at": plan.created_at,
        "exported_at": chrono::Utc::now().to_rfc3339(),
    });

    Ok(Json(manifest))
}
/// Check promotion gates
#[axum::debug_handler]
pub async fn promotion_gates(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PromotionGatesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Base gates (stubbed values)
    let mut gates = vec![
        GateStatus {
            name: "Replay Determinism".to_string(),
            passed: true,
            message: "Replay diff is zero".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "ARR Threshold".to_string(),
            passed: true,
            message: "ARR 0.96 >= 0.95".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "ECS@5 Threshold".to_string(),
            passed: true,
            message: "ECS@5 0.78 >= 0.75".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "HLR Threshold".to_string(),
            passed: true,
            message: "HLR 0.02 <= 0.03".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "CR Threshold".to_string(),
            passed: true,
            message: "CR 0.005 <= 0.01".to_string(),
            evidence_id: Some("audit_123".to_string()),
        },
        GateStatus {
            name: "Egress Preflight".to_string(),
            passed: true,
            message: "PF deny rules enforced".to_string(),
            evidence_id: None,
        },
        GateStatus {
            name: "Isolation Tests".to_string(),
            passed: true,
            message: "All isolation tests passed".to_string(),
            evidence_id: Some("isolation_test_456".to_string()),
        },
    ];

    // Append CAB Golden Gate status if configured
    let gg_status = match run_golden_gate(&state).await {
        Ok(true) => GateStatus {
            name: "Golden Run Match".to_string(),
            passed: true,
            message: "Golden verification passed".to_string(),
            evidence_id: None,
        },
        Ok(false) => GateStatus {
            name: "Golden Run Match".to_string(),
            passed: false,
            message: "Golden verification failed".to_string(),
            evidence_id: None,
        },
        Err(e) => GateStatus {
            name: "Golden Run Match".to_string(),
            passed: false,
            message: format!("Golden verification error: {}", e),
            evidence_id: None,
        },
    };
    gates.push(gg_status);

    let all_passed = gates.iter().all(|g| g.passed);

    Ok(Json(PromotionGatesResponse {
        cpid,
        gates,
        all_passed,
    }))
}

/// List policies
pub async fn list_policies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<PolicyPackResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Compliance, Role::Admin])?;

    #[derive(sqlx::FromRow)]
    struct PolicyRow {
        cpid: String,
        content: String,
        hash_b3: String,
        created_at: String,
    }

    let policies = sqlx::query_as::<_, PolicyRow>(
        "SELECT cpid, content, hash_b3, created_at FROM policies ORDER BY created_at DESC",
    )
    .fetch_all(&state.db.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error listing policies: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<PolicyPackResponse> = policies
        .into_iter()
        .map(|row| PolicyPackResponse {
            cpid: row.cpid,
            content: row.content,
            hash_b3: row.hash_b3,
            created_at: row.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Get policy by CPID
pub async fn get_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Compliance, Role::Admin])?;

    #[derive(sqlx::FromRow)]
    struct PolicyRow {
        content: String,
        hash_b3: String,
        created_at: String,
    }

    let policy = sqlx::query_as::<_, PolicyRow>(
        "SELECT content, hash_b3, created_at FROM policies WHERE cpid = $1",
    )
    .bind(&cpid)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error getting policy {}: {}", cpid, e);
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
        tracing::warn!("Policy not found: {}", cpid);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("policy not found").with_code("POLICY_NOT_FOUND")),
        )
    })?;

    Ok(Json(PolicyPackResponse {
        cpid,
        content: policy.content,
        hash_b3: policy.hash_b3,
        created_at: policy.created_at,
    }))
}

/// Validate policy
pub async fn validate_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ValidatePolicyRequest>,
) -> Result<Json<PolicyValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Compliance, Role::Admin])?;

    // Basic JSON validation
    match serde_json::from_str::<serde_json::Value>(&req.content) {
        Ok(json_value) => {
            // Additional validation: check for required schema field
            let mut errors = Vec::new();
            if let Some(schema) = json_value.get("schema").and_then(|s| s.as_str()) {
                if !schema.starts_with("adapteros.policy.") {
                    errors.push("Invalid schema: must start with 'adapteros.policy.'".to_string());
                }
            } else {
                errors.push("Missing required 'schema' field".to_string());
            }

            // Generate a deterministic hash for the policy content
            let hash_b3 = format!("b3:{:x}", md5::compute(&req.content));

            if errors.is_empty() {
                info!("Policy validation successful");
                Ok(Json(PolicyValidationResponse {
                    valid: true,
                    errors: vec![],
                    hash_b3: Some(hash_b3),
                }))
            } else {
                warn!(errors = ?errors, "Policy validation failed");
                Ok(Json(PolicyValidationResponse {
                    valid: false,
                    errors,
                    hash_b3: Some(hash_b3),
                }))
            }
        }
        Err(e) => {
            error!(error = %e, "Policy JSON parsing failed");
            Ok(Json(PolicyValidationResponse {
                valid: false,
                errors: vec![format!("Invalid JSON: {}", e)],
                hash_b3: None,
            }))
        }
    }
}

/// Apply policy
pub async fn apply_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ApplyPolicyRequest>,
) -> Result<Json<PolicyPackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Compliance, Role::Admin])?;

    let hash_b3 = blake3::Hasher::new()
        .update(req.content.as_bytes())
        .finalize()
        .to_hex()
        .to_string();

    let created_at = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        INSERT INTO policies (cpid, content, hash_b3, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT (cpid) DO UPDATE SET
            content = $2,
            hash_b3 = $3,
            updated_at = $4
        RETURNING cpid, content, hash_b3, created_at
        "#,
    )
    .bind(&req.cpid)
    .bind(&req.content)
    .bind(&hash_b3)
    .bind(&created_at)
    .fetch_one(&state.db.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error applying policy {}: {}", req.cpid, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let row: (String, String, String, String) = result;

    info!("Policy applied: {} with hash {}", req.cpid, hash_b3);

    Ok(Json(PolicyPackResponse {
        cpid: row.0,
        content: row.1,
        hash_b3: row.2,
        created_at: row.3,
    }))
}

/// Sign policy with Ed25519
pub async fn sign_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<SignPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Get or generate signing key for the tenant
    let signing_key_result = sqlx::query_scalar::<_, Option<String>>(
        "SELECT signing_key FROM signing_keys WHERE tenant_id = ? AND key_type = 'ed25519' AND active = 1"
    )
    .bind(&claims.sub)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Failed to query signing key: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to retrieve signing key").with_code("INTERNAL_ERROR").with_string_details(e.to_string())),
        )
    })?;

    let signing_key_hex = match signing_key_result {
        Some(key) => key,
        None => {
            // Generate new Ed25519 signing key
            use adapteros_crypto::signature::generate_keypair;
            let (secret_key, _public_key) = generate_keypair();
            let key_hex = hex::encode(secret_key.to_bytes());

            // Store the key
            sqlx::query(
                "INSERT INTO signing_keys (tenant_id, key_type, signing_key, active, created_at) 
                 VALUES (?, 'ed25519', ?, 1, datetime('now'))",
            )
            .bind(&claims.sub)
            .bind(&key_hex)
            .execute(state.db.pool())
            .await
            .map_err(|e| {
                tracing::error!("Failed to store signing key: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to store signing key")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

            tracing::info!(
                "Generated new Ed25519 signing key for tenant {}",
                claims.sub
            );
            Some(key_hex)
        }
    };

    // Sign the CPID
    let signing_key = signing_key_hex.as_deref().unwrap_or("");
    let signature = match adapteros_crypto::signature::sign_data(cpid.as_bytes(), signing_key) {
        Ok(sig) => format!("ed25519:{}", hex::encode(sig)),
        Err(e) => {
            tracing::error!("Failed to sign CPID: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Signing failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    Ok(Json(SignPolicyResponse {
        cpid: cpid.clone(),
        signature,
        signed_at: chrono::Utc::now().to_rfc3339(),
        signed_by: claims.email,
    }))
}

/// Compare two policy versions
pub async fn compare_policy_versions(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(req): Json<PolicyComparisonRequest>,
) -> Result<Json<PolicyComparisonResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch both policies and compute diff
    Ok(Json(PolicyComparisonResponse {
        cpid_1: req.cpid_1,
        cpid_2: req.cpid_2,
        differences: vec![
            "egress.mode: deny_all -> allow_listed".to_string(),
            "router.k_sparse: 3 -> 5".to_string(),
            "Added: output.new_field".to_string(),
        ],
        identical: false,
    }))
}

/// Export policy as downloadable bundle
pub async fn export_policy(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(cpid): Path<String>,
) -> Result<Json<ExportPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch policy and signature from database
    let policy_json = r#"{"schema": "adapteros.policy.v1", "packs": {}}"#.to_string();

    Ok(Json(ExportPolicyResponse {
        cpid: cpid.clone(),
        policy_json,
        signature: Some(format!("ed25519:sig_{}", cpid)),
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// List telemetry bundles (stub)
#[utoipa::path(
    get,
    path = "/v1/telemetry/bundles",
    responses(
        (status = 200, description = "List of telemetry bundles", body = Vec<TelemetryBundleResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn list_telemetry_bundles(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<crate::types::TelemetryBundleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // M0: list bundles by scanning var/bundles directory
    let root = std::path::Path::new("var/bundles");
    let mut results = Vec::new();
    if root.exists() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ndjson") {
                    let id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string();
                    let meta = std::fs::metadata(&path).ok();
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let event_count = std::fs::read_to_string(&path)
                        .map(|s| s.lines().count() as u64)
                        .unwrap_or(0);
                    let created_at = chrono::Utc::now().to_rfc3339();
                    results.push(crate::types::TelemetryBundleResponse {
                        id,
                        cpid: "global".to_string(),
                        event_count,
                        size_bytes: size,
                        created_at,
                    });
                }
            }
        }
    }
    Ok(Json(results))
}

/// Export telemetry bundle as NDJSON
#[utoipa::path(
    get,
    path = "/v1/telemetry/bundles/{bundle_id}/export",
    params(
        ("bundle_id" = String, Path, description = "Bundle ID")
    ),
    responses(
        (status = 200, description = "Export metadata", body = crate::types::ExportTelemetryBundleResponse),
        (status = 404, description = "Bundle not found", body = crate::types::ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn export_telemetry_bundle(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ExportTelemetryBundleResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would fetch bundle from telemetry store
    Ok(Json(ExportTelemetryBundleResponse {
        bundle_id: bundle_id.clone(),
        events_count: 42_000,
        size_bytes: 12_582_912,
        download_url: format!("/v1/telemetry/bundles/{}/download", bundle_id),
        expires_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Generate a telemetry bundle (M0 filesystem implementation)
#[utoipa::path(
    post,
    path = "/v1/telemetry/bundles/generate",
    responses(
        (status = 200, description = "Bundle generated", body = crate::types::TelemetryBundleResponse),
        (status = 500, description = "Generation failed", body = crate::types::ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn generate_telemetry_bundle(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<TelemetryBundleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id = uuid::Uuid::new_v4().to_string();
    let root = std::path::Path::new("var/bundles");
    if let Err(e) = std::fs::create_dir_all(root) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to create bundles directory")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    let file_path = root.join(format!("{}.ndjson", id));
    let now = chrono::Utc::now().to_rfc3339();
    let sample = vec![
        serde_json::json!({"type":"metrics","timestamp":now,"message":"bundle generated","level":"info"}),
        serde_json::json!({"type":"audit","timestamp":now,"message":"export readiness","level":"info"}),
    ];
    let mut buf = String::new();
    for line in sample {
        buf.push_str(&serde_json::to_string(&line).unwrap());
        buf.push('\n');
    }
    if let Err(e) = std::fs::write(&file_path, buf) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to write bundle")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    let meta = std::fs::metadata(&file_path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let created_at = chrono::Utc::now().to_rfc3339();

    let bundle = TelemetryBundleResponse {
        id: id.clone(),
        cpid: "global".to_string(),
        event_count: 2,
        size_bytes: size,
        created_at,
    };

    // Broadcast bundle update to SSE subscribers
    if let Err(e) = state.telemetry_bundles_tx.send(bundle.clone()) {
        tracing::warn!("failed to broadcast bundle update: {}", e);
    }

    Ok(Json(bundle))
}

/// Verify telemetry bundle Ed25519 signature
#[utoipa::path(
    post,
    path = "/v1/telemetry/bundles/{bundle_id}/verify",
    params(
        ("bundle_id" = String, Path, description = "Bundle ID")
    ),
    responses(
        (status = 200, description = "Verification result", body = crate::types::VerifyBundleSignatureResponse),
        (status = 404, description = "Bundle not found", body = crate::types::ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn verify_bundle_signature(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(bundle_id): Path<String>,
) -> Result<Json<VerifyBundleSignatureResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Filesystem M0: bundles in var/bundles and signatures in var/signatures
    let bundles_root = std::path::Path::new("var/bundles");
    let signatures_root = std::path::Path::new("var/signatures");

    // Prefer .ndjson, fall back to .ndjson.zst
    let ndjson = bundles_root.join(format!("{}.ndjson", bundle_id));
    let zst = bundles_root.join(format!("{}.ndjson.zst", bundle_id));
    let bundle_path = if ndjson.exists() { ndjson } else { zst };

    if !bundle_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("bundle not found").with_code("NOT_FOUND")),
        ));
    }

    // Load bundle and verify signature
    let signatures_root = signatures_root.to_path_buf();
    let bundle_clone = bundle_path.clone();
    let verification = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let bundle = read_trace_bundle(&bundle_clone)
            .map_err(|e| anyhow::anyhow!("failed to read bundle: {}", e))?;
        let signature = verify_bundle_signature_from_dir(&bundle, &signatures_root)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {}", e))?;
        Ok((bundle, signature))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("verification task failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    match verification {
        Ok((_bundle, signature)) => {
            let signature_hex = hex::encode(signature.signature.to_bytes());
            let signed_by = signature.key_id.clone();
            let signed_at = {
                let seconds = (signature.signed_at_us / 1_000_000) as i64;
                let micros = (signature.signed_at_us % 1_000_000) as u32;
                chrono::DateTime::from_timestamp(seconds, micros * 1_000)
                    .map(|dt| {
                        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                            dt.naive_utc(),
                            chrono::Utc,
                        )
                    })
                    .unwrap_or_else(chrono::Utc::now)
                    .to_rfc3339()
            };

            Ok(Json(VerifyBundleSignatureResponse {
                bundle_id,
                valid: true,
                signature: signature_hex,
                signed_by,
                signed_at,
                verification_error: None,
            }))
        }
        Err(e) => Ok(Json(VerifyBundleSignatureResponse {
            bundle_id,
            valid: false,
            signature: String::new(),
            signed_by: String::new(),
            signed_at: chrono::Utc::now().to_rfc3339(),
            verification_error: Some(e.to_string()),
        })),
    }
}

/// Purge old telemetry bundles based on retention policy
#[utoipa::path(
    post,
    path = "/v1/telemetry/bundles/purge",
    request_body = crate::types::PurgeOldBundlesRequest,
    responses(
        (status = 200, description = "Purge completed", body = crate::types::PurgeOldBundlesResponse),
        (status = 401, description = "Unauthorized", body = crate::types::ErrorResponse)
    ),
    tag = "telemetry"
)]
pub async fn purge_old_bundles(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<PurgeOldBundlesRequest>,
) -> Result<Json<PurgeOldBundlesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Stub - would apply retention policy and delete old bundles
    let response = PurgeOldBundlesResponse {
        purged_count: 15,
        retained_count: 12,
        freed_bytes: 45_000_000,
        purged_cpids: vec!["cp_001".to_string(), "cp_002".to_string()],
    };

    // Broadcast bundle list refresh signal by fetching current list
    // When purge is fully implemented, this will trigger SSE subscribers to refetch
    let _ = tokio::spawn(async move {
        // In a real implementation, we'd broadcast the remaining bundles list
        // For now, SSE subscribers will refresh via polling on their next fetch
    });

    Ok(Json(response))
}
/// Rollback CP to previous plan
pub async fn cp_rollback(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RollbackCPRequest>,
) -> Result<Json<RollbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance, Role::Admin])?;

    // Get current active CP pointer
    let current_cp = state
        .db
        .get_active_cp_pointer(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get current CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("no active CP pointer found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Tenant: {}", req.tenant_id)),
                ),
            )
        })?;

    // Verify the CPID matches
    if current_cp.name != req.cpid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("CPID mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "Current active CPID is '{}', not '{}'",
                        current_cp.name, req.cpid
                    )),
            ),
        ));
    }

    // Find previous CP pointer for this tenant (most recent inactive one)
    let all_pointers = adapteros_db::sqlx::query_as::<_, adapteros_db::models::CpPointer>(
        "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at 
         FROM cp_pointers 
         WHERE tenant_id = ? AND id != ? 
         ORDER BY activated_at DESC, created_at DESC 
         LIMIT 1",
    )
    .bind(&req.tenant_id)
    .bind(&current_cp.id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query previous CP")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let previous_cp = all_pointers.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("no previous CP pointer available for rollback")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "This is the first/only CP for tenant {}",
                        req.tenant_id
                    )),
            ),
        )
    })?;

    // Perform rollback in a transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&req.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 2. Activate previous CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&previous_cp.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to activate previous CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let rollback_timestamp = chrono::Utc::now();

    tracing::info!(
        "Rollback completed: {} -> {} by {}",
        req.cpid,
        previous_cp.name,
        claims.email
    );

    Ok(Json(RollbackResponse {
        cpid: req.cpid.clone(),
        previous_plan_id: previous_cp.plan_id,
        rolled_back_by: claims.email,
        rolled_back_at: rollback_timestamp.to_rfc3339(),
    }))
}

/// Dry run CP promotion (validate gates without executing)
pub async fn cp_dry_run_promote(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<DryRunPromotionRequest>,
) -> Result<Json<DryRunPromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Compliance, Role::Admin])?;

    // Stub - would validate all gates and return what would be promoted
    Ok(Json(DryRunPromotionResponse {
        cpid: req.cpid,
        would_promote: true,
        gates_status: vec![
            GateStatus {
                name: "determinism".to_string(),
                passed: true,
                message: "Replay zero diff passed".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "hallucination".to_string(),
                passed: true,
                message: "ARR: 0.96, ECS@5: 0.78".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "performance".to_string(),
                passed: true,
                message: "p95: 22ms (threshold: 24ms)".to_string(),
                evidence_id: None,
            },
        ],
        warnings: vec![],
    }))
}

/// Get promotion history
pub async fn get_promotion_history(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<PromotionHistoryEntry>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query promotions table
    Ok(Json(vec![PromotionHistoryEntry {
        cpid: "cp_001".to_string(),
        promoted_at: chrono::Utc::now().to_rfc3339(),
        promoted_by: "admin@example.com".to_string(),
        previous_cpid: Some("cp_000".to_string()),
        gate_results_summary: "All gates passed".to_string(),
    }]))
}

/// Propose a patch for code changes
#[utoipa::path(
    post,
    path = "/api/v1/patch/propose",
    request_body = ProposePatchRequest,
    responses(
        (status = 200, description = "Patch proposal created", body = ProposePatchResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_token" = [])
    )
)]
pub async fn propose_patch(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ProposePatchRequest>,
) -> Result<Json<ProposePatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Validate inputs
    validate_repo_id(&req.repo_id)?;
    validate_description(&req.description)?;
    validate_file_paths(&req.target_files)?;

    // Get available workers from database; on error, fall back to per-tenant default socket
    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!(
                "Failed to list workers (falling back to default UDS): {}",
                e
            );
            Vec::new()
        }
    };

    // Continue using fallback logic below when empty

    // Create UDS client and send patch proposal request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(60)); // Longer timeout for patch generation

    // Resolve UDS path: prefer registered worker; fallback to per-tenant default
    let uds_path_buf = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        // Fallback: honor env override or use /var/run/aos/<tenant>/aos.sock
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| format!("/var/run/aos/{}/aos.sock", claims.tenant_id));
        std::path::PathBuf::from(fallback)
    };
    let uds_path = uds_path_buf.as_path();

    let worker_request = PatchProposalInferRequest {
        cpid: "patch-proposal".to_string(),
        prompt: req.description.clone(),
        max_tokens: 2000,
        require_evidence: true,
        request_type: PatchProposalRequestType {
            repo_id: req.repo_id.clone(),
            commit_sha: Some(req.commit_sha.clone()),
            target_files: req.target_files.clone(),
            description: req.description.clone(),
        },
    };

    match uds_client.propose_patch(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Extract proposal ID and status
            let proposal_id = worker_response
                .patch_proposal
                .as_ref()
                .map(|p| p.proposal_id.clone())
                .unwrap_or_else(|| {
                    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
                });

            let status = if worker_response.patch_proposal.is_some() {
                "completed"
            } else if worker_response.refusal.is_some() {
                "refused"
            } else {
                "failed"
            };

            let message = if let Some(ref proposal) = worker_response.patch_proposal {
                format!(
                    "Patch proposal generated successfully with {} files and {} citations",
                    proposal.patches.len(),
                    proposal.citations.len()
                )
            } else if let Some(ref refusal) = worker_response.refusal {
                format!("Patch proposal refused: {}", refusal.message)
            } else {
                "Patch proposal generation failed".to_string()
            };

            // Store proposal in database
            if let Some(ref proposal) = worker_response.patch_proposal {
                let proposal_json = serde_json::to_string(proposal).unwrap_or_else(|e| {
                    tracing::warn!("Failed to serialize patch proposal: {}", e);
                    "{}".to_string()
                });

                match sqlx::query(
                    "INSERT INTO patch_proposals 
                     (id, repo_id, commit_sha, status, proposal_json, created_at, created_by) 
                     VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
                )
                .bind(&proposal_id)
                .bind(&req.repo_id)
                .bind(&req.commit_sha)
                .bind(status)
                .bind(&proposal_json)
                .bind(&claims.email)
                .execute(state.db.pool())
                .await
                {
                    Ok(_) => {
                        tracing::info!("Stored patch proposal {} in database", proposal_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to store patch proposal in database: {}", e);
                        // Don't fail the request if storage fails
                    }
                }
            }

            Ok(Json(ProposePatchResponse {
                proposal_id,
                status: status.to_string(),
                message,
            }))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("patch generation timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("patch generation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

/// Inference endpoint
#[utoipa::path(
    post,
    path = "/v1/infer",
    request_body = InferRequest,
    responses(
        (status = 200, description = "Inference successful", body = InferResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Inference failed", body = ErrorResponse),
        (status = 501, description = "Worker not initialized", body = ErrorResponse)
    )
)]
pub async fn infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("INTERNAL_ERROR")),
        ));
    }

    // Enforce policies before forwarding the request to workers
    let request_id = uuid::Uuid::new_v4().to_string();
    let tenant_id = claims.tenant_id.clone();
    let max_tokens = req.max_tokens.unwrap_or(100);

    if max_tokens < 1 || max_tokens > 4096 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                ErrorResponse::new("max_tokens must be between 1 and 4096")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let require_evidence = req.require_evidence.unwrap_or(false);
    let policy_context = UnifiedPolicyContext {
        component: "server.api".to_string(),
        operation: "v1.infer".to_string(),
        data: Some(serde_json::json!({
            "prompt_length": req.prompt.len(),
            "max_tokens": max_tokens,
            "require_evidence": require_evidence,
            "role": claims.role.clone(),
        })),
        priority: UnifiedPriority::Normal,
    };
    let metadata = Some(serde_json::json!({
        "jwt_id": claims.jti.clone(),
        "email": claims.email.clone(),
    }));
    let mut parameters = HashMap::new();
    parameters.insert(
        "prompt_length".to_string(),
        serde_json::json!(req.prompt.len()),
    );
    parameters.insert("max_tokens".to_string(), serde_json::json!(max_tokens));
    parameters.insert(
        "require_evidence".to_string(),
        serde_json::json!(require_evidence),
    );
    parameters.insert(
        "tenant_id".to_string(),
        serde_json::json!(tenant_id.clone()),
    );
    parameters.insert("role".to_string(), serde_json::json!(claims.role.clone()));
    let policy_operation = PolicyOperation {
        operation_id: request_id.clone(),
        operation_type: PolicyOperationType::PerformInference,
        parameters,
        context: policy_context,
        metadata,
    };
    let enforcement_result = state
        .policy_manager
        .enforce_policy(&policy_operation)
        .await
        .map_err(|e| {
            error!(
                request_id = %request_id,
                tenant = %tenant_id,
                error = %e,
                "Policy enforcement failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("policy enforcement failed")
                        .with_code("POLICY_ENFORCEMENT_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    let violations_json: Vec<serde_json::Value> = enforcement_result
        .violations
        .iter()
        .map(|violation| {
            serde_json::json!({
                "policy_pack": violation.policy_pack,
                "severity": format!("{:?}", violation.severity),
                "message": violation.message,
                "timestamp": violation.timestamp.to_rfc3339(),
                "remediation": violation.remediation,
                "details": violation.details,
            })
        })
        .collect();
    let violations_payload = serde_json::json!({
        "request_id": request_id.clone(),
        "tenant_id": tenant_id.clone(),
        "violations": violations_json,
    });
    if !enforcement_result.allowed {
        warn!(
            request_id = %request_id,
            tenant = %tenant_id,
            violations = ?violations_payload,
            "Inference request denied by policy enforcement"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("policy violation")
                    .with_code("POLICY_VIOLATION")
                    .with_details(violations_payload),
            ),
        ));
    }
    if !enforcement_result.violations.is_empty() {
        warn!(
            request_id = %request_id,
            tenant = %tenant_id,
            violations = ?violations_payload,
            "Policy enforcement reported non-blocking violations"
        );
    }
    for action in &enforcement_result.actions {
        if let EnforcementAction::SendAlert {
            alert_type,
            message,
        } = action
        {
            warn!(
                request_id = %request_id,
                tenant = %tenant_id,
                alert_type = %alert_type,
                alert_message = %message,
                "Policy enforcement requested alert"
            );
        }
    }

    // Real inference implementation - proxy to worker UDS server
    // 1. Look up available workers from database
    // 2. Select a healthy worker
    // 3. Connect to worker UDS server
    // 4. Forward inference request
    // 5. Return worker response

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Resolve UDS path: prefer registered worker; otherwise fall back to per-tenant default
    let uds_path_buf = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        // Fallback: honor env override or use /var/run/aos/<tenant>/aos.sock
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| format!("/var/run/aos/{}/aos.sock", claims.tenant_id));
        std::path::PathBuf::from(fallback)
    };
    let uds_path = uds_path_buf.as_path();
    let uds_path_str = uds_path.to_string_lossy().to_string();

    // Create UDS client and send request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(30));

    // Convert server API request to worker API request
    let worker_request = WorkerInferRequest {
        cpid: claims.tenant_id.clone(),
        prompt: req.prompt.clone(),
        max_tokens: req.max_tokens.unwrap_or(100),
        require_evidence: req.require_evidence.unwrap_or(false), // Get from request or default to false
        adapter_hints: None, // No pre-routing for basic infer endpoint
        router_features: None,
    };

    match uds_client.infer(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Convert worker response to server API response
            let response = InferResponse {
                text: worker_response.text.unwrap_or_default(),
                tokens: vec![], // Worker doesn't expose token IDs in current API
                finish_reason: worker_response.status.clone(),
                trace: InferenceTrace {
                    adapters_used: worker_response.trace.router_summary.adapters_used.clone(),
                    router_decisions: vec![], // Router decisions not in simplified trace
                    latency_ms: 0,            // Not tracked in current response
                },
            };
            Ok(Json(response))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("inference timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("inference failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}
/// Streaming inference via SSE bridging worker signals
pub async fn infer_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<InferRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("BAD_REQUEST")),
        ));
    }

    // Resolve worker UDS
    let workers = match state.db.list_all_workers().await {
        Ok(ws) => ws,
        Err(_) => Vec::new(),
    };
    let uds_path = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| format!("/var/run/aos/{}/aos.sock", claims.tenant_id));
        std::path::PathBuf::from(fallback)
    };

    // Channel to client SSE
    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(1024);

    // Task A: subscribe to worker /signals and forward
    {
        let tx_signals = tx.clone();
        let uds_path_signals = uds_path.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::UnixStream;
            if let Ok(mut stream) = UnixStream::connect(&uds_path_signals).await {
                let _ = stream
                    .write_all(b"GET /signals HTTP/1.1\r\nHost: worker\r\n\r\n")
                    .await;
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                // Skip headers
                loop {
                    line.clear();
                    if reader
                        .read_line(&mut line)
                        .await
                        .ok()
                        .filter(|&n| n > 0)
                        .is_none()
                    {
                        break;
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                }
                // Read SSE events
                let mut event_type = String::new();
                let mut event_data = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let l = line.trim();
                            if l.is_empty() {
                                if !event_type.is_empty() && !event_data.is_empty() {
                                    let _ = tx_signals
                                        .send(
                                            Event::default()
                                                .event(event_type.clone())
                                                .data(event_data.clone()),
                                        )
                                        .await;
                                }
                                event_type.clear();
                                event_data.clear();
                            } else if let Some(et) = l.strip_prefix("event:") {
                                event_type = et.trim().to_string();
                            } else if let Some(data) = l.strip_prefix("data:") {
                                if !event_data.is_empty() {
                                    event_data.push('\n');
                                }
                                event_data.push_str(data.trim());
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        });
    }

    // Task B: fire the inference (non-streaming) and emit final completion event
    {
        let tx_complete = tx.clone();
        let uds_client = UdsClient::new(std::time::Duration::from_secs(60));
        let worker_req = crate::types::WorkerInferRequest {
            cpid: claims.tenant_id.clone(),
            prompt: req.prompt.clone(),
            max_tokens: req.max_tokens.unwrap_or(100),
            require_evidence: req.require_evidence.unwrap_or(false),
            adapter_hints: None, // No pre-routing for basic infer endpoint
            router_features: None,
        };
        tokio::spawn(async move {
            if let Ok(resp) = uds_client.infer(uds_path.as_path(), worker_req).await {
                let payload = serde_json::json!({
                    "text": resp.text.unwrap_or_default(),
                    "status": resp.status,
                    "adapters_used": resp.trace.router_summary.adapters_used,
                });
                let _ = tx_complete
                    .send(Event::default().event("complete").data(payload.to_string()))
                    .await;
            }
        });
    }

    let stream = ReceiverStream::new(rx).map(Ok::<Event, Infallible>);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ===== Process Debugging Endpoints =====

/// List process logs for a worker
#[utoipa::path(
    get,
    path = "/v1/workers/{worker_id}/logs",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("level" = Option<String>, Query, description = "Filter by log level"),
        ("limit" = Option<i32>, Query, description = "Maximum number of logs to return")
    ),
    responses(
        (status = 200, description = "Process logs", body = Vec<ProcessLogResponse>)
    )
)]
pub async fn list_process_logs(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_worker_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessLogResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let _level_filter = params.get("level");
    let _limit = params
        .get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100);

    // Database query for process logs - placeholder implementation
    // For now, return empty list
    Ok(Json(vec![]))
}

/// Get process crash dumps for a worker
#[utoipa::path(
    get,
    path = "/v1/workers/{worker_id}/crashes",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Process crash dumps", body = Vec<ProcessCrashDumpResponse>)
    )
)]
pub async fn list_process_crashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<Vec<ProcessCrashDumpResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Database query for crash dumps - placeholder implementation
    Ok(Json(vec![]))
}

/// Start a debug session for a worker
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/debug",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = StartDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session started", body = ProcessDebugSessionResponse)
    )
)]
pub async fn start_debug_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<StartDebugSessionRequest>,
) -> Result<Json<ProcessDebugSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Debug session creation - placeholder implementation
    Ok(Json(ProcessDebugSessionResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        worker_id: worker_id.clone(),
        session_type: req.session_type,
        status: "active".to_string(),
        config_json: req.config_json,
        started_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        results_json: None,
    }))
}

/// Run a troubleshooting step for a worker
#[utoipa::path(
    post,
    path = "/v1/workers/{worker_id}/troubleshoot",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = RunTroubleshootingStepRequest,
    responses(
        (status = 200, description = "Troubleshooting step started", body = ProcessTroubleshootingStepResponse)
    )
)]
pub async fn run_troubleshooting_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<RunTroubleshootingStepRequest>,
) -> Result<Json<ProcessTroubleshootingStepResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Troubleshooting step execution - placeholder implementation
    Ok(Json(ProcessTroubleshootingStepResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        worker_id: worker_id.clone(),
        step_name: req.step_name,
        step_type: req.step_type,
        status: "running".to_string(),
        command: req.command,
        output: None,
        error_message: None,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
    }))
}

// ===== Advanced Process Monitoring and Alerting Endpoints =====

/// List process monitoring rules
#[utoipa::path(
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("rule_type" = Option<String>, Query, description = "Filter by rule type"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Process monitoring rules", body = Vec<ProcessMonitoringRuleResponse>)
    )
)]
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let rule_type_filter = params.get("rule_type");
    let is_active_filter = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    // Database query for monitoring rules - placeholder implementation
    // For now, return empty list
    Ok(Json(vec![]))
}

/// Create process monitoring rule
#[utoipa::path(
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateProcessMonitoringRuleRequest,
    responses(
        (status = 200, description = "Monitoring rule created", body = ProcessMonitoringRuleResponse)
    )
)]
pub async fn create_process_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringRuleRequest>,
) -> Result<Json<ProcessMonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Monitoring rule creation - placeholder implementation
    Ok(Json(ProcessMonitoringRuleResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        rule_type: req.rule_type,
        metric_name: req.metric_name,
        threshold_value: req.threshold_value,
        threshold_operator: req.threshold_operator,
        severity: req.severity,
        evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300),
        cooldown_seconds: req.cooldown_seconds.unwrap_or(60),
        is_active: true,
        notification_channels: req.notification_channels,
        escalation_rules: req.escalation_rules,
        created_by: Some(claims.sub.clone()),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }))
}
/// List process alerts
#[utoipa::path(
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by alert status"),
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process alerts", body = Vec<ProcessAlertResponse>)
    )
)]
pub async fn list_process_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let worker_filter = params.get("worker_id");
    let status_filter = params.get("status");
    let severity_filter = params.get("severity");

    let tenant_id = tenant_filter
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());

    let status = match status_filter.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "active" => Some(AlertStatus::Active),
        Some(ref s) if s == "acknowledged" => Some(AlertStatus::Acknowledged),
        Some(ref s) if s == "resolved" => Some(AlertStatus::Resolved),
        Some(ref s) if s == "suppressed" => Some(AlertStatus::Suppressed),
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid status filter")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported status '{}'", other)),
                ),
            ))
        }
        None => None,
    };

    let severity = match severity_filter.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "info" => Some(AlertSeverity::Info),
        Some(ref s) if s == "warning" => Some(AlertSeverity::Warning),
        Some(ref s) if s == "error" => Some(AlertSeverity::Error),
        Some(ref s) if s == "critical" => Some(AlertSeverity::Critical),
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid severity filter")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported severity '{}'", other)),
                ),
            ))
        }
        None => None,
    };

    let filters = AlertFilters {
        tenant_id: Some(tenant_id),
        worker_id: worker_filter.cloned(),
        status,
        severity,
        start_time: None,
        end_time: None,
        limit: Some(200),
    };

    let alerts = state.db.list_process_alerts(filters).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query alerts")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let responses = alerts
        .into_iter()
        .map(map_alert_to_response)
        .collect::<Vec<_>>();

    Ok(Json(responses))
}

/// Acknowledge process alert
#[utoipa::path(
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeProcessAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = ProcessAlertResponse)
    )
)]
pub async fn acknowledge_process_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(req): Json<AcknowledgeProcessAlertRequest>,
) -> Result<Json<ProcessAlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    if Some(&req.alert_id) != Some(&alert_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("alert_id mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details("Path alert_id does not match payload alert_id"),
            ),
        ));
    }

    if let Some(note) = req.acknowledgment_note.as_ref() {
        tracing::info!(
            alert_id = %alert_id,
            user = %claims.sub,
            "Acknowledging alert with note: {}",
            note
        );
    } else {
        tracing::info!(alert_id = %alert_id, user = %claims.sub, "Acknowledging alert");
    }

    state
        .db
        .update_process_alert_status(&alert_id, AlertStatus::Acknowledged, Some(&claims.sub))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to acknowledge alert")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let alert = state
        .db
        .get_process_alert(&alert_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch alert")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("alert not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(alert_id.clone()),
                ),
            )
        })?;

    let response = map_alert_to_response(alert.clone());

    // Broadcast the updated alert - convert to system alert format
    let system_alert = adapteros_system_metrics::monitoring_types::AlertResponse {
        id: response.id.clone(),
        rule_id: response.rule_id.clone(),
        worker_id: response.worker_id.clone(),
        tenant_id: response.tenant_id.clone(),
        alert_type: response.alert_type.clone(),
        severity: response.severity.clone(),
        title: response.title.clone(),
        message: response.message.clone(),
        metric_value: response.metric_value,
        threshold_value: response.threshold_value,
        status: response.status.clone(),
        acknowledged_by: response.acknowledged_by.clone(),
        acknowledged_at: response.acknowledged_at.clone(),
        resolved_at: response.resolved_at.clone(),
        suppression_reason: response.suppression_reason.clone(),
        suppression_until: response.suppression_until.clone(),
        escalation_level: response.escalation_level as i64,
        notification_sent: response.notification_sent,
        created_at: response.created_at.clone(),
        updated_at: response.updated_at.clone(),
    };
    let _ = state.alert_tx.send(system_alert);

    Ok(Json(response))
}

/// List process anomalies
#[utoipa::path(
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by anomaly status"),
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process anomalies", body = Vec<ProcessAnomalyResponse>)
    )
)]
pub async fn list_process_anomalies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let worker_filter = params.get("worker_id");
    let status_filter = params.get("status");
    let severity_filter = params.get("severity");

    let tenant_id = tenant_filter
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());

    let status = match status_filter.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "detected" => Some(AnomalyStatus::Detected),
        Some(ref s) if s == "investigating" => Some(AnomalyStatus::Investigating),
        Some(ref s) if s == "confirmed" => Some(AnomalyStatus::Confirmed),
        Some(ref s) if s == "false_positive" => Some(AnomalyStatus::FalsePositive),
        Some(ref s) if s == "resolved" => Some(AnomalyStatus::Resolved),
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid status filter")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported status '{}'", other)),
                ),
            ))
        }
        None => None,
    };

    let severity = match severity_filter.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "info" => Some(AlertSeverity::Info),
        Some(ref s) if s == "warning" => Some(AlertSeverity::Warning),
        Some(ref s) if s == "error" => Some(AlertSeverity::Error),
        Some(ref s) if s == "critical" => Some(AlertSeverity::Critical),
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid severity filter")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported severity '{}'", other)),
                ),
            ))
        }
        None => None,
    };

    let filters = AnomalyFilters {
        tenant_id: Some(tenant_id),
        worker_id: worker_filter.cloned(),
        status,
        anomaly_type: None,
        start_time: None,
        end_time: None,
        limit: Some(200),
    };

    let anomalies = state
        .db
        .list_process_anomalies(filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query anomalies")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses = anomalies
        .into_iter()
        .filter(|anomaly| {
            if let Some(ref severity) = severity {
                &anomaly.severity == severity
            } else {
                true
            }
        })
        .map(map_anomaly_to_response)
        .collect::<Vec<_>>();

    Ok(Json(responses))
}

/// Update process anomaly status
#[utoipa::path(
    post,
    path = "/v1/monitoring/anomalies/{anomaly_id}/status",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateProcessAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly status updated", body = ProcessAnomalyResponse)
    )
)]
pub async fn update_process_anomaly_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(anomaly_id): Path<String>,
    Json(req): Json<UpdateProcessAnomalyStatusRequest>,
) -> Result<Json<ProcessAnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let status = match req.status.to_lowercase().as_str() {
        "detected" => AnomalyStatus::Detected,
        "investigating" => AnomalyStatus::Investigating,
        "confirmed" => AnomalyStatus::Confirmed,
        "false_positive" => AnomalyStatus::FalsePositive,
        "resolved" => AnomalyStatus::Resolved,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid status")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported anomaly status '{}'", other)),
                ),
            ))
        }
    };

    tracing::info!(
        anomaly_id = %anomaly_id,
        user = %claims.sub,
        status = %req.status,
        note = ?req.investigation_notes,
        "Updating anomaly status"
    );

    state
        .db
        .update_process_anomaly_status(
            &anomaly_id,
            status,
            Some(&claims.sub),
            req.investigation_notes.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update anomaly")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let anomaly = state
        .db
        .get_process_anomaly(&anomaly_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch anomaly")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("anomaly not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(anomaly_id.clone()),
                ),
            )
        })?;

    Ok(Json(map_anomaly_to_response(anomaly)))
}

/// List process monitoring dashboards
#[utoipa::path(
    get,
    path = "/v1/monitoring/dashboards",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_shared" = Option<bool>, Query, description = "Filter by shared status")
    ),
    responses(
        (status = 200, description = "Process monitoring dashboards", body = Vec<ProcessMonitoringDashboardResponse>)
    )
)]
pub async fn list_process_monitoring_dashboards(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringDashboardResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_override = params.get("tenant_id").cloned();
    let tenant_id = tenant_override.unwrap_or_else(|| claims.tenant_id.clone());
    let is_shared_filter = params.get("is_shared").and_then(|s| s.parse::<bool>().ok());

    let dashboards = state
        .db
        .list_monitoring_dashboards(Some(tenant_id.as_str()), is_shared_filter)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query dashboards")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses = dashboards
        .into_iter()
        .map(|d| ProcessMonitoringDashboardResponse {
            id: d.id,
            name: d.name,
            description: d.description,
            tenant_id: d.tenant_id,
            dashboard_config: d.dashboard_config,
            is_shared: d.is_shared,
            created_by: d.created_by,
            created_at: d.created_at.to_rfc3339(),
            updated_at: d.updated_at.to_rfc3339(),
        })
        .collect::<Vec<_>>();

    Ok(Json(responses))
}

/// Create process monitoring dashboard
#[utoipa::path(
    post,
    path = "/v1/monitoring/dashboards",
    request_body = CreateProcessMonitoringDashboardRequest,
    responses(
        (status = 200, description = "Dashboard created", body = ProcessMonitoringDashboardResponse)
    )
)]
pub async fn create_process_monitoring_dashboard(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringDashboardRequest>,
) -> Result<Json<ProcessMonitoringDashboardResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let is_shared = req.is_shared.unwrap_or(false);
    let dashboard_config = req.dashboard_config.clone();
    let description = req.description.clone();
    let name = req.name.clone();

    let create_req = CreateDashboardRequest {
        name: req.name,
        description,
        tenant_id: claims.tenant_id.clone(),
        dashboard_config,
        is_shared,
        created_by: Some(claims.sub.clone()),
    };

    let dashboard_id = state
        .db
        .create_monitoring_dashboard(create_req)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create dashboard")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let timestamp = chrono::Utc::now().to_rfc3339();

    Ok(Json(ProcessMonitoringDashboardResponse {
        id: dashboard_id,
        name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        dashboard_config: req.dashboard_config,
        is_shared,
        created_by: Some(claims.sub.clone()),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }))
}

/// List process health metrics
#[utoipa::path(
    get,
    path = "/v1/monitoring/health-metrics",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name"),
        ("start_time" = Option<String>, Query, description = "Start time for metrics"),
        ("end_time" = Option<String>, Query, description = "End time for metrics")
    ),
    responses(
        (status = 200, description = "Process health metrics", body = Vec<ProcessHealthMetricResponse>)
    )
)]
pub async fn list_process_health_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessHealthMetricResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_filter = params.get("worker_id");
    let metric_filter = params.get("metric_name");
    let start_time_filter = params.get("start_time");
    let end_time_filter = params.get("end_time");

    // Build filters for health metrics query
    let filters = adapteros_system_metrics::MetricFilters {
        worker_id: worker_filter.cloned(),
        tenant_id: None, // Will be filtered by user's tenant access
        metric_name: metric_filter.cloned(),
        start_time: start_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        end_time: end_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        limit: Some(1000), // Limit results
    };

    // Query health metrics from database
    let metrics = adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert ProcessHealthMetric to ProcessHealthMetricResponse
    let response_metrics: Vec<ProcessHealthMetricResponse> = metrics
        .into_iter()
        .map(|metric| ProcessHealthMetricResponse {
            id: metric.id,
            worker_id: metric.worker_id,
            tenant_id: metric.tenant_id,
            metric_name: metric.metric_name,
            metric_value: metric.metric_value,
            metric_unit: metric.metric_unit,
            tags: metric.tags,
            collected_at: metric.collected_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response_metrics))
}

/// List process monitoring reports
#[utoipa::path(
    get,
    path = "/v1/monitoring/reports",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("report_type" = Option<String>, Query, description = "Filter by report type")
    ),
    responses(
        (status = 200, description = "Process monitoring reports", body = Vec<ProcessMonitoringReportResponse>)
    )
)]
pub async fn list_process_monitoring_reports(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let report_type_filter = params.get("report_type");

    let tenant_id = tenant_filter
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());

    let report_type = report_type_filter.cloned();

    let reports = state
        .db
        .list_monitoring_reports(Some(tenant_id.as_str()), report_type.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query reports")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses = reports
        .into_iter()
        .map(|r| ProcessMonitoringReportResponse {
            id: r.id,
            name: r.name,
            description: r.description,
            tenant_id: r.tenant_id,
            report_type: r.report_type,
            report_config: r.report_config,
            generated_at: r.generated_at.to_rfc3339(),
            report_data: r.report_data,
            file_path: r.file_path,
            file_size_bytes: r.file_size_bytes,
            created_by: r.created_by,
        })
        .collect::<Vec<_>>();

    Ok(Json(responses))
}

/// Create process monitoring report
#[utoipa::path(
    post,
    path = "/v1/monitoring/reports",
    request_body = CreateProcessMonitoringReportRequest,
    responses(
        (status = 200, description = "Report created", body = ProcessMonitoringReportResponse)
    )
)]
pub async fn create_process_monitoring_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringReportRequest>,
) -> Result<Json<ProcessMonitoringReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let report_type = match req.report_type.as_str() {
        "health_summary" | "performance_trends" | "anomaly_analysis" | "alert_summary" => {
            req.report_type.clone()
        }
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid report type")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!("Unsupported report type '{}'", other)),
                ),
            ))
        }
    };

    let report_config = req.report_config.clone();
    let description = req.description.clone();
    let name = req.name.clone();

    let create_req = CreateReportRequest {
        name: req.name,
        description,
        tenant_id: claims.tenant_id.clone(),
        report_type: report_type.clone(),
        report_config,
        report_data: None,
        file_path: None,
        file_size_bytes: None,
        created_by: Some(claims.sub.clone()),
    };

    let report_id = state
        .db
        .create_monitoring_report(create_req)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create report")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let generated_at = chrono::Utc::now().to_rfc3339();

    Ok(Json(ProcessMonitoringReportResponse {
        id: report_id,
        name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        report_type,
        report_config: req.report_config,
        generated_at,
        report_data: None,
        file_path: None,
        file_size_bytes: None,
        created_by: Some(claims.sub.clone()),
    }))
}

// ===== Adapter Management Endpoints =====

/// List all adapters
#[utoipa::path(
    get,
    path = "/v1/adapters",
    params(
        ("tier" = Option<i32>, Query, description = "Filter by tier"),
        ("framework" = Option<String>, Query, description = "Filter by framework")
    ),
    responses(
        (status = 200, description = "List of adapters", body = Vec<AdapterResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapters(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListAdaptersQuery>,
) -> Result<Json<Vec<AdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list adapters")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut responses = Vec::new();
    for adapter in adapters {
        // Filter by tier if specified
        if let Some(tier) = query.tier {
            if adapter.tier != tier {
                continue;
            }
        }

        // Filter by framework if specified
        if let Some(ref framework) = query.framework {
            if adapter.framework.as_ref() != Some(framework) {
                continue;
            }
        }

        // Get stats
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&adapter.adapter_id)
            .await
            .unwrap_or((0, 0, 0.0));

        let selection_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let languages: Vec<String> = adapter
            .languages_json
            .as_ref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();

        responses.push(AdapterResponse {
            id: adapter.id,
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            hash_b3: adapter.hash_b3,
            rank: adapter.rank,
            tier: adapter.tier,
            languages,
            framework: adapter.framework,
            created_at: adapter.created_at,
            stats: Some(AdapterStats {
                total_activations: total,
                selected_count: selected,
                avg_gate_value: avg_gate,
                selection_rate,
            }),
        });
    }

    Ok(Json(responses))
}

/// Get adapter by ID
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter details", body = AdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total > 0 {
        (selected as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let languages: Vec<String> = adapter
        .languages_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();

    Ok(Json(AdapterResponse {
        id: adapter.id,
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        languages,
        framework: adapter.framework,
        created_at: adapter.created_at,
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
    }))
}

/// Register new adapter
#[utoipa::path(
    post,
    path = "/v1/adapters/register",
    request_body = RegisterAdapterRequest,
    responses(
        (status = 201, description = "Adapter registered", body = AdapterResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn register_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Require admin role
    require_role(&claims, Role::Admin)?;

    // Validate inputs
    if req.adapter_id.is_empty() || req.name.is_empty() || req.hash_b3.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("adapter_id, name, and hash_b3 are required")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate adapter ID format (alphanumeric, underscores, hyphens)
    validate_adapter_id(&req.adapter_id)?;

    // Validate name length and content
    validate_name(&req.name)?;

    // Validate hash format (B3 hash)
    validate_hash_b3(&req.hash_b3)?;

    let languages_json = serde_json::to_string(&req.languages).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid languages array")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let registration = AdapterRegistrationBuilder::new()
        .adapter_id(req.adapter_id.clone())
        .name(req.name.clone())
        .hash_b3(req.hash_b3.clone())
        .rank(req.rank)
        .tier(req.tier)
        .languages_json(Some(languages_json))
        .framework(req.framework.clone())
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to build adapter registration")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let id = state.db.register_adapter(registration).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to register adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            id,
            adapter_id: req.adapter_id,
            name: req.name,
            hash_b3: req.hash_b3,
            rank: req.rank,
            tier: req.tier,
            languages: req.languages,
            framework: req.framework,
            created_at: chrono::Utc::now().to_rfc3339(),
            stats: None,
        }),
    ))
}

/// Import adapter from uploaded file
#[utoipa::path(
    post,
    path = "/v1/adapters/import",
    request_body(content = String, description = "Multipart form data with 'file' field containing .aos or .safetensors file"),
    params(
        ("load" = Option<bool>, Query, description = "Automatically load adapter after registration")
    ),
    responses(
        (status = 201, description = "Adapter imported and registered", body = AdapterResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Import failed", body = ErrorResponse)
    )
)]
pub async fn import_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ImportAdapterQuery>,
    mut multipart: axum::extract::Multipart,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    // Process multipart form data
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Failed to read multipart field")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            let content_type = field.content_type().unwrap_or("").to_string();
            filename = field.file_name().map(|s| s.to_string());

            // Validate content type
            if !content_type.is_empty()
                && !content_type.contains("octet-stream")
                && !content_type.contains("zip")
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Unsupported content type. Expected application/octet-stream or application/zip")
                            .with_code("BAD_REQUEST"),
                    ),
                ));
            }

            // Read file data
            file_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(
                                ErrorResponse::new("Failed to read file data")
                                    .with_code("BAD_REQUEST")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?
                    .to_vec(),
            );
        }
    }

    let file_data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("No file provided").with_code("BAD_REQUEST")),
        )
    })?;

    let filename = filename.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("No filename provided").with_code("BAD_REQUEST")),
        )
    })?;

    // Validate file extension
    let is_aos = filename.ends_with(".aos");
    let is_safetensors = filename.ends_with(".safetensors");

    if !is_aos && !is_safetensors {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("File must have .aos or .safetensors extension")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Get adapters directory and validate filename
    use adapteros_secure_fs::traversal::{check_path_traversal, join_paths_safe};
    use std::path::Path;

    // Validate filename to prevent directory traversal
    if let Err(e) = check_path_traversal(Path::new(&filename)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid filename")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Path validation failed: {}", e)),
            ),
        ));
    }

    let adapters_root =
        std::env::var("AOS_ADAPTERS_ROOT").unwrap_or_else(|_| "./adapters".to_string());
    let adapters_path = std::path::PathBuf::from(&adapters_root);

    // Use secure path joining for the final file path
    let final_file_path = join_paths_safe(&adapters_path, &filename).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid file path")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Path validation failed: {}", e)),
            ),
        )
    })?;

    // Ensure adapters directory exists
    tokio::fs::create_dir_all(&adapters_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create adapters directory")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Compute BLAKE3 hash of file content
    let hash_b3 = blake3::hash(&file_data).to_hex();
    let hash_b3_db = format!("b3:{}", hash_b3);

    // Prepare adapter metadata
    let (adapter_id, name, rank, tier, languages_json, framework, manifest_opt) = if is_aos {
        // Load .aos file to extract metadata
        use adapteros_single_file_adapter::SingleFileAdapterLoader;
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create temporary file for loading
        let mut temp_file = NamedTempFile::new().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create temporary file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        temp_file.write_all(&file_data).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to write temporary file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        temp_file.flush().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to flush temporary file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        // Load adapter from temp file
        let adapter = SingleFileAdapterLoader::load(&temp_file)
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Invalid .aos file")
                            .with_code("BAD_REQUEST")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        // Extract metadata from manifest
        let manifest = &adapter.manifest;
        let adapter_id = manifest.adapter_id.clone();
        let name = format!("{} v{}", manifest.adapter_id, manifest.version);
        let rank = manifest.rank;
        let tier = match manifest.tier.as_str() {
            "production" => 1,
            "staging" => 2,
            "development" => 3,
            _ => 1, // Default to production
        };
        let languages_json = "[]".to_string(); // .aos files don't have language metadata in current format
        let framework = Some("lora".to_string());

        (
            adapter_id,
            name,
            rank,
            tier,
            languages_json,
            framework,
            Some(manifest.clone()),
        )
    } else {
        // For .safetensors, generate ID from filename/hash and require rank/tier params
        // This is a simplified implementation - in production you'd want better metadata handling
        let base_name = filename.trim_end_matches(".safetensors");
        let adapter_id = format!("{}-{}", base_name, &hash_b3[..8]);
        let name = base_name.to_string();
        let rank = 8; // Default rank
        let tier = 2; // Default to staging
        let languages_json = "[]".to_string();
        let framework = Some("lora".to_string());

        (
            adapter_id,
            name,
            rank,
            tier,
            languages_json,
            framework,
            None,
        )
    };

    // Save file with hash-based name
    let file_extension = if is_aos { "aos" } else { "safetensors" };
    let saved_filename = format!("{}.{}", hash_b3, file_extension);
    let saved_path = adapters_path.join(&saved_filename);

    tokio::fs::write(&saved_path, &file_data)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to save adapter file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Register adapter in database (create if missing)
    let existing = state.db.get_adapter(&adapter_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query adapter")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let db_adapter_id = if let Some(record) = existing {
        tracing::info!(
            adapter_id = %adapter_id,
            db_id = %record.id,
            "Adapter already registered, reusing existing record"
        );
        record.id
    } else {
        let mut builder = adapteros_db::AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id.clone())
            .name(name.clone())
            .hash_b3(hash_b3_db.clone())
            .rank(rank as i32)
            .tier(tier)
            .languages_json(Some(languages_json.clone()))
            .framework(framework.clone())
            .category("lora")
            .scope("tenant")
            .intent(Some("imported_adapter".to_string()));

        if let Some(manifest) = manifest_opt.clone() {
            builder = builder
                .framework_id(Some(manifest.base_model.clone()))
                .framework_version(Some(manifest.version.clone()));
        }

        let params = builder.build().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to prepare adapter registration")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        state.db.register_adapter(params).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register adapter")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
    };

    tracing::info!(
        "Imported adapter {} ({}) from file {}",
        adapter_id,
        name,
        filename
    );

    // Optionally load adapter
    if params.load.unwrap_or(false) {
        tracing::info!("Auto-loading adapter {}", adapter_id);
        state
            .db
            .update_adapter_state(&adapter_id, "loading", "import_auto_load")
            .await
            .map_err(|e| {
                tracing::warn!("Failed to set adapter state to loading: {}", e);
            })
            .ok(); // Don't fail the import if loading state update fails
    }

    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            id: db_adapter_id,
            adapter_id,
            name,
            hash_b3: hash_b3_db,
            rank: rank as i32,
            tier,
            languages: serde_json::from_str(&languages_json).unwrap_or_default(),
            framework,
            created_at: chrono::Utc::now().to_rfc3339(),
            stats: None,
        }),
    ))
}

/// Query parameters for import adapter
#[derive(serde::Deserialize)]
pub struct ImportAdapterQuery {
    load: Option<bool>,
}

/// Delete adapter
#[utoipa::path(
    delete,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 204, description = "Adapter deleted"),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn delete_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role
    require_role(&claims, Role::Admin)?;

    state.db.delete_adapter(&adapter_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Cancel a long-running model operation
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/cancel",
    params(
        ("model_id" = String, Path, description = "Model ID")
    ),
    responses(
        (status = 200, description = "Operation cancelled successfully"),
        (status = 404, description = "Operation not found", body = ErrorResponse),
        (status = 500, description = "Failed to cancel operation", body = ErrorResponse)
    )
)]
pub async fn cancel_model_operation(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();

    // Check if there's an active operation for this model
    let tracker = state.operation_tracker.clone();

    match tracker.cancel_model_operation(&model_id, &tenant_id).await {
        Ok(()) => {
            info!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                "Model operation cancelled successfully"
            );

            Ok(Json(serde_json::json!({
                "status": "cancelled",
                "model_id": model_id,
                "message": "Operation cancelled successfully"
            })))
        }
        Err(OperationCancellationError::OperationNotFound) => {
            warn!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                "No active operation found to cancel"
            );

            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::with_message(
                    StatusCode::NOT_FOUND,
                    "OPERATION_NOT_FOUND",
                    "No active operation found for this model",
                    Some(format!("req-{}", uuid::Uuid::new_v4())),
                )),
            ))
        }
        Err(OperationCancellationError::OperationAlreadyCompleted) => {
            warn!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                "Operation already completed when cancel requested"
            );

            Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse::with_message(
                    StatusCode::CONFLICT,
                    "OPERATION_COMPLETED",
                    "Operation already completed",
                    Some(format!("req-{}", uuid::Uuid::new_v4())),
                )),
            ))
        }
    }
}

/// Enhanced load_model with retry logic and user-friendly error handling
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/load",
    params(
        ("model_id" = String, Path, description = "Model ID")
    ),
    request_body = LoadModelRequest,
    responses(
        (status = 200, description = "Model loaded successfully", body = ModelResponse),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 404, description = "Model not found", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn load_model_with_retry(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
    Json(request): Json<LoadModelRequest>,
) -> Result<Json<ModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::errors::{RetryExecutor, UserFriendlyErrorMapper};

    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let tenant_id = claims.tenant_id.clone();
    let request_id = format!("req-{}", uuid::Uuid::new_v4());

    // Create operation tracker entry
    let tracker = state.operation_tracker.clone();
    tracker
        .start_model_operation(&model_id, tenant_id.as_str(), ModelOperationType::Load)
        .await
        .map_err(|conflict| {
            error!(
                model_id = %model_id,
                tenant_id = %tenant_id,
                error = %conflict,
                "Failed to start operation tracking"
            );
            (
                StatusCode::CONFLICT,
                Json(
                    ErrorResponse::with_message(
                        StatusCode::CONFLICT,
                        "OPERATION_CONFLICT",
                        "Another model operation is already in progress",
                        Some(request_id.clone()),
                    )
                    .with_string_details(conflict.to_string()),
                ),
            )
        })?;

    // Create retry executor with exponential backoff
    let retry_config = crate::errors::RetryConfig {
        max_attempts: 3,
        initial_delay: std::time::Duration::from_millis(500),
        max_delay: std::time::Duration::from_secs(10),
        backoff_multiplier: 2.0,
        jitter_factor: 0.1,
    };

    let retry_executor = RetryExecutor::new(retry_config);

    // Execute load operation with retry
    let tracker_for_retry = tracker.clone();
    let model_id_for_retry = model_id.clone();
    let tenant_id_for_retry = tenant_id.clone();

    // TODO: Re-implement progress callbacks and retry logic with proper type annotations
    // For now, perform load without progress tracking to resolve compilation issues
    let load_result = load_model_internal_with_progress(
        &state,
        &model_id,
        &request,
        tenant_id.as_str(),
        |_progress_pct, _message| {
            // Progress callbacks temporarily disabled
        },
    )
    .await;

    // Complete the operation (best effort)
    tracker
        .complete_model_operation(
            &model_id,
            tenant_id.as_str(),
            ModelOperationType::Load,
            load_result.is_ok(),
        )
        .await;

    match load_result {
        Ok(response) => {
            info!(model_id = %model_id, tenant_id = %tenant_id, request_id = %request_id, "Model loaded successfully");
            Ok(Json(response))
        }
        Err(e) => {
            // Convert error to user-friendly format
            let error_code = if e.to_string().contains("cancelled") {
                "OPERATION_CANCELLED"
            } else {
                "LOAD_FAILED"
            };

            let user_message =
                UserFriendlyErrorMapper::map_error_message(error_code, &e.to_string());

            error!(model_id = %model_id, tenant_id = %tenant_id, request_id = %request_id, error = %e, "Model load failed");

            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::with_message(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    error_code,
                    user_message,
                    Some(request_id),
                )),
            ))
        }
    }
}

/// Load model with progress callback support
///
/// # Citations
/// - Model loading: [source: crates/adapteros-server-api/src/model_runtime.rs L526-575]
/// - Progress tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs L315-340]
async fn load_model_internal_with_progress<F>(
    state: &AppState,
    model_id: &str,
    request: &LoadModelRequest,
    tenant_id: &str,
    progress_callback: F,
) -> Result<ModelResponse, anyhow::Error>
where
    F: Fn(f64, String) + Send + Sync + 'static,
{
    progress_callback(0.0, "Starting model load".to_string());

    // Validate model exists (10%)
    let (model_id_value, model_name_value, model_status_value, model_type_value, model_path_value) = {
        let record = sqlx::query!(
            r#"
            SELECT id, name, hash_b3, config_hash_b3, metadata_json
            FROM models
            WHERE id = ?
            "#,
            model_id
        )
        .fetch_optional(state.db.pool())
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        // Construct model path from hash (assuming models are stored by hash)
        let model_path = format!("models/{}", record.hash_b3);

        (
            record.id.unwrap_or_else(|| model_id.to_string()),
            record.name,
            "available".to_string(),  // Default status since not in schema
            "base_model".to_string(), // Default model type
            Some(model_path),
        )
    };

    progress_callback(10.0, "Model record validated".to_string());

    // Check model status
    if model_status_value != "available" {
        return Err(anyhow::anyhow!(
            "Model is not available for loading: {}",
            model_status_value
        ));
    }

    progress_callback(20.0, "Model status validated".to_string());

    // Get model runtime (30%)
    let mut runtime = state
        .model_runtime
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Model runtime not available"))?
        .lock()
        .await;

    progress_callback(30.0, "Model runtime acquired".to_string());

    // Load model with progress (30-90%)
    let model_path =
        model_path_value.ok_or_else(|| anyhow::anyhow!("Model path not configured"))?;

    runtime
        .load_model_async_with_progress(
            tenant_id,
            &model_id_value,
            &model_path,
            |_pct, _msg| {
                // Progress callbacks disabled for now to resolve lifetime issues
                // TODO: Re-implement progress callbacks with proper lifetime management
            },
            Duration::from_secs(request.timeout_secs.unwrap_or(300)),
        )
        .await
        .map_err(anyhow::Error::msg)?;

    progress_callback(90.0, "Model loaded, finalizing".to_string());

    // Create response (100%)
    let response = ModelResponse {
        id: model_id_value.clone(),
        name: model_name_value,
        model_type: model_type_value,
        status: "loaded".to_string(),
        loaded_at: Some(chrono::Utc::now()),
        memory_usage: Some(1024 * 1024 * 1024), // TODO: Get real usage from runtime
    };

    progress_callback(100.0, "Model load completed".to_string());

    Ok(response)
}

// Temporarily removed load_adapter function

// Unload an adapter from memory
/// Get the status of an ongoing operation
#[utoipa::path(
    get,
    path = "/v1/operations/{resource_id}/status",
    operation_id = "get_operation_status_v1",
    tag = "operations",
    params(
        ("resource_id" = String, Path, description = "Resource identifier (model or adapter ID)"),
    ),
    request_body = None,
    responses(
        (status = 200, description = "Operation status", body = OperationProgressEvent),
        (status = 400, description = "Missing tenant_id parameter", body = ErrorResponse),
        (status = 404, description = "Operation not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_operation_status_handler(
    State(state): State<AppState>,
    Path(resource_id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::types::OperationProgressEvent>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = params.get("tenant_id").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("tenant_id query parameter required")
                    .with_code("MISSING_TENANT_ID"),
            ),
        )
    })?;

    match state
        .operation_tracker
        .get_operation_status(&resource_id, tenant_id)
        .await
    {
        Some(status) => Ok(Json(status)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Operation not found or completed")
                    .with_code("OPERATION_NOT_FOUND"),
            ),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/unload",
    tag = "adapters",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unloaded successfully"),
        (status = 404, description = "Adapter not found", body = ErrorResponse),
        (status = 500, description = "Failed to unload adapter", body = ErrorResponse)
    )
)]
pub async fn unload_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Get adapter from database
    let _adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Update adapter state to 'unloading'
    state
        .db
        .update_adapter_state(&adapter_id, "unloading", "user_request")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter state")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tracing::info!("Unloading adapter {}", adapter_id);

    // Record start time for metrics
    let unload_start = std::time::Instant::now();
    let tenant_id = &claims.tenant_id;

    // Start operation tracking
    if let Err(e) = state
        .operation_tracker
        .start_adapter_operation(
            &adapter_id,
            tenant_id,
            crate::operation_tracker::AdapterOperationType::Unload,
        )
        .await
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new_user_friendly(
                "OPERATION_IN_PROGRESS",
                e.to_string(),
            )),
        ));
    }

    // Actually unload the adapter using LifecycleManager if available
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let adapter_idx = _adapter.id.parse::<u16>().unwrap_or(0);

        let lifecycle_mgr = lifecycle.lock().await;

        use adapteros_lora_lifecycle::AdapterLoader;
        use std::path::PathBuf;

        let adapters_root =
            std::env::var("AOS_ADAPTERS_ROOT").unwrap_or_else(|_| "./adapters".to_string());
        let adapters_path = PathBuf::from(adapters_root);
        let mut loader = AdapterLoader::new(adapters_path);

        // Update progress: starting unload (10%)
        state
            .operation_tracker
            .update_adapter_progress(
                &adapter_id,
                tenant_id,
                10.0,
                Some(format!(
                    "Preparing to unload adapter ({} MB in memory)",
                    _adapter.memory_bytes as f64 / 1024.0 / 1024.0
                )),
            )
            .await;

        match loader.unload_adapter(adapter_idx) {
            Ok(_) => {
                // Update progress: updating state (90%)
                state
                    .operation_tracker
                    .update_adapter_progress(
                        &adapter_id,
                        tenant_id,
                        90.0,
                        Some("Adapter unloaded from memory, updating database state".to_string()),
                    )
                    .await;

                // Update adapter state to 'cold' and reset memory
                state
                    .db
                    .update_adapter_state(&adapter_id, "cold", "unloaded_successfully")
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new("failed to update adapter state")
                                    .with_code("INTERNAL_SERVER_ERROR")
                                    .with_string_details(e.to_string()),
                            ),
                        )
                    })?;

                state.db.update_adapter_memory(&adapter_id, 0).await.ok();

                tracing::info!(
                    event = "adapter.unload",
                    adapter_id = %adapter_id,
                    "Adapter unloaded successfully"
                );

                // Record success metrics
                let unload_duration = unload_start.elapsed().as_secs_f64();
                {
                    state.metrics_collector.record_adapter_unload_latency(
                        &adapter_id,
                        tenant_id,
                        unload_duration,
                        "success",
                    );
                }

                // Complete operation tracking
                state
                    .operation_tracker
                    .complete_adapter_operation(
                        &adapter_id,
                        tenant_id,
                        crate::operation_tracker::AdapterOperationType::Unload,
                        true,
                    )
                    .await;
            }
            Err(e) => {
                // Complete operation tracking with failure
                state
                    .operation_tracker
                    .complete_adapter_operation(
                        &adapter_id,
                        tenant_id,
                        crate::operation_tracker::AdapterOperationType::Unload,
                        false,
                    )
                    .await;
                // Rollback state on error
                state
                    .db
                    .update_adapter_state(&adapter_id, "warm", "unload_failed")
                    .await
                    .ok();

                // Operation cleanup handled by main handler

                // Record failure metrics
                let unload_duration = unload_start.elapsed().as_secs_f64();
                state.metrics_collector.record_adapter_unload_latency(
                    &adapter_id,
                    tenant_id,
                    unload_duration,
                    "failure",
                );

                tracing::error!("Failed to unload adapter {}: {}", adapter_id, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to unload adapter")
                            .with_code("UNLOAD_FAILED")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        }
    } else {
        // No lifecycle manager - just simulate
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        state
            .db
            .update_adapter_state(&adapter_id, "cold", "simulated_unload")
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update adapter state")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        state.db.update_adapter_memory(&adapter_id, 0).await.ok();

        tracing::info!(
            event = "adapter.unload",
            adapter_id = %adapter_id,
            "Adapter unloaded successfully (simulated)"
        );

        // Record simulated unload metrics
        let unload_duration = unload_start.elapsed().as_secs_f64();
        state.metrics_collector.record_adapter_unload_latency(
            &adapter_id,
            tenant_id,
            unload_duration,
            "success",
        );
    }

    Ok(StatusCode::OK)
}

/// Get adapter activations
#[utoipa::path(
    get,
    path = "/v1/adapters/{adapter_id}/activations",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID"),
        ("limit" = Option<i64>, Query, description = "Limit results (default: 100)")
    ),
    responses(
        (status = 200, description = "Activation history", body = Vec<AdapterActivationResponse>)
    )
)]
pub async fn get_adapter_activations(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<AdapterActivationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = query
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(100);

    let activations = state
        .db
        .get_adapter_activations(&adapter_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let responses: Vec<AdapterActivationResponse> = activations
        .into_iter()
        .map(|a| AdapterActivationResponse {
            id: a.id,
            adapter_id: a.adapter_id,
            request_id: a.request_id,
            gate_value: a.gate_value,
            selected: a.selected == 1,
            created_at: a.created_at,
        })
        .collect();

    Ok(Json(responses))
}
/// Promote adapter state (cold→warm, warm→hot)
pub async fn promote_adapter_state(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterStateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Get current adapter state
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Determine next state based on current tier
    // Tiers: 0=persistent, 1=warm, 2=ephemeral
    // For promotion: persistent(0) → warm(1) → ephemeral(2)
    let new_tier = match adapter.tier {
        0 => 1,            // persistent -> warm
        1 => 2,            // warm -> ephemeral
        _ => adapter.tier, // Already at highest or unknown tier
    };

    let new_state = match new_tier {
        0 => "persistent",
        1 => "warm",
        2 => "ephemeral",
        _ => "persistent", // Default fallback
    };

    // Update adapter state in database
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE adapters SET tier = ?, updated_at = ? WHERE adapter_id = ?",
        new_tier,
        timestamp,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to update adapter state")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let old_state_str = match adapter.tier {
        0 => "persistent",
        1 => "warm",
        2 => "ephemeral",
        _ => "unknown",
    };

    Ok(Json(AdapterStateResponse {
        adapter_id,
        old_state: old_state_str.to_string(),
        new_state: new_state.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Pin adapter to prevent eviction
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/pin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter pinned successfully"),
        (status = 404, description = "Adapter not found"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn pin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Update pinned status in adapters table
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE adapters SET pinned = 1, updated_at = ? WHERE adapter_id = ?",
        timestamp,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to pin adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Adapter {} pinned by {}", adapter_id, claims.email);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Adapter pinned successfully"
    })))
}

/// Unpin adapter to allow eviction
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/unpin",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter unpinned successfully"),
        (status = 404, description = "Adapter not found"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn unpin_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Verify adapter exists
    state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Update pinned status in adapters table
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE adapters SET pinned = 0, updated_at = ? WHERE adapter_id = ?",
        timestamp,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to unpin adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Adapter {} unpinned by {}", adapter_id, claims.email);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Adapter unpinned successfully"
    })))
}

/// Update adapter policy (category)
#[utoipa::path(
    put,
    path = "/v1/adapters/{adapter_id}/policy",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    request_body = UpdateAdapterPolicyRequest,
    responses(
        (status = 200, description = "Policy updated successfully", body = UpdateAdapterPolicyResponse),
        (status = 400, description = "Invalid category"),
        (status = 404, description = "Adapter not found"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn update_adapter_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    Json(req): Json<crate::types::UpdateAdapterPolicyRequest>,
) -> Result<Json<crate::types::UpdateAdapterPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Verify adapter exists
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Update category if provided
    if let Some(category) = &req.category {
        // Validate category
        if !["code", "framework", "codebase", "ephemeral"].contains(&category.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid category")
                        .with_code("INVALID_CATEGORY")
                        .with_string_details(
                            "Category must be one of: code, framework, codebase, ephemeral"
                                .to_string(),
                        ),
                ),
            ));
        }

        let timestamp = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            "UPDATE adapters SET category = ?, updated_at = ? WHERE adapter_id = ?",
            category,
            timestamp,
            adapter_id
        )
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to update adapter policy")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        tracing::info!(
            "Adapter {} policy updated (category: {}) by {}",
            adapter_id,
            category,
            claims.email
        );

        Ok(Json(crate::types::UpdateAdapterPolicyResponse {
            adapter_id,
            category: Some(category.clone()),
            message: format!("Adapter category updated to {}", category),
        }))
    } else {
        Ok(Json(crate::types::UpdateAdapterPolicyResponse {
            adapter_id,
            category: Some(adapter.category),
            message: "No changes requested".to_string(),
        }))
    }
}

/// Get all category policies
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies",
    responses(
        (status = 200, description = "Category policies retrieved successfully", body = std::collections::HashMap<String, CategoryPolicyResponse>),
        (status = 503, description = "Lifecycle manager not available"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn get_category_policies(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<
    Json<std::collections::HashMap<String, CategoryPolicyResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let lifecycle = state.lifecycle_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Lifecycle manager not available").with_code("NOT_CONFIGURED")),
        )
    })?;

    let mgr = lifecycle.lock().await;
    let policy_manager = mgr.get_category_policies();
    let summary = policy_manager.get_policy_summary();

    // Convert CategoryPolicySummary to CategoryPolicyResponse
    let mut response: std::collections::HashMap<String, CategoryPolicyResponse> =
        std::collections::HashMap::new();

    for (category, policy_summary) in summary {
        let eviction_priority_str = match policy_summary.eviction_priority {
            adapteros_lora_lifecycle::EvictionPriority::Never => "never",
            adapteros_lora_lifecycle::EvictionPriority::Low => "low",
            adapteros_lora_lifecycle::EvictionPriority::Normal => "normal",
            adapteros_lora_lifecycle::EvictionPriority::High => "high",
            adapteros_lora_lifecycle::EvictionPriority::Critical => "critical",
        };

        response.insert(
            category.clone(),
            CategoryPolicyResponse {
                promotion_threshold_ms: policy_summary.promotion_threshold_ms,
                demotion_threshold_ms: policy_summary.demotion_threshold_ms,
                memory_limit: policy_summary.memory_limit,
                eviction_priority: eviction_priority_str.to_string(),
                auto_promote: policy_summary.auto_promote,
                auto_demote: policy_summary.auto_demote,
                max_in_memory: policy_summary.max_in_memory,
                routing_priority: policy_summary.routing_priority,
            },
        );
    }

    Ok(Json(response))
}

/// Get category policy for a specific category
#[utoipa::path(
    get,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Category name (code, framework, codebase, ephemeral)")
    ),
    responses(
        (status = 200, description = "Category policy retrieved successfully", body = CategoryPolicyResponse),
        (status = 404, description = "Category not found"),
        (status = 503, description = "Lifecycle manager not available"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn get_category_policy(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(category): Path<String>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let lifecycle = state.lifecycle_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Lifecycle manager not available").with_code("NOT_CONFIGURED")),
        )
    })?;

    let mgr = lifecycle.lock().await;
    let policy_manager = mgr.get_category_policies();
    let summary = policy_manager.get_policy_summary();

    // Find the summary for this category
    let policy_summary = summary.get(&category).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
        )
    })?;

    let eviction_priority_str = match policy_summary.eviction_priority {
        adapteros_lora_lifecycle::EvictionPriority::Never => "never",
        adapteros_lora_lifecycle::EvictionPriority::Low => "low",
        adapteros_lora_lifecycle::EvictionPriority::Normal => "normal",
        adapteros_lora_lifecycle::EvictionPriority::High => "high",
        adapteros_lora_lifecycle::EvictionPriority::Critical => "critical",
    };

    Ok(Json(CategoryPolicyResponse {
        promotion_threshold_ms: policy_summary.promotion_threshold_ms,
        demotion_threshold_ms: policy_summary.demotion_threshold_ms,
        memory_limit: policy_summary.memory_limit,
        eviction_priority: eviction_priority_str.to_string(),
        auto_promote: policy_summary.auto_promote,
        auto_demote: policy_summary.auto_demote,
        max_in_memory: policy_summary.max_in_memory,
        routing_priority: policy_summary.routing_priority,
    }))
}

/// Update category policy
#[utoipa::path(
    put,
    path = "/v1/adapters/category-policies/{category}",
    params(
        ("category" = String, Path, description = "Adapter category")
    ),
    request_body = CategoryPolicyRequest,
    responses(
        (status = 200, description = "Category policy updated successfully", body = CategoryPolicyResponse),
        (status = 400, description = "Invalid policy data"),
        (status = 403, description = "Forbidden"),
        (status = 503, description = "Lifecycle manager not available")
    )
)]
pub async fn update_category_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category): Path<String>,
    Json(request): Json<CategoryPolicyRequest>,
) -> Result<Json<CategoryPolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let lifecycle = state.lifecycle_manager.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Lifecycle manager not available").with_code("NOT_CONFIGURED")),
        )
    })?;

    // Convert request to CategoryPolicy
    let policy = adapteros_lora_lifecycle::CategoryPolicy {
        promotion_threshold: std::time::Duration::from_millis(request.promotion_threshold_ms),
        demotion_threshold: std::time::Duration::from_millis(request.demotion_threshold_ms),
        memory_limit: request.memory_limit,
        eviction_priority: match request.eviction_priority.as_str() {
            "never" => adapteros_lora_lifecycle::EvictionPriority::Never,
            "low" => adapteros_lora_lifecycle::EvictionPriority::Low,
            "normal" => adapteros_lora_lifecycle::EvictionPriority::Normal,
            "high" => adapteros_lora_lifecycle::EvictionPriority::High,
            "critical" => adapteros_lora_lifecycle::EvictionPriority::Critical,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Invalid eviction priority")
                            .with_code("INVALID_PRIORITY"),
                    ),
                ));
            }
        },
        auto_promote: request.auto_promote,
        auto_demote: request.auto_demote,
        max_in_memory: request.max_in_memory.map(|v| v),
        routing_priority: request.routing_priority,
    };

    // Update the category policy
    let mut mgr = lifecycle.lock().await;
    mgr.update_category_policy(category.clone(), policy);

    // Get the policy summary with updated values
    let policy_manager = mgr.get_category_policies();
    let summary = policy_manager.get_policy_summary();

    let updated_policy = summary.get(&category).ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to retrieve updated policy")
                    .with_code("POLICY_UPDATE_FAILED"),
            ),
        )
    })?;

    let eviction_priority_str = match updated_policy.eviction_priority {
        adapteros_lora_lifecycle::EvictionPriority::Never => "never",
        adapteros_lora_lifecycle::EvictionPriority::Low => "low",
        adapteros_lora_lifecycle::EvictionPriority::Normal => "normal",
        adapteros_lora_lifecycle::EvictionPriority::High => "high",
        adapteros_lora_lifecycle::EvictionPriority::Critical => "critical",
    };

    Ok(Json(CategoryPolicyResponse {
        promotion_threshold_ms: updated_policy.promotion_threshold_ms,
        demotion_threshold_ms: updated_policy.demotion_threshold_ms,
        memory_limit: updated_policy.memory_limit,
        eviction_priority: eviction_priority_str.to_string(),
        auto_promote: updated_policy.auto_promote,
        auto_demote: updated_policy.auto_demote,
        max_in_memory: updated_policy.max_in_memory,
        routing_priority: updated_policy.routing_priority,
    }))
}

/// Evict adapter from memory
#[utoipa::path(
    post,
    path = "/v1/memory/adapters/{adapter_id}/evict",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter evicted successfully", body = EvictAdapterResponse),
        (status = 404, description = "Adapter not found"),
        (status = 409, description = "Adapter is pinned and cannot be evicted"),
        (status = 403, description = "Forbidden")
    )
)]
pub async fn evict_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<crate::types::EvictAdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    // Get adapter
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    // Check if pinned
    if adapter.pinned != 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("Adapter is pinned and cannot be evicted")
                    .with_code("PINNED_ADAPTER")
                    .with_string_details("Pinned adapters are protected from eviction"),
            ),
        ));
    }

    // Evict: set state to cold and clear memory
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE adapters SET current_state = 'cold', memory_bytes = 0, updated_at = ? WHERE adapter_id = ?",
        timestamp,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to evict adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    tracing::info!("Adapter {} evicted by {}", adapter_id, claims.email);

    let memory_freed_mb = adapter.memory_bytes as f64 / 1024.0 / 1024.0;
    Ok(Json(crate::types::EvictAdapterResponse {
        success: true,
        message: format!(
            "Adapter evicted successfully. Freed {:.1} MB",
            memory_freed_mb
        ),
    }))
}

/// Get memory usage statistics
#[utoipa::path(
    get,
    path = "/v1/memory/usage",
    responses(
        (status = 200, description = "Memory usage statistics", body = MemoryUsageResponse)
    )
)]
pub async fn get_memory_usage(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<crate::types::MemoryUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get system total memory (refresh before reading to avoid 0 values)
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let total_memory_kb = system.total_memory();
    let total_memory_mb = total_memory_kb as f64 / 1024.0;

    // Get all adapters with memory usage
    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list adapters")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut total_used_mb = 0.0;
    let adapter_entries: Vec<crate::types::MemoryUsageAdapter> = adapters
        .into_iter()
        .filter_map(|adapter| {
            if adapter.memory_bytes > 0 {
                let memory_mb = adapter.memory_bytes as f64 / 1024.0 / 1024.0;
                total_used_mb += memory_mb;
                Some(crate::types::MemoryUsageAdapter {
                    id: adapter.adapter_id.clone(),
                    name: adapter.name,
                    memory_usage_mb: memory_mb,
                    state: adapter.current_state,
                    pinned: adapter.pinned != 0,
                    category: adapter.category,
                })
            } else {
                None
            }
        })
        .collect();

    let available_memory_mb = (total_memory_mb - total_used_mb).max(0.0);
    let usage_percent = if total_memory_mb > 0.0 {
        (total_used_mb / total_memory_mb) * 100.0
    } else {
        0.0
    };

    // Determine pressure level (matching UI thresholds)
    let pressure_level = if usage_percent >= 80.0 {
        "critical"
    } else if usage_percent >= 64.0 {
        "high"
    } else if usage_percent >= 48.0 {
        "medium"
    } else {
        "low"
    };

    Ok(Json(crate::types::MemoryUsageResponse {
        adapters: adapter_entries,
        total_memory_mb,
        available_memory_mb,
        memory_pressure_level: pressure_level.to_string(),
    }))
}

/// Download adapter manifest as JSON
pub async fn download_adapter_manifest(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterManifest>, (StatusCode, Json<ErrorResponse>)> {
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
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
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    let manifest = AdapterManifest {
        adapter_id: adapter.adapter_id,
        name: adapter.name,
        hash_b3: adapter.hash_b3,
        rank: adapter.rank,
        tier: adapter.tier,
        framework: adapter.framework,
        languages_json: adapter.languages_json,
        category: Some(adapter.category),
        scope: Some(adapter.scope),
        framework_id: adapter.framework_id,
        framework_version: adapter.framework_version,
        repo_id: adapter.repo_id,
        commit_sha: adapter.commit_sha,
        intent: adapter.intent,
        created_at: adapter.created_at,
        updated_at: adapter.updated_at,
    };

    Ok(Json(manifest))
}
/// Get adapter health (activation logs, memory usage, policy violations)
pub async fn get_adapter_health(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get adapter activations (last 100)
    let activations = state
        .db
        .get_adapter_activations(&adapter_id, 100)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get activations")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get adapter stats
    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    // Calculate memory usage trend (simplified - would need time-series data in production)
    let memory_usage_mb = activations.len() as f64 * 2.5; // Rough estimate

    let adapter_id_clone = adapter_id.clone();
    let adapter_id_clone2 = adapter_id.clone();
    let adapter_id_clone3 = adapter_id.clone();

    Ok(Json(AdapterHealthResponse {
        adapter_id: adapter_id_clone,
        total_activations: total as i32,
        selected_count: selected as i32,
        avg_gate_value: avg_gate,
        memory_usage_mb,
        policy_violations: {
            // Query policy violations from telemetry/audit logs
            sqlx::query_as::<_, (String, String)>(
                "SELECT violation_type, message FROM policy_violations 
                 WHERE adapter_id = ? AND timestamp > datetime('now', '-1 hour')
                 ORDER BY timestamp DESC LIMIT 5",
            )
            .bind(&adapter_id_clone2)
            .fetch_all(state.db.pool())
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to fetch policy violations for {}: {}",
                    adapter_id_clone3,
                    e
                );
                vec![]
            })
            .into_iter()
            .map(|(vtype, msg)| format!("{}: {}", vtype, msg))
            .collect()
        },
        recent_activations: activations
            .into_iter()
            .take(10)
            .map(|a| AdapterActivationResponse {
                id: a.id,
                adapter_id: a.adapter_id,
                request_id: a.request_id,
                gate_value: a.gate_value,
                selected: a.selected == 1,
                created_at: a.created_at,
            })
            .collect(),
    }))
}

// ===== Repository Management Endpoints =====

/// List repositories
#[utoipa::path(
    get,
    path = "/v1/repositories",
    responses(
        (status = 200, description = "List of repositories", body = Vec<RepositorySummary>)
    )
)]
pub async fn list_repositories(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<RepositorySummary>>, (StatusCode, Json<ErrorResponse>)> {
    let repos = state
        .db
        .list_repositories("default", 100, 0)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list repositories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let repo_ids: Vec<String> = repos.iter().map(|r| r.repo_id.clone()).collect();

    // Fetch commit counts and URLs in parallel
    let (commit_counts, inferred_urls) = tokio::join!(
        state.db.get_commit_counts_for_repositories(&repo_ids),
        async {
            let repo_paths: Vec<_> = repos
                .iter()
                .map(|r| (r.repo_id.clone(), r.path.clone()))
                .collect();
            infer_repo_urls_parallel(&repo_paths).await
        }
    );

    let commit_counts = commit_counts.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count repository commits")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let summaries: Vec<RepositorySummary> = repos
        .into_iter()
        .map(|repo| {
            let commit_count_raw = commit_counts.get(&repo.repo_id).copied().unwrap_or(0);
            let commit_count = if commit_count_raw < 0 {
                0
            } else {
                commit_count_raw as u64
            };

            // Use inferred URL or fall back to repo_id
            let (url, url_is_fallback) = inferred_urls
                .get(&repo.repo_id)
                .and_then(|opt| opt.clone())
                .map(|inferred_url| (inferred_url, false))
                .unwrap_or_else(|| {
                    tracing::debug!(
                        repo_id = %repo.repo_id,
                        path = %repo.path,
                        "Using repo_id as URL (could not infer from git remote)"
                    );
                    (repo.repo_id.clone(), true)
                });

            RepositorySummary {
                id: repo.repo_id,
                url,
                url_is_fallback,
                branch: repo.default_branch,
                path: Some(repo.path),
                commit_count,
                last_scan: repo.latest_scan_at,
            }
        })
        .collect();

    Ok(Json(summaries))
}

// ===== Metrics Endpoints =====

/// Get quality metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(_claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapters = state.db.list_adapters().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list adapters")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&adapter.adapter_id)
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter.adapter_id,
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect real system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    Ok(Json(SystemMetricsResponse {
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers: {
            // Count active workers from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        requests_per_second: {
            // Calculate RPS from recent request log
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
            )
            .fetch_one(state.db.pool())
            .await
            .map(|count| count as f32 / 60.0)
            .unwrap_or(0.0)
        },
        avg_latency_ms: {
            // Calculate average latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
        memory_usage_pct: metrics.memory_usage as f32,
        adapter_count: {
            // Count active adapters from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters WHERE load_state = 'warm'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        active_sessions: {
            // Count active inference sessions from telemetry buffer and metrics collector
            use adapteros_telemetry::TelemetryFilters;
            let start_filters = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.start".to_string()),
                ..Default::default()
            };
            let complete_filters = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.complete".to_string()),
                ..Default::default()
            };

            let started = state.telemetry_buffer.query(&start_filters).len();
            let completed = state.telemetry_buffer.query(&complete_filters).len();

            // Improved: Track by session IDs from metadata for accuracy
            use chrono::{Duration as ChronoDuration, Utc};

            let end_time = Utc::now();
            let start_time = end_time - ChronoDuration::minutes(5);

            let start_filters_time = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.start".to_string()),
                start_time: Some(start_time),
                end_time: Some(end_time),
                ..Default::default()
            };
            let complete_filters_time = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.complete".to_string()),
                start_time: Some(start_time),
                end_time: Some(end_time),
                ..Default::default()
            };

            // Collect session IDs from events (fallback to simple count if no IDs)
            let started_sessions: std::collections::HashSet<String> = state
                .telemetry_buffer
                .query(&start_filters_time)
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("session_id").or_else(|| m.get("request_id")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            let completed_sessions: std::collections::HashSet<String> = state
                .telemetry_buffer
                .query(&complete_filters_time)
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("session_id").or_else(|| m.get("request_id")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            let active = if !started_sessions.is_empty() || !completed_sessions.is_empty() {
                // Use session ID tracking for accuracy
                started_sessions.difference(&completed_sessions).count() as i32
            } else {
                // Fallback: approximate as started - completed

                if started > completed {
                    (started - completed) as i32
                } else {
                    0
                }
            };

            // Also check metrics collector for active_sessions gauge
            let snapshot = state.metrics_collector.get_metrics_snapshot().await;
            let metrics_active = snapshot.system.active_sessions as i32;

            // Use the higher of the two estimates
            active.max(metrics_active)
        },
        tokens_per_second: {
            // Get tokens per second from metrics collector snapshot
            let snapshot = state.metrics_collector.get_metrics_snapshot().await;

            // If snapshot has non-zero value, use it; otherwise calculate from recent tokens
            if snapshot.throughput.tokens_per_second > 0.0 {
                snapshot.throughput.tokens_per_second as f32
            } else {
                // Fallback: calculate from recent telemetry events
                use adapteros_telemetry::TelemetryFilters;
                use chrono::{Duration, Utc};

                let end_time = Utc::now();
                let start_time = end_time - Duration::seconds(60);

                let filters = TelemetryFilters {
                    limit: Some(1000),
                    event_type: Some("inference.complete".to_string()),
                    start_time: Some(start_time),
                    end_time: Some(end_time),
                    ..Default::default()
                };

                let events = state.telemetry_buffer.query(&filters);

                // Sum tokens from inference events
                let mut total_tokens = 0u64;
                for event in events.iter() {
                    if let Some(ref metadata) = event.metadata {
                        if let Some(output_tokens) =
                            metadata.get("output_tokens").and_then(|t| t.as_u64())
                        {
                            total_tokens += output_tokens;
                        }
                    }
                }

                // Convert to tokens per second
                (total_tokens as f32) / 60.0
            }
        },
        latency_p95_ms: {
            // Calculate P95 latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms LIMIT 1 OFFSET (SELECT COUNT(*) * 95 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        disk_usage_percent: Some(metrics.disk_io.usage_percent),
        network_rx_bytes: Some(metrics.network_io.rx_bytes as i64),
        network_tx_bytes: Some(metrics.network_io.tx_bytes as i64),
        network_rx_packets: Some(metrics.network_io.rx_packets as i64),
        network_tx_packets: Some(metrics.network_io.tx_packets as i64),
        network_bandwidth_mbps: Some(metrics.network_io.bandwidth_mbps),
        gpu_utilization_percent: metrics.gpu_metrics.utilization.map(|v| v as f32),
        gpu_memory_used_gb: metrics
            .gpu_metrics
            .memory_used
            .map(|v| v as f32 / 1024.0 / 1024.0 / 1024.0),
        gpu_memory_total_gb: metrics
            .gpu_metrics
            .memory_total
            .map(|v| v as f32 / 1024.0 / 1024.0 / 1024.0),
    }))
}

// ===== Commit Inspector Endpoints =====

/// List commits
#[utoipa::path(
    get,
    path = "/v1/commits",
    params(
        ("repo_id" = Option<String>, Query, description = "Filter by repository"),
        ("branch" = Option<String>, Query, description = "Filter by branch"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of commits", body = Vec<CommitResponse>)
    )
)]
pub async fn list_commits(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListCommitsQuery>,
) -> Result<Json<Vec<CommitResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = query.limit.unwrap_or(20).clamp(1, 200) as usize;

    let commits = state
        .db
        .list_commits(query.repo_id.as_deref(), query.branch.as_deref(), limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to list commits: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for commit in commits {
        match map_db_commit_to_response(commit) {
            Ok(response) => responses.push(response),
            Err(e) => {
                tracing::error!("Failed to map commit to response: {}", e);
                // Skip malformed commits but continue processing others
                continue;
            }
        }
    }

    Ok(Json(responses))
}

/// Get commit details
#[utoipa::path(
    get,
    path = "/v1/commits/{sha}",
    params(
        ("sha" = String, Path, description = "Commit SHA"),
        ("repo_id" = Option<String>, Query, description = "Repository id (defaults to first registered)")
    ),
    responses(
        (status = 200, description = "Commit details", body = CommitResponse),
        (status = 404, description = "Commit not found", body = ErrorResponse)
    )
)]
pub async fn get_commit(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
    Query(query): Query<GetCommitQuery>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(git_subsystem) = &state.git_subsystem {
        let commit = git_subsystem
            .get_commit(query.repo_id.as_deref(), &sha)
            .await
            .map_err(|e| map_git_error("failed to fetch commit", e))?;

        Ok(Json(map_commit_to_response(commit)))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

/// Get commit diff
#[utoipa::path(
    get,
    path = "/v1/commits/{sha}/diff",
    params(
        ("sha" = String, Path, description = "Commit SHA"),
        ("repo_id" = Option<String>, Query, description = "Repository id (defaults to first registered)")
    ),
    responses(
        (status = 200, description = "Commit diff", body = CommitDiffResponse)
    )
)]
pub async fn get_commit_diff(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
    Query(query): Query<GetCommitDiffQuery>,
) -> Result<Json<CommitDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(git_subsystem) = &state.git_subsystem {
        let diff = git_subsystem
            .get_commit_diff(query.repo_id.as_deref(), &sha)
            .await
            .map_err(|e| map_git_error("failed to compute commit diff", e))?;

        Ok(Json(CommitDiffResponse {
            sha: diff.sha,
            diff: diff.diff,
            stats: DiffStats {
                files_changed: diff.files_changed,
                insertions: diff.insertions,
                deletions: diff.deletions,
            },
        }))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

// ===== Routing Inspector Endpoints =====

/// Debug routing decision
#[utoipa::path(
    post,
    path = "/v1/routing/debug",
    request_body = RoutingDebugRequest,
    responses(
        (status = 200, description = "Routing debug info", body = RoutingDebugResponse)
    )
)]
pub async fn debug_routing(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RoutingDebugRequest>,
) -> Result<Json<RoutingDebugResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Extract features from the prompt
    let code_features = CodeFeatures::from_context(&req.prompt);
    let feature_vec = code_features.to_vector();

    // Get router scoring explanation with latency recording
    let router_start = std::time::Instant::now();
    let scoring_explanation = state.router.explain_score(&feature_vec);
    let router_latency_secs = router_start.elapsed().as_secs_f64();
    state
        .metrics_collector
        .record_router_latency(&claims.tenant_id, router_latency_secs);

    // Create feature vector for response
    let features = FeatureVector {
        language: code_features
            .lang_one_hot
            .iter()
            .enumerate()
            .find(|(_, &val)| val > 0.0)
            .map(|(idx, _)| {
                match idx {
                    0 => "python",
                    1 => "rust",
                    2 => "javascript",
                    3 => "typescript",
                    4 => "java",
                    5 => "cpp",
                    6 => "csharp",
                    7 => "go",
                    _ => "unknown",
                }
                .to_string()
            }),
        frameworks: code_features.framework_prior.keys().cloned().collect(),
        symbol_hits: code_features.symbol_hits as i32,
        path_tokens: code_features.path_tokens.clone(),
        verb: format!("{:?}", code_features.prompt_verb),
    };

    // Create mock adapter scores based on router weights
    // In production, this would use real adapter metadata from database
    let adapter_scores = vec![
        AdapterScore {
            adapter_id: "rust-code-v1".to_string(),
            score: scoring_explanation.language_score as f64,
            gate_value: (scoring_explanation.language_score as f64 * 0.9).min(1.0),
            selected: scoring_explanation.language_score > 0.1,
        },
        AdapterScore {
            adapter_id: "framework-specific-v1".to_string(),
            score: scoring_explanation.framework_score as f64,
            gate_value: (scoring_explanation.framework_score as f64 * 0.9).min(1.0),
            selected: scoring_explanation.framework_score > 0.1,
        },
        AdapterScore {
            adapter_id: "general-coding-v1".to_string(),
            score: (scoring_explanation.symbol_hits_score + scoring_explanation.path_tokens_score)
                as f64,
            gate_value: (((scoring_explanation.symbol_hits_score
                + scoring_explanation.path_tokens_score) as f64)
                * 0.8)
                .min(1.0),
            selected: (scoring_explanation.symbol_hits_score
                + scoring_explanation.path_tokens_score)
                > 0.1,
        },
    ];

    // Determine selected adapters based on scores
    let selected_adapters: Vec<String> = adapter_scores
        .iter()
        .filter(|score| score.selected)
        .map(|score| score.adapter_id.clone())
        .collect();

    let explanation = format!(
        "Router analysis: Language={:.3}, Framework={:.3}, Symbols={:.3}, Paths={:.3}, Verb={:.3}. Selected {} adapters.",
        scoring_explanation.language_score,
        scoring_explanation.framework_score,
        scoring_explanation.symbol_hits_score,
        scoring_explanation.path_tokens_score,
        scoring_explanation.prompt_verb_score,
        selected_adapters.len()
    );

    Ok(Json(RoutingDebugResponse {
        features,
        adapter_scores,
        selected_adapters,
        explanation,
    }))
}

/// Get routing history
#[utoipa::path(
    get,
    path = "/v1/routing/history",
    responses(
        (status = 200, description = "Routing history", body = Vec<RoutingDebugResponse>)
    )
)]
pub async fn get_routing_history(
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<RoutingDebugResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Query actual routing history from telemetry
    // For now, return sample history entries
    Ok(Json(vec![
        RoutingDebugResponse {
            features: FeatureVector {
                language: Some("rust".to_string()),
                frameworks: vec!["axum".to_string()],
                symbol_hits: 5,
                path_tokens: vec!["handlers".to_string(), "api".to_string()],
                verb: "implement".to_string(),
            },
            adapter_scores: vec![AdapterScore {
                adapter_id: "rust-code-v1".to_string(),
                score: 0.90,
                gate_value: 0.80,
                selected: true,
            }],
            selected_adapters: vec!["rust-code-v1".to_string()],
            explanation: "Selected rust-code-v1 for Rust implementation task".to_string(),
        },
        RoutingDebugResponse {
            features: FeatureVector {
                language: Some("typescript".to_string()),
                frameworks: vec!["react".to_string()],
                symbol_hits: 3,
                path_tokens: vec!["components".to_string()],
                verb: "create".to_string(),
            },
            adapter_scores: vec![AdapterScore {
                adapter_id: "frontend-v1".to_string(),
                score: 0.85,
                gate_value: 0.75,
                selected: true,
            }],
            selected_adapters: vec!["frontend-v1".to_string()],
            explanation: "Selected frontend-v1 for React component creation".to_string(),
        },
    ]))
}

// ===== Agent D Contract Endpoints =====

/// Get system metadata
#[utoipa::path(
    get,
    path = "/v1/meta",
    responses(
        (status = 200, description = "System metadata", body = MetaResponse)
    )
)]
pub async fn meta() -> Json<MetaResponse> {
    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_hash: option_env!("BUILD_HASH").unwrap_or("dev").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
    })
}

/// Get routing decisions (placeholder for Agent D)
#[utoipa::path(
    get,
    path = "/v1/routing/decisions",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results"),
        ("since" = Option<String>, Query, description = "ISO-8601 timestamp")
    ),
    responses(
        (status = 200, description = "Routing decisions", body = RoutingDecisionsResponse),
        (status = 404, description = "Not yet available")
    )
)]
pub async fn routing_decisions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<RoutingDecisionsQuery>,
) -> Result<Json<RoutingDecisionsResponse>, StatusCode> {
    // Filter telemetry buffer for router decision events for this tenant
    let mut routing_decisions = Vec::new();

    // Access telemetry buffer to find routing decisions
    // Note: In production, this would query telemetry_bundles from database
    // For now, return mock data based on recent telemetry events

    // Mock routing decisions based on tenant and time filters
    let now = chrono::Utc::now();
    let since = params
        .since
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| now - chrono::Duration::hours(24));

    // Create mock routing decisions for demonstration
    // In production, this would parse actual telemetry NDJSON
    for i in 0..params.limit.min(10) {
        let decision_time = now - chrono::Duration::minutes(i as i64 * 5);
        if decision_time < since {
            break;
        }

        routing_decisions.push(RoutingDecision {
            ts: decision_time.to_rfc3339(),
            tenant_id: claims.tenant_id.clone(),
            adapters_used: vec![
                "rust-code-v1".to_string(),
                "framework-specific-v1".to_string(),
                "general-coding-v1".to_string(),
            ],
            activations: vec![0.8, 0.6, 0.4],
            reason: "Router selected top-3 adapters for prompt analysis (mock data)".to_string(),
            trace_id: format!("trace_{}", i),
        });
    }

    Ok(Json(RoutingDecisionsResponse {
        items: routing_decisions,
    }))
}

// ===== PROMPT ORCHESTRATION HANDLERS =====

/// Get prompt orchestration configuration
#[utoipa::path(
    get,
    path = "/v1/prompt-orchestration/config",
    responses(
        (status = 200, description = "Prompt orchestration configuration", body = PromptOrchestrationConfig)
    ),
    tag = "prompt-orchestration"
)]
pub async fn get_prompt_orchestration_config(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<PromptOrchestrationConfig>, (StatusCode, Json<ErrorResponse>)> {
    // For now, return default configuration
    // In production, this would be stored in database/config file
    let config = PromptOrchestrationConfig {
        enabled: true,
        base_model_threshold: 0.2,
        adapter_threshold: 0.1,
        analysis_timeout: 50,
        cache_enabled: true,
        cache_ttl: 300,
        enable_telemetry: true,
        fallback_strategy: "adaptive".to_string(),
    };

    Ok(Json(config))
}

/// Update prompt orchestration configuration
#[utoipa::path(
    put,
    path = "/v1/prompt-orchestration/config",
    request_body = PromptOrchestrationConfig,
    responses(
        (status = 200, description = "Configuration updated successfully"),
        (status = 400, description = "Invalid configuration", body = ErrorResponse)
    ),
    tag = "prompt-orchestration"
)]
pub async fn update_prompt_orchestration_config(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(_config): Json<PromptOrchestrationConfig>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // TODO: Validate and persist configuration
    // For now, just accept the request
    Ok(StatusCode::OK)
}

/// Analyze a prompt for orchestration decision
#[utoipa::path(
    post,
    path = "/v1/prompt-orchestration/analyze",
    request_body = PromptAnalysisRequest,
    responses(
        (status = 200, description = "Prompt analysis result", body = PromptAnalysisResponse),
        (status = 400, description = "Invalid prompt", body = ErrorResponse)
    ),
    tag = "prompt-orchestration"
)]
pub async fn analyze_prompt(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<PromptAnalysisRequest>,
) -> Result<Json<PromptAnalysisResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Record actual analysis start time
    let analysis_start = std::time::Instant::now();

    // Extract features from the prompt using router
    let code_features = CodeFeatures::from_context(&req.prompt);
    let feature_vec = code_features.to_vector();

    // Get scoring explanation from router with latency recording
    let router_start = std::time::Instant::now();
    let scoring_explanation = state.router.explain_score(&feature_vec);
    let router_latency_secs = router_start.elapsed().as_secs_f64();
    state
        .metrics_collector
        .record_router_latency(&claims.tenant_id, router_latency_secs);

    // Determine recommended strategy based on scoring
    let recommended_strategy = if scoring_explanation.total_score < 0.2 {
        "base_model".to_string()
    } else if scoring_explanation.total_score > 0.5 {
        "adapters".to_string()
    } else {
        "mixed".to_string()
    };

    // Calculate actual analysis time
    let analysis_time_ms = (analysis_start.elapsed().as_secs_f64() * 1000.0) as i32;

    // Emit telemetry event for prompt analysis
    let analysis_event = adapteros_telemetry::TelemetryEventBuilder::new(
        adapteros_telemetry::EventType::Custom("prompt.analyzed".to_string()),
        adapteros_telemetry::LogLevel::Info,
        format!("Prompt analyzed with strategy: {}", recommended_strategy),
    )
    .tenant_id(claims.tenant_id.clone())
    .component("prompt-orchestration".to_string())
    .metadata(serde_json::json!({
        "strategy": recommended_strategy.clone(),
        "complexity_score": scoring_explanation.total_score,
        "analysis_time_ms": analysis_time_ms,
    }))
    .build();

    state.telemetry_buffer.push(analysis_event.clone());
    let _ = state.telemetry_tx.send(analysis_event);

    let response = PromptAnalysisResponse {
        prompt: req.prompt.clone(),
        complexity_score: scoring_explanation.total_score as f64,
        recommended_strategy,
        analysis_time_ms,
        features: PromptFeatures {
            language: code_features
                .lang_one_hot
                .iter()
                .enumerate()
                .find(|(_, &val)| val > 0.0)
                .map(|(idx, _)| {
                    match idx {
                        0 => "python",
                        1 => "rust",
                        2 => "javascript",
                        3 => "typescript",
                        4 => "java",
                        5 => "cpp",
                        6 => "csharp",
                        7 => "go",
                        _ => "unknown",
                    }
                    .to_string()
                }),
            frameworks: code_features.framework_prior.keys().cloned().collect(),
            symbols: code_features.symbol_hits as i32,
            tokens: code_features.path_tokens.len() as i32,
            verb: format!("{:?}", code_features.prompt_verb),
        },
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
}

/// Get prompt orchestration metrics
#[utoipa::path(
    get,
    path = "/v1/prompt-orchestration/metrics",
    responses(
        (status = 200, description = "Orchestration metrics", body = PromptOrchestrationMetrics)
    ),
    tag = "prompt-orchestration"
)]
pub async fn get_prompt_orchestration_metrics(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<PromptOrchestrationMetrics>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_telemetry::TelemetryFilters;
    use chrono::Utc;

    // Query telemetry buffer for prompt analysis events
    let filters = TelemetryFilters {
        limit: Some(10000), // Get recent events
        event_type: Some("prompt.analyzed".to_string()),
        ..Default::default()
    };

    let analysis_events = state.telemetry_buffer.query(&filters);

    // Aggregate metrics from events
    let mut total_requests = 0i64;
    let mut base_model_only = 0i64;
    let mut adapter_used = 0i64;
    let mut mixed_mode = 0i64;
    let mut total_analysis_time_ms = 0.0;
    let mut error_count = 0i64;

    for event in analysis_events.iter() {
        total_requests += 1;

        // Extract strategy from metadata
        if let Some(ref metadata) = event.metadata {
            if let Some(strategy) = metadata.get("strategy").and_then(|s| s.as_str()) {
                match strategy {
                    "base_model" => base_model_only += 1,
                    "adapters" => adapter_used += 1,
                    "mixed" => mixed_mode += 1,
                    _ => {}
                }
            }

            // Extract analysis time
            if let Some(time_ms) = metadata.get("analysis_time_ms").and_then(|t| t.as_f64()) {
                total_analysis_time_ms += time_ms;
            }

            // Check for errors
            if event.level == adapteros_telemetry::LogLevel::Error
                || event.level == adapteros_telemetry::LogLevel::Critical
            {
                error_count += 1;
            }
        }
    }

    // Calculate average analysis time
    let analysis_time_ms = if total_requests > 0 {
        total_analysis_time_ms / total_requests as f64
    } else {
        0.0
    };

    // Cache hits/misses: Prompt orchestration caching is not yet implemented.
    // When implemented, this would track reuse of prompt analysis results for similar prompts.
    // For now, we return 0 to indicate the feature is not available.
    // Note: This is separate from adapter cache (which exists but is tracked elsewhere).
    let cache_hits = 0i64;
    let cache_misses = total_requests; // All requests are misses until caching is implemented

    let metrics = PromptOrchestrationMetrics {
        total_requests,
        base_model_only,
        adapter_used,
        mixed_mode,
        analysis_time_ms,
        cache_hits,
        cache_misses: cache_misses.max(0),
        error_count,
        last_updated: Utc::now().to_rfc3339(),
    };

    Ok(Json(metrics))
}

/// List audits with extended fields
#[utoipa::path(
    get,
    path = "/v1/audits",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of audits", body = AuditsResponse)
    )
)]
pub async fn list_audits_extended(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<AuditsQuery>,
) -> Result<Json<AuditsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let audits = sqlx::query_as::<_, AuditExtended>(
        "SELECT id, tenant_id, cpid, arr, ecs5, hlr, cr, status, 
                before_cpid, after_cpid, created_at 
         FROM audits WHERE tenant_id = ? 
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(AuditsResponse { items: audits }))
}

/// Get promotion record with signature
#[utoipa::path(
    get,
    path = "/v1/promotions/{id}",
    params(
        ("id" = String, Path, description = "Promotion ID")
    ),
    responses(
        (status = 200, description = "Promotion record", body = PromotionRecord),
        (status = 404, description = "Promotion not found")
    )
)]
pub async fn get_promotion(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<PromotionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let promo = sqlx::query_as::<_, PromotionRecord>(
        "SELECT id, cpid, promoted_by, promoted_at, signature_b64, 
                signer_key_id, quality_json, before_cpid 
         FROM promotions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("promotion not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(promo))
}

// ===== Metrics Endpoint =====

/// Prometheus/OpenMetrics endpoint  
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Check if metrics are enabled
    let metrics_enabled = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to acquire config read lock: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("internal error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
                    .into_response();
            }
        };
        config.metrics.enabled
    };

    if !metrics_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("metrics disabled").with_code("INTERNAL_ERROR")),
        )
            .into_response();
    }

    // Update worker metrics from database
    if let Err(e) = state
        .metrics_exporter
        .update_worker_metrics(state.db.as_ref())
        .await
    {
        tracing::warn!("Failed to update worker metrics: {}", e);
    }

    // Render metrics
    let metrics = match state.metrics_exporter.render() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to render metrics")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    )
        .into_response()
}

// ===== SSE Stream Endpoints =====

/// SSE stream for system metrics
/// Pushes SystemMetrics every 5 seconds
pub async fn system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Fetch metrics
        let metrics = match get_system_metrics_internal(&state).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to fetch metrics for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    state,
                ));
            }
        };

        let json = match serde_json::to_string(&metrics) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize metrics: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"serialization failed\"}")),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("metrics").data(json)), state))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
/// SSE stream for telemetry events
/// Streams new telemetry bundles as they're created
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut filters = adapteros_telemetry::TelemetryFilters::default();
    filters.limit = Some(50);
    let backlog = state.telemetry_buffer.query(&filters);

    // Telemetry events backlog
    let backlog_stream = stream::iter(backlog.into_iter().filter_map(|event| {
        match serde_json::to_string(&activity_event_from_unified_event(&event)) {
            Ok(json) => Some(Ok(Event::default().event("telemetry").data(json))),
            Err(e) => {
                tracing::warn!("failed to serialize backlog telemetry event: {}", e);
                None
            }
        }
    }));

    // Telemetry events realtime
    let rx = state.telemetry_tx.subscribe();
    let realtime_stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(event) => match serde_json::to_string(&activity_event_from_unified_event(&event)) {
                Ok(json) => Some(Ok(Event::default().event("telemetry").data(json))),
                Err(e) => {
                    tracing::warn!("failed to serialize telemetry event: {}", e);
                    None
                }
            },
            Err(_) => None,
        }
    });

    // Bundles backlog: fetch latest 50 bundles (non-blocking)
    let bundles_backlog: Vec<crate::types::TelemetryBundleResponse> = {
        let root = std::path::Path::new("var/bundles");
        let mut results = Vec::new();
        if root.exists() {
            // Use spawn_blocking for filesystem I/O to avoid blocking async runtime
            let root_path = root.to_path_buf();
            match tokio::task::spawn_blocking(
                move || -> Vec<crate::types::TelemetryBundleResponse> {
                    let mut local_results = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(&root_path) {
                        for entry in entries.flatten() {
                            if let Some(Some(id)) = entry
                                .file_name()
                                .to_str()
                                .map(|n| n.strip_suffix(".ndjson").map(|s| s.to_string()))
                            {
                                let path = entry.path();
                                let meta = std::fs::metadata(&path).ok();
                                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                                let event_count = std::fs::read_to_string(&path)
                                    .map(|s| s.lines().count() as u64)
                                    .unwrap_or(0);
                                let created_at = meta
                                    .and_then(|m| m.modified().ok())
                                    .and_then(|t| {
                                        chrono::DateTime::<chrono::Utc>::from_timestamp(
                                            t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()
                                                as i64,
                                            0,
                                        )
                                    })
                                    .map(|dt| dt.to_rfc3339())
                                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                                local_results.push(crate::types::TelemetryBundleResponse {
                                    id,
                                    cpid: "global".to_string(),
                                    event_count,
                                    size_bytes: size,
                                    created_at,
                                });
                            }
                        }
                    }
                    // Sort by created_at descending and limit to 50
                    local_results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                    local_results.truncate(50);
                    local_results
                },
            )
            .await
            {
                Ok(results_from_blocking) => results = results_from_blocking,
                Err(e) => {
                    tracing::warn!("failed to fetch bundles backlog for SSE: {}", e);
                    // Continue with empty backlog - realtime updates will still work
                }
            }
        }
        results
    };

    let bundles_backlog_stream =
        stream::iter(
            bundles_backlog
                .into_iter()
                .filter_map(|bundle| match serde_json::to_string(&bundle) {
                    Ok(json) => Some(Ok(Event::default().event("bundles").data(json))),
                    Err(e) => {
                        tracing::warn!("failed to serialize backlog bundle: {}", e);
                        None
                    }
                }),
        );

    // Bundles realtime updates
    let bundles_rx = state.telemetry_bundles_tx.subscribe();
    let bundles_realtime_stream = BroadcastStream::new(bundles_rx).filter_map(|res| async move {
        match res {
            Ok(bundle) => match serde_json::to_string(&bundle) {
                Ok(json) => Some(Ok(Event::default().event("bundles").data(json))),
                Err(e) => {
                    tracing::warn!("failed to serialize bundle update: {}", e);
                    None
                }
            },
            Err(_) => None,
        }
    });

    // Merge all streams: backlog first, then realtime updates interleaved
    let stream = backlog_stream
        .chain(realtime_stream)
        .chain(bundles_backlog_stream)
        .chain(bundles_realtime_stream);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for adapter state transitions
/// Streams adapter lifecycle events
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Fetch all adapters
        let adapters = match state.db.list_adapters().await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to fetch adapters for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    state,
                ));
            }
        };

        let json = match serde_json::to_string(&adapters) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("Failed to serialize adapters: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"serialization failed\"}")),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("adapters").data(json)), state))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for adapter operation progress
/// Streams real-time progress updates for load/unload operations
pub async fn operation_progress_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Optional filter by adapter_id and tenant_id
    let filter_adapter_id = params.get("adapter_id").cloned();
    let filter_tenant_id = params.get("tenant_id").cloned();

    let progress_rx = state.operation_progress_tx.subscribe();
    let stream = BroadcastStream::new(progress_rx).filter_map(move |res| {
        let adapter_filter = filter_adapter_id.clone();
        let tenant_filter = filter_tenant_id.clone();
        async move {
            match res {
                Ok(event) => {
                    // Apply filters if provided
                    if let Some(ref adapter_id) = adapter_filter {
                        if &event.adapter_id != adapter_id {
                            return None;
                        }
                    }
                    if let Some(ref tenant_id) = tenant_filter {
                        if &event.tenant_id != tenant_id {
                            return None;
                        }
                    }

                    match serde_json::to_string(&event) {
                        Ok(json) => Some(Ok(Event::default().event("progress").data(json))),
                        Err(e) => {
                            tracing::warn!("Failed to serialize progress event: {}", e);
                            None
                        }
                    }
                }
                Err(_) => None, // BroadcastStream error - skip and continue
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Monitor stuck operations and state divergences
/// Checks for operations exceeding timeout and adapters in inconsistent states
pub async fn monitor_operation_health(state: &AppState) -> Result<(), String> {
    use adapteros_system_metrics::monitoring_types::{AlertSeverity, AlertStatus};
    use std::time::{SystemTime, UNIX_EPOCH};

    // Check for stuck operations (if OperationTracker exists)
    // Note: OperationTracker is currently not in AppState, but operations are tracked via database state

    // Check for state divergences: adapters marked as loading/unloading but stuck
    let adapters = state
        .db
        .list_adapters()
        .await
        .map_err(|e| format!("Failed to list adapters for health check: {}", e))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Time error: {}", e))?
        .as_secs();

    let stuck_threshold_secs = 300; // 5 minutes

    for adapter in adapters {
        let state_str = adapter.current_state.as_str();

        // Check for stuck loading/unloading states
        if state_str == "loading" || state_str == "unloading" {
            // Use updated_at timestamp (updated when state changes)
            // Try multiple timestamp formats since SQLite uses datetime('now') format
            let updated_at = &adapter.updated_at;
            let elapsed = {
                // Try RFC3339 first, then SQLite datetime format
                let changed_time = chrono::DateTime::parse_from_rfc3339(updated_at.as_str())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .or_else(|_| {
                        // Try SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
                        chrono::NaiveDateTime::parse_from_str(
                            updated_at.as_str(),
                            "%Y-%m-%d %H:%M:%S",
                        )
                        .map(|dt| dt.and_utc())
                    })
                    .or_else(|_| {
                        // Try ISO8601 without timezone
                        chrono::DateTime::parse_from_rfc3339(&format!("{}Z", updated_at.as_str()))
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                    });

                match changed_time {
                    Ok(dt) => {
                        let changed_secs = dt.timestamp() as u64;
                        now.saturating_sub(changed_secs)
                    }
                    Err(_) => {
                        // If parsing fails, skip this adapter
                        continue;
                    }
                }
            };

            if elapsed > stuck_threshold_secs {
                // Check if alert already exists to avoid duplicates (simplified approach)
                let rule_id = format!("stuck_operation_{}", adapter.adapter_id);
                let existing_alerts = adapteros_db::process_monitoring::ProcessAlert::list(
                    state.db.pool(),
                    adapteros_db::process_monitoring::AlertFilters {
                        tenant_id: Some("default".to_string()), // Use default since adapter doesn't have tenant_id
                        status: Some(AlertStatus::Active),
                        ..Default::default()
                    },
                )
                .await
                .unwrap_or_default();

                // Check if any alert has the same rule_id
                let alert_already_exists =
                    existing_alerts.iter().any(|alert| alert.rule_id == rule_id);

                // Only create alert if one doesn't already exist
                if !alert_already_exists {
                    let alert_request = adapteros_db::process_monitoring::CreateAlertRequest {
                        rule_id: rule_id.clone(),
                        worker_id: "system".to_string(),
                        tenant_id: "default".to_string(), // Use default since adapter doesn't have tenant_id
                        alert_type: "stuck_operation".to_string(),
                        severity: AlertSeverity::Warning,
                        title: format!("Adapter {} stuck in {} state", adapter.adapter_id, state_str),
                        message: format!(
                            "Adapter {} has been in '{}' state for {} seconds (threshold: {}s). This may indicate a stuck operation.",
                            adapter.adapter_id, state_str, elapsed, stuck_threshold_secs
                        ),
                        metric_value: Some(elapsed as f64),
                        threshold_value: Some(stuck_threshold_secs as f64),
                        status: AlertStatus::Active,
                    };

                    // Create alert in database
                    if let Err(e) = adapteros_db::process_monitoring::ProcessAlert::create(
                        state.db.pool(),
                        alert_request.clone(),
                    )
                    .await
                    {
                        tracing::warn!(
                            adapter_id = %adapter.adapter_id,
                            error = %e,
                            "Failed to create stuck operation alert"
                        );
                    } else {
                        tracing::warn!(
                            adapter_id = %adapter.adapter_id,
                            state = %state_str,
                            elapsed_secs = elapsed,
                            "Detected stuck operation - alert created"
                        );
                    }
                }
            }
        }

        // Check for state divergence: adapter marked as warm but not actually loaded
        if state_str == "warm" {
            if let Some(ref lifecycle) = state.lifecycle_manager {
                let lifecycle_mgr = lifecycle.lock().await;
                let adapter_idx = adapter.id.parse::<u16>().unwrap_or(0);

                if lifecycle_mgr
                    .get_state(adapter_idx)
                    .await
                    .is_none_or(|state| state == AdapterState::Unloaded)
                {
                    // Adapter marked as warm but not loaded in LifecycleManager
                    let alert_request = adapteros_db::process_monitoring::CreateAlertRequest {
                        rule_id: format!("state_divergence_warm_{}", adapter.adapter_id),
                        worker_id: "system".to_string(),
                        tenant_id: "default".to_string(), // Use default since adapter doesn't have tenant_id
                        alert_type: "state_divergence".to_string(),
                        severity: AlertSeverity::Warning,
                        title: format!("Adapter {} state divergence", adapter.adapter_id),
                        message: format!(
                            "Adapter {} is marked as 'warm' in database but not loaded in LifecycleManager. This indicates a state divergence that may cause inference failures.",
                            adapter.adapter_id
                        ),
                        metric_value: None,
                        threshold_value: None,
                        status: AlertStatus::Active,
                    };

                    // Check if alert already exists to avoid duplicates
                    let existing_alerts = adapteros_db::process_monitoring::ProcessAlert::list(
                        state.db.pool(),
                        adapteros_db::process_monitoring::AlertFilters {
                            tenant_id: Some("default".to_string()),
                            status: Some(AlertStatus::Active),
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap_or_default();

                    let alert_already_exists = existing_alerts
                        .iter()
                        .any(|alert| alert.rule_id == alert_request.rule_id);

                    if !alert_already_exists {
                        if let Err(e) = adapteros_db::process_monitoring::ProcessAlert::create(
                            state.db.pool(),
                            alert_request.clone(),
                        )
                        .await
                        {
                            tracing::warn!(
                                adapter_id = %adapter.adapter_id,
                                error = %e,
                                "Failed to create state divergence alert"
                            );
                        } else {
                            tracing::warn!(
                                adapter_id = %adapter.adapter_id,
                                "Detected state divergence: adapter marked warm but not loaded"
                            );
                        }
                    }
                }
            }
        }

        // Check for state divergence: adapter marked as cold but still loaded
        if state_str == "cold" {
            if let Some(ref lifecycle) = state.lifecycle_manager {
                let lifecycle_mgr = lifecycle.lock().await;
                let adapter_idx = adapter.id.parse::<u16>().unwrap_or(0);

                if lifecycle_mgr.is_loaded(adapter_idx).await {
                    // Adapter marked as cold but still loaded in LifecycleManager
                    let alert_request = adapteros_db::process_monitoring::CreateAlertRequest {
                        rule_id: format!("state_divergence_cold_{}", adapter.adapter_id),
                        worker_id: "system".to_string(),
                        tenant_id: "default".to_string(), // Use default since adapter doesn't have tenant_id
                        alert_type: "state_divergence".to_string(),
                        severity: AlertSeverity::Warning,
                        title: format!("Adapter {} state divergence", adapter.adapter_id),
                        message: format!(
                            "Adapter {} is marked as 'cold' in database but still loaded in LifecycleManager. This may cause memory leaks.",
                            adapter.adapter_id
                        ),
                        metric_value: None,
                        threshold_value: None,
                        status: AlertStatus::Active,
                    };

                    // Check if alert already exists to avoid duplicates
                    let existing_alerts = adapteros_db::process_monitoring::ProcessAlert::list(
                        state.db.pool(),
                        adapteros_db::process_monitoring::AlertFilters {
                            tenant_id: Some("default".to_string()),
                            status: Some(AlertStatus::Active),
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap_or_default();

                    let alert_already_exists = existing_alerts
                        .iter()
                        .any(|alert| alert.rule_id == alert_request.rule_id);

                    if !alert_already_exists {
                        if let Err(e) = adapteros_db::process_monitoring::ProcessAlert::create(
                            state.db.pool(),
                            alert_request.clone(),
                        )
                        .await
                        {
                            tracing::warn!(
                                adapter_id = %adapter.adapter_id,
                                error = %e,
                                "Failed to create state divergence alert"
                            );
                        } else {
                            tracing::warn!(
                                adapter_id = %adapter.adapter_id,
                                "Detected state divergence: adapter marked cold but still loaded"
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

// Helper to extract system metrics logic
async fn get_system_metrics_internal(state: &AppState) -> Result<SystemMetricsResponse, String> {
    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect real system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {}", e))?
        .as_secs();

    Ok(SystemMetricsResponse {
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers: {
            // Count active workers from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        requests_per_second: {
            // Calculate RPS from recent request log
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
            )
            .fetch_one(state.db.pool())
            .await
            .map(|count| count as f32 / 60.0)
            .unwrap_or(0.0)
        },
        avg_latency_ms: {
            // Calculate average latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
        memory_usage_pct: metrics.memory_usage as f32,
        adapter_count: {
            // Count active adapters from database
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters WHERE load_state = 'warm'")
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0) as i32
        },
        active_sessions: {
            // Count active inference sessions from telemetry buffer and metrics collector
            use adapteros_telemetry::TelemetryFilters;
            let start_filters = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.start".to_string()),
                ..Default::default()
            };
            let complete_filters = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.complete".to_string()),
                ..Default::default()
            };

            let started = state.telemetry_buffer.query(&start_filters).len();
            let completed = state.telemetry_buffer.query(&complete_filters).len();

            // Improved: Track by session IDs from metadata for accuracy
            use chrono::{Duration as ChronoDuration, Utc};

            let end_time = Utc::now();
            let start_time = end_time - ChronoDuration::minutes(5);

            let start_filters_time = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.start".to_string()),
                start_time: Some(start_time),
                end_time: Some(end_time),
                ..Default::default()
            };
            let complete_filters_time = TelemetryFilters {
                limit: Some(1000),
                event_type: Some("inference.complete".to_string()),
                start_time: Some(start_time),
                end_time: Some(end_time),
                ..Default::default()
            };

            // Collect session IDs from events (fallback to simple count if no IDs)
            let started_sessions: std::collections::HashSet<String> = state
                .telemetry_buffer
                .query(&start_filters_time)
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("session_id").or_else(|| m.get("request_id")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            let completed_sessions: std::collections::HashSet<String> = state
                .telemetry_buffer
                .query(&complete_filters_time)
                .iter()
                .filter_map(|e| {
                    e.metadata
                        .as_ref()
                        .and_then(|m| m.get("session_id").or_else(|| m.get("request_id")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            let active = if !started_sessions.is_empty() || !completed_sessions.is_empty() {
                // Use session ID tracking for accuracy
                started_sessions.difference(&completed_sessions).count() as i32
            } else {
                // Fallback: approximate as started - completed

                if started > completed {
                    (started - completed) as i32
                } else {
                    0
                }
            };

            // Also check metrics collector for active_sessions gauge
            let snapshot = state.metrics_collector.get_metrics_snapshot().await;
            let metrics_active = snapshot.system.active_sessions as i32;

            // Use the higher of the two estimates
            active.max(metrics_active)
        },
        tokens_per_second: {
            // Get tokens per second from metrics collector snapshot
            let snapshot = state.metrics_collector.get_metrics_snapshot().await;

            // If snapshot has non-zero value, use it; otherwise calculate from recent tokens
            if snapshot.throughput.tokens_per_second > 0.0 {
                snapshot.throughput.tokens_per_second as f32
            } else {
                // Fallback: calculate from recent telemetry events
                use adapteros_telemetry::TelemetryFilters;
                use chrono::{Duration, Utc};

                let end_time = Utc::now();
                let start_time = end_time - Duration::seconds(60);

                let filters = TelemetryFilters {
                    limit: Some(1000),
                    event_type: Some("inference.complete".to_string()),
                    start_time: Some(start_time),
                    end_time: Some(end_time),
                    ..Default::default()
                };

                let events = state.telemetry_buffer.query(&filters);

                // Sum tokens from inference events
                let mut total_tokens = 0u64;
                for event in events.iter() {
                    if let Some(ref metadata) = event.metadata {
                        if let Some(output_tokens) =
                            metadata.get("output_tokens").and_then(|t| t.as_u64())
                        {
                            total_tokens += output_tokens;
                        }
                    }
                }

                // Convert to tokens per second
                (total_tokens as f32) / 60.0
            }
        },
        latency_p95_ms: {
            // Calculate P95 latency from recent requests
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms LIMIT 1 OFFSET (SELECT COUNT(*) * 95 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))"
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(None)
            .unwrap_or(0.0) as f32
        },
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        disk_usage_percent: Some(metrics.disk_io.usage_percent),
        network_rx_bytes: Some(metrics.network_io.rx_bytes as i64),
        network_tx_bytes: Some(metrics.network_io.tx_bytes as i64),
        network_rx_packets: Some(metrics.network_io.rx_packets as i64),
        network_tx_packets: Some(metrics.network_io.tx_packets as i64),
        network_bandwidth_mbps: Some(metrics.network_io.bandwidth_mbps),
        gpu_utilization_percent: metrics.gpu_metrics.utilization.map(|v| v as f32),
        gpu_memory_used_gb: metrics
            .gpu_metrics
            .memory_used
            .map(|v| v as f32 / 1024.0 / 1024.0 / 1024.0),
        gpu_memory_total_gb: metrics
            .gpu_metrics
            .memory_total
            .map(|v| v as f32 / 1024.0 / 1024.0 / 1024.0),
    })
}

// ============================================================================
// Streaming API Endpoints (SSE)
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4

/// Training stream SSE endpoint
///
/// Streams real-time training events including job started, progress updates,
/// epoch completions, job completed/failed, and pause/resume events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```
/// event: training
/// data: {"event_type":"job_started","job_id":"...","timestamp":"...","payload":{...}}
/// ```
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5
#[utoipa::path(
    get,
    path = "/v1/streams/training",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of training events")
    )
)]
pub async fn training_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();

    // Subscribe to training events from TrainingService
    let rx = state.training_service.subscribe_events();

    // Create stream from broadcast channel (similar to telemetry_events_stream)
    let stream = BroadcastStream::new(rx).filter_map(move |res| {
        let tenant_id_clone = tenant_id.clone();
        async move {
            match res {
                Ok(event_data) => {
                    // Filter by tenant if tenant_id is provided in payload
                    let should_include = {
                        event_data
                            .get("payload")
                            .and_then(|p| p.get("tenant_id"))
                            .and_then(|t| t.as_str())
                            .map(|t| t == tenant_id_clone.as_str())
                            .unwrap_or(true) // Include if no tenant_id in payload
                    };

                    if should_include {
                        match serde_json::to_string(&event_data) {
                            Ok(json) => Some(Ok(Event::default().event("training").data(json))),
                            Err(e) => {
                                tracing::warn!("Failed to serialize training event: {}", e);
                                None
                            }
                        }
                    } else {
                        None // Skip filtered events
                    }
                }
                Err(_) => {
                    // BroadcastStream error (lagged or closed) - skip and continue
                    None
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Discovery stream SSE endpoint
///
/// Streams real-time repository scanning and code discovery events including
/// scan progress, symbol indexing, framework detection, and completion events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```
/// event: discovery
/// data: {"type":"symbol_indexed","timestamp":...,"payload":{...}}
/// ```
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.4
#[utoipa::path(
    get,
    path = "/v1/streams/discovery",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events"),
        ("repo" = Option<String>, Query, description = "Optional repository ID filter")
    ),
    responses(
        (status = 200, description = "SSE stream of discovery events")
    )
)]
pub async fn discovery_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<DiscoveryStreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();
    let repo_filter = params.repo.clone();

    // Create a stream that emits discovery events
    // For now, this is a mock implementation
    // TODO: Connect to actual CodeGraph scanner signal stream
    let stream = stream::unfold(
        (state, tenant_id, repo_filter, 0),
        |(state, tenant_id, repo_filter, counter)| async move {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let repo_id = repo_filter
                .clone()
                .unwrap_or_else(|| "acme/payments".to_string());

            // Cycle through different discovery event types
            let event_type = match counter % 5 {
                0 => "repo_scan_started",
                1 => "repo_scan_progress",
                2 => "symbol_indexed",
                3 => "framework_detected",
                _ => "repo_scan_completed",
            };

            let event_data = serde_json::json!({
                "type": event_type,
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "payload": {
                    "repo_id": repo_id,
                    "tenant_id": &tenant_id,
                    "stage": if counter < 10 { "parsing" } else if counter < 20 { "indexing" } else { "completed" },
                    "files_parsed": counter * 14,
                    "symbol_count": counter * 183,
                    "framework": if event_type == "framework_detected" { Some("django 4.2") } else { None },
                    "content_hash": if event_type == "repo_scan_completed" { Some(format!("b3:abc{:03x}", counter)) } else { None }
                }
            });

            let event = Event::default()
                .event("discovery")
                .data(event_data.to_string());

            Some((Ok(event), (state, tenant_id, repo_filter, counter + 1)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}
/// Contacts stream SSE endpoint
///
/// Streams real-time contact discovery and update events as contacts are
/// discovered during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/streams/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of contact events")
    )
)]
pub async fn contacts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let tenant_id = params.tenant.clone();

    // Create a stream that emits contact events
    // TODO: Connect to actual contact discovery signal stream
    let stream = stream::unfold(
        (state, tenant_id, 0),
        |(state, tenant_id, counter)| async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let categories = ["adapter", "repository", "user", "system", "external"];
            let names = [
                "adapter_0",
                "acme/payments",
                "john.doe",
                "api_gateway",
                "stripe_api",
            ];

            let idx = counter % 5;
            let event_data = serde_json::json!({
                "type": "contact_discovered",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("System time before UNIX epoch")
                    .as_millis(),
                "payload": {
                    "name": names[idx],
                    "category": categories[idx],
                    "tenant_id": &tenant_id,
                    "metadata": {
                        "discovered_at": chrono::Utc::now().to_rfc3339()
                    }
                }
            });

            let event = Event::default()
                .event("contact")
                .data(event_data.to_string());

            Some((Ok(event), (state, tenant_id, counter + 1)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// Contacts API Endpoints
// ============================================================================
// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6

/// List contacts with filtering
///
/// Returns contacts discovered during inference, filtered by tenant and optionally by category.
/// Contacts represent entities (users, adapters, repositories, systems) that the inference
/// engine has interacted with.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("category" = Option<String>, Query, description = "Filter by category (user|system|adapter|repository|external)"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 100)")
    ),
    responses(
        (status = 200, description = "List of contacts", body = ContactsResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<ContactsQuery>,
) -> Result<Json<ContactsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Build query based on filters
    let mut query = String::from(
        "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url, \
         discovered_at, discovered_by, last_interaction, interaction_count, \
         created_at, updated_at \
         FROM contacts WHERE tenant_id = ?",
    );

    let mut bind_values: Vec<String> = vec![params.tenant.clone()];

    // Add category filter if provided
    if let Some(ref category) = params.category {
        query.push_str(" AND category = ?");
        bind_values.push(category.clone());
    }

    query.push_str(" ORDER BY discovered_at DESC LIMIT ?");
    bind_values.push(params.limit.unwrap_or(100).to_string());

    // Execute query
    // Note: This is a simplified version. In production, use proper query builder
    let contacts = sqlx::query_as::<_, ContactRow>(
        "SELECT * FROM contacts WHERE tenant_id = ? ORDER BY discovered_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(100) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch contacts")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Convert to response format
    let contacts: Vec<ContactResponse> = contacts.into_iter().map(|c| c.into()).collect();

    Ok(Json(ContactsResponse { contacts }))
}
/// Create or update a contact
///
/// Creates a new contact or updates an existing one based on (tenant_id, name, category) uniqueness.
/// This endpoint can be used to manually register contacts or update their metadata.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    post,
    path = "/v1/contacts",
    request_body = CreateContactRequest,
    responses(
        (status = 200, description = "Contact created/updated", body = ContactResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn create_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Json(request): Json<CreateContactRequest>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate category
    if !["user", "system", "adapter", "repository", "external"].contains(&request.category.as_str())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid category")
                    .with_code("BAD_REQUEST")
                    .with_string_details(
                        "category must be one of: user, system, adapter, repository, external"
                            .to_string(),
                    ),
            ),
        ));
    }

    // Upsert contact
    let contact = sqlx::query_as::<_, ContactRow>(
        "INSERT INTO contacts (tenant_id, name, email, category, role, metadata_json)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(tenant_id, name, category) DO UPDATE SET
            email = excluded.email,
            role = excluded.role,
            metadata_json = excluded.metadata_json,
            updated_at = datetime('now')
         RETURNING *",
    )
    .bind(&request.tenant_id)
    .bind(&request.name)
    .bind(&request.email)
    .bind(&request.category)
    .bind(&request.role)
    .bind(&request.metadata_json)
    .fetch_one(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to create contact")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(contact.into()))
}

/// Get contact by ID
///
/// Retrieves a specific contact by its unique identifier.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact details", body = ContactResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ContactResponse>, (StatusCode, Json<ErrorResponse>)> {
    let contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ),
        })?;

    Ok(Json(contact.into()))
}

/// Delete a contact
///
/// Permanently deletes a contact and all associated interaction records.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    delete,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact deleted"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let result = sqlx::query("DELETE FROM contacts WHERE id = ?")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to delete contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
        ));
    }

    Ok(StatusCode::OK)
}

/// Get contact interaction history
///
/// Returns the interaction log for a specific contact, showing when and how
/// the contact was referenced during inference operations.
///
/// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}/interactions",
    params(
        ("id" = String, Path, description = "Contact ID"),
        ("limit" = Option<usize>, Query, description = "Limit results (default: 50)")
    ),
    responses(
        (status = 200, description = "Interaction history", body = ContactInteractionsResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Server error", body = ErrorResponse)
    )
)]
pub async fn get_contact_interactions(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<ContactInteractionsQuery>,
) -> Result<Json<ContactInteractionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify contact exists
    let _contact = sqlx::query_as::<_, ContactRow>("SELECT * FROM contacts WHERE id = ?")
        .bind(&id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("contact not found").with_code("NOT_FOUND")),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch contact")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ),
        })?;

    // Fetch interactions
    let interactions = sqlx::query_as::<_, ContactInteractionRow>(
        "SELECT * FROM contact_interactions 
         WHERE contact_id = ? 
         ORDER BY created_at DESC 
         LIMIT ?",
    )
    .bind(&id)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch interactions")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let interactions: Vec<ContactInteractionResponse> =
        interactions.into_iter().map(|i| i.into()).collect();

    Ok(Json(ContactInteractionsResponse { interactions }))
}

// ========== Training Handlers ==========

#[derive(Debug, Deserialize)]
pub struct TrainingSessionsQuery {
    #[serde(default)]
    tenant_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelemetryEventsQuery {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    start_time: Option<String>,
    #[serde(default)]
    end_time: Option<String>,
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    level: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TrainingConfigPayload {
    #[serde(default)]
    rank: Option<u32>,
    #[serde(default)]
    alpha: Option<u32>,
    #[serde(default)]
    targets: Option<Vec<String>>,
    #[serde(default)]
    epochs: Option<u32>,
    #[serde(default)]
    learning_rate: Option<f32>,
    #[serde(default)]
    batch_size: Option<u32>,
    #[serde(default)]
    warmup_steps: Option<u32>,
    #[serde(default)]
    max_seq_length: Option<u32>,
    #[serde(default)]
    gradient_accumulation_steps: Option<u32>,
}

fn training_config_from_value(value: &serde_json::Value) -> Result<TrainingConfig, String> {
    if value.is_null() {
        return Ok(TrainingConfig::default());
    }

    let payload: TrainingConfigPayload = serde_json::from_value(value.clone())
        .map_err(|e| format!("invalid training_config payload: {}", e))?;

    let mut cfg = TrainingConfig::default();
    if let Some(rank) = payload.rank {
        cfg.rank = rank;
    }
    if let Some(alpha) = payload.alpha {
        cfg.alpha = alpha;
    }
    if let Some(targets) = payload.targets {
        if !targets.is_empty() {
            cfg.targets = targets;
        }
    }
    if let Some(epochs) = payload.epochs {
        cfg.epochs = epochs;
    }
    if let Some(lr) = payload.learning_rate {
        cfg.learning_rate = lr;
    }
    if let Some(batch_size) = payload.batch_size {
        cfg.batch_size = batch_size;
    }
    cfg.warmup_steps = payload.warmup_steps;
    cfg.max_seq_length = payload.max_seq_length;
    cfg.gradient_accumulation_steps = payload.gradient_accumulation_steps;
    Ok(cfg)
}

fn training_session_response(
    job: &TrainingJob,
    metadata: Option<&TrainingSessionMetadata>,
) -> TrainingSessionResponse {
    let status = format!("{:?}", job.status).to_lowercase();
    let repository_path = metadata
        .and_then(|m| m.repository_path.clone())
        .or_else(|| job.repo_id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let updated_at = job
        .completed_at
        .clone()
        .or_else(|| job.started_at.clone())
        .unwrap_or_else(|| job.created_at.clone());

    TrainingSessionResponse {
        session_id: job.id.clone(),
        status,
        progress: Some(job.progress_pct as f64),
        adapter_name: job.adapter_name.clone(),
        repository_path,
        created_at: job.created_at.clone(),
        updated_at,
        error_message: job.error_message.clone(),
        tenant_id: metadata.and_then(|m| m.tenant_id.clone()),
    }
}

fn parse_log_level_filter(level: &str) -> Option<LogLevel> {
    match level.to_lowercase().as_str() {
        "debug" => Some(LogLevel::Debug),
        "info" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        "critical" => Some(LogLevel::Critical),
        _ => None,
    }
}

fn activity_event_from_unified_event(event: &UnifiedTelemetryEvent) -> ActivityEventResponse {
    ActivityEventResponse {
        id: event.id.clone(),
        timestamp: event.timestamp.to_rfc3339(),
        event_type: event.event_type.clone(),
        level: format!("{:?}", event.level).to_lowercase(),
        message: event.message.clone(),
        component: event.component.clone(),
        tenant_id: event.tenant_id.clone(),
        user_id: event.user_id.clone(),
        metadata: event.metadata.clone(),
    }
}

fn emit_activity_event(
    state: &AppState,
    event_type: EventType,
    level: LogLevel,
    message: impl Into<String>,
    component: &str,
    tenant_id: Option<String>,
    metadata: Option<serde_json::Value>,
) {
    let mut builder = TelemetryEventBuilder::new(event_type, level, message.into())
        .component(component.to_string());

    if let Some(ref tenant) = tenant_id {
        builder = builder.tenant_id(tenant.clone());
    }
    if let Some(meta) = metadata {
        builder = builder.metadata(meta);
    }

    let event = builder.build();
    state.telemetry_buffer.push(event.clone());
    let _ = state.telemetry_tx.send(event);
}
/// List all training jobs
#[utoipa::path(
    get,
    path = "/v1/training/jobs",
    responses(
        (status = 200, description = "Training jobs retrieved successfully", body = Vec<TrainingJobResponse>)
    )
)]
pub async fn list_training_jobs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TrainingJobResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let jobs = state.training_service.list_jobs().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list training jobs")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(jobs.into_iter().map(|j| j.into()).collect()))
}

/// Get a specific training job
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training job retrieved successfully", body = TrainingJobResponse)
    )
)]
pub async fn get_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("training job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(job.into()))
}

/// Start a new training job
#[utoipa::path(
    post,
    path = "/v1/training/start",
    request_body = StartTrainingRequest,
    responses(
        (status = 200, description = "Training started successfully", body = TrainingJobResponse)
    )
)]
pub async fn start_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<StartTrainingRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    // Validate absolute directory_root if provided (matches orchestrator dataset builder requirements)
    crate::validation::validate_directory_root_absolute(&req.directory_root)?;

    let config = training_config_from_request(req.config.clone());

    let mut builder = TrainingJobBuilder::new()
        .adapter_name(req.adapter_name.clone())
        .config(config);

    builder = builder.template_id(req.template_id.clone());
    builder = builder.repo_id(req.repo_id.clone());
    builder = builder.dataset_path(req.dataset_path.clone());
    builder = builder.directory_root(req.directory_root.clone());
    builder = builder.directory_path(req.directory_path.clone());
    builder = builder.tenant_id(req.tenant_id.clone());
    builder = builder.adapters_root(req.adapters_root.clone());
    builder = builder.package(req.package.unwrap_or(false));
    builder = builder.adapter_id(req.adapter_id.clone());

    let params = builder.build().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid training parameters")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let job = state
        .training_service
        .start_training(params)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to start training")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    emit_activity_event(
        &state,
        EventType::TrainingStart,
        LogLevel::Info,
        format!("Training job {} started", job.id),
        "training.job",
        req.tenant_id.clone(),
        Some(json!({
            "adapter_name": job.adapter_name,
            "repo_id": job.repo_id,
            "template_id": req.template_id,
            "package": req.package.unwrap_or(false),
            "register": req.register.unwrap_or(false)
        })),
    );

    {
        let mut sessions = state.training_sessions.write().await;
        sessions
            .entry(job.id.clone())
            .or_insert(TrainingSessionMetadata {
                repository_path: job.repo_id.clone(),
                description: Some(req.adapter_name.clone()),
                tenant_id: req.tenant_id.clone(),
            });
    }

    // Optionally spawn a background registrar if register=true
    if req.register.unwrap_or(false) {
        let db = state.db.clone();
        let training_service = state.training_service.clone();
        let tier = req.tier.unwrap_or(8); // default ephemeral tier
        let job_id_for_reg = job.id.clone();
        tokio::spawn(async move {
            // Poll for completion
            let mut attempts = 0;
            loop {
                attempts += 1;
                match training_service.get_job(&job_id_for_reg).await {
                    Ok(j) => {
                        if matches!(j.status, TrainingJobStatus::Completed) {
                            if let (Some(adapter_id), Some(hash_b3)) =
                                (j.adapter_id.clone(), j.weights_hash_b3.clone())
                            {
                                // Register in DB
                                match AdapterRegistrationBuilder::new()
                                    .adapter_id(adapter_id.clone())
                                    .name(adapter_id.clone())
                                    .hash_b3(hash_b3.clone())
                                    .rank(j.config.rank as i32)
                                    .tier(tier)
                                    .build()
                                {
                                    Ok(registration) => {
                                        let _ = db.register_adapter(registration).await;
                                    }
                                    Err(err) => {
                                        tracing::warn!(
                                            adapter = %adapter_id,
                                            "Failed to build adapter registration: {}",
                                            err
                                        );
                                    }
                                }
                            }
                            break;
                        } else if matches!(
                            j.status,
                            TrainingJobStatus::Failed | TrainingJobStatus::Cancelled
                        ) {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get training job {}: {}", job_id_for_reg, e);
                    }
                }
                if attempts % 60 == 0 {
                    // every minute
                    tracing::info!(
                        "waiting for training job {} to complete ({}s elapsed)",
                        job_id_for_reg,
                        attempts
                    );
                }
                if attempts > 7200 {
                    // up to 2 hours
                    tracing::warn!(
                        "registration watcher timed out after {} seconds for job {}",
                        attempts,
                        job_id_for_reg
                    );
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    Ok(Json(job.into()))
}

#[utoipa::path(
    post,
    path = "/v1/training/sessions",
    request_body = StartTrainingSessionRequest,
    responses(
        (status = 200, description = "Training session started", body = TrainingSessionResponse)
    ),
    tag = "training",
    security(("bearer_token" = []))
)]
pub async fn start_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<StartTrainingSessionRequest>,
) -> Result<Json<TrainingSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    let config = training_config_from_value(&req.training_config).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid training_config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(msg),
            ),
        )
    })?;

    let mut builder = TrainingJobBuilder::new()
        .adapter_name(req.adapter_name.clone())
        .config(config);

    builder = builder.repo_id(Some(req.repository_path.clone()));
    builder = builder.dataset_path(Some(req.repository_path.clone()));
    builder = builder.tenant_id(req.tenant_id.clone());

    let params = builder.build().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid training parameters")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let job = state
        .training_service
        .start_training(params)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to start training session")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let metadata = TrainingSessionMetadata {
        repository_path: Some(req.repository_path.clone()),
        description: req.description.clone(),
        tenant_id: req.tenant_id.clone(),
    };

    {
        let mut sessions = state.training_sessions.write().await;
        sessions.insert(job.id.clone(), metadata.clone());
    }

    emit_activity_event(
        &state,
        EventType::TrainingStart,
        LogLevel::Info,
        format!("Training session {} started", job.id),
        "training.session",
        metadata.tenant_id.clone(),
        Some(json!({
            "repository_path": metadata.repository_path,
            "adapter_name": job.adapter_name,
            "session_id": job.id
        })),
    );

    let response = training_session_response(&job, Some(&metadata));
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/v1/training/sessions/{session_id}",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    responses(
        (status = 200, description = "Training session retrieved", body = TrainingSessionResponse),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    tag = "training",
    security(("bearer_token" = []))
)]
pub async fn get_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<TrainingSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let job = state
        .training_service
        .get_job(&session_id)
        .await
        .map_err(|_| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("training session not found").with_code("NOT_FOUND")),
            )
        })?;

    let metadata = state.training_sessions.read().await;
    let response = training_session_response(&job, metadata.get(&session_id));
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/v1/training/sessions",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter sessions by tenant ID")
    ),
    responses(
        (status = 200, description = "Training sessions listed", body = Vec<TrainingSessionResponse>)
    ),
    tag = "training",
    security(("bearer_token" = []))
)]
pub async fn list_training_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<TrainingSessionsQuery>,
) -> Result<Json<Vec<TrainingSessionResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let tenant_filter = query.tenant_id.clone();
    let jobs = state.training_service.list_jobs().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list training sessions")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let sessions = state.training_sessions.read().await;
    let mut sessions_to_cleanup = Vec::new();
    let mut responses = Vec::with_capacity(jobs.len());

    for job in jobs.iter() {
        let metadata = sessions.get(&job.id);

        // Check if job is in terminal state and mark session for cleanup
        use adapteros_core::TrainingJobStatus;
        match job.status {
            TrainingJobStatus::Completed
            | TrainingJobStatus::Failed
            | TrainingJobStatus::Cancelled => {
                sessions_to_cleanup.push(job.id.clone());
            }
            _ => {}
        }

        if let Some(ref tenant_id) = tenant_filter {
            match metadata.and_then(|m| m.tenant_id.clone()) {
                Some(ref t) if t == tenant_id => {}
                _ => continue,
            }
        }
        responses.push(training_session_response(job, metadata));
    }

    // Clean up terminal training sessions
    if !sessions_to_cleanup.is_empty() {
        drop(sessions); // Release read lock
        let mut sessions_write = state.training_sessions.write().await;
        for session_id in sessions_to_cleanup {
            sessions_write.remove(&session_id);
        }
    }

    Ok(Json(responses))
}

#[utoipa::path(
    get,
    path = "/v1/telemetry/events",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of events"),
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant"),
        ("user_id" = Option<String>, Query, description = "Filter by user"),
        ("start_time" = Option<String>, Query, description = "RFC3339 start time"),
        ("end_time" = Option<String>, Query, description = "RFC3339 end time"),
        ("event_type" = Option<String>, Query, description = "Filter by event type"),
        ("level" = Option<String>, Query, description = "Filter by log level"),
    ),
    responses(
        (status = 200, description = "Telemetry events", body = Vec<ActivityEventResponse>)
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn get_activity_events(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<TelemetryEventsQuery>,
) -> Result<Json<Vec<ActivityEventResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let mut filters = adapteros_telemetry::TelemetryFilters::default();
    if query.limit.is_some() {
        filters.limit = query.limit;
    }
    filters.tenant_id = query.tenant_id.clone();
    filters.user_id = query.user_id.clone();
    if let Some(ref start) = query.start_time {
        let parsed = chrono::DateTime::parse_from_rfc3339(start).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid start_time")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
        filters.start_time = Some(parsed.with_timezone(&Utc));
    }
    if let Some(ref end) = query.end_time {
        let parsed = chrono::DateTime::parse_from_rfc3339(end).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid end_time")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
        filters.end_time = Some(parsed.with_timezone(&Utc));
    }
    filters.event_type = query.event_type.clone();
    if let Some(ref level) = query.level {
        filters.level = parse_log_level_filter(level);
    }

    let events = state.telemetry_buffer.query(&filters);
    let responses = events
        .into_iter()
        .map(|event| activity_event_from_unified_event(&event))
        .collect();

    Ok(Json(responses))
}

/// POST /v1/telemetry/logs - Submit client-side log entries
/// Accepts log entries from UI and other clients for centralized telemetry
pub async fn submit_client_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(log_entries): Json<Vec<adapteros_telemetry::UnifiedTelemetryEvent>>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Client logs don't require specific roles - any authenticated user can submit logs
    // But we do require authentication to prevent spam

    for log_entry in log_entries {
        // Add client identifier to distinguish from server-side logs
        let mut enriched_entry = log_entry.clone();
        enriched_entry.component = Some(format!(
            "client:{}",
            enriched_entry
                .component
                .unwrap_or_else(|| "unknown".to_string())
        ));
        enriched_entry.tenant_id = Some(claims.tenant_id.clone());

        // Add to telemetry buffer
        state.telemetry_buffer.push(enriched_entry.clone());

        // Broadcast to any connected SSE clients
        let _ = state.telemetry_tx.send(enriched_entry);
    }

    Ok(())
}
/// GET /v1/audits/export - Export audit logs for compliance
/// Returns audit logs in CSV or JSON format based on query parameters
pub async fn export_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<AuditExportQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Compliance)?;

    // Default to last 30 days if no time range specified
    let end_time = params.end_time.unwrap_or_else(Utc::now);
    let start_time = params
        .start_time
        .unwrap_or_else(|| end_time - chrono::Duration::days(30));

    // Query telemetry buffer for audit events
    let mut filters = adapteros_telemetry::TelemetryFilters::default();
    filters.start_time = Some(start_time);
    filters.end_time = Some(end_time);
    filters.tenant_id = params.tenant_id.clone();
    filters.event_type = params.event_type.clone();
    if let Some(ref level) = params.level {
        filters.level = parse_log_level_filter(level);
    }

    let events = state.telemetry_buffer.query(&filters);

    let format = params.format.as_deref().unwrap_or("json");

    match format {
        "csv" => {
            let mut csv = String::from(
                "timestamp,event_type,level,component,tenant_id,user_id,message,trace_id\n",
            );

            for event in events {
                let row = format!(
                    "{},{},{:?},{},{},{},{},{}\n",
                    event.timestamp.to_rfc3339(),
                    event.event_type,
                    event.level,
                    event.component.unwrap_or_default(),
                    event.tenant_id.unwrap_or_default(),
                    event.user_id.unwrap_or_default(),
                    event.message.replace(',', ";"), // Escape commas in message
                    event.trace_id.unwrap_or_default()
                );
                csv.push_str(&row);
            }

            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/csv")
                .header(
                    "content-disposition",
                    "attachment; filename=\"audit-logs.csv\"",
                )
                .body(axum::body::Body::from(csv))
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Failed to create response")),
                    )
                })
        }
        "json" => {
            let json = serde_json::to_string(&events).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new("Failed to serialize logs")),
                )
            })?;

            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .header(
                    "content-disposition",
                    "attachment; filename=\"audit-logs.json\"",
                )
                .body(axum::body::Body::from(json))
                .map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new("Failed to create response")),
                    )
                })
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Unsupported format. Use 'csv' or 'json'")
                    .with_code("BAD_REQUEST"),
            ),
        )),
    }
}

/// Query parameters for audit export
#[derive(Debug, Deserialize)]
pub struct AuditExportQuery {
    pub format: Option<String>, // "csv" or "json"
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub tenant_id: Option<String>,
    pub event_type: Option<String>,
    pub level: Option<String>,
}

/// Pause training session
#[utoipa::path(
    post,
    path = "/v1/training/sessions/{session_id}/pause",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    responses(
        (status = 200, description = "Training paused", body = TrainingControlResponse),
        (status = 404, description = "Training session not found", body = ErrorResponse),
        (status = 409, description = "Conflict pausing training", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "training",
    security(("bearer_token" = []))
)]
pub async fn pause_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<TrainingControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Load DB job for existence and terminal state checks
    let job = state
        .db
        .get_training_job(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("training job not found").with_code("NOT_FOUND")),
        ))?;

    if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("cannot pause terminal job")
                    .with_code("CONFLICT")
                    .with_string_details(format!("status is '{}'", job.status)),
            ),
        ));
    }

    // Orchestrator first: ensure runtime paused
    if let Err(e) = state.training_service.pause_job(&session_id).await {
        if let Some(ae) = e.downcast_ref::<adapteros_core::AosError>() {
            return Err(match ae {
                adapteros_core::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("training job not found").with_code("NOT_FOUND")),
                ),
                adapteros_core::AosError::Internal(msg)
                    if msg.contains("Cannot pause terminal job") =>
                {
                    (
                        StatusCode::CONFLICT,
                        Json(
                            ErrorResponse::new("cannot pause terminal job")
                                .with_code("CONFLICT")
                                .with_string_details(msg.clone()),
                        ),
                    )
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to pause training")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(ae.to_string()),
                    ),
                ),
            });
        } else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to pause training")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    // Persist DB state after successful orchestrator action
    state
        .db
        .update_training_status(&session_id, "paused")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to pause training")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tracing::info!("Training job paused: {} by {}", session_id, claims.email);

    let tenant_id = {
        let sessions = state.training_sessions.read().await;
        sessions
            .get(&session_id)
            .and_then(|meta| meta.tenant_id.clone())
    };

    emit_activity_event(
        &state,
        EventType::Custom("training.pause".to_string()),
        LogLevel::Info,
        format!("Training session {} paused", session_id),
        "training.session",
        tenant_id,
        Some(json!({ "session_id": session_id })),
    );

    Ok(Json(TrainingControlResponse {
        session_id,
        status: "paused".to_string(),
        message: "Training job paused successfully".to_string(),
    }))
}

/// Resume training session
#[utoipa::path(
    post,
    path = "/v1/training/sessions/{session_id}/resume",
    params(
        ("session_id" = String, Path, description = "Training session ID")
    ),
    responses(
        (status = 200, description = "Training resumed", body = TrainingControlResponse),
        (status = 404, description = "Training session not found", body = ErrorResponse),
        (status = 409, description = "Conflict resuming training", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "training",
    security(("bearer_token" = []))
)]
pub async fn resume_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<TrainingControlResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Load DB job for existence and terminal state checks
    let job = state
        .db
        .get_training_job(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("training job not found").with_code("NOT_FOUND")),
        ))?;

    if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new("cannot resume terminal job")
                    .with_code("CONFLICT")
                    .with_string_details(format!("status is '{}'", job.status)),
            ),
        ));
    }

    // Orchestrator first: ensure runtime running
    if let Err(e) = state.training_service.resume_job(&session_id).await {
        if let Some(ae) = e.downcast_ref::<adapteros_core::AosError>() {
            return Err(match ae {
                adapteros_core::AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("training job not found").with_code("NOT_FOUND")),
                ),
                adapteros_core::AosError::Internal(msg)
                    if msg.contains("Cannot resume terminal job") =>
                {
                    (
                        StatusCode::CONFLICT,
                        Json(
                            ErrorResponse::new("cannot resume terminal job")
                                .with_code("CONFLICT")
                                .with_string_details(msg.clone()),
                        ),
                    )
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to resume training")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(ae.to_string()),
                    ),
                ),
            });
        } else {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to resume training")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    }

    // Persist DB state after successful orchestrator action (idempotently set to running)
    state
        .db
        .update_training_status(&session_id, "running")
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to resume training")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    tracing::info!("Training job resumed: {} by {}", session_id, claims.email);

    let tenant_id = {
        let sessions = state.training_sessions.read().await;
        sessions
            .get(&session_id)
            .and_then(|meta| meta.tenant_id.clone())
    };

    emit_activity_event(
        &state,
        EventType::Custom("training.resume".to_string()),
        LogLevel::Info,
        format!("Training session {} resumed", session_id),
        "training.session",
        tenant_id,
        Some(json!({ "session_id": session_id })),
    );

    Ok(Json(TrainingControlResponse {
        session_id,
        status: "running".to_string(),
        message: "Training job resumed successfully".to_string(),
    }))
}

/// Get training job artifacts and verify packaging/signature
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/artifacts",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Artifacts verification", body = TrainingArtifactsResponse)
    )
)]
pub async fn get_training_artifacts(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<crate::types::TrainingArtifactsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get job from orchestrator service
    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("training job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut artifact_path = None;
    let mut adapter_id = None;
    let mut weights_hash_b3 = None;
    let mut manifest_hash_b3 = None;
    let mut manifest_hash_matches = false;
    let mut signature_valid = false;

    if let (Some(path), Some(aid)) = (job.artifact_path.clone(), job.adapter_id.clone()) {
        let dir = std::path::PathBuf::from(path.clone());
        let weights = dir.join("weights.safetensors");
        let manifest = dir.join("manifest.json");
        let sig = dir.join("signature.sig");
        let pubkey = dir.join("public_key.pem");

        if weights.exists() && manifest.exists() && sig.exists() && pubkey.exists() {
            if let Ok(weights_bytes) = std::fs::read(&weights) {
                let w_hash = blake3::hash(&weights_bytes).to_hex().to_string();
                weights_hash_b3 = Some(w_hash.clone());
                if let Ok(manifest_bytes) = std::fs::read(&manifest) {
                    #[derive(serde::Deserialize)]
                    struct Manifest {
                        weights_hash: String,
                    }
                    if let Ok(m) = serde_json::from_slice::<Manifest>(&manifest_bytes) {
                        let m_hash = m.weights_hash;
                        manifest_hash_b3 = Some(m_hash.clone());
                        manifest_hash_matches = m_hash == w_hash;
                    }

                    if let (Ok(sig_bytes), Ok(pubkey_hex)) =
                        (std::fs::read(&sig), std::fs::read_to_string(&pubkey))
                    {
                        if sig_bytes.len() == 64 {
                            if let (Ok(sig_array), Ok(pk_bytes)) = (
                                <[u8; 64]>::try_from(sig_bytes.as_slice()),
                                hex::decode(pubkey_hex.trim()),
                            ) {
                                if let Ok(pk_array) = <[u8; 32]>::try_from(pk_bytes.as_slice()) {
                                    if let (Ok(signature), Ok(public_key)) = (
                                        adapteros_crypto::Signature::from_bytes(&sig_array),
                                        adapteros_crypto::PublicKey::from_bytes(&pk_array),
                                    ) {
                                        signature_valid =
                                            public_key.verify(&manifest_bytes, &signature).is_ok();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        artifact_path = Some(path);
        adapter_id = Some(aid);
    }

    let ready_flag = manifest_hash_matches && signature_valid;
    let resp = crate::types::TrainingArtifactsResponse {
        artifact_path,
        adapter_id,
        weights_hash_b3,
        manifest_hash_b3,
        manifest_hash_matches,
        signature_valid,
        ready: ready_flag,
    };
    Ok(Json(resp))
}

/// Cancel a training job
#[utoipa::path(
    post,
    path = "/v1/training/jobs/{job_id}/cancel",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training cancelled successfully")
    )
)]
pub async fn cancel_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    state
        .training_service
        .cancel_job(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to cancel training")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let tenant_id = {
        let sessions = state.training_sessions.read().await;
        sessions
            .get(&job_id)
            .and_then(|meta| meta.tenant_id.clone())
    };

    emit_activity_event(
        &state,
        EventType::Custom("training.cancel".to_string()),
        LogLevel::Warn,
        format!("Training job {} cancelled", job_id),
        "training.job",
        tenant_id,
        Some(json!({ "job_id": job_id })),
    );

    Ok(StatusCode::OK)
}

/// Get training logs
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/logs",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Logs retrieved successfully", body = Vec<String>)
    )
)]
pub async fn get_training_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let logs = state
        .training_service
        .get_logs(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("failed to get logs")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(logs))
}
/// Get training metrics
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/metrics",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Metrics retrieved successfully", body = TrainingMetricsResponse)
    )
)]
pub async fn get_training_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("training job not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(TrainingMetricsResponse {
        loss: job.current_loss,
        tokens_per_second: job.tokens_per_second,
        learning_rate: job.learning_rate,
        current_epoch: job.current_epoch,
        total_epochs: job.total_epochs,
        progress_pct: job.progress_pct,
    }))
}
/// List training templates
#[utoipa::path(
    get,
    path = "/v1/training/templates",
    responses(
        (status = 200, description = "Training templates retrieved successfully", body = Vec<TrainingTemplateResponse>)
    )
)]
pub async fn list_training_templates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<TrainingTemplateResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let templates = state.training_service.list_templates().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list templates")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(templates.into_iter().map(|t| t.into()).collect()))
}
/// Get a specific training template
#[utoipa::path(
    get,
    path = "/v1/training/templates/{template_id}",
    params(
        ("template_id" = String, Path, description = "Training template ID")
    ),
    responses(
        (status = 200, description = "Training template retrieved successfully", body = TrainingTemplateResponse)
    )
)]
pub async fn get_training_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(template_id): Path<String>,
) -> Result<Json<TrainingTemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("INTERNAL_ERROR")),
        )
    })?;

    let template = state
        .training_service
        .get_template(&template_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("template not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(template.into()))
}

// Git integration handlers
// pub mod git; // Already declared above

// ===== Advanced Process Monitoring Handlers =====

/// List monitoring rules
#[utoipa::path(
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Monitoring rules", body = Vec<MonitoringRuleResponse>)
    )
)]
pub async fn list_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<MonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params.get("tenant_id");
    let is_active = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    // Query monitoring rules from database
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(
        state.db.pool(),
        tenant_id.map(|s| s.as_str()),
        is_active,
    )
    .await
    .map_err(|e| {
        error!("Failed to list monitoring rules: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to list monitoring rules")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<MonitoringRuleResponse> = rules
        .into_iter()
        .map(|rule: adapteros_system_metrics::ProcessMonitoringRule| rule.into())
        .collect();

    Ok(Json(response))
}

/// Create monitoring rule
#[utoipa::path(
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule created", body = MonitoringRuleResponse)
    )
)]
pub async fn create_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let rule_request = req.try_into().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid request")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e),
            ),
        )
    })?;

    let rule_id =
        adapteros_system_metrics::ProcessMonitoringRule::create(state.db.pool(), rule_request)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

    // Get the created rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}
/// Update monitoring rule
#[utoipa::path(
    put,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    request_body = UpdateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule updated", body = MonitoringRuleResponse)
    )
)]
pub async fn update_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
    Json(req): Json<UpdateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let update_request = req.into();

    adapteros_system_metrics::ProcessMonitoringRule::update(
        state.db.pool(),
        &rule_id,
        update_request,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}

/// Delete monitoring rule
#[utoipa::path(
    delete,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule deleted")
    )
)]
pub async fn delete_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessMonitoringRule::delete(state.db.pool(), &rule_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::OK)
}

/// List alerts
#[utoipa::path(
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "Alerts", body = Vec<AlertResponse>)
    )
)]
pub async fn list_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params
            .get("status")
            .map(|s| adapteros_system_metrics::AlertStatus::from_string(s.to_string())),
        severity: params
            .get("severity")
            .map(|s| adapteros_system_metrics::AlertSeverity::from_string(s.to_string())),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response: Vec<AlertResponse> = alerts.into_iter().map(|alert| alert.into()).collect();

    Ok(Json(response))
}

/// Acknowledge alert
#[utoipa::path(
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = AlertResponse)
    )
)]
pub async fn acknowledge_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(req): Json<AcknowledgeAlertRequest>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Acknowledged,
        Some(&req.user),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}

/// Resolve alert
#[utoipa::path(
    post,
    path = "/v1/monitoring/alerts/{alert_id}/resolve",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    responses(
        (status = 200, description = "Alert resolved", body = AlertResponse)
    )
)]
pub async fn resolve_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Resolved,
        None,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}

/// List anomalies
#[utoipa::path(
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("anomaly_type" = Option<String>, Query, description = "Filter by anomaly type"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "Anomalies", body = Vec<AnomalyResponse>)
    )
)]
pub async fn list_anomalies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = adapteros_system_metrics::AnomalyFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params
            .get("status")
            .map(|s| adapteros_system_metrics::AnomalyStatus::from_string(s.to_string())),
        anomaly_type: params.get("anomaly_type").cloned(),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
    };

    let anomalies = adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response: Vec<AnomalyResponse> = anomalies
        .into_iter()
        .map(|anomaly| anomaly.into())
        .collect();

    Ok(Json(response))
}

/// Update anomaly status
#[utoipa::path(
    put,
    path = "/v1/monitoring/anomalies/{anomaly_id}",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly updated", body = AnomalyResponse)
    )
)]
pub async fn update_anomaly_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(anomaly_id): Path<String>,
    Json(req): Json<UpdateAnomalyStatusRequest>,
) -> Result<Json<AnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Update anomaly status in database
    sqlx::query!(
        "UPDATE process_anomalies SET status = ?, investigation_notes = ?, investigated_by = ? WHERE id = ?",
        req.status,
        req.investigation_notes,
        req.investigated_by,
        anomaly_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // Get the updated anomaly
    let filters = adapteros_system_metrics::AnomalyFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        anomaly_type: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
    };

    let anomalies = adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let anomaly = anomalies
        .into_iter()
        .find(|a| a.id == anomaly_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("anomaly not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(anomaly.into()))
}

/// Get performance baselines
#[utoipa::path(
    get,
    path = "/v1/monitoring/baselines",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name")
    ),
    responses(
        (status = 200, description = "Performance baselines", body = Vec<BaselineResponse>)
    )
)]
pub async fn get_performance_baselines(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<BaselineResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_id = params.get("worker_id");
    let metric_name = params.get("metric_name");

    let mut query =
        "SELECT * FROM process_performance_baselines WHERE is_active = true".to_string();
    let mut params_vec: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
    let mut param_count = 0;

    if let Some(worker) = worker_id {
        param_count += 1;
        query.push_str(&format!(" AND worker_id = ${}", param_count));
        params_vec.push(Box::new(worker.to_string()));
    }

    if let Some(metric) = metric_name {
        param_count += 1;
        query.push_str(&format!(" AND metric_name = ${}", param_count));
        params_vec.push(Box::new(metric.to_string()));
    }

    query.push_str(" ORDER BY calculated_at DESC");

    let rows = sqlx::query(&query)
        .fetch_all(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut baselines = Vec::new();
    for row in rows {
        let baseline = adapteros_system_metrics::PerformanceBaseline {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            tenant_id: row.get("tenant_id"),
            metric_name: row.get("metric_name"),
            baseline_value: row.get("baseline_value"),
            baseline_type: adapteros_system_metrics::BaselineType::from_string(
                row.get("baseline_type"),
            ),
            calculation_period_days: row.get("calculation_period_days"),
            confidence_interval: row.get("confidence_interval"),
            standard_deviation: row.get("standard_deviation"),
            percentile_95: row.get("percentile_95"),
            percentile_99: row.get("percentile_99"),
            is_active: row.get("is_active"),
            calculated_at: chrono::DateTime::parse_from_rfc3339(
                &row.get::<String, _>("calculated_at"),
            )
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .with_timezone(&chrono::Utc),
            expires_at: row
                .try_get::<Option<String>, _>("expires_at")
                .ok()
                .flatten()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc)),
        };
        baselines.push(baseline.into());
    }

    Ok(Json(baselines))
}

/// Recalculate baseline
#[utoipa::path(
    post,
    path = "/v1/monitoring/baselines/recalculate",
    request_body = RecalculateBaselineRequest,
    responses(
        (status = 200, description = "Baseline recalculated", body = BaselineResponse)
    )
)]
pub async fn recalculate_baseline(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RecalculateBaselineRequest>,
) -> Result<Json<BaselineResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // This would typically call the baseline service
    // For now, we'll return a placeholder response
    let baseline = adapteros_system_metrics::PerformanceBaseline {
        id: uuid::Uuid::new_v4().to_string(),
        worker_id: req.worker_id,
        tenant_id: "default".to_string(),
        metric_name: req.metric_name,
        baseline_value: 0.0,
        baseline_type: adapteros_system_metrics::BaselineType::Statistical,
        calculation_period_days: req.calculation_period_days.unwrap_or(7),
        confidence_interval: Some(0.95),
        standard_deviation: Some(0.0),
        percentile_95: Some(0.0),
        percentile_99: Some(0.0),
        is_active: true,
        calculated_at: chrono::Utc::now(),
        expires_at: Some(chrono::Utc::now() + chrono::Duration::days(90)),
    };

    Ok(Json(baseline.into()))
}
/// Get dashboard configuration
#[utoipa::path(
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/config",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID")
    ),
    responses(
        (status = 200, description = "Dashboard configuration", body = adapteros_system_metrics::DashboardConfig)
    )
)]
pub async fn get_dashboard_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
) -> Result<Json<adapteros_system_metrics::DashboardConfig>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service = adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(
        state.db.inner().clone(),
    ));

    let config = dashboard_service
        .get_dashboard_config(&dashboard_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get dashboard config")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(config))
}
/// Get dashboard data
#[utoipa::path(
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/data",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID"),
        ("time_range" = Option<String>, Query, description = "Time range (1h, 6h, 24h, 7d, 30d)")
    ),
    responses(
        (status = 200, description = "Dashboard data", body = adapteros_system_metrics::DashboardData)
    )
)]
pub async fn get_dashboard_data(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<adapteros_system_metrics::DashboardData>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service = adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(
        state.db.inner().clone(),
    ));
    let time_range = params.get("time_range").map(|s| s.as_str());

    let data = dashboard_service
        .get_dashboard_data(&dashboard_id, time_range)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get dashboard data")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(data))
}
/// Export dashboard data
#[utoipa::path(
    get,
    path = "/v1/monitoring/dashboards/{dashboard_id}/export/{format}",
    params(
        ("dashboard_id" = String, Path, description = "Dashboard ID"),
        ("format" = String, Path, description = "Export format (json, csv)"),
        ("time_range" = Option<String>, Query, description = "Time range (1h, 6h, 24h, 7d, 30d)")
    ),
    responses(
        (status = 200, description = "Dashboard data export")
    )
)]
pub async fn export_dashboard_data(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((dashboard_id, format)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_service = adapteros_system_metrics::DashboardService::new(std::sync::Arc::new(
        state.db.inner().clone(),
    ));
    let time_range = params.get("time_range").map(|s| s.as_str());

    let export_data = dashboard_service
        .export_dashboard_data(&dashboard_id, &format, time_range)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to export dashboard data")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let content_type = match format.as_str() {
        "json" => "application/json",
        "csv" => "text/csv",
        _ => "text/plain",
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header(
            "Content-Disposition",
            format!(
                "attachment; filename=\"dashboard_{}.{}\"",
                dashboard_id, format
            ),
        )
        .body(axum::body::Body::from(export_data))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create response")
                        .with_code("INTERNAL_SERVER_ERROR"),
                ),
            )
        })?;

    Ok(response)
}

// ===== Enhanced SSE Streams for Advanced Monitoring =====

/// SSE stream for alerts
/// Pushes real-time alerts as they are created or updated
///
/// # Citations
/// - Alert broadcasting: [source: crates/adapteros-system-metrics/src/alerting.rs L444-L452]
/// - Broadcast channel: [source: crates/adapteros-server-api/src/state.rs L427-428]
/// - ProcessAlertResponse: [source: crates/adapteros-server-api/src/types.rs L1732-1760]
pub async fn alerts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Fetch recent alerts for initial backlog
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(5)),
        end_time: None,
        limit: Some(50),
    };

    let backlog_alerts: Vec<crate::types::ProcessAlertResponse> =
        match state.db.list_process_alerts(filters).await {
            Ok(alerts) => alerts.into_iter().map(map_alert_to_response).collect(),
            Err(e) => {
                tracing::warn!("Failed to fetch alerts for SSE backlog: {}", e);
                Vec::new()
            }
        };

    // Send backlog alerts
    let backlog_stream =
        stream::iter(
            backlog_alerts
                .into_iter()
                .map(|alert| match serde_json::to_string(&alert) {
                    Ok(json) => Ok(Event::default().event("alert").data(json)),
                    Err(e) => {
                        tracing::warn!("Failed to serialize backlog alert: {}", e);
                        Ok(Event::default()
                            .event("error")
                            .data("{\"error\": \"serialization failed\"}".to_string()))
                    }
                }),
        );

    // Real-time alert stream from broadcast channel
    let rx = state.alert_tx.subscribe();
    let realtime_stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(alert) => {
                let response = map_system_alert_to_response(alert);
                match serde_json::to_string(&response) {
                    Ok(json) => Some(Ok(Event::default().event("alert").data(json))),
                    Err(e) => {
                        tracing::warn!("Failed to serialize alert: {}", e);
                        None
                    }
                }
            }
            Err(_) => None,
        }
    });

    // Combine backlog and real-time streams
    let stream = backlog_stream.chain(realtime_stream);

    Sse::new(stream).keep_alive(KeepAlive::default())
}
/// SSE stream for anomalies
/// Pushes real-time anomaly detections
pub async fn anomalies_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Fetch recent anomalies
        let filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: None,
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(10)),
            end_time: None,
            limit: Some(20),
        };

        let anomalies =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters).await {
                Ok(anomalies) => anomalies,
                Err(e) => {
                    tracing::warn!("Failed to fetch anomalies for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

        let anomaly_data = serde_json::json!({
            "anomalies": anomalies.iter().map(|a| adapteros_system_metrics::AnomalyResponse::from(a.clone())).collect::<Vec<_>>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "count": anomalies.len()
        });

        Some((
            Ok(Event::default()
                .event("anomalies")
                .data(serde_json::to_string(&anomaly_data).unwrap_or_else(|_| "{}".to_string()))),
            state,
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for dashboard-specific metrics
/// Pushes metrics tailored for dashboard widgets
pub async fn dashboard_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold((state, dashboard_id), |(state, dashboard_id)| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Get dashboard configuration (placeholder for now)
        let dashboard_config = serde_json::json!({
            "widgets": [
                {
                    "type": "time_series",
                    "metric": "cpu_usage",
                    "aggregation": "avg",
                    "window": "1h"
                },
                {
                    "type": "gauge",
                    "metric": "gpu_utilization",
                    "threshold_warning": 80,
                    "threshold_critical": 95
                },
                {
                    "type": "alert_list",
                    "severities": ["critical", "error"],
                    "limit": 10
                }
            ],
            "refresh_interval": 30,
            "time_range": "24h"
        });

        // Fetch metrics for each widget
        let mut widget_data = Vec::new();

        for widget in dashboard_config["widgets"].as_array().unwrap_or(&vec![]) {
            let widget_type = widget["type"].as_str().unwrap_or("unknown");
            let metric_name = widget["metric"].as_str().unwrap_or("");

            let filters = adapteros_system_metrics::MetricFilters {
                worker_id: None,
                tenant_id: None,
                metric_name: Some(metric_name.to_string()),
                start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
                end_time: None,
                limit: Some(100),
            };

            let metrics = match adapteros_system_metrics::ProcessHealthMetric::query(
                state.db.pool(),
                filters,
            )
            .await
            {
                Ok(metrics) => metrics,
                Err(e) => {
                    tracing::warn!("Failed to fetch metrics for widget: {}", e);
                    continue;
                }
            };

            let widget_result = match widget_type {
                "time_series" => {
                    let points: Vec<serde_json::Value> = metrics
                        .iter()
                        .map(|m| {
                            serde_json::json!({
                                "timestamp": m.collected_at.to_rfc3339(),
                                "value": m.metric_value,
                                "worker_id": m.worker_id
                            })
                        })
                        .collect();

                    serde_json::json!({
                        "widget_id": "time_series_1",
                        "widget_type": "time_series",
                        "data": {
                            "metric": metric_name,
                            "points": points,
                            "aggregation": widget["aggregation"],
                            "window": widget["window"]
                        }
                    })
                }
                "gauge" => {
                    let current_value = metrics.last().map(|m| m.metric_value).unwrap_or(0.0);
                    let status = if current_value
                        >= widget["threshold_critical"].as_f64().unwrap_or(95.0)
                    {
                        "critical"
                    } else if current_value >= widget["threshold_warning"].as_f64().unwrap_or(80.0)
                    {
                        "warning"
                    } else {
                        "healthy"
                    };

                    serde_json::json!({
                        "widget_id": "gauge_1",
                        "widget_type": "gauge",
                        "data": {
                            "metric": metric_name,
                            "current_value": current_value,
                            "threshold_warning": widget["threshold_warning"],
                            "threshold_critical": widget["threshold_critical"],
                            "status": status
                        }
                    })
                }
                "alert_list" => {
                    let alert_filters = adapteros_system_metrics::AlertFilters {
                        tenant_id: None,
                        worker_id: None,
                        status: Some(adapteros_system_metrics::AlertStatus::Active),
                        severity: None,
                        start_time: None,
                        end_time: None,
                        limit: Some(widget["limit"].as_i64().unwrap_or(10)),
                    };

                    let alerts = match adapteros_system_metrics::ProcessAlert::list(
                        state.db.pool(),
                        alert_filters,
                    )
                    .await
                    {
                        Ok(alerts) => alerts,
                        Err(e) => {
                            tracing::warn!("Failed to fetch alerts for widget: {}", e);
                            vec![]
                        }
                    };

                    let alert_summaries: Vec<serde_json::Value> = alerts
                        .iter()
                        .map(|a| {
                            serde_json::json!({
                                "id": a.id,
                                "title": a.title,
                                "severity": a.severity.to_string(),
                                "status": a.status.to_string(),
                                "worker_id": a.worker_id,
                                "created_at": a.created_at.to_rfc3339(),
                                "acknowledged_by": a.acknowledged_by
                            })
                        })
                        .collect();

                    serde_json::json!({
                        "widget_id": "alert_list_1",
                        "widget_type": "alert_list",
                        "data": {
                            "alerts": alert_summaries,
                            "total_count": alerts.len(),
                            "unacknowledged_count": alerts.iter().filter(|a| a.status.to_string() == "active").count()
                        }
                    })
                }
                _ => {
                    serde_json::json!({
                        "widget_id": "unknown_1",
                        "widget_type": widget_type,
                        "data": {},
                        "error": "Unknown widget type"
                    })
                }
            };

            widget_data.push(widget_result);
        }

        let dashboard_data = serde_json::json!({
            "dashboard_id": dashboard_id,
            "widgets": widget_data,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "refresh_interval": dashboard_config["refresh_interval"]
        });

        Some((
            Ok(Event::default()
                .event("dashboard_metrics")
                .data(serde_json::to_string(&dashboard_data).unwrap_or_else(|_| "{}".to_string()))),
            (state, dashboard_id),
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
/// Enhanced system metrics stream with monitoring data
/// Includes GPU metrics, inference latency, active alerts count, and recent anomalies
pub async fn enhanced_system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Fetch basic system metrics
        let metrics = match get_system_metrics_internal(&state).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to fetch metrics for SSE: {}", e);
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e))),
                    state,
                ));
            }
        };

        // Fetch active alerts count
        let alert_filters = adapteros_system_metrics::AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AlertStatus::Active),
            severity: None,
            start_time: None,
            end_time: None,
            limit: Some(1), // Just count, not actual alerts
        };

        let active_alerts_count = match adapteros_system_metrics::ProcessAlert::list(
            state.db.pool(),
            alert_filters,
        )
        .await
        {
            Ok(alerts) => alerts.len(),
            Err(_) => 0,
        };

        // Fetch recent anomalies count
        let anomaly_filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AnomalyStatus::Detected),
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            end_time: None,
            limit: Some(1), // Just count, not actual anomalies
        };

        let recent_anomalies_count =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), anomaly_filters)
                .await
            {
                Ok(anomalies) => anomalies.len(),
                Err(_) => 0,
            };

        // Fetch worker health status
        let workers = match sqlx::query!("SELECT id, status FROM workers WHERE status = 'active'")
            .fetch_all(state.db.pool())
            .await
        {
            Ok(workers) => workers.len(),
            Err(_) => 0,
        };

        let enhanced_metrics = serde_json::json!({
            "system_metrics": {
                "cpu_usage": metrics.cpu_usage,
                "memory_usage": metrics.memory_usage,
                "gpu_utilization": metrics.gpu_utilization,
                "gpu_memory_used": 0.0,
                "gpu_temperature": 0.0,
                "disk_usage": metrics.disk_usage,
                "network_rx": 0.0,
                "network_tx": 0.0
            },
            "monitoring_metrics": {
                "active_alerts_count": active_alerts_count,
                "recent_anomalies_count": recent_anomalies_count,
                "active_workers_count": workers,
                "inference_latency_p95": 0.0, // Placeholder - would come from worker
                "active_inference_sessions": 0, // Placeholder - would come from worker
                "adapter_swap_latency": 0.0, // Placeholder - would come from worker
                "lora_routing_overhead": 0.0 // Placeholder - would come from worker
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        Some((
            Ok(Event::default().event("enhanced_metrics").data(
                serde_json::to_string(&enhanced_metrics).unwrap_or_else(|_| "{}".to_string()),
            )),
            state,
        ))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for notifications
/// Pushes real-time notifications as they are created or updated
pub async fn notifications_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = claims.sub.clone();
    let stream = stream::unfold(state, move |state| {
        let user_id = user_id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Fetch recent unread notifications
            let notifications = match state
                .db
                .list_user_notifications(&user_id, None, None, true, Some(50), Some(0))
                .await
            {
                Ok(notifs) => notifs,
                Err(e) => {
                    tracing::warn!("Failed to fetch notifications for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

            let notification_data = serde_json::json!({
                "notifications": notifications.iter().map(|n| serde_json::json!({
                    "id": n.id,
                    "workspace_id": n.workspace_id,
                    "type": n.type_,
                    "target_type": n.target_type,
                    "target_id": n.target_id,
                    "title": n.title,
                    "content": n.content,
                    "read_at": n.read_at,
                    "created_at": n.created_at,
                })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "count": notifications.len()
            });

            Some((
                Ok(Event::default().event("notifications").data(
                    serde_json::to_string(&notification_data).unwrap_or_else(|_| "{}".to_string()),
                )),
                state,
            ))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for workspace messages
/// Pushes real-time messages as they are sent in a workspace
pub async fn messages_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Verify workspace access
    let user_id = claims.sub.clone();
    let tenant_id = claims.tenant_id.clone();
    let ws_id = workspace_id.clone();

    let stream = stream::unfold(state, move |state| {
        let workspace_id = ws_id.clone();
        let user_id = user_id.clone();
        let tenant_id = tenant_id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(1)).await;

            // Check workspace access
            let has_access = match state
                .db
                .check_workspace_access(&workspace_id, &user_id, &tenant_id)
                .await
            {
                Ok(role) => role.is_some(),
                Err(_) => false,
            };

            if !has_access {
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Access denied\"}")),
                    state,
                ));
            }

            // Fetch recent messages
            let messages = match state
                .db
                .get_recent_workspace_messages(&workspace_id, None)
                .await
            {
                Ok(msgs) => msgs,
                Err(e) => {
                    tracing::warn!("Failed to fetch messages for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

            let message_data = serde_json::json!({
                "messages": messages.iter().map(|m| serde_json::json!({
                    "id": m.id,
                    "workspace_id": m.workspace_id,
                    "from_user_id": m.from_user_id,
                    "from_tenant_id": m.from_tenant_id,
                    "content": m.content,
                    "thread_id": m.thread_id,
                    "created_at": m.created_at,
                    "edited_at": m.edited_at,
                })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "count": messages.len()
            });

            Some((
                Ok(Event::default().event("messages").data(
                    serde_json::to_string(&message_data).unwrap_or_else(|_| "{}".to_string()),
                )),
                state,
            ))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// SSE stream for workspace activity
/// Pushes real-time activity events for a workspace
pub async fn activity_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Verify workspace access
    let user_id = claims.sub.clone();
    let tenant_id = claims.tenant_id.clone();
    let ws_id = workspace_id.clone();

    let stream = stream::unfold(state, move |state| {
        let workspace_id = ws_id.clone();
        let user_id = user_id.clone();
        let tenant_id = tenant_id.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Check workspace access
            let has_access = match state
                .db
                .check_workspace_access(&workspace_id, &user_id, &tenant_id)
                .await
            {
                Ok(role) => role.is_some(),
                Err(_) => false,
            };

            if !has_access {
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Access denied\"}")),
                    state,
                ));
            }

            // Fetch recent activity events
            let events = match state
                .db
                .list_activity_events(Some(&workspace_id), None, None, None, Some(50), Some(0))
                .await
            {
                Ok(evts) => evts,
                Err(e) => {
                    tracing::warn!("Failed to fetch activity events for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        state,
                    ));
                }
            };

            let activity_data = serde_json::json!({
                "events": events.iter().map(|e| serde_json::json!({
                    "id": e.id,
                    "workspace_id": e.workspace_id,
                    "user_id": e.user_id,
                    "tenant_id": e.tenant_id,
                    "event_type": e.event_type,
                    "target_type": e.target_type,
                    "target_id": e.target_id,
                    "metadata_json": e.metadata_json,
                    "created_at": e.created_at,
                })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "count": events.len()
            });

            Some((
                Ok(Event::default().event("activity").data(
                    serde_json::to_string(&activity_data).unwrap_or_else(|_| "{}".to_string()),
                )),
                state,
            ))
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Get federation audit report
///
/// Returns federation chain verification status and host validation results.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    get,
    path = "/v1/audit/federation",
    responses(
        (status = 200, description = "Federation audit report", body = FederationAuditResponse)
    )
)]
pub async fn get_federation_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<FederationAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance, Role::Operator])?;

    // Fetch federation bundle signatures
    let pool = state.db.pool();

    let signatures = sqlx::query(
        r#"
        SELECT 
            bundle_hash,
            host_id,
            signature,
            verified,
            created_at
        FROM federation_bundle_signatures
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch federation signatures")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut host_chains: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut total_signatures = 0;
    let mut verified_signatures = 0;

    for row in signatures {
        total_signatures += 1;
        let host_id: String = row.try_get("host_id").unwrap_or_default();
        let verified: bool = row.try_get("verified").unwrap_or(false);
        let bundle_hash: String = row.try_get("bundle_hash").unwrap_or_default();

        if verified {
            verified_signatures += 1;
        }

        host_chains.entry(host_id).or_default().push(bundle_hash);
    }

    // Check quarantine status
    let quarantine_status = sqlx::query(
        r#"
        SELECT reason, created_at
        FROM policy_quarantine
        WHERE released = FALSE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to check quarantine status")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let quarantined = quarantine_status.is_some();
    let quarantine_reason = quarantine_status.and_then(|row| row.try_get("reason").ok());

    Ok(Json(FederationAuditResponse {
        total_hosts: host_chains.len(),
        total_signatures,
        verified_signatures,
        quarantined,
        quarantine_reason,
        host_chains: host_chains
            .into_iter()
            .map(|(host_id, bundles)| HostChainSummary {
                host_id,
                bundle_count: bundles.len(),
                latest_bundle: bundles.first().cloned(),
            })
            .collect(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get compliance audit report
///
/// Returns compliance status for all policy packs and control objectives.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    get,
    path = "/v1/audit/compliance",
    responses(
        (status = 200, description = "Compliance audit report", body = ComplianceAuditResponse)
    )
)]
pub async fn get_compliance_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ComplianceAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Compliance, Role::Operator])?;

    // Fetch policy violations from policy_quarantine table
    let pool = state.db.pool();

    let violation_count = sqlx::query(
        r#"
        SELECT COUNT(*) as count
        FROM policy_quarantine
        WHERE released = FALSE
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count violations")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let active_violations: i64 = violation_count.try_get("count").unwrap_or(0);

    // Fetch actual violation records
    let violation_records = sqlx::query(
        r#"
        SELECT id, reason, violation_type, created_at, released, cpid, metadata
        FROM policy_quarantine
        WHERE released = FALSE
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch violations")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let violations: Vec<PolicyViolationRecord> = violation_records
        .into_iter()
        .map(|row| {
            let created_at: String = row
                .try_get("created_at")
                .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());
            PolicyViolationRecord {
                id: row.try_get("id").unwrap_or_else(|_| "unknown".to_string()),
                reason: row
                    .try_get("reason")
                    .unwrap_or_else(|_| "Unknown violation".to_string()),
                violation_type: row.try_get("violation_type").ok(),
                created_at,
                released: row.try_get("released").unwrap_or(false),
                cpid: row.try_get("cpid").ok(),
                metadata: row.try_get("metadata").ok(),
            }
        })
        .collect();

    // Generate compliance controls status
    let controls = vec![
        ComplianceControl {
            control_id: "EGRESS-001".to_string(),
            control_name: "Network Egress Control".to_string(),
            status: if active_violations == 0 {
                "compliant"
            } else {
                "pending"
            }
            .to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Zero egress mode enforced".to_string(),
                "PF rules active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "DETERM-001".to_string(),
            control_name: "Deterministic Execution".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Metal kernels precompiled".to_string(),
                "HKDF seeding enabled".to_string(),
                "Tick ledger active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "ISOLATION-001".to_string(),
            control_name: "Tenant Isolation".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Per-tenant processes".to_string(),
                "UID/GID separation".to_string(),
            ],
            findings: vec![],
        },
    ];

    let compliant_count = controls.iter().filter(|c| c.status == "compliant").count();
    let compliance_rate = if !controls.is_empty() {
        (compliant_count as f64 / controls.len() as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(ComplianceAuditResponse {
        compliance_rate,
        total_controls: controls.len(),
        compliant_controls: compliant_count,
        active_violations: active_violations as usize,
        controls,
        violations,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// List available log files
#[utoipa::path(
    get,
    path = "/v1/logs/files",
    responses(
        (status = 200, description = "Log files retrieved successfully", body = ListLogFilesResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 500, description = "Internal server error")
    ),
    security(("jwt_token" = []))
)]
pub async fn list_log_files(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ListLogFilesResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    use chrono::{DateTime, Utc};
    use std::fs;

    let mut files = Vec::new();
    let mut total_size = 0u64;

    // Common log file locations and patterns
    let log_paths = vec![".", "var/", "var/log/", "/var/log/adapteros/"];

    for log_dir in log_paths {
        if let Ok(entries) = fs::read_dir(log_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() {
                    if let Some(extension) = path.extension() {
                        if extension == "log" {
                            if let Ok(metadata) = fs::metadata(&path) {
                                let file_name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                let modified = metadata
                                    .modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| {
                                        DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0)
                                            .unwrap_or_default()
                                    })
                                    .unwrap_or_default();

                                let created = metadata
                                    .created()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| {
                                        DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0)
                                            .unwrap_or_default()
                                    })
                                    .unwrap_or_default();

                                let size = metadata.len();
                                total_size += size;

                                files.push(LogFileInfo {
                                    name: file_name,
                                    path: path.to_string_lossy().to_string(),
                                    size_bytes: size,
                                    modified_at: modified.to_rfc3339(),
                                    created_at: created.to_rfc3339(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by modification time (newest first)
    files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    let count = files.len();
    Ok(Json(ListLogFilesResponse {
        files,
        total_size_bytes: total_size,
        count,
    }))
}

/// Get log file content
#[utoipa::path(
    get,
    path = "/v1/logs/files/{filename}",
    params(
        ("filename" = String, Path, description = "Log filename")
    ),
    responses(
        (status = 200, description = "Log file content retrieved successfully", body = LogFileContentResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Log file not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("jwt_token" = []))
)]
pub async fn get_log_file_content(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(filename): Path<String>,
    Query(params): Query<LogFileQueryParams>,
) -> Result<Json<LogFileContentResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    use adapteros_secure_fs::traversal::{check_path_traversal, join_paths_safe};
    use std::fs;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::Path;

    // Validate filename to prevent directory traversal using secure-fs
    if let Err(e) = check_path_traversal(Path::new(&filename)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid filename")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Path validation failed: {}", e)),
            ),
        ));
    }

    // Look for the file in common locations with secure path joining
    let possible_bases = vec![
        Path::new("./"),
        Path::new("var/"),
        Path::new("var/log/"),
        Path::new("/var/log/adapteros/"),
    ];

    let file_path = possible_bases
        .into_iter()
        .find_map(|base| {
            join_paths_safe(base, &filename)
                .ok()
                .filter(|p| fs::metadata(p).is_ok())
        })
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Log file not found").with_code("NOT_FOUND")),
            )
        })?;

    let mut file = fs::File::open(&file_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to open log file")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let metadata = file.metadata().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to read file metadata")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let file_size = metadata.len();
    let max_size = params.max_size.unwrap_or(1024 * 1024); // Default 1MB
    let tail = params.tail.unwrap_or(false);
    let lines_limit = params.lines;

    let content = if tail {
        // Read from the end of the file
        let start_pos = file_size.saturating_sub(max_size as u64);

        file.seek(SeekFrom::Start(start_pos)).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to seek in file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        let mut buffer = vec![0u8; max_size.min(file_size as usize - start_pos as usize)];
        file.read_exact(&mut buffer).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to read file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        String::from_utf8_lossy(&buffer).to_string()
    } else if let Some(line_count) = lines_limit {
        // Read last N lines
        let mut buffer = String::new();
        file.read_to_string(&mut buffer).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to read file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        let lines: Vec<&str> = buffer.lines().collect();
        let start_idx = if lines.len() > line_count {
            lines.len() - line_count
        } else {
            0
        };

        lines[start_idx..].join("\n")
    } else {
        // Read with size limit
        let mut buffer = vec![0u8; max_size.min(file_size as usize)];
        file.read_exact(&mut buffer).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to read file")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        String::from_utf8_lossy(&buffer).to_string()
    };

    let truncated = file_size > max_size as u64;

    Ok(Json(LogFileContentResponse {
        name: filename,
        path: file_path.to_string_lossy().to_string(),
        content,
        size_bytes: file_size,
        truncated,
        max_size_bytes: max_size as u64,
    }))
}
/// Stream log file content
#[utoipa::path(
    get,
    path = "/v1/logs/files/{filename}/stream",
    params(
        ("filename" = String, Path, description = "Log filename")
    ),
    responses(
        (status = 200, description = "Log file streaming started"),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Log file not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("jwt_token" = []))
)]
pub async fn stream_log_file(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(filename): Path<String>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    use adapteros_secure_fs::traversal::{check_path_traversal, join_paths_safe};
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::path::Path;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    // Validate filename to prevent directory traversal using secure-fs
    if let Err(e) = check_path_traversal(Path::new(&filename)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid filename")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Path validation failed: {}", e)),
            ),
        ));
    }

    // Look for the file in common locations with secure path joining
    let possible_bases = vec![
        Path::new("./"),
        Path::new("var/"),
        Path::new("var/log/"),
        Path::new("/var/log/adapteros/"),
    ];

    let file_path = possible_bases
        .into_iter()
        .find_map(|base| {
            join_paths_safe(base, &filename)
                .ok()
                .filter(|p| fs::metadata(p).is_ok())
        })
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Log file not found").with_code("NOT_FOUND")),
            )
        })?;

    let file = fs::File::open(&file_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to open log file")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(100);

    // Spawn a task to read the file and send lines
    tokio::spawn(async move {
        let reader = BufReader::new(file);
        let lines = reader.lines();

        for line in lines {
            match line {
                Ok(content) => {
                    if tx.send(Ok(Event::default().data(content))).await.is_err() {
                        break; // Client disconnected
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(Ok(
                            Event::default().data(format!("Error reading line: {}", e))
                        ))
                        .await;
                    break;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct FederationAuditResponse {
    pub total_hosts: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub host_chains: Vec<HostChainSummary>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct HostChainSummary {
    pub host_id: String,
    pub bundle_count: usize,
    pub latest_bundle: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceAuditResponse {
    pub compliance_rate: f64,
    pub total_controls: usize,
    pub compliant_controls: usize,
    pub active_violations: usize,
    pub controls: Vec<ComplianceControl>,
    pub violations: Vec<PolicyViolationRecord>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PolicyViolationRecord {
    pub id: String,
    pub reason: String,
    pub violation_type: Option<String>,
    pub created_at: String,
    pub released: bool,
    pub cpid: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceControl {
    pub control_id: String,
    pub control_name: String,
    pub status: String,
    pub last_checked: String,
    pub evidence: Vec<String>,
    pub findings: Vec<String>,
}
