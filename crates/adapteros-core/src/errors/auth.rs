//! Authentication and authorization errors
//!
//! Covers authentication failures and permission denials.

use thiserror::Error;

/// Authentication and authorization errors
#[derive(Error, Debug)]
pub enum AosAuthError {
    /// Authentication failure (identity verification failed)
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Authorization failure (permission denied)
    #[error("Authorization error: {0}")]
    Authorization(String),

    /// Token expired
    #[error("Token expired: {0}")]
    TokenExpired(String),

    /// Token revoked
    #[error("Token revoked: {0}")]
    TokenRevoked(String),

    /// Invalid token format or signature
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    /// Auth token is missing from the request
    #[error("Auth token is missing from the request")]
    TokenMissing,

    /// Auth token signature is invalid
    #[error("Auth token signature is invalid")]
    TokenSignatureInvalid,

    /// Session storage entry is corrupted
    #[error("Session storage entry is corrupted: {0}")]
    SessionCorrupted(String),

    /// Tenant selection header is absent when required
    #[error("Tenant selection header is absent when required")]
    TenantHeaderMissing,
}

impl AosAuthError {
    /// Check if this is an authentication (401) vs authorization (403) error
    pub fn is_authentication(&self) -> bool {
        matches!(
            self,
            Self::Authentication(_)
                | Self::TokenExpired(_)
                | Self::TokenRevoked(_)
                | Self::InvalidToken(_)
                | Self::TokenMissing
                | Self::TokenSignatureInvalid
                | Self::SessionCorrupted(_)
                | Self::TenantHeaderMissing
        )
    }

    /// Check if this is an authorization (403) error
    pub fn is_authorization(&self) -> bool {
        matches!(self, Self::Authorization(_))
    }

    /// Get the error code for this error type
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Authentication(_) => "UNAUTHORIZED",
            Self::Authorization(_) => "FORBIDDEN",
            Self::TokenExpired(_) => "TOKEN_EXPIRED",
            Self::TokenRevoked(_) => "TOKEN_REVOKED",
            Self::InvalidToken(_) => "INVALID_TOKEN",
            Self::TokenMissing => "TOKEN_MISSING",
            Self::TokenSignatureInvalid => "TOKEN_SIGNATURE_INVALID",
            Self::SessionCorrupted(_) => "SESSION_CORRUPTED",
            Self::TenantHeaderMissing => "TENANT_HEADER_MISSING",
        }
    }
}
