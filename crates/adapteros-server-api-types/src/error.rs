//! API error types for server-api crates
//!
//! This module provides a unified error type that can be used across
//! all server-api handler crates.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// API error type for server handlers.
///
/// This is a placeholder that will be populated with specific error variants
/// as types are migrated from the main server-api crate.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Internal server error
    #[error("internal server error: {0}")]
    Internal(String),

    /// Resource not found
    #[error("not found: {0}")]
    NotFound(String),

    /// Bad request with validation details
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Authentication required
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Permission denied
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Conflict with existing resource
    #[error("conflict: {0}")]
    Conflict(String),

    /// Service unavailable
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl ApiError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// Get a machine-readable error code
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::Conflict(_) => "CONFLICT",
            Self::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

/// Serializable error response body
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Machine-readable error code
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse {
            code: self.error_code().to_string(),
            message: self.to_string(),
            details: None,
        };

        let json = serde_json::to_string(&body).unwrap_or_else(|_| {
            r#"{"code":"INTERNAL_ERROR","message":"failed to serialize error"}"#.to_string()
        });

        (status, [("content-type", "application/json")], json).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            ApiError::Internal("test".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ApiError::NotFound("test".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::BadRequest("test".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::Unauthorized("test".into()).status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ApiError::Forbidden("test".into()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ApiError::Conflict("test".into()).status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            ApiError::ServiceUnavailable("test".into()).status_code(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            ApiError::Internal("test".into()).error_code(),
            "INTERNAL_ERROR"
        );
        assert_eq!(ApiError::NotFound("test".into()).error_code(), "NOT_FOUND");
        assert_eq!(
            ApiError::BadRequest("test".into()).error_code(),
            "BAD_REQUEST"
        );
        assert_eq!(
            ApiError::Unauthorized("test".into()).error_code(),
            "UNAUTHORIZED"
        );
        assert_eq!(ApiError::Forbidden("test".into()).error_code(), "FORBIDDEN");
        assert_eq!(ApiError::Conflict("test".into()).error_code(), "CONFLICT");
        assert_eq!(
            ApiError::ServiceUnavailable("test".into()).error_code(),
            "SERVICE_UNAVAILABLE"
        );
    }

    #[test]
    fn test_error_display() {
        let err = ApiError::Internal("database connection failed".to_string());
        assert_eq!(
            err.to_string(),
            "internal server error: database connection failed"
        );

        let err = ApiError::NotFound("resource not found".to_string());
        assert_eq!(err.to_string(), "not found: resource not found");

        let err = ApiError::BadRequest("invalid input".to_string());
        assert_eq!(err.to_string(), "bad request: invalid input");

        let err = ApiError::Unauthorized("missing token".to_string());
        assert_eq!(err.to_string(), "unauthorized: missing token");

        let err = ApiError::Forbidden("insufficient permissions".to_string());
        assert_eq!(err.to_string(), "forbidden: insufficient permissions");

        let err = ApiError::Conflict("resource already exists".to_string());
        assert_eq!(err.to_string(), "conflict: resource already exists");

        let err = ApiError::ServiceUnavailable("maintenance mode".to_string());
        assert_eq!(err.to_string(), "service unavailable: maintenance mode");
    }

    #[test]
    fn test_error_response_serialize() {
        let err_response = ErrorResponse {
            code: "TEST_ERROR".to_string(),
            message: "test message".to_string(),
            details: None,
        };

        let json = serde_json::to_string(&err_response).unwrap();
        assert!(json.contains(r#""code":"TEST_ERROR""#));
        assert!(json.contains(r#""message":"test message""#));
        assert!(!json.contains("details")); // Should skip None
    }

    #[test]
    fn test_error_response_serialize_with_details() {
        let details = serde_json::json!({
            "field": "email",
            "reason": "invalid format"
        });

        let err_response = ErrorResponse {
            code: "VALIDATION_ERROR".to_string(),
            message: "validation failed".to_string(),
            details: Some(details),
        };

        let json = serde_json::to_string(&err_response).unwrap();
        assert!(json.contains(r#""code":"VALIDATION_ERROR""#));
        assert!(json.contains(r#""message":"validation failed""#));
        assert!(json.contains(r#""field":"email""#));
        assert!(json.contains(r#""reason":"invalid format""#));
    }

    #[test]
    fn test_error_response_deserialize() {
        let json = r#"{"code":"NOT_FOUND","message":"resource not found"}"#;
        let err_response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err_response.code, "NOT_FOUND");
        assert_eq!(err_response.message, "resource not found");
        assert!(err_response.details.is_none());
    }

    #[test]
    fn test_error_response_deserialize_with_details() {
        let json = r#"{
            "code":"VALIDATION_ERROR",
            "message":"validation failed",
            "details":{"field":"email","reason":"invalid format"}
        }"#;
        let err_response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(err_response.code, "VALIDATION_ERROR");
        assert_eq!(err_response.message, "validation failed");
        assert!(err_response.details.is_some());

        let details = err_response.details.unwrap();
        assert_eq!(details["field"], "email");
        assert_eq!(details["reason"], "invalid format");
    }

    #[test]
    fn test_api_error_into_response() {
        let err = ApiError::NotFound("resource not found".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_all_error_variants_into_response() {
        let errors = vec![
            (
                ApiError::Internal("internal error".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                ApiError::NotFound("not found".to_string()),
                StatusCode::NOT_FOUND,
            ),
            (
                ApiError::BadRequest("bad request".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (
                ApiError::Unauthorized("unauthorized".to_string()),
                StatusCode::UNAUTHORIZED,
            ),
            (
                ApiError::Forbidden("forbidden".to_string()),
                StatusCode::FORBIDDEN,
            ),
            (
                ApiError::Conflict("conflict".to_string()),
                StatusCode::CONFLICT,
            ),
            (
                ApiError::ServiceUnavailable("unavailable".to_string()),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ];

        for (err, expected_status) in errors {
            let response = err.into_response();
            assert_eq!(response.status(), expected_status);
        }
    }

    #[test]
    fn test_error_debug_format() {
        let err = ApiError::Internal("debug test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Internal"));
        assert!(debug_str.contains("debug test"));
    }

    // Edge cases
    #[test]
    fn test_error_with_empty_message() {
        let err = ApiError::Internal("".to_string());
        assert_eq!(err.to_string(), "internal server error: ");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.error_code(), "INTERNAL_ERROR");
    }

    #[test]
    fn test_error_with_special_characters() {
        let err = ApiError::BadRequest("invalid json: unexpected token '}'".to_string());
        assert_eq!(
            err.to_string(),
            "bad request: invalid json: unexpected token '}'"
        );
    }

    #[test]
    fn test_error_response_roundtrip() {
        let original = ErrorResponse {
            code: "TEST_CODE".to_string(),
            message: "test message".to_string(),
            details: Some(serde_json::json!({
                "key": "value",
                "count": 42
            })),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ErrorResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.code, original.code);
        assert_eq!(parsed.message, original.message);
        assert_eq!(parsed.details, original.details);
    }
}
