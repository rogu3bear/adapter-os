//! Secure Enclave Daemon for adapterOS
//!
//! This daemon provides a privileged interface to macOS Secure Enclave operations.
//! It runs as a separate process with restricted entitlements and exposes a minimal
//! UDS API for signing and encryption operations.
//!
//! ## Security Model
//!
//! - Runs under dedicated service account
//! - Only has keychain + enclave entitlements
//! - Workers cannot directly access enclave
//! - All keys stored in Secure Enclave
//! - No network access (UDS only)
//!
//! ## Stub Implementations and Hardware Requirements
//!
//! Several components in this module have stub or partial implementations due to
//! hardware and platform requirements that cannot be satisfied in all environments:
//!
//! | Component | Stub Reason | Full Implementation Requires |
//! |-----------|-------------|------------------------------|
//! | [`key_lifecycle`] | Keychain creation date extraction | macOS `kSecAttrCreationDate` CFDictionary access |
//! | [`rotation_daemon`] | KMS provider mode | External KMS integration (AWS KMS, HashiCorp Vault, etc.) |
//! | [`secure_enclave`] | SEP attestation | macOS 13.0+ with `SecKeyCopyAttestation` FFI bindings |
//! | [`enclave::stub`] | Software fallback | macOS Secure Enclave Processor (Apple Silicon/T2) |
//! | [`host_identity`] | Mock secure enclave connection | Hardware SEP with proper entitlements |
//!
//! ### Why Stubs Exist
//!
//! 1. **Secure Enclave Processor (SEP)**: The SEP is a hardware coprocessor available
//!    only on Apple Silicon and Intel Macs with T2 chips. Keys generated in the SEP
//!    never leave the hardware and cannot be exported. Cross-platform builds and CI
//!    environments lack this hardware.
//!
//! 2. **macOS Keychain Deep Integration**: Extracting key creation dates requires
//!    low-level CFDictionary operations with `kSecAttrCreationDate`. The
//!    `security-framework` crate provides high-level access but not all attributes.
//!
//! 3. **KMS Provider Integration**: Production deployments may use external KMS
//!    providers (AWS KMS, HashiCorp Vault, Azure Key Vault) for key management.
//!    These require provider-specific SDKs and authentication.
//!
//! 4. **SEP Attestation**: Real hardware attestation via `SecKeyCopyAttestation`
//!    requires macOS 13.0+ and proper app entitlements. It provides cryptographic
//!    proof that a key resides in the Secure Enclave.
//!
//! ### Stub Behavior
//!
//! When hardware features are unavailable, stubs provide:
//! - **Graceful degradation**: Operations succeed with software-based alternatives
//! - **Placeholder values**: Functions return safe defaults (e.g., current timestamp
//!   instead of actual key creation date)
//! - **Clear logging**: Warnings indicate when fallback behavior is active
//! - **API compatibility**: The same interface is maintained for production code
//!
//! ### Enabling Full Hardware Support
//!
//! To enable hardware-backed operations:
//!
//! ```toml
//! # In Cargo.toml
//! adapteros-secd = { path = "...", features = ["secure-enclave"] }
//! ```
//!
//! Requirements:
//! - macOS 12.0+ (Monterey or later)
//! - Apple Silicon (M1/M2/M3) or Intel Mac with T2 chip
//! - Proper code signing entitlements for Secure Enclave access

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_match)]

pub mod audit;
pub mod enclave;
pub mod federation_auth;
pub mod heartbeat;
pub mod host_identity;
pub mod key_lifecycle;
pub mod pidfile;
pub mod protocol;
pub mod server;

pub use audit::AuditLogger;
pub use enclave::EnclaveManager;
pub use federation_auth::{validate_federation_token, FederationAuthError, FederationClaims};
pub use heartbeat::Heartbeat;
pub use host_identity::{
    AttestationMetadata, AttestationReport, HostIdentity, HostIdentityManager,
    SecureEnclaveConnection,
};
pub use key_lifecycle::{KeyAgeWarning, KeyLifecycleManager};
pub use pidfile::{is_process_running, read_pid, remove_pid, write_pid};
pub use protocol::{Request, Response};
pub use server::serve_uds;
