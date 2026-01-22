//! Authentication error types.
//!
//! All auth errors map cleanly to HTTP status codes via the
//! [`AuthError::status_code`] method.

use axum::http::StatusCode;
use thiserror::Error;

/// Result type alias for auth operations.
pub type AuthResult<T> = Result<T, AuthError>;

/// Authentication and authorization errors.
///
/// Each variant maps to a specific HTTP status code and error code
/// for consistent API responses.
#[derive(Debug, Error)]
pub enum AuthError {
    // =========================================================================
    // 401 Unauthorized - Authentication required or failed
    // =========================================================================
    /// No token was provided in the request.
    #[error("authentication token is missing")]
    TokenMissing,

    /// Token format is invalid (malformed JWT, wrong prefix, etc.).
    #[error("authentication token format is invalid: {0}")]
    TokenInvalid(String),

    /// Token signature verification failed.
    #[error("token signature is invalid")]
    TokenSignatureInvalid,

    /// Token has expired.
    #[error("token has expired")]
    TokenExpired,

    /// Token was revoked (explicit revocation via jti).
    #[error("token has been revoked")]
    TokenRevoked,

    /// Token issuer doesn't match expected value.
    #[error("invalid token issuer")]
    InvalidIssuer,

    /// Token audience doesn't match expected value.
    #[error("invalid token audience")]
    InvalidAudience,

    /// API key is invalid or not found.
    #[error("invalid API key")]
    InvalidApiKey,

    /// Session not found or expired.
    #[error("session expired or not found")]
    SessionExpired,

    /// Session was locked (e.g., due to suspicious activity).
    #[error("session is locked")]
    SessionLocked,

    /// Device ID in token doesn't match session device.
    #[error("device mismatch")]
    DeviceMismatch,

    /// Credentials are invalid (username/password).
    #[error("invalid credentials")]
    InvalidCredentials,

    // =========================================================================
    // 403 Forbidden - Authenticated but not authorized
    // =========================================================================
    /// User lacks permission for this action.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Tenant isolation violation - cross-tenant access attempted.
    #[error("tenant isolation violation")]
    TenantIsolation,

    /// CSRF validation failed.
    #[error("CSRF token validation failed")]
    CsrfError,

    /// Role-based access control rejection.
    #[error("insufficient role permissions: {0}")]
    InsufficientRole(String),

    /// MFA required but not completed.
    #[error("multi-factor authentication required")]
    MfaRequired,

    // =========================================================================
    // 400 Bad Request - Invalid request format
    // =========================================================================
    /// Missing required field in request.
    #[error("missing required field: {0}")]
    MissingField(String),

    /// Invalid tenant ID format.
    #[error("invalid tenant ID format")]
    InvalidTenantId,

    /// Invalid session ID format.
    #[error("invalid session ID format")]
    InvalidSessionId,

    // =========================================================================
    // 500 Internal Server Error - System failures
    // =========================================================================
    /// Database error during auth operation.
    #[error("database error: {0}")]
    DatabaseError(String),

    /// Key derivation or crypto failure.
    #[error("cryptographic error: {0}")]
    CryptoError(String),

    /// Configuration error.
    #[error("configuration error: {0}")]
    ConfigError(String),

    /// Internal error (unexpected state).
    #[error("internal error: {0}")]
    InternalError(String),

    // =========================================================================
    // Boot-time errors
    // =========================================================================
    /// Dev bypass requested in release build (boot-time fatal).
    #[error("dev bypass mode is not allowed in release builds")]
    DevBypassInRelease,

    /// JWT mode requires key configuration (boot-time fatal).
    #[error("JWT mode requires issuer, audience, and key configuration")]
    JwtModeNotConfigured,

    /// API key mode requires at least one key (boot-time fatal).
    #[error("API key mode requires at least one key configured")]
    ApiKeyModeNotConfigured,
}

impl AuthError {
    /// Returns the HTTP status code for this error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            // 401 Unauthorized
            AuthError::TokenMissing
            | AuthError::TokenInvalid(_)
            | AuthError::TokenSignatureInvalid
            | AuthError::TokenExpired
            | AuthError::TokenRevoked
            | AuthError::InvalidIssuer
            | AuthError::InvalidAudience
            | AuthError::InvalidApiKey
            | AuthError::SessionExpired
            | AuthError::SessionLocked
            | AuthError::DeviceMismatch
            | AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,

            // 403 Forbidden
            AuthError::PermissionDenied(_)
            | AuthError::TenantIsolation
            | AuthError::CsrfError
            | AuthError::InsufficientRole(_)
            | AuthError::MfaRequired => StatusCode::FORBIDDEN,

            // 400 Bad Request
            AuthError::MissingField(_)
            | AuthError::InvalidTenantId
            | AuthError::InvalidSessionId => StatusCode::BAD_REQUEST,

            // 500 Internal Server Error
            AuthError::DatabaseError(_)
            | AuthError::CryptoError(_)
            | AuthError::ConfigError(_)
            | AuthError::InternalError(_)
            | AuthError::DevBypassInRelease
            | AuthError::JwtModeNotConfigured
            | AuthError::ApiKeyModeNotConfigured => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Returns the error code for API responses.
    pub fn error_code(&self) -> &'static str {
        match self {
            AuthError::TokenMissing => "TOKEN_MISSING",
            AuthError::TokenInvalid(_) => "TOKEN_INVALID",
            AuthError::TokenSignatureInvalid => "TOKEN_SIGNATURE_INVALID",
            AuthError::TokenExpired => "TOKEN_EXPIRED",
            AuthError::TokenRevoked => "TOKEN_REVOKED",
            AuthError::InvalidIssuer => "INVALID_ISSUER",
            AuthError::InvalidAudience => "INVALID_AUDIENCE",
            AuthError::InvalidApiKey => "INVALID_API_KEY",
            AuthError::SessionExpired => "SESSION_EXPIRED",
            AuthError::SessionLocked => "SESSION_LOCKED",
            AuthError::DeviceMismatch => "DEVICE_MISMATCH",
            AuthError::InvalidCredentials => "INVALID_CREDENTIALS",
            AuthError::PermissionDenied(_) => "PERMISSION_DENIED",
            AuthError::TenantIsolation => "TENANT_ISOLATION_ERROR",
            AuthError::CsrfError => "CSRF_ERROR",
            AuthError::InsufficientRole(_) => "INSUFFICIENT_ROLE",
            AuthError::MfaRequired => "MFA_REQUIRED",
            AuthError::MissingField(_) => "MISSING_FIELD",
            AuthError::InvalidTenantId => "INVALID_TENANT_ID",
            AuthError::InvalidSessionId => "INVALID_SESSION_ID",
            AuthError::DatabaseError(_) => "DATABASE_ERROR",
            AuthError::CryptoError(_) => "CRYPTO_ERROR",
            AuthError::ConfigError(_) => "CONFIG_ERROR",
            AuthError::InternalError(_) => "INTERNAL_ERROR",
            AuthError::DevBypassInRelease => "DEV_BYPASS_IN_RELEASE",
            AuthError::JwtModeNotConfigured => "JWT_MODE_NOT_CONFIGURED",
            AuthError::ApiKeyModeNotConfigured => "API_KEY_MODE_NOT_CONFIGURED",
        }
    }

    /// Returns true if this error is a boot-time fatal error.
    pub fn is_boot_fatal(&self) -> bool {
        matches!(
            self,
            AuthError::DevBypassInRelease
                | AuthError::JwtModeNotConfigured
                | AuthError::ApiKeyModeNotConfigured
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_codes() {
        assert_eq!(
            AuthError::TokenMissing.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AuthError::TokenExpired.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AuthError::PermissionDenied("test".into()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AuthError::TenantIsolation.status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AuthError::MissingField("test".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AuthError::DatabaseError("test".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(AuthError::TokenMissing.error_code(), "TOKEN_MISSING");
        assert_eq!(
            AuthError::TenantIsolation.error_code(),
            "TENANT_ISOLATION_ERROR"
        );
    }

    #[test]
    fn test_boot_fatal() {
        assert!(AuthError::DevBypassInRelease.is_boot_fatal());
        assert!(AuthError::JwtModeNotConfigured.is_boot_fatal());
        assert!(!AuthError::TokenMissing.is_boot_fatal());
    }
}
