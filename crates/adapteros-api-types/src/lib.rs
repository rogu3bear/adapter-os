//! Shared API types for AdapterOS Control Plane
//!
//! This crate provides unified request/response types used across
//! the control plane API, client libraries, and UI components.

pub mod adapters;
pub mod auth;
pub mod dashboard;
pub mod domain_adapters;
pub mod git;
pub mod inference;
pub mod metrics;
pub mod nodes;
pub mod openai;
pub mod plans;
pub mod repositories;
pub mod telemetry;
pub mod tenants;
pub mod training;
pub mod workers;

// Re-export commonly used types
pub use adapters::*;
pub use auth::*;
pub use dashboard::*;
pub use domain_adapters::*;
pub use git::*;
pub use inference::*;
pub use metrics::*;
pub use nodes::*;
pub use openai::*;
pub use plans::*;
pub use repositories::*;
// Note: telemetry types are not re-exported to avoid conflicts with metrics types
pub use tenants::*;
pub use training::*;
pub use workers::*;

/// Common error response structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(default)]
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: "INTERNAL_ERROR".to_string(),
            details: None,
        }
    }

    /// Set the error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = code.into();
        self
    }

    /// Set the error details
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Set the error details from string
    pub fn with_string_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(serde_json::json!(details.into()));
        self
    }

    /// Create an error response with user-friendly message mapping
    /// Note: This method requires access to UserFriendlyErrorMapper from server-api crate
    /// For unified API types, use ErrorResponse::new() and map messages at the server level
    pub fn with_user_friendly_message(mut self, user_friendly_msg: impl Into<String>) -> Self {
        self.error = user_friendly_msg.into();
        self
    }
}

/// Health check response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    /// Model runtime health information
    pub models: Option<ModelRuntimeHealth>,
}

/// Model runtime health summary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ModelRuntimeHealth {
    pub total_models: i32,
    pub loaded_count: i32,
    pub healthy: bool,
    pub inconsistencies_count: usize,
}

/// Pagination parameters
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    50
}

/// Paginated response wrapper
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub pages: u32,
}
