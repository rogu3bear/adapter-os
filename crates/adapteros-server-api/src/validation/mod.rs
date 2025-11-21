//! Validation modules for API request and response validation

pub mod response_schemas;

pub use response_schemas::{
    ResponseSchema, ResponseSchemaValidator, SharedResponseValidator, ValidationResult,
    ResponseValidationMiddleware,
};

use crate::types::ErrorResponse;
use axum::{http::StatusCode, Json};

/// Validates a repository ID format
/// Must be non-empty, alphanumeric with slashes, hyphens, underscores
pub fn validate_repo_id(repo_id: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if repo_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("repo_id cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    if repo_id.len() > 256 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("repo_id too long (max 256 chars)").with_code("VALIDATION_ERROR")),
        ));
    }
    // Allow alphanumeric, slashes, hyphens, underscores, dots
    if !repo_id.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("repo_id contains invalid characters").with_code("VALIDATION_ERROR")),
        ));
    }
    Ok(())
}

/// Validates a description field
/// Must be reasonable length and not contain dangerous content
pub fn validate_description(description: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if description.len() > 10000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("description too long (max 10000 chars)").with_code("VALIDATION_ERROR")),
        ));
    }
    Ok(())
}

/// Validates file paths array
/// Each path must be valid and safe (no path traversal)
pub fn validate_file_paths(paths: &[String]) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if paths.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("target_files cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    if paths.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("too many target files (max 100)").with_code("VALIDATION_ERROR")),
        ));
    }
    for path in paths {
        if path.contains("..") {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("path traversal not allowed").with_code("VALIDATION_ERROR")),
            ));
        }
        if path.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("empty path not allowed").with_code("VALIDATION_ERROR")),
            ));
        }
    }
    Ok(())
}

/// Validates an adapter ID format
/// Must be alphanumeric with hyphens, underscores, slashes
pub fn validate_adapter_id(adapter_id: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if adapter_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("adapter_id cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    if adapter_id.len() > 128 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("adapter_id too long (max 128 chars)").with_code("VALIDATION_ERROR")),
        ));
    }
    // Allow alphanumeric, hyphens, underscores, slashes (for semantic naming)
    if !adapter_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("adapter_id contains invalid characters").with_code("VALIDATION_ERROR")),
        ));
    }
    Ok(())
}

/// Validates a name field
/// Must be non-empty and reasonable length
pub fn validate_name(name: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    if name.len() > 256 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("name too long (max 256 chars)").with_code("VALIDATION_ERROR")),
        ));
    }
    Ok(())
}

/// Validates a BLAKE3 hash format
/// Must be 64 hex characters (256-bit hash)
pub fn validate_hash_b3(hash: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if hash.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("hash_b3 cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    // BLAKE3 produces 256-bit (32-byte) hashes, which is 64 hex chars
    if hash.len() != 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("hash_b3 must be 64 hex characters").with_code("VALIDATION_ERROR")),
        ));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("hash_b3 must contain only hex characters").with_code("VALIDATION_ERROR")),
        ));
    }
    Ok(())
}
