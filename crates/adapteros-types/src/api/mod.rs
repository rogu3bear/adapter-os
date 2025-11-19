//! Common API request/response patterns
//!
//! This module contains standard API types that are framework-agnostic:
//! - ErrorResponse
//! - HealthResponse
//! - Common request patterns

use serde::{Deserialize, Serialize};

/// Common error response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorResponse {
    /// Error message
    pub error: String,

    /// Error code (e.g., "NOT_FOUND", "INTERNAL_ERROR")
    #[serde(default)]
    pub code: String,

    /// Additional error details (optional)
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
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthResponse {
    /// Health status ("healthy", "degraded", "unhealthy")
    pub status: String,

    /// Service version
    pub version: String,
}

impl HealthResponse {
    /// Create a healthy response
    pub fn healthy(version: impl Into<String>) -> Self {
        Self {
            status: "healthy".to_string(),
            version: version.into(),
        }
    }

    /// Create a degraded response
    pub fn degraded(version: impl Into<String>) -> Self {
        Self {
            status: "degraded".to_string(),
            version: version.into(),
        }
    }

    /// Create an unhealthy response
    pub fn unhealthy(version: impl Into<String>) -> Self {
        Self {
            status: "unhealthy".to_string(),
            version: version.into(),
        }
    }
}
