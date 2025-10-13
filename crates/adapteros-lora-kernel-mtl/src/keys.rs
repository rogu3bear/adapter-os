//! Embedded signing keys for manifest verification
//!
//! This module contains the embedded public key used to verify
//! kernel manifest signatures. The key is replaced during CI build.

/// Embedded Ed25519 public key for manifest verification
pub const SIGNING_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAplaceholder_public_key_will_be_replaced_by_ci_build_process
-----END PUBLIC KEY-----"#;
