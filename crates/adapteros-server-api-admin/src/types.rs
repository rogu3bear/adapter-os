//! Type definitions for admin handlers
//!
//! Re-exports `ErrorResponse` from `adapteros-api-types` as the canonical error type.
//! `AdminErrorResponse` is preserved as a type alias for backward compatibility.

pub use adapteros_api_types::ErrorResponse;

/// Backward-compatible alias for admin error responses.
///
/// Prefer using `ErrorResponse` directly in new code.
pub type AdminErrorResponse = ErrorResponse;
