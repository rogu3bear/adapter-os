//! Shared API types for AdapterOS Control Plane
//!
//! This crate provides unified request/response types used across
//! the control plane API, client libraries, and UI components.

pub mod adapters;
pub mod auth;
pub mod domain_adapters;
pub mod git;
pub mod inference;
pub mod metrics;
pub mod nodes;
pub mod plans;
pub mod repositories;
pub mod telemetry;
pub mod tenants;
pub mod training;
pub mod workers;

// Re-export commonly used types
pub use adapters::*;
pub use auth::*;
pub use domain_adapters::*;
pub use git::*;
pub use inference::*;
pub use metrics::*;
pub use nodes::*;
pub use plans::*;
pub use repositories::*;
pub use telemetry::*;
pub use tenants::*;
pub use training::*;
pub use workers::*;

/// Common error response structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub code: Option<String>,
}

impl ErrorResponse {
    /// Create a new error response
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: String::new(),
            code: Some("INTERNAL_ERROR".to_string()),
        }
    }

    /// Set the error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set the error message/details
    pub fn with_string_details(mut self, details: impl Into<String>) -> Self {
        self.message = details.into();
        self
    }

    /// Set the error message/details from serde_json::Value
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.message = details.to_string();
        self
    }
}

/// Health check response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
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
