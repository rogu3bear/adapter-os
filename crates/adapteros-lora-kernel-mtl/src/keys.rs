//! Embedded signing keys for manifest verification
//!
//! This module contains the embedded public key used to verify
//! kernel manifest signatures. The key is replaced during CI build.

/// Embedded Ed25519 public key for manifest verification
///
/// NOTE: This is a test key for development. In production, this is replaced
/// by the CI build process with the actual signing public key.
pub const SIGNING_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAYWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY=
-----END PUBLIC KEY-----"#;
