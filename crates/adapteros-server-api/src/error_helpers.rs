// TODO: Migrate callers to ApiError::* methods and remove these helpers
// Tracking: POST-BETA-CLEANUP

//! Standardized error response helpers for API handlers
//!
//! **DEPRECATED**: This module uses the legacy tuple pattern `(StatusCode, Json<ErrorResponse>)`.
//! Use `crate::api_error::ApiError` instead for all new code.
//!
//! # Migration
//! ```ignore
//! // Old (deprecated):
//! use crate::error_helpers::{ApiResult, db_error, not_found};
//! pub async fn handler() -> ApiResult<Response> { ... }
//!
//! // New (preferred):
//! use crate::api_error::{ApiError, ApiResult};
//! pub async fn handler() -> ApiResult<Response> { ... }
//! ```
//!
//! See `crate::api_error` for the new API.

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
        Json(ErrorResponse::new(e.to_string()).with_code("DATABASE_ERROR")),
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
        Json(ErrorResponse::new(format!("{} not found", resource)).with_code("NOT_FOUND")),
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
        Json(ErrorResponse::new(e.to_string()).with_code("BAD_REQUEST")),
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
        Json(ErrorResponse::new(e.to_string()).with_code("INTERNAL_ERROR")),
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

/// Database error handler with custom message - logs the error and returns a 500 response
///
/// Similar to `db_error` but allows customizing the error message shown to the user.
///
/// # Example
/// ```ignore
/// state.db.create_tenant(&name).await.map_err(|e| db_error_msg("failed to create tenant", e))?;
/// ```
pub fn db_error_msg<E: std::fmt::Display>(msg: &str, e: E) -> (StatusCode, Json<ErrorResponse>) {
    error!("Database error ({}): {}", msg, e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            ErrorResponse::new(msg)
                .with_code("INTERNAL_SERVER_ERROR")
                .with_string_details(e.to_string()),
        ),
    )
}

/// Standard database error with "database error" message and details
///
/// This matches the most common pattern in the codebase of using "database error" as the message.
///
/// # Example
/// ```ignore
/// state.db.get_tenant(&id).await.map_err(db_error_with_details)?;
/// ```
pub fn db_error_with_details<E: std::fmt::Display>(e: E) -> (StatusCode, Json<ErrorResponse>) {
    db_error_msg("database error", e)
}

/// Not found error with custom details
///
/// # Example
/// ```ignore
/// node.ok_or_else(|| not_found_with_details("node not found", format!("Node ID: {}", id)))?;
/// ```
pub fn not_found_with_details(msg: &str, details: String) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(
            ErrorResponse::new(msg)
                .with_code("NOT_FOUND")
                .with_string_details(details),
        ),
    )
}

/// Bad gateway error - returns a 502 response
///
/// Used when a downstream service fails or returns an error.
///
/// # Example
/// ```ignore
/// client.post(&url).send().await.map_err(|e| bad_gateway("failed to contact node agent", e))?;
/// ```
pub fn bad_gateway<E: std::fmt::Display>(msg: &str, e: E) -> (StatusCode, Json<ErrorResponse>) {
    error!("Bad gateway ({}): {}", msg, e);
    (
        StatusCode::BAD_GATEWAY,
        Json(
            ErrorResponse::new(msg)
                .with_code("BAD_GATEWAY")
                .with_string_details(e.to_string()),
        ),
    )
}

/// Internal error with custom message and details
///
/// # Example
/// ```ignore
/// response.json().await.map_err(|e| internal_error_msg("failed to parse response", e))?;
/// ```
pub fn internal_error_msg<E: std::fmt::Display>(
    msg: &str,
    e: E,
) -> (StatusCode, Json<ErrorResponse>) {
    error!("Internal error ({}): {}", msg, e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            ErrorResponse::new(msg)
                .with_code("INTERNAL_SERVER_ERROR")
                .with_string_details(e.to_string()),
        ),
    )
}

/// Service unavailable error - returns a 503 response
///
/// Used when a required service is temporarily unavailable.
///
/// # Example
/// ```ignore
/// if !service.is_ready() { return Err(service_unavailable("Worker service is starting up")); }
/// ```
pub fn service_unavailable(msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse::new(msg).with_code("SERVICE_UNAVAILABLE")),
    )
}

// ============================================================================
// Audit-aware error helpers
// ============================================================================
//
// These helpers combine error creation with audit logging for critical operations.
// Use these when failures should be persisted to the audit_logs table.

use crate::audit_helper::{log_failure_or_warn, resources};
use crate::auth::Claims;
use adapteros_db::Db;

/// Context for audit-aware error handling
///
/// Holds references needed to log failures to the audit system.
/// Create once at the start of a handler and use for all error handling.
///
/// # Example
/// ```ignore
/// let audit = AuditContext::new(&state.db, &claims, actions::ADAPTER_REGISTER, resources::ADAPTER);
///
/// let adapter = audit.on_error(
///     state.db.get_adapter(&id).await,
///     db_error,
///     Some(&id),
/// ).await?;
/// ```
pub struct AuditContext<'a> {
    pub db: &'a Db,
    pub claims: &'a Claims,
    pub action: &'static str,
    pub resource_type: &'static str,
}

impl<'a> AuditContext<'a> {
    /// Create a new audit context
    pub fn new(
        db: &'a Db,
        claims: &'a Claims,
        action: &'static str,
        resource_type: &'static str,
    ) -> Self {
        Self {
            db,
            claims,
            action,
            resource_type,
        }
    }

    /// Handle a result, logging failures to audit before converting to API error
    ///
    /// # Example
    /// ```ignore
    /// let result = audit.on_error(
    ///     state.db.get_adapter(&id).await,
    ///     db_error,
    ///     Some(&id),
    /// ).await?;
    /// ```
    pub async fn on_error<T, E: std::fmt::Display>(
        &self,
        result: Result<T, E>,
        error_converter: impl FnOnce(E) -> (StatusCode, Json<ErrorResponse>),
        resource_id: Option<&str>,
    ) -> Result<T, (StatusCode, Json<ErrorResponse>)> {
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let error_msg = e.to_string();
                log_failure_or_warn(
                    self.db,
                    self.claims,
                    self.action,
                    self.resource_type,
                    resource_id,
                    &error_msg,
                )
                .await;
                Err(error_converter(e))
            }
        }
    }

    /// Log a failure and return an error (for cases where you construct the error manually)
    ///
    /// # Example
    /// ```ignore
    /// if req.name.is_empty() {
    ///     return Err(audit.fail(bad_request("name is required"), None).await);
    /// }
    /// ```
    pub async fn fail(
        &self,
        error: (StatusCode, Json<ErrorResponse>),
        resource_id: Option<&str>,
    ) -> (StatusCode, Json<ErrorResponse>) {
        let error_msg = &error.1 .0.error;
        log_failure_or_warn(
            self.db,
            self.claims,
            self.action,
            self.resource_type,
            resource_id,
            error_msg,
        )
        .await;
        error
    }

    /// Log a failure with a custom message and return an error
    ///
    /// # Example
    /// ```ignore
    /// return Err(audit.fail_with_msg("validation failed", bad_request("invalid input"), None).await);
    /// ```
    pub async fn fail_with_msg(
        &self,
        msg: &str,
        error: (StatusCode, Json<ErrorResponse>),
        resource_id: Option<&str>,
    ) -> (StatusCode, Json<ErrorResponse>) {
        log_failure_or_warn(
            self.db,
            self.claims,
            self.action,
            self.resource_type,
            resource_id,
            msg,
        )
        .await;
        error
    }

    /// Log success (convenience method)
    pub async fn success(&self, resource_id: Option<&str>) {
        crate::audit_helper::log_success_or_warn(
            self.db,
            self.claims,
            self.action,
            self.resource_type,
            resource_id,
        )
        .await;
    }
}

/// Adapter-specific audit context (common case)
pub fn adapter_audit<'a>(db: &'a Db, claims: &'a Claims, action: &'static str) -> AuditContext<'a> {
    AuditContext::new(db, claims, action, resources::ADAPTER)
}

/// Training-specific audit context
pub fn training_audit<'a>(
    db: &'a Db,
    claims: &'a Claims,
    action: &'static str,
) -> AuditContext<'a> {
    AuditContext::new(db, claims, action, resources::TRAINING_JOB)
}
