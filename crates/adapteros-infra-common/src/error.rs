//! Minimal error types for adapteros-infra-common
//!
//! This module provides a subset of AosError variants needed for foundational
//! utilities, avoiding a circular dependency back to adapteros-core.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AosError>;

/// Foundational error type for common utilities
#[derive(Error, Debug)]
pub enum AosError {
    #[error("Invalid hash: {0}")]
    InvalidHash(String),

    #[error("Invalid CPID: {0}")]
    InvalidCPID(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl AosError {
    pub fn validation(msg: impl Into<String>) -> Self {
        AosError::Validation(msg.into())
    }

    pub fn parse(msg: impl Into<String>) -> Self {
        AosError::Parse(msg.into())
    }
}
