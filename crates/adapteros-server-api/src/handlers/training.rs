//! Training job handlers
//!
//! Provides REST endpoints for managing training jobs, sessions,
//! metrics, and training templates.
//!
//! 【2025-01-20†rectification†training_handlers】

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::State,
    extract::{Extension, Path, Query},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

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
    Extension(_claims): Extension<Claims>,
    Query(params): Query<TrainingListParams>,
) -> Result<Json<TrainingJobListResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    // Apply filters
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
    Extension(_claims): Extension<Claims>,
    Path(job_id): Path<String>,
) -> Result<Json<TrainingJobResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    info!(job_id = %job_id, status = %job.status, "Retrieved training job");
    Ok(Json(TrainingJobResponse::from(job)))
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
    // Validate adapter name
    if request.adapter_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Adapter name is required").with_code("VALIDATION_ERROR")),
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
        )
        .await
        .map_err(|e| {
            error!(adapter_name = %request.adapter_name, error = %e, "Failed to start training job");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to start training: {}", e)).with_code("TRAINING_ERROR")),
            )
        })?;

    info!(
        job_id = %job.id,
        adapter_name = %job.adapter_name,
        user_id = %claims.sub,
        "Started training job"
    );

    Ok(Json(TrainingJobResponse::from(job)))
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
    state
        .training_service
        .cancel_job(&job_id)
        .await
        .map_err(|e| {
            error!(job_id = %job_id, error = %e, "Failed to cancel training job");
            let error_str = e.to_string();
            if error_str.contains("not found") || error_str.contains("NotFound") {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new(&format!("Training job not found: {}", job_id))
                            .with_code("NOT_FOUND"),
                    ),
                )
            } else if error_str.contains("cannot be cancelled") || error_str.contains("already") {
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

    info!(job_id = %job_id, user_id = %claims.sub, "Cancelled training job");
    Ok(StatusCode::NO_CONTENT)
}
