//! Utility functions for handlers

use crate::types::ErrorResponse;
use adapteros_core::AosError;
use axum::{http::StatusCode, Json};

/// Utility function to convert AosError to axum response format
/// This ensures consistent error handling across all handlers
pub fn aos_error_to_response(error: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let (status_code, error_code) = match &error {
        AosError::Auth(_) => (StatusCode::UNAUTHORIZED, "AUTHENTICATION_ERROR"),
        AosError::Authz(_) => (StatusCode::FORBIDDEN, "AUTHORIZATION_ERROR"),
        AosError::Database(_) | AosError::Sqlx(_) | AosError::Sqlite(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR")
        }
        AosError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        AosError::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
        AosError::PolicyViolation(_) => (StatusCode::FORBIDDEN, "POLICY_VIOLATION"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
    };

    (
        status_code,
        Json(ErrorResponse::new(error.to_string()).with_code(error_code)),
    )
}
