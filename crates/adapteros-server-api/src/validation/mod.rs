//! Validation modules for API request and response validation

pub mod response_schemas;

pub use response_schemas::{
    ResponseSchema, ResponseSchemaValidator, ResponseValidationMiddleware, SharedResponseValidator,
    ValidationResult,
};

use crate::types::ErrorResponse;
use adapteros_core::validation as core_validation;
use adapteros_core::AosError;
use axum::{http::StatusCode, Json};

fn map_validation_error(error: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let message = match error {
        AosError::Validation(message) => message,
        other => other.to_string(),
    };

    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(&message).with_code("VALIDATION_ERROR")),
    )
}

/// Validates a repository ID format
/// Delegates to core validation rules.
pub fn validate_repo_id(repo_id: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    core_validation::validate_repo_id(repo_id).map_err(map_validation_error)
}

/// Validates a description field
/// Must be reasonable length and not contain dangerous content
pub fn validate_description(description: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if description.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Description cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }
    if description.len() > 10000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(
                    "Invalid description: exceeds maximum length of 10000 characters",
                )
                .with_code("VALIDATION_ERROR"),
            ),
        ));
    }
    Ok(())
}

/// Validates file paths array
/// Delegates to core validation rules.
pub fn validate_file_paths(paths: &[String]) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    core_validation::validate_file_paths(paths).map_err(map_validation_error)
}

/// Validates an adapter ID format
/// Delegates to core validation rules.
pub fn validate_adapter_id(adapter_id: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    core_validation::validate_adapter_id(adapter_id).map_err(map_validation_error)
}

/// Validates a name field
/// Delegates to core validation rules.
pub fn validate_name(name: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    core_validation::validate_name(name).map_err(map_validation_error)
}

/// Validates a BLAKE3 hash format
/// Delegates to core validation rules.
pub fn validate_hash_b3(hash: &str) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if hash.starts_with("b3:") {
        return core_validation::validate_hash_b3(hash).map_err(map_validation_error);
    }

    let prefixed = format!("b3:{}", hash);
    core_validation::validate_hash_b3(&prefixed).map_err(map_validation_error)
}
