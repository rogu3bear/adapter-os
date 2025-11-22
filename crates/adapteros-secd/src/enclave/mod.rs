//! Secure Enclave abstraction with platform-specific implementations.
//!
//! ## Backend Selection
//!
//! - **macOS with `secure-enclave` feature:** Uses hardware-backed Secure Enclave (ECDSA signing, hardware-sealed keys)
//! - **macOS without feature or other platforms:** Uses software fallback (Ed25519 + HKDF-derived keys)
//!
//! ## Security Properties
//!
//! | Operation | macOS (HW) | Software Fallback |
//! |-----------|-----------|-------------------|
//! | Signing | ECDSA (Secure Enclave) | Ed25519 (software) |
//! | Encryption | ChaCha20-Poly1305 | ChaCha20-Poly1305 |
//! | Key Derivation | Keychain + master key | HKDF (SHA256) |
//! | Nonce Generation | Deterministic (data-derived) | Deterministic (data-derived) |
//! | Key Storage | Tamper-resistant hardware | Ephemeral (process memory) |
//!
//! The software fallback maintains cryptographic security properties suitable for
//! development and testing but should not be used in production without additional
//! hardening (e.g., TPM/TEE integration, encrypted key storage).

use thiserror::Error;

/// Result type for enclave operations
pub type Result<T> = std::result::Result<T, EnclaveError>;

/// Errors produced by Secure Enclave helpers
#[derive(Debug, Error)]
pub enum EnclaveError {
    #[error("Security framework error: {0}")]
    Security(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Use macOS Secure Enclave on macOS with feature enabled
#[cfg(all(target_os = "macos", feature = "secure-enclave"))]
mod macos;
#[cfg(all(target_os = "macos", feature = "secure-enclave"))]
pub use macos::*;

/// Use software fallback on all other platforms or when feature is disabled
#[cfg(not(all(target_os = "macos", feature = "secure-enclave")))]
mod stub;
#[cfg(not(all(target_os = "macos", feature = "secure-enclave")))]
pub use stub::*;
