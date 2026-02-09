//! Secure Enclave abstraction with platform-specific implementations.
//!
//! This module provides a unified `EnclaveManager` API with two implementations:
//!
//! 1. **Hardware-backed** ([`macos`]): Uses macOS Secure Enclave for tamper-resistant
//!    key storage and cryptographic operations
//! 2. **Software fallback** ([`stub`]): Uses standard cryptography libraries for
//!    cross-platform compatibility
//!
//! ## Backend Selection
//!
//! The backend is selected at compile time based on target OS and feature flags:
//!
//! | Condition | Backend | Module |
//! |-----------|---------|--------|
//! | macOS + `secure-enclave` feature | Hardware SEP | [`macos`] |
//! | macOS without feature | Software stub | [`stub`] |
//! | Non-macOS (Linux, Windows) | Software stub | [`stub`] |
//!
//! ## Why the Stub Exists
//!
//! The software stub provides:
//!
//! - **Cross-platform builds**: CI/CD on Linux, development on non-Apple hardware
//! - **Feature parity**: Same API regardless of hardware availability
//! - **Graceful degradation**: Applications continue to work without SEP
//! - **Testing**: Unit tests can run without hardware dependencies
//!
//! ## Security Properties
//!
//! | Operation | macOS (HW) | Software Fallback |
//! |-----------|-----------|-------------------|
//! | Signing | ECDSA (Secure Enclave) | Ed25519 (software) |
//! | Encryption | ChaCha20-Poly1305 | ChaCha20-Poly1305 |
//! | Key Derivation | Keychain + root key | HKDF (SHA256) |
//! | Nonce Generation | Deterministic (data-derived) | Deterministic (data-derived) |
//! | Key Storage | Tamper-resistant hardware | Ephemeral (process memory) |
//! | Key Extraction | Impossible | Possible (keys in memory) |
//! | Attestation | Synthetic (stub - real SEP not implemented) | Not available |
//!
//! ## Production Recommendations
//!
//! The software fallback maintains cryptographic security properties suitable for
//! development and testing but should not be used in production without additional
//! hardening (e.g., TPM/TEE integration, encrypted key storage, HSM backing).
//!
//! For production deployments requiring hardware security:
//! - Use macOS with `secure-enclave` feature enabled
//! - Ensure proper code signing entitlements
//! - Verify SEP availability at runtime before trusting attestation
//!
//! ## Checking Backend at Runtime
//!
//! ```rust,ignore
//! let manager = EnclaveManager::new()?;
//! if manager.is_software_fallback() {
//!     warn!("Running with software fallback - hardware security not available");
//! }
//! ```

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
