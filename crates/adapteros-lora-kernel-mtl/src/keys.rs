//! Embedded signing keys for manifest verification
//!
//! This module contains the embedded public key used to verify
//! kernel manifest signatures. The key is replaced during CI build.

/// Embedded Ed25519 public key for manifest verification.
///
/// In CI, this constant should be replaced with the production public key.
/// For development, you can override via env var `AOS_KERNEL_PUBKEY_PEM`.
pub const SIGNING_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAplaceholder_public_key_will_be_replaced_by_ci_build_process
-----END PUBLIC KEY-----"#;

/// Resolve the signing public key PEM, optionally from environment override.
pub fn resolve_public_key_pem() -> String {
    if let Ok(pem) = std::env::var("AOS_KERNEL_PUBKEY_PEM") {
        return pem;
    }
    SIGNING_PUBLIC_KEY_PEM.to_string()
}
