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
use adapteros_core::AosError;
use adapteros_orchestrator::TrainingJobStatus;
use axum::{
    extract::State,
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

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

    // Get jobs from training service
    let all_jobs = state.training_service.list_jobs().await.map_err(|e| {
        error!(error = %e, "Failed to list training jobs");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(&format!("Failed to list jobs: {}", e))
                    .with_code("DATABASE_ERROR"),
            ),
        )
    })?;

    // Apply filters including tenant isolation
    // Non-admin users can only see jobs belonging to their tenant
    let is_admin = claims.role == "admin";
    let user_tenant_id = &claims.tenant_id;

    let mut filtered_jobs: Vec<_> = all_jobs
        .into_iter()
        .filter(|job| {
            // CRITICAL: Tenant isolation - non-admin users can only see their own tenant's jobs
            if !is_admin {
                match &job.tenant_id {
                    Some(job_tenant) if job_tenant != user_tenant_id => return false,
                    None => return false, // Jobs without tenant_id are hidden from non-admins
                    _ => {}
                }
            }

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
    let page_size = params.page_size.unwrap_or(20).min(100).max(1);
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
                    ErrorResponse::new(&format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(&format!("Failed to get job: {}", e))
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
            ErrorResponse::new(&format!("Failed to start training: {}", error))
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
                    ErrorResponse::new(&format!("Failed to validate request: {}", e))
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
            // Note: Config lock failures return AosError::Other, not AosError::Config
            AosError::Other(_) => (
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
    let config = training_config_from_request(request.config);

    // Serialize post_actions to JSON if provided
    let post_actions_json = request
        .post_actions
        .as_ref()
        .and_then(|pa| serde_json::to_string(pa).ok());

    // Start training via service
    let job = state
        .training_service
        .start_training(
            request.adapter_name.clone(),
            config,
            request.template_id.clone(),
            request.repo_id.clone(),
            request.dataset_id.clone(),
            Some(claims.tenant_id.clone()),
            Some(claims.sub.clone()),
            Some(claims.role.clone()),
            request.base_model_id.clone(),
            request.collection_id.clone(),
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
        )
        .await
        .map_err(|e| {
            error!(adapter_name = %request.adapter_name, error = %e, "Failed to start training job");

            // Audit log: training start failure
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let _ = crate::audit_helper::log_failure(
                        &state.db,
                        &claims,
                        crate::audit_helper::actions::TRAINING_START,
                        crate::audit_helper::resources::TRAINING_JOB,
                        Some(&request.adapter_name),
                        &e.to_string(),
                    )
                    .await;
                })
            });

            let as_aos = AosError::Other(e.to_string());
            build_training_error_response(&as_aos)
        })?;

    // Audit log: training start success
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_START,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&job.id),
    )
    .await;

    info!(
        job_id = %job.id,
        adapter_name = %job.adapter_name,
        user_id = %claims.sub,
        "Started training job"
    );

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
                    ErrorResponse::new(&format!("Training job not found: {}", job_id))
                        .with_code("NOT_FOUND"),
                ),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(&format!("Failed to get job: {}", e))
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
    let socket_path = std::env::var("AOS_WORKER_SOCKET")
        .unwrap_or_else(|_| "/var/run/adapteros.sock".to_string());

    state
        .training_service
        .cancel_job(&job_id, Some(&uds_client), Some(&socket_path))
        .await
        .map_err(|e| {
            error!(job_id = %job_id, error = %e, "Failed to cancel training job");

            // Audit log: training cancel failure
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let _ = crate::audit_helper::log_failure(
                        &state.db,
                        &claims,
                        crate::audit_helper::actions::TRAINING_CANCEL,
                        crate::audit_helper::resources::TRAINING_JOB,
                        Some(&job_id),
                        &e.to_string(),
                    )
                    .await;
                })
            });

            let error_str = e.to_string();
            if error_str.contains("cannot be cancelled") || error_str.contains("already") {
                (
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new(&format!("Job cannot be cancelled: {}", e))
                            .with_code("INVALID_STATE"),
                    ),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(&format!("Failed to cancel job: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            }
        })?;

    // Audit log: training cancel success
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_CANCEL,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&job_id),
    )
    .await;

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
                ErrorResponse::new(&format!("Training job not found: {}", job_id))
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
                ErrorResponse::new(&format!(
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
    let new_job = state
        .training_service
        .start_training(
            original_job.adapter_name.clone(),
            original_job.config.clone(),
            original_job.template_id.clone(),
            original_job.repo_id.clone(),
            original_job.dataset_id.clone(),
            original_job.tenant_id.clone(),
            Some(claims.sub.clone()),
            Some(claims.role.clone()),
            original_job.base_model_id.clone(),
            original_job.collection_id.clone(),
            original_job.category.clone(),
            original_job.description.clone(),
            original_job.language.clone(),
            original_job.framework_id.clone(),
            original_job.framework_version.clone(),
            original_job.post_actions_json.clone(),
            // Link to original job for retry chain tracking
            Some(job_id.clone()),
        )
        .await
        .map_err(|e| {
            error!(original_job_id = %job_id, error = %e, "Failed to create retry job");

            // Audit log: retry failure
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let _ = crate::audit_helper::log_failure(
                        &state.db,
                        &claims,
                        crate::audit_helper::actions::TRAINING_START,
                        crate::audit_helper::resources::TRAINING_JOB,
                        Some(&job_id),
                        &e.to_string(),
                    )
                    .await;
                })
            });

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new(&format!("Failed to create retry job: {}", e))
                        .with_code("TRAINING_START_FAILED"),
                ),
            )
        })?;

    // Audit log: retry success
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TRAINING_START,
        crate::audit_helper::resources::TRAINING_JOB,
        Some(&new_job.id),
    )
    .await;

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
    ) = match state.training_service.get_job(&job_id).await {
        Ok(job) => {
            let status_str = format!("{:?}", job.status).to_lowercase();
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
            )
        }
        Err(_) => {
            // Fall back to database for completed jobs not in memory (e.g., after server restart)
            let db_job = state.db.get_training_job(&job_id).await.map_err(|e| {
                error!(job_id = %job_id, error = %e, "Failed to get training job from DB");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(&format!("Failed to get job: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            })?;

            let job = db_job.ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(&format!("Training job not found: {}", job_id))
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
        dataset_id,
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
                                ErrorResponse::new(&format!("Failed to get job: {}", e))
                                    .with_code("DATABASE_ERROR"),
                            ),
                        )
                    })?;

            let job = db_job.ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(&format!(
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
    };

    state.db.create_chat_session(params).await.map_err(|e| {
        error!(error = %e, "Failed to create chat session from training job");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new(&format!("Failed to create chat session: {}", e))
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
