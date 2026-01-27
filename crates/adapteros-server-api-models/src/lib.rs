//! Model management endpoints for adapteros server
//!
//! This crate contains model-related API endpoints migrated from
//! adapteros-server-api for the spoke pattern. It provides handlers and routes
//! for managing base models, model registry, and related operations.
//!
//! # Routes
//!
//! - `GET /v1/models` - List all models with statistics
//! - `POST /v1/models/import` - Import a model from disk
//! - `GET /v1/models/download-progress` - Get download/import progress
//! - `GET /v1/models/status/all` - Get all models status
//! - `POST /v1/models/{model_id}/load` - Load a model into memory
//! - `POST /v1/models/{model_id}/unload` - Unload a model from memory
//! - `GET /v1/models/{model_id}/status` - Get single model status
//! - `GET /v1/models/{model_id}/validate` - Validate model integrity

pub mod handlers;
pub mod routes;

// Re-export main entry point
pub use routes::models_routes;

// Re-export handler types for OpenAPI documentation
pub use handlers::{
    AllModelsStatusResponse, AneMemoryStatus, DownloadProgressResponse, ImportModelRequest,
    ImportModelResponse, ModelArchitectureSummary, ModelDownloadProgress, ModelListResponse,
    ModelRuntimeHealthResponse, ModelStatusResponse, ModelValidationResponse,
    ModelWithStatsResponse, SeedModelRequest, SeedModelResponse, ValidationIssue,
};

// Re-export handlers for direct use if needed
pub use handlers::{
    get_all_models_status, get_download_progress, get_model_status, import_model,
    list_models_with_stats, load_model, unload_model, validate_model,
};
