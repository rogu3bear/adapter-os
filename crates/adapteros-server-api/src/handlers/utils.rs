//! Utility functions for handlers

use crate::api_error::ApiError;
use crate::types::ErrorResponse;
use adapteros_core::AosError;
use axum::{http::StatusCode, Json};

/// Utility function to convert AosError to axum response format
/// This ensures consistent error handling across all handlers
pub fn aos_error_to_response(error: AosError) -> (StatusCode, Json<ErrorResponse>) {
    let api_error: ApiError = error.into();
    api_error.into()
}
