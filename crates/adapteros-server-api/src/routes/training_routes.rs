//! Training job routes.
//!
//! This module contains all routes for:
//! - `/v1/training/jobs/*` - Training job CRUD, status, logs
//! - `/v1/training/queue` - Training queue management
//! - `/v1/training/templates/*` - Training templates

use crate::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, patch, post},
    Router,
};

/// Build the training routes subrouter.
///
/// These routes require authentication and are merged into the protected routes.
pub fn training_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/training/jobs",
            get(handlers::list_training_jobs).post(handlers::create_training_job),
        )
        .route(
            "/v1/training/queue",
            get(handlers::training::get_training_queue),
        )
        .route(
            "/v1/training/backend-readiness",
            get(handlers::training::get_training_backend_readiness),
        )
        .route(
            "/v1/training/preprocessing/status",
            post(handlers::get_preprocess_status),
        )
        .route(
            "/v1/training/preprocessed-cache/count",
            get(handlers::training::get_preprocessed_cache_count),
        )
        .route(
            "/v1/training/preprocessed-cache",
            get(handlers::training::list_preprocessed_cache),
        )
        .route(
            "/v1/training/jobs/{job_id}",
            get(handlers::get_training_job),
        )
        .route(
            "/v1/training/start",
            post(handlers::training::start_training),
        )
        .route(
            "/v1/training/repos/{repo_id}/versions/{version_id}/promote",
            post(handlers::promote_version),
        )
        .route(
            "/v1/training/jobs/{job_id}/cancel",
            post(handlers::cancel_training),
        )
        .route(
            "/v1/training/jobs/{job_id}/retry",
            post(handlers::retry_training),
        )
        .route(
            "/v1/training/jobs/{job_id}/priority",
            patch(handlers::training::update_training_priority),
        )
        .route(
            "/v1/training/jobs/{job_id}/export/coreml",
            post(handlers::export_coreml_training_job),
        )
        .route(
            "/v1/training/sessions",
            post(handlers::training::create_training_session),
        )
        .route(
            "/v1/training/jobs/{job_id}/logs",
            get(handlers::training::get_training_logs),
        )
        .route(
            "/v1/training/jobs/{job_id}/metrics",
            get(handlers::training::get_training_metrics),
        )
        .route(
            "/v1/training/jobs/{job_id}/report",
            get(handlers::training::get_training_report),
        )
        .route(
            "/v1/training/jobs/{job_id}/progress",
            get(handlers::training::stream_training_progress),
        )
        .route(
            "/v1/training/jobs/batch-status",
            post(handlers::training::batch_training_status),
        )
        .route(
            "/v1/training/jobs/{job_id}/chat_bootstrap",
            get(handlers::get_chat_bootstrap),
        )
        .route(
            "/v1/training/templates",
            get(handlers::training::list_training_templates),
        )
        .route(
            "/v1/training/templates/{template_id}",
            get(handlers::training::get_training_template),
        )
        // Checkpoint verification
        .route(
            "/v1/training/checkpoints/verify",
            post(handlers::checkpoint_verify::verify_checkpoint),
        )
        .route(
            "/v1/training/jobs/{job_id}/checkpoints/{epoch}/verify",
            get(handlers::checkpoint_verify::verify_job_checkpoint),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_training_routes_builds() {
        // Verify routes compile and build without panic
        let _router: Router<AppState> = training_routes();
    }
}
