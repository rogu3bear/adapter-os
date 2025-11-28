//! Standardized error response helpers for API handlers
//!
//! Provides unified error conversion functions to ensure consistent error handling
//! across all API endpoints. All handlers should use these helpers to return errors
//! in the standard format: `Result<Json<T>, (StatusCode, Json<ErrorResponse>)>`
//!
//! # Usage
//! ```ignore
//! use crate::error_helpers::{ApiResult, db_error, not_found, bad_request};
//!
//! pub async fn my_handler(
//!     State(state): State<AppState>,
//! ) -> ApiResult<MyResponse> {
//!     let data = state.db.get_data().await.map_err(db_error)?;
//!     let item = data.ok_or_else(|| not_found("Item"))?;
//!     Ok(Json(MyResponse { ... }))
//! }
//! ```

use crate::types::ErrorResponse;
use axum::{http::StatusCode, Json};
use tracing::error;

/// Standard API result type - all handlers should use this
pub type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

/// Database error handler - logs the error and returns a 500 response
///
/// # Example
/// ```ignore
/// state.db.get_adapter(&id).await.map_err(db_error)?;
/// ```
pub fn db_error<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    error!("Database error: {}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(&e.to_string()).with_code("DATABASE_ERROR")),
    )
}

/// Not found error handler - returns a 404 response
///
/// # Example
/// ```ignore
/// let adapter = adapters.get(&id).ok_or_else(|| not_found("Adapter"))?;
/// ```
pub fn not_found(resource: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new(&format!("{} not found", resource)).with_code("NOT_FOUND"),
        ),
    )
}

/// Bad request error handler - returns a 400 response
///
/// # Example
/// ```ignore
/// validate_input(&req).map_err(bad_request)?;
/// ```
pub fn bad_request<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(&e.to_string()).with_code("BAD_REQUEST")),
    )
}

/// Internal error handler - logs the error and returns a 500 response
///
/// # Example
/// ```ignore
/// process_data(&input).map_err(internal_error)?;
/// ```
pub fn internal_error<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    error!("Internal error: {}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(&e.to_string()).with_code("INTERNAL_ERROR")),
    )
}

/// Unauthorized error handler - returns a 401 response
///
/// # Example
/// ```ignore
/// verify_token(&token).ok_or_else(|| unauthorized("Invalid token"))?;
/// ```
pub fn unauthorized(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new(msg).with_code("UNAUTHORIZED")),
    )
}

/// Forbidden error handler - returns a 403 response
///
/// # Example
/// ```ignore
/// check_permission(&user).ok_or_else(|| forbidden("Insufficient permissions"))?;
/// ```
pub fn forbidden(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(msg).with_code("FORBIDDEN")),
    )
}

/// Conflict error handler - returns a 409 response
///
/// # Example
/// ```ignore
/// if exists { return Err(conflict("Resource already exists")); }
/// ```
pub fn conflict(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse::new(msg).with_code("CONFLICT")),
    )
}

/// Payload too large error handler - returns a 413 response
///
/// # Example
/// ```ignore
/// if size > MAX_SIZE { return Err(payload_too_large("File exceeds maximum size")); }
/// ```
pub fn payload_too_large(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrorResponse::new(msg).with_code("PAYLOAD_TOO_LARGE")),
    )
}

/// Not implemented error handler - returns a 501 response
///
/// Used for feature-gated functionality that requires optional features to be enabled.
///
/// # Example
/// ```ignore
/// #[cfg(not(feature = "embeddings"))]
/// pub async fn process_document(...) -> ApiResult<Response> {
///     Err(not_implemented("Document processing requires the 'embeddings' feature"))
/// }
/// ```
pub fn not_implemented(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new(msg).with_code("FEATURE_DISABLED")),
    )
}
