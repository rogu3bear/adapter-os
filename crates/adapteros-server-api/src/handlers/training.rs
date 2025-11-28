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
use adapteros_lora_worker::memory::MemoryPressureLevel;
use axum::{
    extract::State,
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use tracing::{error, info, warn};

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
        let error_code = validation.error_code.as_deref().unwrap_or("VALIDATION_ERROR");
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
                    if let Ok(Some(pack)) = state.db.get_policy_pack(&assignment.policy_pack_id).await {
                        if pack.policy_type == "evidence" && pack.status == "active" {
                            let resource_id = if error_message.contains("Dataset") && request.dataset_id.is_some() {
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
        let error_message = e.to_string();
        let (status_code, error_code) = if error_message.contains("concurrent training jobs") {
            (StatusCode::SERVICE_UNAVAILABLE, "TRAINING_CAPACITY_LIMIT")
        } else if error_message.contains("memory pressure") {
            (StatusCode::SERVICE_UNAVAILABLE, "MEMORY_PRESSURE_CRITICAL")
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, "CAPACITY_CHECK_ERROR")
        };

        warn!(
            user_id = %claims.sub,
            adapter_name = %request.adapter_name,
            error = %error_message,
            "Training job rejected due to capacity or memory constraints"
        );

        return Err((
            status_code,
            Json(ErrorResponse::new(&error_message).with_code(error_code)),
        ));
    }

    // Convert request config to training config
    let config = training_config_from_request(request.config);

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
        )
        .await
        .map_err(|e| {
            error!(adapter_name = %request.adapter_name, error = %e, "Failed to start training job");

            // Audit log: training start failure
            let _ = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::TRAINING_START,
                crate::audit_helper::resources::TRAINING_JOB,
                Some(&request.adapter_name),
                &e.to_string(),
            );

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

    state
        .training_service
        .cancel_job(&job_id)
        .await
        .map_err(|e| {
            error!(job_id = %job_id, error = %e, "Failed to cancel training job");

            // Audit log: training cancel failure
            let _ = crate::audit_helper::log_failure(
                &state.db,
                &claims,
                crate::audit_helper::actions::TRAINING_CANCEL,
                crate::audit_helper::resources::TRAINING_JOB,
                Some(&job_id),
                &e.to_string(),
            );

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
