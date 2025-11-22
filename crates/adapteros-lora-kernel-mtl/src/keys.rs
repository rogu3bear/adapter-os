//! Embedded signing keys for manifest verification
//!
//! This module contains the embedded public key used to verify
//! kernel manifest signatures. The key is replaced during CI build.

/// Embedded Ed25519 public key for manifest verification
///
/// NOTE: This is a test key for development. In production, this is replaced
/// by the CI build process with the actual signing public key.
/// The format expected is raw 32-byte Ed25519 public key in base64 (not ASN.1/DER).
pub const SIGNING_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
-----END PUBLIC KEY-----"#;
