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
        )
    }

    /// Check if this is an authorization (403) error
    pub fn is_authorization(&self) -> bool {
        matches!(self, Self::Authorization(_))
    }
}
