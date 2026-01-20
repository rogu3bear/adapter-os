//! Common API request/response patterns
//!
//! This module contains standard API types that are framework-agnostic:
//! - ErrorResponse
//! - HealthResponse
//! - Common request patterns
//!
//! NOTE: The canonical ErrorResponse is in adapteros-api-types.
//! This module re-exports a compatible type for backwards compatibility.

use serde::{Deserialize, Serialize};

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
