//! Training endpoints for adapteros server
//!
//! This crate contains training-related API endpoints migrated from adapteros-server-api.
//! It provides handlers and routes for managing training jobs, checkpoints, preprocessing,
//! metrics, and related operations.
//!
//! # Handlers
//!
//! - `list_training_jobs` - List training jobs with optional filters
//! - `get_training_job` - Get specific training job details
//! - `create_training_job` - Create a minimal training job (workspace-scoped)
//! - `start_training` - Start a new training job
//! - `cancel_training` - Cancel a running training job
//! - `retry_training` - Retry a failed training job
//! - `promote_version` - Promote an adapter version to active
//! - `publish_version` - Publish an adapter version with attach mode
//! - `get_preprocess_status` - Inspect preprocessing cache status
//! - `get_training_backend_readiness` - Report backend readiness for training
//! - `export_coreml_training_job` - Trigger CoreML export for a completed job
//! - `get_training_logs` - Get training logs for a job
//! - `get_training_metrics` - Get training metrics for a job
//! - `get_training_report` - Get training report artifact
//! - `get_training_queue` - Get current training queue status
//! - `update_training_priority` - Update training job priority
//! - `list_training_templates` - List training templates
//! - `get_training_template` - Get a specific training template
//! - `create_training_session` - Create a training session (alias for create_training_job)
//! - `get_chat_bootstrap` - Get chat bootstrap data for a training job
//! - `create_chat_from_training_job` - Create a chat session from a training job
//!
//! # SSE Streaming
//!
//! - `stream_training_progress` - Stream real-time training progress via SSE
//!
//! # Batch Operations
//!
//! - `batch_training_status` - Get batch status for multiple training jobs

pub mod handlers;
pub mod routes;
pub mod streaming;
mod types;

pub use handlers::*;
pub use routes::training_routes;
pub use streaming::stream_training_progress;
pub use types::*;
