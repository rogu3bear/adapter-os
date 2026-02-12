//! Training API routes
//!
//! Defines the router for training-related endpoints.
//!
//! # Route Structure
//!
//! ## Job Management
//! - `GET  /v1/training/jobs` - List training jobs
//! - `POST /v1/training/jobs` - Create workspace-scoped training job
//! - `GET  /v1/training/jobs/{job_id}` - Get specific training job
//! - `POST /v1/training/jobs/{job_id}/cancel` - Cancel training job
//! - `POST /v1/training/jobs/{job_id}/retry` - Retry failed training job
//! - `POST /v1/training/jobs/batch-status` - Get batch status for multiple jobs
//!
//! ## Training Start
//! - `POST /v1/training/start` - Start full training job
//!
//! ## Job Details
//! - `GET  /v1/training/jobs/{job_id}/logs` - Get training logs
//! - `GET  /v1/training/jobs/{job_id}/metrics` - Get training metrics
//! - `GET  /v1/training/jobs/{job_id}/report` - Get training report artifact
//! - `GET  /v1/training/jobs/{job_id}/progress` - SSE stream of training progress
//! - `GET  /v1/training/jobs/{job_id}/chat_bootstrap` - Get chat bootstrap data
//!
//! ## CoreML Export
//! - `POST /v1/training/jobs/{job_id}/export/coreml` - Trigger CoreML export
//!
//! ## Backend Readiness
//! - `GET  /v1/training/backend-readiness` - Check backend availability
//!
//! ## Preprocessing
//! - `POST /v1/training/preprocessing/status` - Check preprocessing cache status
//!
//! ## Version Management
//! - `POST /v1/training/repos/{repo_id}/versions/{version_id}/promote` - Promote version
//! - `POST /v1/training/repos/{repo_id}/versions/{version_id}/publish` - Publish version
//!
//! ## Queue & Priority
//! - `GET   /v1/training/queue` - Get training queue status
//! - `PATCH /v1/training/{job_id}/priority` - Update job priority
//!
//! ## Templates
//! - `GET  /v1/training/templates` - List training templates
//! - `GET  /v1/training/templates/{template_id}` - Get specific template
//!
//! ## Sessions
//! - `POST /v1/training/sessions` - Create training session (alias for create job)
//!
//! ## Chat Integration
//! - `POST /v1/chats/from-training-job` - Create chat session from training job

use axum::Router;

/// Build the training routes router
///
/// Returns an Axum Router configured with all training-related endpoints.
/// This router expects to be integrated with AppState from adapteros-server-api.
///
/// # Integration
///
/// The actual handler implementations live in adapteros-server-api's training.rs
/// module. This spoke crate provides:
/// 1. Route definitions
/// 2. Helper functions for backend readiness
/// 3. Type definitions used by handlers
/// 4. SSE streaming utilities
///
/// When integrated, the parent crate wires up routes like:
///
/// ```ignore
/// use adapteros_server_api_training::training_routes;
///
/// let router = Router::new()
///     .merge(training_routes())
///     .with_state(app_state);
/// ```
///
/// # Routes
///
/// See module documentation for full route list.
pub fn training_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // Note: The actual route wiring happens in adapteros-server-api's finalization.rs
    // This function returns an empty router that will be populated with handlers
    // from the parent crate's training.rs module.
    //
    // The routes defined here serve as documentation for the expected API surface.
    // The parent crate (adapteros-server-api) imports the handler helpers and types
    // from this crate and wires them up with its own state and middleware.
    Router::new()
}

/// Route paths for training endpoints
///
/// These constants define the API paths for training operations.
/// They are used by both the spoke crate routes and the parent crate integration.
pub mod paths {
    /// Base path for training endpoints
    pub const TRAINING_BASE: &str = "/v1/training";

    /// Training jobs endpoints
    pub const JOBS: &str = "/v1/training/jobs";
    pub const JOB_BY_ID: &str = "/v1/training/jobs/{job_id}";
    pub const JOB_CANCEL: &str = "/v1/training/jobs/{job_id}/cancel";
    pub const JOB_RETRY: &str = "/v1/training/jobs/{job_id}/retry";
    pub const JOB_LOGS: &str = "/v1/training/jobs/{job_id}/logs";
    pub const JOB_METRICS: &str = "/v1/training/jobs/{job_id}/metrics";
    pub const JOB_REPORT: &str = "/v1/training/jobs/{job_id}/report";
    pub const JOB_PROGRESS: &str = "/v1/training/jobs/{job_id}/progress";
    pub const JOB_CHAT_BOOTSTRAP: &str = "/v1/training/jobs/{job_id}/chat_bootstrap";
    pub const JOB_EXPORT_COREML: &str = "/v1/training/jobs/{job_id}/export/coreml";
    pub const JOBS_BATCH_STATUS: &str = "/v1/training/jobs/batch-status";

    /// Training start endpoint
    pub const START: &str = "/v1/training/start";

    /// Backend readiness endpoint
    pub const BACKEND_READINESS: &str = "/v1/training/backend-readiness";

    /// Preprocessing status endpoint
    pub const PREPROCESSING_STATUS: &str = "/v1/training/preprocessing/status";

    /// Version management endpoints
    pub const VERSION_PROMOTE: &str = "/v1/training/repos/{repo_id}/versions/{version_id}/promote";
    pub const VERSION_PUBLISH: &str = "/v1/training/repos/{repo_id}/versions/{version_id}/publish";

    /// Queue and priority endpoints
    pub const QUEUE: &str = "/v1/training/queue";
    pub const JOB_PRIORITY: &str = "/v1/training/{job_id}/priority";

    /// Template endpoints
    pub const TEMPLATES: &str = "/v1/training/templates";
    pub const TEMPLATE_BY_ID: &str = "/v1/training/templates/{template_id}";

    /// Session endpoint
    pub const SESSIONS: &str = "/v1/training/sessions";

    /// Chat integration endpoint (note: different base path)
    pub const CHAT_FROM_JOB: &str = "/v1/chats/from-training-job";
}

/// OpenAPI tags for training endpoints
pub mod tags {
    pub const TRAINING: &str = "training";
    pub const CHAT: &str = "chat";
}
