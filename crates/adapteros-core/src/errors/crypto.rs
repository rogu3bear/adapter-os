//! Cryptographic errors
//!
//! Covers hashing, encryption, decryption, and sealed data operations.

use adapteros_error_registry::ECode;
use thiserror::Error;

/// Cryptographic operation errors
#[derive(Error, Debug)]
pub enum AosCryptoError {
    /// Invalid hash format or value
    #[error("Invalid hash: {0}")]
    InvalidHash(String),

    /// Generic cryptographic error
    #[error("Cryptographic error: {0}")]
    Crypto(String),

    /// Encryption operation failed
    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    /// Decryption operation failed
    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    /// Invalid sealed/encrypted data format
    #[error("Invalid sealed data: {reason}")]
    InvalidSealedData { reason: String },

    /// RNG (random number generator) error with deterministic context
    #[error("RNG error [seed:{seed_hash}|label:{label}|counter:{counter}]: {message}")]
    RngError {
        seed_hash: String,
        label: String,
        counter: u64,
        message: String,
    },
}

impl AosCryptoError {
    /// Get the error code for this crypto error (compile-time exhaustive)
    pub const fn ecode(&self) -> ECode {
        match self {
            Self::InvalidHash(_) => ECode::E1004,
            Self::Crypto(_) => ECode::E1001,
            Self::EncryptionFailed { .. } => ECode::E1001,
            Self::DecryptionFailed { .. } => ECode::E1001,
            Self::InvalidSealedData { .. } => ECode::E1001,
            Self::RngError { .. } => ECode::E2001, // Determinism-related
        }
    }
}
