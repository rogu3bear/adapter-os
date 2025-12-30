//! Training job handlers
//!
//! Provides REST endpoints for managing training jobs, sessions,
//! metrics, and training templates.
//!
//! 【2025-01-20†rectification†training_handlers】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::services::{DefaultTrainingService, TrainingService};
use crate::state::AppState;
use crate::types::*;
use adapteros_config::resolve_worker_socket_for_cp;
use adapteros_core::AosError;
use adapteros_db::CreateDraftVersionParams as CreateDraftAdapterVersionParams;
use adapteros_orchestrator::{
    training::{compute_combined_data_spec_hash, TrainingVersioningContext},
    TrainingJobStatus,
};
use adapteros_types::training::{
    BranchClassification, DataLineageMode, DatasetVersionSelection as CoreDatasetVersionSelection,
    LoraTier, TrainingBackendKind, TrainingBackendPolicy,
};
use axum::{
    extract::State,
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::Json,
};
use blake3::Hasher;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info, warn};
use utoipa::IntoParams;
use uuid::Uuid;

const METRIC_LINEAGE_REQUIRED: &str = "training_jobs_rejected_lineage_required";
const METRIC_TRUST_BLOCKED: &str = "training_jobs_rejected_trust_blocked";
const METRIC_TRUST_NEEDS_APPROVAL: &str = "training_jobs_rejected_trust_needs_approval";

// Canonical tokens for public trust state surfaces.
const CANONICAL_TRUST_STATES: &[&str] = &[
    "allowed",
    "allowed_with_warning",
    "needs_approval",
    "blocked",
    "unknown",
];

fn canonical_trust_state(raw: &str) -> String {
    let normalized = match raw.trim().to_ascii_lowercase().as_str() {
        "allowed" => "allowed",
        "allowed_with_warning" | "warn" => "allowed_with_warning",
        "needs_approval" => "needs_approval",
        "blocked" | "blocked_regressed" => "blocked",
        "unknown" => "unknown",
        other => {
            warn!(state = %other, "Unknown trust_state; normalizing to unknown");
            "unknown"
        }
    };

    // Guardrail: ensure only canonical tokens escape public APIs.
    if !CANONICAL_TRUST_STATES.contains(&normalized) {
        warn!(state = %normalized, "Non-canonical trust_state emitted; forcing unknown");
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

fn parse_lora_tier(value: Option<&str>) -> Option<LoraTier> {
    match value {
        Some("micro") => Some(LoraTier::Micro),
        Some("standard") => Some(LoraTier::Standard),
        Some("max") => Some(LoraTier::Max),
        _ => None,
    }
}

async fn record_training_rejection_metric(state: &AppState, series: &str) {
    state
        .metrics_registry
        .record_metric(series.to_string(), 1.0)
        .await;
}

/// List training jobs with optional filters
#[utoipa::path(
    get,
    path = "/v1/training/jobs",
    params(TrainingListParams),
    responses(
        (status = 200, description = "Training jobs retrieved successfully", body = TrainingJobListResponse)
    ),
    tag = "training"
)]
pub async fn list_training_jobs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<TrainingListParams>,
) -> Result<Json<TrainingJobListResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingView)?;

    // HARMONIZED: Use tenant-scoped database query for non-admin users
    let is_admin = claims.role == "admin";
    let user_tenant_id = &claims.tenant_id;

    let all_jobs = if is_admin {
        // Admin: fetch all jobs from in-memory training service
        state.training_service.list_jobs().await.map_err(|e| {
            error!(error = %e, "Failed to list training jobs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to list jobs: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        })?
    } else {
        // Non-admin: use tenant-scoped database query
        let db_jobs = state
            .db
            .list_training_jobs_for_tenant(user_tenant_id)
            .await
            .map_err(|e| {
                error!(error = %e, tenant_id = %user_tenant_id, "Failed to list tenant training jobs");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(format!("Failed to list jobs: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            })?;

        // Convert DB records to TrainingJob domain objects
        db_jobs
            .into_iter()
            .map(|record| {
                // Map TrainingJobRecord to TrainingJob
                use adapteros_orchestrator::{TrainingJob, TrainingJobStatus};
                use adapteros_types::training::{DataLineageMode, TrainingConfig};

                let status = match record.status.to_lowercase().as_str() {
                    "pending" => TrainingJobStatus::Pending,
                    "running" => TrainingJobStatus::Running,
                    "completed" => TrainingJobStatus::Completed,
                    "failed" => TrainingJobStatus::Failed,
                    "cancelled" => TrainingJobStatus::Cancelled,
                    _ => TrainingJobStatus::Pending,
                };

                let config: TrainingConfig =
                    serde_json::from_str(&record.training_config_json).unwrap_or_default();

                let data_lineage_mode = record.data_lineage_mode.as_deref().and_then(|s| {
                    match s.to_lowercase().as_str() {
                        "versioned" => Some(DataLineageMode::Versioned),
                        "synthetic" => Some(DataLineageMode::Synthetic),
                        "dataset_only" => Some(DataLineageMode::DatasetOnly),
                        "legacy_unpinned" => Some(DataLineageMode::LegacyUnpinned),
                        _ => None,
                    }
                });

                let dataset_version_ids = record
                    .data_spec_json
                    .as_ref()
                    .and_then(|json| serde_json::from_str(json).ok());

                // Parse progress from JSON to extract individual metrics
                let progress_data: Option<serde_json::Value> =
                    serde_json::from_str(&record.progress_json).ok();

                let progress_pct = progress_data
                    .as_ref()
                    .and_then(|p| p.get("progress_pct"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;

                let current_epoch = progress_data
                    .as_ref()
                    .and_then(|p| p.get("current_epoch"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                let total_epochs = config.epochs;

                let current_loss = progress_data
                    .as_ref()
                    .and_then(|p| p.get("current_loss"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;

                let learning_rate = config.learning_rate;

                let tokens_per_second = progress_data
                    .as_ref()
                    .and_then(|p| p.get("tokens_per_second"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;

                let lora_tier = parse_lora_tier(record.lora_tier.as_deref());

                TrainingJob {
                    id: record.id.clone(),
                    adapter_name: record.adapter_name.unwrap_or_default(),
                    config,
                    template_id: record.template_id,
                    repo_id: Some(record.repo_id),
                    repo_name: None,
                    target_branch: record.target_branch,
                    base_version_id: record.base_version_id,
                    draft_version_id: record.draft_version_id,
                    adapter_version_id: None,
                    produced_version_id: record.produced_version_id,
                    version_label: None,
                    code_commit_sha: record.code_commit_sha.clone(),
                    data_spec_json: record.data_spec_json.clone(),
                    data_spec_hash: None,
                    dataset_id: record.dataset_id,
                    dataset_version_ids,
                    dataset_version_trust: None,
                    dataset_hash_b3: record.dataset_hash_b3,
                    synthetic_mode: record.synthetic_mode.map(|v| v != 0).unwrap_or(false),
                    data_lineage_mode,
                    base_model_id: record.base_model_id,
                    collection_id: record.collection_id,
                    build_id: record.build_id,
                    source_documents_json: record.source_documents_json,
                    config_hash_b3: record.config_hash_b3,
                    status,
                    progress_pct,
                    current_epoch,
                    total_epochs,
                    current_loss,
                    learning_rate,
                    tokens_per_second,
                    created_at: record.created_at.unwrap_or_else(|| Utc::now().to_rfc3339()),
                    started_at: Some(record.started_at),
                    completed_at: record.completed_at,
                    error_message: None,
                    artifact_path: record.artifact_path,
                    adapter_id: record.adapter_id,
                    weights_hash_b3: record.weights_hash_b3,
                    tenant_id: record.tenant_id,
                    stack_id: record.stack_id,
                    initiated_by: Some(record.created_by),
                    initiated_by_role: None,
                    category: record.category,
                    description: record.description,
                    language: record.language,
                    symbol_targets_json: record.symbol_targets_json,
                    framework_id: record.framework_id,
                    framework_version: record.framework_version,
                    lora_tier,
                    lora_strength: record.lora_strength.map(|v| v as f32),
                    scope: record.scope,
                    api_patterns_json: record.api_patterns_json,
                    repo_scope: record.repo_scope,
                    file_patterns_json: record.file_patterns_json,
                    exclude_patterns_json: record.exclude_patterns_json,
                    post_actions_json: None,
                    retryable: record.retryable.map(|v| v != 0),
                    retry_of_job_id: record.retry_of_job_id,
                    requested_backend: None,
                    backend_policy: None,
                    coreml_training_fallback: None,
                    backend: record.backend,
                    backend_reason: record.backend_reason,
                    backend_device: record.backend_device,
                    coreml_export_requested: None,
                    coreml_export_status: None,
                    coreml_export_reason: None,
                    coreml_fused_package_hash: None,
                    coreml_package_path: None,
                    coreml_metadata_path: None,
                    coreml_base_manifest_hash: None,
                    coreml_adapter_hash_b3: None,
                    determinism_mode: None,
                    training_seed: None,
                    seed_inputs_json: None,
                    require_gpu: None,
                    max_gpu_memory_mb: None,
                    examples_processed: None,
                    tokens_processed: None,
                    training_time_ms: None,
                    throughput_examples_per_sec: None,
                    gpu_utilization_pct: None,
                    peak_gpu_memory_mb: None,
                    aos_path: None,
                    package_hash_b3: None,
                    manifest_hash_b3: None,
                    manifest_rank: None,
                    manifest_base_model: None,
                    manifest_per_layer_hashes: None,
                    signature_status: None,
                }
            })
            .collect()
    };

    // Apply additional filters (status, adapter_name, template_id, dataset_id)
    let mut filtered_jobs: Vec<_> = all_jobs
        .into_iter()
        .filter(|job| {
            // Filter by status
            if let Some(ref status) = params.status {
                if job.status.to_string().to_lowercase() != status.to_lowercase() {
                    return false;
                }
            }
            // Filter by adapter name
            if let Some(ref name) = params.adapter_name {
                if !job.adapter_name.contains(name) {
                    return false;
                }
            }
            // Filter by template ID
            if let Some(ref template) = params.template_id {
                if job.template_id.as_ref() != Some(template) {
                    return false;
                }
            }
            // Filter by dataset ID
            if let Some(ref dataset_id) = params.dataset_id {
                if job.dataset_id.as_ref() != Some(dataset_id) {
                    return false;
                }
            }
            true
        })
        .collect();

    let total = filtered_jobs.len();

    // Apply pagination
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
    let start = ((page - 1) * page_size) as usize;

    let jobs: Vec<TrainingJobResponse> = filtered_jobs
        .drain(..)
        .skip(start)
        .take(page_size as usize)
        .map(TrainingJobResponse::from)
        .collect();

    Ok(Json(TrainingJobListResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        jobs,
        total,
        page,
        page_size,
    }))
}

/// Create a minimal training job (workspace-scoped)
#[utoipa::path(
    post,
    path = "/v1/training/jobs",
    request_body = CreateTrainingJobRequest,
    responses(
        (status = 201, description = "Training job created", body = TrainingJobResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Workspace access denied", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn create_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTrainingJobRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingStart)?;

    let workspace_id = if req.workspace_id.is_empty() {
        claims.tenant_id.clone()
    } else {
        req.workspace_id.clone()
    };

    // Enforce workspace access (owner/member/viewer permitted for now)
    let workspace_role = if workspace_id == claims.tenant_id {
        Some(adapteros_db::workspaces::WorkspaceRole::Owner)
    } else {
        state
            .db
            .check_workspace_access(&workspace_id, &claims.sub, &claims.tenant_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to check workspace access")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
    };

    if workspace_role.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("workspace access denied")
                    .with_code("TENANT_ISOLATION_ERROR")
                    .with_string_details("user is not a member of the workspace"),
            ),
        ));
    }

    // Resolve dataset version (default to latest)
    let dataset_version_id = match req.dataset_version_id {
        Some(id) => id,
        None => state
            .db
            .ensure_dataset_version_exists(&req.dataset_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("failed to resolve dataset version")
                            .with_code("DATASET_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?,
    };

    let adapter_name = req
        .adapter_name
        .clone()
        .unwrap_or_else(|| format!("ws-{}-{}", workspace_id, Uuid::now_v7()));

    let config = training_config_from_request(req.params.clone());
    let dataset_version_ids = vec![CoreDatasetVersionSelection {
        dataset_version_id,
        weight: 1.0,
    }];

    let job = state
        .training_service
        .start_training(
            adapter_name,
            config,
            None,                            // template_id
            None,                            // repo_id
            None,                            // target_branch
            None,                            // base_version_id
            Some(req.dataset_id.clone()),    // dataset_id
            Some(dataset_version_ids),       // dataset_version_ids
            false,                           // synthetic_mode
            DataLineageMode::DatasetOnly,    // lineage
            Some(claims.tenant_id.clone()),  // tenant_id
            Some(claims.sub.clone()),        // initiated_by
            Some(claims.role.clone()),       // initiated_by_role
            Some(req.base_model_id.clone()), // base_model_id
            None,                            // collection_id
            Some(workspace_id.clone()),      // scope
            req.lora_tier,                   // lora tier
            None,                            // category
            None,                            // description
            None,                            // language
            None,                            // framework_id
            None,                            // framework_version
            None,                            // post_actions_json
            None,                            // retry_of_job_id
            None,                            // versioning
            None,                            // code_commit_sha
            None,                            // data_spec_json
            None,                            // data_spec_hash
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create training job");
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("failed to create training job")
                        .with_code("TRAINING_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(training_job_to_response(job)))
}

/// Get specific training job details
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training job details", body = TrainingJobResponse),
        (status = 404, description = "Job not found", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn get_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingView)?;

    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        error!(job_id = %job_id, error = %e, "Failed to get training job");
        let error_str = e.to_string();
        if error_str.contains("not found") || error_str.contains("NotFound") {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new(format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to get job: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        }
    })?;

    // CRITICAL: Validate tenant isolation - non-admin users can only access their own tenant's jobs
    if let Some(ref job_tenant_id) = job.tenant_id {
        validate_tenant_isolation(&claims, job_tenant_id)?;
    } else if claims.role != "admin" {
        // Jobs without tenant_id are only accessible to admins
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied: job has no tenant association")
                    .with_code("TENANT_ISOLATION_ERROR"),
            ),
        ));
    }

    info!(job_id = %job_id, status = %job.status, "Retrieved training job");
    Ok(Json(TrainingJobResponse::from(job)))
}

/// Trigger a CoreML export for a completed training job
#[utoipa::path(
    post,
    path = "/v1/training/jobs/{job_id}/export/coreml",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "CoreML export triggered", body = TrainingJobResponse),
        (status = 400, description = "Export failed", body = ErrorResponse),
        (status = 404, description = "Job not found", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn export_coreml_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingStart)?;

    // Execute export via orchestrator (per-tenant enforcement)
    let job = state
        .training_service
        .export_coreml_for_job(&job_id)
        .await
        .map_err(|e| {
            error!(job_id = %job_id, error = %e, "Failed to export CoreML for training job");
            let error_str = e.to_string();
            let status = if error_str.to_lowercase().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (
                status,
                Json(
                    ErrorResponse::new(format!("Failed to export CoreML: {}", e))
                        .with_code("EXPORT_ERROR"),
                ),
            )
        })?;

    if let Some(ref job_tenant_id) = job.tenant_id {
        validate_tenant_isolation(&claims, job_tenant_id)?;
    }

    Ok(Json(TrainingJobResponse::from(job)))
}

fn build_training_error_response(error: &AosError) -> (StatusCode, Json<ErrorResponse>) {
    let error_message = error.to_string();
    let is_validation_variant = matches!(error, AosError::Validation(_));
    let is_dataset_validation_message = error_message
        .to_ascii_lowercase()
        .contains("not validated (status:");

    if is_validation_variant || is_dataset_validation_message {
        // Preserve the original validation message so the client can show actionable guidance
        let message = match error {
            AosError::Validation(msg) => msg.clone(),
            AosError::Database(msg) => msg.clone(),
            _ => error_message.clone(),
        };

        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&message).with_code("VALIDATION_ERROR")),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            ErrorResponse::new(format!("Failed to start training: {}", error))
                .with_code("TRAINING_ERROR"),
        ),
    )
}

/// Start a new training job
#[utoipa::path(
    post,
    path = "/v1/training/start",
    request_body = StartTrainingRequest,
    responses(
        (status = 200, description = "Training job started", body = TrainingJobResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Failed to start training", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn start_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<StartTrainingRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingStart)?;

    // Create training service instance
    let service = DefaultTrainingService::new(Arc::new(state.clone()));

    // Validate adapter name
    if request.adapter_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Adapter name is required").with_code("VALIDATION_ERROR")),
        ));
    }

    // Enforce tenant isolation and commit provenance for system repositories
    if let Some(repo_id) = request.repo_id.as_deref() {
        // Prefer code repository lookup; fall back to adapter repository if not found
        let repo_tenant = match state
            .db
            .get_repository_by_repo_id(&claims.tenant_id, repo_id)
            .await
        {
            Ok(Some(repo)) => Some(repo.tenant_id),
            Ok(None) => state
                .db
                .get_adapter_repository(&claims.tenant_id, repo_id)
                .await
                .map_err(|e| {
                    error!(repo_id = %repo_id, error = %e, "Failed to load adapter repository");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to load repository")
                                .with_code("INTERNAL_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?
                .map(|repo| repo.tenant_id),
            Err(e) => {
                error!(repo_id = %repo_id, error = %e, "Failed to load repository");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to load repository")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        };

        if let Some(ref repo_tenant_id) = repo_tenant {
            validate_tenant_isolation(&claims, repo_tenant_id)?;

            if repo_tenant_id == "system"
                && request
                    .code_commit_sha
                    .as_deref()
                    .map(str::trim)
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new(
                            "code_commit_sha is required for system-owned repositories",
                        )
                        .with_code("VALIDATION_ERROR"),
                    ),
                ));
            }
        }
    }

    // Check if evidence policy is enforced for this tenant
    let evidence_policy_enforced = {
        let policy_assignments = state
            .db
            .get_policy_assignments("tenant", Some(&claims.tenant_id))
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to get policy assignments");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to check policy assignments")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        let mut enforced = false;
        for assignment in &policy_assignments {
            if assignment.enforced {
                if let Ok(Some(pack)) = state.db.get_policy_pack(&assignment.policy_pack_id).await {
                    if pack.policy_type == "evidence" && pack.status == "active" {
                        enforced = true;
                        break;
                    }
                }
            }
        }
        enforced
    };

    // Use service to validate training request
    let validation = service
        .validate_training_request(
            &claims.tenant_id,
            request.dataset_id.as_deref(),
            request.collection_id.as_deref(),
            evidence_policy_enforced,
        )
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to validate training request");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to validate request: {}", e))
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    // If validation failed, return appropriate error
    if !validation.is_valid {
        let error_code = validation
            .error_code
            .as_deref()
            .unwrap_or("VALIDATION_ERROR");
        let error_message = validation
            .error_message
            .unwrap_or_else(|| "Validation failed".to_string());

        let status_code = match error_code {
            "NOT_FOUND" => StatusCode::NOT_FOUND,
            "TENANT_ISOLATION_ERROR" => StatusCode::FORBIDDEN,
            "POLICY_VIOLATION" => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        };

        // Record policy violation if this is a policy-related error
        if error_code == "POLICY_VIOLATION" {
            // Get the enforced evidence policy pack to record violation
            let policy_assignments = state
                .db
                .get_policy_assignments("tenant", Some(&claims.tenant_id))
                .await
                .unwrap_or_default();

            for assignment in &policy_assignments {
                if assignment.enforced {
                    if let Ok(Some(pack)) =
                        state.db.get_policy_pack(&assignment.policy_pack_id).await
                    {
                        if pack.policy_type == "evidence" && pack.status == "active" {
                            let resource_id = if error_message.contains("Dataset")
                                && request.dataset_id.is_some()
                            {
                                request.dataset_id.as_deref()
                            } else {
                                None
                            };

                            let _ = state
                                .db
                                .record_policy_violation(
                                    &pack.id,
                                    Some(&assignment.id),
                                    "evidence",
                                    "critical",
                                    "training_request",
                                    resource_id,
                                    &claims.tenant_id,
                                    &format!("Evidence policy violation: {}", error_message),
                                    None,
                                )
                                .await;
                            break;
                        }
                    }
                }
            }
        }

        return Err((
            status_code,
            Json(ErrorResponse::new(&error_message).with_code(error_code)),
        ));
    }

    // Use service to check if training can start (capacity + memory pressure)
    if let Err(e) = service.can_start_training().await {
        let (status_code, error_code, user_message) = match &e {
            // Validation errors: check message for capacity or memory pressure
            // Keep original message for Validation errors (user-friendly and actionable)
            AosError::Validation(msg) => {
                if msg.contains("concurrent training jobs") {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "TRAINING_CAPACITY_LIMIT",
                        msg.clone(),
                    )
                } else if msg.contains("memory pressure") {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "MEMORY_PRESSURE_CRITICAL",
                        msg.clone(),
                    )
                } else {
                    // Other validation errors
                    (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone())
                }
            }
            // Database errors: service temporarily unavailable
            // Preserve original error details for debugging via with_string_details()
            AosError::Database(_) | AosError::Sqlx(_) | AosError::Sqlite(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "DATABASE_ERROR",
                "Unable to check training capacity: database temporarily unavailable".to_string(),
            ),
            // Other errors: internal server error (includes config lock failures)
            // Note: Config lock failures return AosError::Internal
            AosError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Unable to check training capacity: internal error".to_string(),
            ),
            // Fallback for any other error variants
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "CAPACITY_CHECK_ERROR",
                "Unable to check training capacity".to_string(),
            ),
        };

        warn!(
            user_id = %claims.sub,
            adapter_name = %request.adapter_name,
            error = %e,
            "Training job rejected due to capacity check failure"
        );

        return Err((
            status_code,
            Json(
                ErrorResponse::new(&user_message)
                    .with_code(error_code)
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    // Convert request config to training config
    let mut config = training_config_from_request(request.config);

    // Resolve repository + branch context (required for versioning)
    let repo_id = match request.repo_id.clone() {
        Some(id) => id,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("repo_id is required for training")
                        .with_code("VALIDATION_ERROR"),
                ),
            ))
        }
    };

    let repo = state
        .db
        .get_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            error!(error = %e, repo_id = %repo_id, "Failed to load adapter repository");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load repository")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Repository not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(repo_id.clone()),
                ),
            )
        })?;

    let target_branch = request
        .target_branch
        .clone()
        .unwrap_or_else(|| repo.default_branch.clone());

    // Validate parent version (if provided) belongs to same repo/tenant
    if let Some(ref base_version_id) = request.base_version_id {
        let base_version = state
            .db
            .get_adapter_version(&claims.tenant_id, base_version_id)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    version_id = %base_version_id,
                    "Failed to load base adapter version"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to load base version")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Base version not found")
                            .with_code("NOT_FOUND")
                            .with_string_details(base_version_id.clone()),
                    ),
                )
            })?;

        if base_version.repo_id != repo.id {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Base version does not belong to repository")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(format!("Base version '{}' belongs to repository '{}', but training job is for repository '{}'", base_version_id, base_version.repo_id, repo.id)),
                ),
            ));
        }
    }

    let data_spec_json = request.data_spec.clone();
    let mut data_spec_hash = request.data_spec_hash.clone().or_else(|| {
        data_spec_json.as_ref().map(|json| {
            let mut hasher = Hasher::new();
            hasher.update(json.as_bytes());
            hasher.finalize().to_hex().to_string()
        })
    });

    fn parse_backend_kind(label: &str) -> Option<TrainingBackendKind> {
        match label.to_ascii_lowercase().as_str() {
            "coreml" => Some(TrainingBackendKind::CoreML),
            "mlx" => Some(TrainingBackendKind::Mlx),
            "metal" => Some(TrainingBackendKind::Metal),
            "cpu" => Some(TrainingBackendKind::Cpu),
            _ => None,
        }
    }

    // Apply repository policy (CoreML allowances and backend preferences)
    let repo_policy = state
        .db
        .get_adapter_repository_policy(&claims.tenant_id, &repo.id)
        .await
        .map_err(|e| {
            error!(
                error = %e,
                repo_id = %repo_id,
                "Failed to load adapter repository policy"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load repository policy")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if let Some(policy) = repo_policy {
        if policy.coreml_required {
            if config.preferred_backend.is_none() {
                config.preferred_backend = Some(TrainingBackendKind::CoreML);
            } else if !matches!(config.preferred_backend, Some(TrainingBackendKind::CoreML)) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("CoreML backend required by repository policy")
                            .with_code("POLICY_VIOLATION"),
                    ),
                ));
            }
            config.backend_policy = Some(TrainingBackendPolicy::CoremlOnly);
        }

        if config.preferred_backend.is_none() {
            if let Some(pref_json) = policy.preferred_backends_json.as_ref() {
                if let Ok(preferred) = serde_json::from_str::<Vec<String>>(pref_json) {
                    for backend in preferred {
                        if let Some(kind) = parse_backend_kind(&backend) {
                            config.preferred_backend = Some(kind);
                            break;
                        }
                    }
                }
            }
        }

        if !policy.coreml_allowed
            && matches!(config.preferred_backend, Some(TrainingBackendKind::CoreML))
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("CoreML backend disallowed by repository policy")
                        .with_code("POLICY_VIOLATION"),
                ),
            ));
        }
    }
    let requested_backend = config.preferred_backend.map(|b| b.to_string());

    if matches!(
        request.data_lineage_mode,
        Some(DataLineageMode::LegacyUnpinned)
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("lineage_mode=legacy_unpinned is blocked for new jobs")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let branch_classification = request
        .branch_classification
        .unwrap_or(BranchClassification::Protected);

    let synthetic_mode = request.synthetic_mode;

    let dataset_version_ids_core: Option<Vec<CoreDatasetVersionSelection>> =
        request.dataset_version_ids.as_ref().map(|versions| {
            versions
                .iter()
                .map(|v| CoreDatasetVersionSelection {
                    dataset_version_id: v.dataset_version_id.clone(),
                    weight: v.weight,
                })
                .collect()
        });

    if let Some(dataset_versions) = dataset_version_ids_core.as_ref() {
        if dataset_versions.is_empty() {
            record_training_rejection_metric(&state, METRIC_LINEAGE_REQUIRED).await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("dataset_version_ids cannot be empty")
                        .with_code("LINEAGE_REQUIRED"),
                ),
            ));
        }
    }

    let has_dataset_versions = dataset_version_ids_core
        .as_ref()
        .map(|versions| !versions.is_empty())
        .unwrap_or(false);

    if synthetic_mode && has_dataset_versions {
        record_training_rejection_metric(&state, METRIC_LINEAGE_REQUIRED).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("synthetic_mode=true requires dataset_version_ids to be empty")
                    .with_code("LINEAGE_REQUIRED"),
            ),
        ));
    }

    if !synthetic_mode && !has_dataset_versions {
        record_training_rejection_metric(&state, METRIC_LINEAGE_REQUIRED).await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(
                    "dataset_version_ids are required for non-synthetic training jobs",
                )
                .with_code("LINEAGE_REQUIRED"),
            ),
        ));
    }

    let data_lineage_mode = if synthetic_mode {
        DataLineageMode::Synthetic
    } else {
        DataLineageMode::Versioned
    };

    if let Some(mode) = request.data_lineage_mode {
        if mode != data_lineage_mode {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("data_lineage_mode does not match inferred mode")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
    }

    let high_assurance_tenant = match state.db.get_tenant(&claims.tenant_id).await {
        Ok(Some(tenant)) => tenant
            .status
            .as_deref()
            .map(|s| {
                let lowered = s.to_ascii_lowercase();
                lowered == "production" || lowered == "high_assurance"
            })
            .unwrap_or(false),
        Ok(None) => false,
        Err(e) => {
            error!(tenant_id = %claims.tenant_id, error = %e, "Failed to load tenant for assurance check");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to evaluate tenant assurance level")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    if high_assurance_tenant && matches!(data_lineage_mode, DataLineageMode::DatasetOnly) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(
                    "dataset_version_ids are required for high-assurance tenants (set synthetic_mode=true for diagnostics)",
                )
                .with_code("LINEAGE_REQUIRED"),
            ),
        ));
    }

    info!(
        repo_id = %repo.id,
        tenant_id = %claims.tenant_id,
        preferred_backend = ?requested_backend,
        dataset_version_ids = ?dataset_version_ids_core
            .as_ref()
            .map(|v| v.iter().map(|d| d.dataset_version_id.clone()).collect::<Vec<_>>()),
        "Training request backend/dataset selection recorded"
    );

    if let Some(dataset_versions) = dataset_version_ids_core.as_ref() {
        let mut combined_inputs: Vec<(String, String, f32)> = Vec::new();

        for sel in dataset_versions {
            let ds_version = state
                .db
                .get_training_dataset_version_for_tenant(&sel.dataset_version_id, &claims.tenant_id)
                .await
                .map_err(|e| {
                    error!(
                        error = %e,
                        dataset_version_id = %sel.dataset_version_id,
                        "Failed to load dataset version"
                    );
                    (
                        StatusCode::BAD_REQUEST,
                        Json(
                            ErrorResponse::new("Failed to load dataset version")
                                .with_code("VALIDATION_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        Json(
                            ErrorResponse::new("Dataset version not found")
                                .with_code("NOT_FOUND")
                                .with_string_details(sel.dataset_version_id.clone()),
                        ),
                    )
                })?;

            let trust_state = canonical_trust_state(&ds_version.trust_state);
            match trust_state.as_str() {
                "blocked" => {
                    record_training_rejection_metric(&state, METRIC_TRUST_BLOCKED).await;
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(
                            ErrorResponse::new(format!(
                                "dataset version {} trust_state={} blocks training",
                                sel.dataset_version_id, trust_state
                            ))
                            .with_code("DATASET_TRUST_BLOCKED"),
                        ),
                    ));
                }
                "needs_approval" | "unknown" => {
                    record_training_rejection_metric(&state, METRIC_TRUST_NEEDS_APPROVAL).await;
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(
                            ErrorResponse::new(format!(
                                "dataset version {} trust_state={} blocks training",
                                sel.dataset_version_id, trust_state
                            ))
                            .with_code("DATASET_TRUST_NEEDS_APPROVAL"),
                        ),
                    ));
                }
                _ => {}
            }

            let weight = if sel.weight <= 0.0 { 1.0 } else { sel.weight };
            combined_inputs.push((
                sel.dataset_version_id.clone(),
                ds_version.hash_b3.clone(),
                weight,
            ));
        }

        // Deterministic combined hash over all dataset manifests (weight-sensitive)
        let combined_hash = compute_combined_data_spec_hash(&combined_inputs);

        if let Some(ref expected_hash) = data_spec_hash {
            if expected_hash != &combined_hash {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("data_spec_hash mismatch vs dataset manifests")
                            .with_code("DATA_SPEC_HASH_MISMATCH"),
                    ),
                ));
            }
        }

        data_spec_hash = Some(combined_hash);
    }

    // Create draft adapter version before enqueuing training
    let adapter_version_id = state
        .db
        .create_adapter_draft_version(CreateDraftAdapterVersionParams {
            repo_id: &repo.id,
            tenant_id: &claims.tenant_id,
            branch: &target_branch,
            branch_classification: branch_classification.as_str(),
            parent_version_id: request.base_version_id.as_deref(),
            code_commit_sha: request.code_commit_sha.as_deref(),
            data_spec_hash: data_spec_hash.as_deref(),
            training_backend: requested_backend.as_deref(),
            dataset_version_ids: dataset_version_ids_core
                .as_ref()
                .map(|v| {
                    v.iter()
                        .map(|d| d.dataset_version_id.clone())
                        .collect::<Vec<_>>()
                })
                .as_deref(),
            actor: Some(&claims.sub),
            reason: Some("training_start"),
        })
        .await
        .map_err(|e| {
            error!(error = %e, repo_id = %repo_id, "Failed to create adapter version draft");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    let version_label = format!("draft-{}", &adapter_version_id[..8]);

    let versioning_context = TrainingVersioningContext {
        adapter_version_id: adapter_version_id.clone(),
        version_label: version_label.clone(),
        branch: target_branch.clone(),
        repo_id: repo.id.clone(),
        repo_name: repo.name.clone(),
        parent_version_id: request.base_version_id.clone(),
        draft_version_id: Some(adapter_version_id.clone()),
        code_commit_sha: request.code_commit_sha.clone(),
        data_spec_json: data_spec_json.clone(),
        data_spec_hash: data_spec_hash.clone(),
    };

    // Serialize post_actions to JSON if provided
    let post_actions_json = request
        .post_actions
        .as_ref()
        .and_then(|pa| serde_json::to_string(pa).ok());

    if let Some(dataset_versions) = dataset_version_ids_core.as_ref() {
        let ids: Vec<String> = dataset_versions
            .iter()
            .map(|v| v.dataset_version_id.clone())
            .collect();
        state
            .db
            .upsert_adapter_version_dataset_versions(&claims.tenant_id, &adapter_version_id, &ids)
            .await
            .map_err(|e| {
                error!(
                    error = %e,
                    version_id = %adapter_version_id,
                    "Failed to link dataset versions to adapter version"
                );
                (
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("Failed to link dataset versions")
                            .with_code("VALIDATION_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Start training via service
    let job = match state
        .training_service
        .start_training(
            request.adapter_name.clone(),
            config,
            request.template_id.clone(),
            Some(repo_id.clone()),
            Some(target_branch.clone()),
            request.base_version_id.clone(),
            request.dataset_id.clone(),
            dataset_version_ids_core.clone(),
            synthetic_mode,
            data_lineage_mode,
            Some(claims.tenant_id.clone()),
            Some(claims.sub.clone()),
            Some(claims.role.clone()),
            request.base_model_id.clone(),
            request.collection_id.clone(),
            request.scope.clone(),
            request.lora_tier,
            // Category metadata
            request.category.clone(),
            request.description.clone(),
            request.language.clone(),
            request.framework_id.clone(),
            request.framework_version.clone(),
            // Post-training actions
            post_actions_json,
            // Not a retry - new training job
            None,
            // Versioning context (draft)
            Some(versioning_context),
            // Provenance passthrough
            request.code_commit_sha.clone(),
            data_spec_json.clone(),
            data_spec_hash.clone(),
        )
        .await
    {
        Ok(job) => job,
        Err(e) => {
            error!(adapter_name = %request.adapter_name, error = %e, "Failed to start training job");

            // Audit log: training start failure
            if let Err(audit_err) = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::TRAINING_START,
                crate::audit_helper::resources::TRAINING_JOB,
                Some(&request.adapter_name),
                &e.to_string(),
            )
            .await
            {
                tracing::warn!(error = %audit_err, "Audit log failed");
            }

            let as_aos = AosError::Internal(e.to_string());
            return Err(build_training_error_response(&as_aos));
        }
    };

    // Audit log: training start success
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_START,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&job.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    info!(
        job_id = %job.id,
        adapter_name = %job.adapter_name,
        user_id = %claims.sub,
        "Started training job"
    );

    // Emit plugin event for training job start (if event bus configured)
    if let Some(ref event_bus) = state.event_bus {
        use adapteros_core::plugin_events::{PluginEvent, TrainingJobEvent};
        use chrono::Utc;

        let training_event = TrainingJobEvent {
            job_id: job.id.clone(),
            status: job.status.to_string(),
            progress_pct: Some(job.progress_pct as f64),
            loss: None,
            tokens_per_sec: None,
            dataset_id: job.dataset_id.clone(),
            adapter_id: job.adapter_id.clone(),
            tenant_id: Some(claims.tenant_id.clone()),
            error: None,
            timestamp: Utc::now().to_rfc3339(),
            metadata: std::collections::HashMap::new(),
        };

        let event = PluginEvent::TrainingJob(training_event);
        let event_bus_clone = event_bus.clone();
        tokio::spawn(async move {
            if let Err(failures) = event_bus_clone.emit(event).await {
                warn!(
                    failed_plugins = ?failures,
                    "Some plugins failed to handle TrainingJob event"
                );
            }
        });
    }

    // Project training job into repository/version model (best-effort)
    let repo_id = request
        .repo_id
        .clone()
        .or_else(|| job.repo_id.clone())
        .unwrap_or_else(|| format!("repo-{}", job.id));
    let target_branch = request
        .target_branch
        .clone()
        .unwrap_or_else(|| "main".to_string());

    Ok(Json(TrainingJobResponse::from(job)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn training_error_response_preserves_dataset_validation_message() {
        let error =
            AosError::Validation("Dataset ds-123 is not validated (status: draft)".to_string());

        let (status, axum::Json(body)) = build_training_error_response(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.code, "VALIDATION_ERROR");
        assert_eq!(
            body.error,
            "Dataset ds-123 is not validated (status: draft)"
        );
    }

    #[test]
    fn training_error_response_maps_dataset_validation_string_to_400() {
        // Some call sites wrap the validation message in a non-validation error; the handler
        // should still surface a 400 with the actionable message.
        let error =
            AosError::Database("Dataset ds-123 is not validated (status: draft)".to_string());

        let (status, axum::Json(body)) = build_training_error_response(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.code, "VALIDATION_ERROR");
        assert_eq!(
            body.error,
            "Dataset ds-123 is not validated (status: draft)"
        );
    }

    #[test]
    fn canonical_trust_state_normalizes_legacy_tokens() {
        assert_eq!(canonical_trust_state("warn"), "allowed_with_warning");
        assert_eq!(canonical_trust_state("blocked_regressed"), "blocked");
        assert_eq!(canonical_trust_state("Unknown"), "unknown");
    }

    #[test]
    fn canonical_trust_state_rejects_non_canonical_tokens() {
        assert_eq!(canonical_trust_state("custom-state"), "unknown");
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "snake_case")]
pub struct PromoteVersionQuery {
    /// Branch to promote on; defaults to the version's branch
    pub branch: Option<String>,
}

/// Promote an adapter version to active for a branch
#[utoipa::path(
    post,
    path = "/v1/training/repos/{repo_id}/versions/{version_id}/promote",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Adapter version ID"),
        PromoteVersionQuery
    ),
    responses(
        (status = 204, description = "Version promoted to active"),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn promote_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
    Query(params): Query<PromoteVersionQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingStart)?;

    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for promotion"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Database error loading version '{}': {}",
                            version_id, e
                        )),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!(
                            "Adapter version '{}' does not exist for tenant '{}'",
                            version_id, claims.tenant_id
                        )),
                ),
            )
        })?;

    if version.repo_id != repo_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Version does not belong to repository")
                    .with_code("VALIDATION_ERROR")
                    .with_string_details(format!(
                        "Version '{}' belongs to repository '{}', not '{}'",
                        version_id, version.repo_id, repo_id
                    )),
            ),
        ));
    }

    let branch = params.branch.unwrap_or(version.branch.clone());

    state
        .db
        .promote_adapter_version(
            &claims.tenant_id,
            &repo_id,
            &version_id,
            Some(&claims.sub),
            Some("training_promotion"),
        )
        .await
        .map_err(|e| {
            error!(
                repo_id = %repo_id,
                version_id = %version_id,
                error = %e,
                "Failed to promote adapter version"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to promote version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        repo_id = %repo_id,
        version_id = %version_id,
        branch = %branch,
        "Promoted adapter version to active"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// Publish an adapter version with attach mode configuration.
///
/// This endpoint publishes a trained adapter version, making it available
/// for use in inference stacks. The attach mode controls whether the adapter
/// requires specific dataset context when attached.
#[utoipa::path(
    post,
    path = "/v1/training/repos/{repo_id}/versions/{version_id}/publish",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("version_id" = String, Path, description = "Adapter version ID to publish"),
    ),
    request_body = adapteros_api_types::training::PublishAdapterVersionRequest,
    responses(
        (status = 200, description = "Version published successfully", body = adapteros_api_types::training::PublishAdapterVersionResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 404, description = "Version not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn publish_version(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((repo_id, version_id)): Path<(String, String)>,
    Json(req): Json<adapteros_api_types::training::PublishAdapterVersionRequest>,
) -> Result<
    Json<adapteros_api_types::training::PublishAdapterVersionResponse>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::TrainingStart)?;

    // Verify version exists and belongs to tenant/repo
    let version = state
        .db
        .get_adapter_version(&claims.tenant_id, &version_id)
        .await
        .map_err(|e| {
            error!(
                version_id = %version_id,
                error = %e,
                "Failed to load adapter version for publish"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to load adapter version")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Adapter version not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(version_id.clone()),
                ),
            )
        })?;

    if version.repo_id != repo_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Version does not belong to repository")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &version.tenant_id)?;

    let attach_mode_str = req.attach_mode.as_str();

    // Call database publish method
    state
        .db
        .publish_adapter_version(
            &claims.tenant_id,
            &repo_id,
            &version_id,
            attach_mode_str,
            req.required_scope_dataset_version_id.as_deref(),
            req.short_description.as_deref(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| {
            error!(
                repo_id = %repo_id,
                version_id = %version_id,
                error = %e,
                "Failed to publish adapter version"
            );
            match &e {
                AosError::NotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new(e.to_string()).with_code("NOT_FOUND")),
                ),
                AosError::Validation(_) => (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(e.to_string()).with_code("VALIDATION_ERROR")),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to publish version")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ),
            }
        })?;

    info!(
        repo_id = %repo_id,
        version_id = %version_id,
        attach_mode = %attach_mode_str,
        "Published adapter version"
    );

    Ok(Json(
        adapteros_api_types::training::PublishAdapterVersionResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            version_id,
            repo_id,
            attach_mode: req.attach_mode,
            required_scope_dataset_version_id: req.required_scope_dataset_version_id,
            published_at: Utc::now().to_rfc3339(),
            short_description: req.short_description,
        },
    ))
}

/// Cancel a running training job
#[utoipa::path(
    post,
    path = "/v1/training/jobs/{job_id}/cancel",
    params(
        ("job_id" = String, Path, description = "Training job ID to cancel")
    ),
    responses(
        (status = 204, description = "Training job cancelled"),
        (status = 404, description = "Job not found", body = ErrorResponse),
        (status = 409, description = "Job cannot be cancelled", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn cancel_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingCancel)?;

    // CRITICAL: Fetch job first to validate tenant isolation before cancellation
    let job = state.training_service.get_job(&job_id).await.map_err(|e| {
        error!(job_id = %job_id, error = %e, "Failed to get training job for cancellation");
        let error_str = e.to_string();
        if error_str.contains("not found") || error_str.contains("NotFound") {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new(format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to get job: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        }
    })?;

    // CRITICAL: Validate tenant isolation - non-admin users can only cancel their own tenant's jobs
    if let Some(ref job_tenant_id) = job.tenant_id {
        validate_tenant_isolation(&claims, job_tenant_id)?;
    } else if claims.role != "admin" {
        // Jobs without tenant_id can only be cancelled by admins
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied: job has no tenant association")
                    .with_code("TENANT_ISOLATION_ERROR"),
            ),
        ));
    }

    // Create UDS client for worker communication
    let uds_client = adapteros_client::UdsClient::default();
    let socket_path = resolve_worker_socket_for_cp().map_err(|e| {
        error!(job_id = %job_id, error = %e, "Failed to resolve worker socket for CP");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Invalid worker socket path: {}", e))
                    .with_code("CONFIG_ERROR"),
            ),
        )
    })?;
    let socket_path_str = socket_path.path.to_string_lossy().to_string();
    info!(
        job_id = %job_id,
        socket_path = %socket_path_str,
        socket_source = %socket_path.source,
        "Resolved worker socket for training cancel"
    );

    state
        .training_service
        .cancel_job(&job_id, Some(&uds_client), Some(&socket_path_str))
        .await
        .map_err(|e| {
            error!(job_id = %job_id, error = %e, "Failed to cancel training job");

            // Audit log: training cancel failure
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(audit_err) = crate::audit_helper::log_failure(
                        &state.db,
                        &claims,
                        crate::audit_helper::actions::TRAINING_CANCEL,
                        crate::audit_helper::resources::TRAINING_JOB,
                        Some(&job_id),
                        &e.to_string(),
                    )
                    .await
                    {
                        tracing::warn!(error = %audit_err, "Audit log failed");
                    }
                })
            });

            let error_str = e.to_string();
            if error_str.contains("cannot be cancelled") || error_str.contains("already") {
                (
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new(format!("Job cannot be cancelled: {}", e))
                            .with_code("INVALID_STATE"),
                    ),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(format!("Failed to cancel job: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            }
        })?;

    // Audit log: training cancel success
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_CANCEL,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&job_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    info!(job_id = %job_id, user_id = %claims.sub, "Cancelled training job");
    Ok(StatusCode::NO_CONTENT)
}

/// Retry a failed training job
///
/// Creates a new training job with the same configuration as the failed job.
/// The new job will have a different ID and will reference the original via retry_of_job_id.
#[utoipa::path(
    post,
    path = "/v1/training/jobs/{job_id}/retry",
    params(
        ("job_id" = String, Path, description = "Training job ID to retry")
    ),
    responses(
        (status = 201, description = "New training job created", body = TrainingJobResponse),
        (status = 404, description = "Original job not found", body = ErrorResponse),
        (status = 409, description = "Job cannot be retried (not failed or not retryable)", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn retry_training(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<(StatusCode, Json<TrainingJobResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingStart)?;

    // Get the original job
    let original_job = state.training_service.get_job(&job_id).await.map_err(|e| {
        error!(job_id = %job_id, error = %e, "Failed to get training job for retry");
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new(format!("Training job not found: {}", job_id))
                    .with_code("NOT_FOUND"),
            ),
        )
    })?;

    // Validate tenant isolation
    if let Some(ref job_tenant_id) = original_job.tenant_id {
        validate_tenant_isolation(&claims, job_tenant_id)?;
    } else if claims.role != "admin" {
        // Jobs without tenant_id can only be retried by admins
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Access denied: job has no tenant association")
                    .with_code("TENANT_ISOLATION_ERROR"),
            ),
        ));
    }

    // Validate job can be retried
    if original_job.status != TrainingJobStatus::Failed {
        return Err((
            StatusCode::CONFLICT,
            Json(
                ErrorResponse::new(format!(
                    "Job cannot be retried: status is {:?}, must be Failed",
                    original_job.status
                ))
                .with_code("INVALID_STATE"),
            ),
        ));
    }

    if original_job.retryable != Some(true) {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(
                "Job is not retryable. This may be due to a configuration error that would fail again."
            ).with_code("NOT_RETRYABLE")),
        ));
    }

    // Create a new job with the same configuration, linking to original as retry
    let data_lineage_mode = original_job
        .data_lineage_mode
        .unwrap_or(DataLineageMode::Versioned);

    let new_job = state
        .training_service
        .start_training(
            original_job.adapter_name.clone(),
            original_job.config.clone(),
            original_job.template_id.clone(),
            original_job.repo_id.clone(),
            original_job.target_branch.clone(),
            original_job.base_version_id.clone(),
            original_job.dataset_id.clone(),
            original_job.dataset_version_ids.clone(),
            original_job.synthetic_mode,
            data_lineage_mode,
            original_job.tenant_id.clone(),
            Some(claims.sub.clone()),
            Some(claims.role.clone()),
            original_job.base_model_id.clone(),
            original_job.collection_id.clone(),
            original_job.scope.clone(),
            original_job.lora_tier,
            original_job.category.clone(),
            original_job.description.clone(),
            original_job.language.clone(),
            original_job.framework_id.clone(),
            original_job.framework_version.clone(),
            original_job.post_actions_json.clone(),
            // Link to original job for retry chain tracking
            Some(job_id.clone()),
            None, // versioning (reuse existing versioning if needed)
            original_job.code_commit_sha.clone(),
            original_job.data_spec_json.clone(),
            original_job.data_spec_hash.clone(),
        )
        .await
        .map_err(|e| {
            error!(original_job_id = %job_id, error = %e, "Failed to create retry job");

            // Audit log: retry failure
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    if let Err(audit_err) = crate::audit_helper::log_failure(
                        &state.db,
                        &claims,
                        crate::audit_helper::actions::TRAINING_START,
                        crate::audit_helper::resources::TRAINING_JOB,
                        Some(&job_id),
                        &e.to_string(),
                    )
                    .await
                    {
                        tracing::warn!(error = %audit_err, "Audit log failed");
                    }
                })
            });

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to create retry job: {}", e))
                        .with_code("TRAINING_START_FAILED"),
                ),
            )
        })?;

    // Audit log: retry success
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_START,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&new_job.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    info!(
        original_job_id = %job_id,
        new_job_id = %new_job.id,
        user_id = %claims.sub,
        "Created retry job"
    );

    Ok((
        StatusCode::CREATED,
        Json(TrainingJobResponse::from(new_job)),
    ))
}

// ============================================================================
// PRD-CORE-03: Chat Bootstrap Handlers
// ============================================================================

/// Get chat bootstrap data for a training job
///
/// Returns the "recipe" for starting a chat from a completed training job.
/// Used by any UI flow to quickly get the payload needed to create a chat session.
#[utoipa::path(
    get,
    path = "/v1/training/jobs/{job_id}/chat_bootstrap",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Chat bootstrap data", body = ChatBootstrapResponse),
        (status = 404, description = "Job not found", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn get_chat_bootstrap(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<adapteros_api_types::ChatBootstrapResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TrainingView)?;

    // Try in-memory first (for running jobs), fall back to DB (for completed jobs after restart)
    let (
        stack_id,
        adapter_name,
        base_model_id,
        collection_id,
        status_completed,
        tenant_id,
        adapter_id,
        dataset_id,
        status_str,
        adapter_version_id,
        dataset_version_id,
    ) = match state.training_service.get_job(&job_id).await {
        Ok(job) => {
            let status_str = format!("{:?}", job.status).to_lowercase();
            // Extract first dataset_version_id from the list if available
            let dataset_ver_id = job
                .dataset_version_ids
                .as_ref()
                .and_then(|v| v.first().map(|s| s.dataset_version_id.clone()));
            (
                job.stack_id,
                job.adapter_name,
                job.base_model_id,
                job.collection_id,
                job.status == TrainingJobStatus::Completed,
                job.tenant_id,
                job.adapter_id,
                job.dataset_id,
                status_str,
                job.adapter_version_id,
                dataset_ver_id,
            )
        }
        Err(_) => {
            // Fall back to database for completed jobs not in memory (e.g., after server restart)
            let db_job = state.db.get_training_job(&job_id).await.map_err(|e| {
                error!(job_id = %job_id, error = %e, "Failed to get training job from DB");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(format!("Failed to get job: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            })?;

            let job = db_job.ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(format!("Training job not found: {}", job_id))
                            .with_code("NOT_FOUND"),
                    ),
                )
            })?;

            (
                job.stack_id,
                job.adapter_name.unwrap_or_default(),
                job.base_model_id,
                job.collection_id,
                job.status == "completed",
                job.tenant_id,
                job.adapter_id,
                job.dataset_id,
                job.status.clone(),
                // Use produced_version_id as adapter_version_id from DB record
                job.produced_version_id,
                // dataset_version_id not directly stored on DB record, will be None
                None,
            )
        }
    };

    // Tenant isolation check - require tenant_id for security
    let tid = tenant_id.as_deref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Training job has no tenant_id").with_code("NO_TENANT")),
        )
    })?;
    validate_tenant_isolation(&claims, tid)?;

    let ready = status_completed && stack_id.is_some();

    // Get adapter IDs from stack if available
    let adapter_ids = if let Some(ref sid) = stack_id {
        match state.db.get_stack(tid, sid).await {
            Ok(Some(stack)) => serde_json::from_str(&stack.adapter_ids_json).unwrap_or_default(),
            _ => vec![],
        }
    } else {
        vec![]
    };

    // base_model comes from job, not stack
    let base_model = base_model_id;

    let suggested_title = format!("Chat with {}", adapter_name);

    // Fetch dataset name if dataset_id is available
    let dataset_name = if let Some(ref did) = dataset_id {
        match state.db.get_training_dataset(did).await {
            Ok(Some(dataset)) => Some(dataset.name),
            _ => None,
        }
    } else {
        None
    };

    Ok(Json(adapteros_api_types::ChatBootstrapResponse {
        ready,
        stack_id,
        adapter_ids,
        base_model,
        collection_id,
        suggested_chat_title: suggested_title,
        // Provenance fields
        training_job_id: job_id,
        status: status_str,
        adapter_id,
        adapter_version_id,
        dataset_id,
        dataset_version_id,
        dataset_name,
    }))
}

/// Create a chat session from a training job
///
/// Creates a chat session bound to the training job's stack in one call.
/// Centralizes tenant/auth checks, job-ready validation, and session creation.
#[utoipa::path(
    post,
    path = "/v1/chats/from_training_job",
    request_body = CreateChatFromJobRequest,
    responses(
        (status = 200, description = "Chat session created", body = CreateChatFromJobResponse),
        (status = 400, description = "Job not ready for chat", body = ErrorResponse),
        (status = 404, description = "Job not found", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse)
    ),
    tag = "chat"
)]
pub async fn create_chat_from_training_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<adapteros_api_types::CreateChatFromJobRequest>,
) -> Result<Json<adapteros_api_types::CreateChatFromJobResponse>, (StatusCode, Json<ErrorResponse>)>
{
    require_permission(&claims, Permission::InferenceExecute)?;

    // Try in-memory first, fall back to DB for completed jobs after server restart
    let (
        stack_id_opt,
        adapter_name,
        collection_id,
        status_completed,
        tenant_id,
        adapter_id,
        dataset_id,
    ) = match state.training_service.get_job(&req.training_job_id).await {
        Ok(job) => (
            job.stack_id,
            job.adapter_name,
            job.collection_id,
            job.status == TrainingJobStatus::Completed,
            job.tenant_id,
            job.adapter_id,
            job.dataset_id,
        ),
        Err(_) => {
            // Fall back to database
            let db_job = state
                    .db
                    .get_training_job(&req.training_job_id)
                    .await
                    .map_err(|e| {
                        error!(job_id = %req.training_job_id, error = %e, "Failed to get training job from DB");
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                ErrorResponse::new(format!("Failed to get job: {}", e))
                                    .with_code("DATABASE_ERROR"),
                            ),
                        )
                    })?;

            let job = db_job.ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(format!(
                            "Training job not found: {}",
                            req.training_job_id
                        ))
                        .with_code("NOT_FOUND"),
                    ),
                )
            })?;

            (
                job.stack_id,
                job.adapter_name.unwrap_or_default(),
                job.collection_id.clone(),
                job.status == "completed",
                job.tenant_id,
                job.adapter_id,
                job.dataset_id,
            )
        }
    };

    // Tenant isolation check - require tenant_id for security
    let tid = tenant_id.as_deref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Training job has no tenant_id").with_code("NO_TENANT")),
        )
    })?;
    validate_tenant_isolation(&claims, tid)?;

    // Check job is ready for chat
    if !status_completed {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Training job has not completed successfully")
                    .with_code("JOB_NOT_COMPLETED"),
            ),
        ));
    }

    let stack_id = stack_id_opt.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(
                    "Training job did not create a stack (post_actions.create_stack may be false)",
                )
                .with_code("NO_STACK"),
            ),
        )
    })?;

    let name = req
        .name
        .unwrap_or_else(|| format!("Chat with {}", adapter_name));

    // Clone collection_id for response before moving into params
    let collection_id_for_response = collection_id.clone();

    // Create chat session
    let session_id = format!("session-{}", Uuid::new_v4());
    let params = adapteros_db::CreateChatSessionParams {
        id: session_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        user_id: Some(claims.sub.clone()),
        created_by: Some(claims.sub.clone()),
        stack_id: Some(stack_id.clone()),
        collection_id,
        document_id: None,
        name: name.clone(),
        title: None,
        source_type: Some("training_job".to_string()),
        source_ref_id: Some(req.training_job_id.clone()),
        metadata_json: req.metadata_json,
        tags_json: None,
        pinned_adapter_ids: None, // Inherits from tenant default
        codebase_adapter_id: None,
    };

    state.db.create_chat_session(params).await.map_err(|e| {
        error!(error = %e, "Failed to create chat session from training job");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(format!("Failed to create chat session: {}", e))
                    .with_code("DATABASE_ERROR"),
            ),
        )
    })?;

    let created_at = chrono::Utc::now().to_rfc3339();

    Ok(Json(adapteros_api_types::CreateChatFromJobResponse {
        session_id,
        stack_id,
        name,
        created_at,
        // Provenance fields
        training_job_id: req.training_job_id,
        adapter_id,
        dataset_id,
        collection_id: collection_id_for_response,
    }))
}

// ============================================================================
// Training Queue Status Handler
// ============================================================================

/// Get current training queue status
///
/// Returns queue depth, pending/running counts, and wait time estimates.
/// Operators and admins can see all jobs; regular users see their own tenant's jobs.
#[utoipa::path(
    get,
    path = "/v1/training/queue",
    responses(
        (status = 200, description = "Training queue status", body = adapteros_api_types::training::TrainingQueueResponse),
        (status = 403, description = "Access denied", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn get_training_queue(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<
    Json<adapteros_api_types::training::TrainingQueueResponse>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_permission(&claims, Permission::TrainingView)?;

    let is_admin = claims.role == "admin" || claims.role == "operator";

    // Get pending jobs
    let pending_records = state
        .db
        .list_training_jobs_by_status("pending")
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list pending training jobs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to query queue: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        })?;

    // Get running jobs
    let running_records = state
        .db
        .list_training_jobs_by_status("running")
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list running training jobs");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(format!("Failed to query queue: {}", e))
                        .with_code("DATABASE_ERROR"),
                ),
            )
        })?;

    // Filter by tenant if not admin
    let pending_records: Vec<_> = if is_admin {
        pending_records
    } else {
        pending_records
            .into_iter()
            .filter(|job| job.tenant_id.as_deref() == Some(&claims.tenant_id))
            .collect()
    };

    let running_records: Vec<_> = if is_admin {
        running_records
    } else {
        running_records
            .into_iter()
            .filter(|job| job.tenant_id.as_deref() == Some(&claims.tenant_id))
            .collect()
    };

    let now = Utc::now();

    // Calculate wait times for pending jobs
    let mut total_wait_secs = 0.0;
    let mut max_wait_time_secs: Option<f64> = None;

    for job in pending_records.iter() {
        if let Some(created_at) = job
            .created_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
        {
            let wait_secs = (now - created_at).num_seconds() as f64;
            total_wait_secs += wait_secs;
            if max_wait_time_secs.is_none() || wait_secs > max_wait_time_secs.unwrap_or(0.0) {
                max_wait_time_secs = Some(wait_secs);
            }
        }
    }

    let pending_jobs: Vec<adapteros_api_types::training::TrainingQueueJobSummary> = pending_records
        .iter()
        .take(10)
        .map(
            |job| adapteros_api_types::training::TrainingQueueJobSummary {
                id: job.id.clone(),
                adapter_name: job.adapter_name.clone().unwrap_or_default(),
                status: job.status.clone(),
                progress_pct: 0.0,
                created_at: job.created_at.clone().unwrap_or_default(),
                started_at: None,
                tenant_id: if is_admin {
                    job.tenant_id.clone()
                } else {
                    None
                },
            },
        )
        .collect();

    let avg_wait_time_secs = if !pending_records.is_empty() {
        total_wait_secs / pending_records.len() as f64
    } else {
        0.0
    };

    // Calculate training durations for running jobs
    let mut total_duration_secs = 0.0;
    let running_jobs: Vec<adapteros_api_types::training::TrainingQueueJobSummary> = running_records
        .iter()
        .map(|job| {
            let started_at = chrono::DateTime::parse_from_rfc3339(&job.started_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc));

            if let Some(started) = started_at {
                let duration_secs = (now - started).num_seconds() as f64;
                total_duration_secs += duration_secs;
            }

            // Parse progress from JSON
            let progress_pct = serde_json::from_str::<serde_json::Value>(&job.progress_json)
                .ok()
                .and_then(|v| v.get("progress_pct")?.as_f64())
                .unwrap_or(0.0) as f32;

            adapteros_api_types::training::TrainingQueueJobSummary {
                id: job.id.clone(),
                adapter_name: job.adapter_name.clone().unwrap_or_default(),
                status: job.status.clone(),
                progress_pct,
                created_at: job.created_at.clone().unwrap_or_default(),
                started_at: Some(job.started_at.clone()),
                tenant_id: if is_admin {
                    job.tenant_id.clone()
                } else {
                    None
                },
            }
        })
        .collect();

    let avg_training_duration_secs = if !running_records.is_empty() {
        total_duration_secs / running_records.len() as f64
    } else {
        0.0
    };

    let queue_depth = pending_records.len() + running_records.len();

    info!(
        pending = pending_records.len(),
        running = running_records.len(),
        queue_depth = queue_depth,
        "Training queue status retrieved"
    );

    Ok(Json(adapteros_api_types::training::TrainingQueueResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        queue_depth,
        pending_count: pending_records.len(),
        running_count: running_records.len(),
        avg_wait_time_secs,
        max_wait_time_secs,
        avg_training_duration_secs,
        pending_jobs,
        running_jobs,
    }))
}

// ============================================================================
// Training Priority Management
// ============================================================================

/// Request to update training job priority
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTrainingPriorityRequest {
    /// Priority value (0-100, higher = more urgent)
    pub priority: i32,
}

/// Response after updating training job priority
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTrainingPriorityResponse {
    pub job_id: String,
    pub priority: i32,
    pub message: String,
}

/// Update training job priority
///
/// Allows operators to adjust the scheduling priority of pending training jobs.
/// Priority ranges from 0 (lowest) to 100 (highest), with 50 being the default.
/// Higher priority jobs are scheduled before lower priority ones.
#[utoipa::path(
    patch,
    path = "/v1/training/{job_id}/priority",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    request_body = UpdateTrainingPriorityRequest,
    responses(
        (status = 200, description = "Priority updated", body = UpdateTrainingPriorityResponse),
        (status = 400, description = "Invalid priority value", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Job not found", body = ErrorResponse)
    ),
    tag = "training"
)]
pub async fn update_training_priority(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
    Json(req): Json<UpdateTrainingPriorityRequest>,
) -> Result<Json<UpdateTrainingPriorityResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role for priority changes
    use crate::middleware::require_any_role;
    use adapteros_db::users::Role;

    require_any_role(&claims, &[Role::Admin, Role::Operator]).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Only operators and admins can update training priority")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Validate priority range
    if req.priority < 0 || req.priority > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Priority must be between 0 and 100")
                    .with_code("INVALID_PRIORITY"),
            ),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Update priority in database
    state
        .db
        .update_training_job_priority(&job_id, tenant_id, req.priority)
        .await
        .map_err(|e| {
            let (status, code) = match &e {
                adapteros_core::AosError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
                adapteros_core::AosError::PolicyViolation(_) => {
                    (StatusCode::FORBIDDEN, "FORBIDDEN")
                }
                adapteros_core::AosError::Validation(_) => {
                    (StatusCode::BAD_REQUEST, "INVALID_STATUS")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            };
            error!(error = %e, job_id = %job_id, "Failed to update training priority");
            (
                status,
                Json(ErrorResponse::new(e.to_string()).with_code(code)),
            )
        })?;

    info!(
        job_id = %job_id,
        tenant_id = %tenant_id,
        priority = req.priority,
        "Training job priority updated"
    );

    Ok(Json(UpdateTrainingPriorityResponse {
        job_id,
        priority: req.priority,
        message: format!("Priority updated to {}", req.priority),
    }))
}

// ========== Additional Training Handlers ==========

/// Query parameters for training metrics
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct TrainingMetricsQuery {
    pub metric_name: Option<String>,
    pub limit: Option<i64>,
}

/// Get training logs for a job
///
/// Note: Training stdout/stderr logs are not currently persisted in the database.
/// This endpoint returns job status information. For training metrics (loss, accuracy, etc.),
/// use the `/v1/training/jobs/{job_id}/metrics` endpoint instead.
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/jobs/{job_id}/logs",
    params(
        ("job_id" = String, Path, description = "Training job ID")
    ),
    responses(
        (status = 200, description = "Training job status (logs not persisted)", body = Vec<String>),
        (status = 404, description = "Training job not found", body = ErrorResponse)
    )
)]
pub async fn get_training_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<ErrorResponse>)> {
    let job = state.db.get_training_job(&job_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get training job")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let job = job.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Training job not found").with_code("NOT_FOUND")),
        )
    })?;

    // Validate tenant isolation: job must belong to caller's tenant
    if job.tenant_id.as_deref() != Some(&claims.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to training job").with_code("FORBIDDEN")),
        ));
    }

    // Build informative status response (logs are not persisted in DB)
    let mut logs = vec![
        "=== Training Job Status ===".to_string(),
        format!("Job ID: {}", job.id),
        format!("Status: {}", job.status),
    ];

    if let Some(created) = &job.created_at {
        logs.push(format!("Created: {}", created));
    }
    if !job.started_at.is_empty() {
        logs.push(format!("Started: {}", job.started_at));
    }
    if let Some(completed) = &job.completed_at {
        logs.push(format!("Completed: {}", completed));
    }

    // Parse and include progress if available
    if !job.progress_json.is_empty() {
        if let Ok(progress) = serde_json::from_str::<serde_json::Value>(&job.progress_json) {
            if let Some(pct) = progress.get("percent").and_then(|v| v.as_f64()) {
                logs.push(format!("Progress: {:.1}%", pct));
            }
            if let Some(step) = progress.get("current_step").and_then(|v| v.as_i64()) {
                if let Some(total) = progress.get("total_steps").and_then(|v| v.as_i64()) {
                    logs.push(format!("Step: {} / {}", step, total));
                }
            }
        }
    }

    logs.push("".to_string());
    logs.push("Note: Stdout/stderr logs are not persisted. Use GET /v1/training/jobs/{job_id}/metrics for training metrics.".to_string());

    Ok(Json(logs))
}

/// Get training metrics for a job
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/jobs/{job_id}/metrics",
    params(
        ("job_id" = String, Path, description = "Training job ID"),
        TrainingMetricsQuery
    ),
    responses(
        (status = 200, description = "Training metrics (loss, accuracy, etc.)", body = adapteros_api_types::TrainingMetricsListResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Training job not found", body = ErrorResponse)
    )
)]
pub async fn get_training_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(job_id): Path<String>,
    Query(params): Query<TrainingMetricsQuery>,
) -> Result<Json<adapteros_api_types::TrainingMetricsListResponse>, (StatusCode, Json<ErrorResponse>)>
{
    // First verify the job exists and belongs to the caller's tenant
    let job = state
        .db
        .get_training_job(&job_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get training job")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Training job not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    if job.tenant_id.as_deref() != Some(&claims.tenant_id) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied to training job").with_code("FORBIDDEN")),
        ));
    }

    let db_metrics = state
        .db
        .get_training_metrics(&job_id, params.metric_name.as_deref(), params.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get training metrics")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Separate loss metrics from tokens_processed metrics, then merge by step.
    // The DB stores each metric type as a separate row with metric_name discriminator.
    use std::collections::HashMap;

    let mut loss_by_step: HashMap<i64, (f64, Option<i64>, i32, String)> = HashMap::new();
    let mut tokens_by_step: HashMap<i64, i64> = HashMap::new();

    for row in db_metrics {
        match row.metric_name.as_str() {
            "loss" => {
                loss_by_step.insert(
                    row.step,
                    (
                        row.metric_value,
                        row.epoch,
                        row.epoch.unwrap_or(0) as i32,
                        row.metric_timestamp.unwrap_or_default(),
                    ),
                );
            }
            "tokens_processed" => {
                tokens_by_step.insert(row.step, row.metric_value as i64);
            }
            _ => {
                // Other metrics (tokens_per_sec, etc.) - skip for now
            }
        }
    }

    // Build response entries from loss metrics, merging tokens_processed where available
    let mut metrics: Vec<adapteros_api_types::TrainingMetricEntry> = loss_by_step
        .into_iter()
        .map(|(step, (loss, _epoch_opt, epoch, timestamp))| {
            adapteros_api_types::TrainingMetricEntry {
                step,
                loss,
                learning_rate: None, // Not stored per-step in current schema
                epoch,
                tokens_processed: tokens_by_step.get(&step).copied(),
                timestamp,
            }
        })
        .collect();

    // Sort by step ascending for consistent ordering
    metrics.sort_by_key(|m| m.step);

    Ok(Json(adapteros_api_types::TrainingMetricsListResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        job_id,
        metrics,
    }))
}

/// List training templates
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/templates",
    responses(
        (status = 200, description = "List of training templates", body = Vec<adapteros_api_types::TrainingTemplateResponse>)
    )
)]
pub async fn list_training_templates(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<
    Json<Vec<adapteros_api_types::TrainingTemplateResponse>>,
    (StatusCode, Json<ErrorResponse>),
> {
    // Stub - would return pre-configured training templates
    Ok(Json(vec![]))
}

/// Get a specific training template
#[utoipa::path(
    tag = "training",
    get,
    path = "/v1/training/templates/{template_id}",
    params(
        ("template_id" = String, Path, description = "Template ID")
    ),
    responses(
        (status = 200, description = "Training template details", body = adapteros_api_types::TrainingTemplateResponse),
        (status = 404, description = "Template not found")
    )
)]
pub async fn get_training_template(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(template_id): Path<String>,
) -> Result<Json<adapteros_api_types::TrainingTemplateResponse>, (StatusCode, Json<ErrorResponse>)>
{
    Err((
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new("Template not found")
                .with_code("NOT_FOUND")
                .with_string_details(format!("Template {} not found", template_id)),
        ),
    ))
}

/// Create a training session
#[utoipa::path(
    tag = "training",
    post,
    path = "/v1/training/sessions",
    request_body = CreateTrainingJobRequest,
    responses(
        (status = 200, description = "Training session created", body = TrainingJobResponse)
    )
)]
pub async fn create_training_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTrainingJobRequest>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Delegate to create_training_job
    create_training_job(State(state), Extension(claims), Json(req)).await
}
