//! Error handling utilities for HTTP responses
//! 【2025-01-27†refactor(server)†extract-error-handling】
//!
//! Extracted from handlers.rs to standardize error response formatting.

use adapteros_core::AosError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::warn;

/// Convert database error to HTTP response
/// 【2025-01-27†refactor(server)†extract-error-handling】
pub fn db_error_to_response(e: AosError) -> Response {
    warn!(error = %e, "DB error response");
    let body = json!({ "error": e.to_string() });
    (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
}

/// Create validation error response
/// 【2025-01-27†refactor(server)†extract-error-handling】
pub fn validation_error(msg: &str) -> Response {
    let body = json!({ "error": "Validation failed", "detail": msg });
    (StatusCode::BAD_REQUEST, body).into_response()
}
