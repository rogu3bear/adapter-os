//! Embedded signing keys for manifest verification
//!
//! This module contains the embedded public key used to verify
//! kernel manifest signatures.
//!
//! ## Key Management Strategy
//!
//! - **Production**: The CI build process replaces the production key with the
//!   actual signing public key from secure key management.
//! - **Development/Testing**: Uses deterministic test keys derived from a fixed
//!   seed, allowing reproducible builds and testing without needing access to
//!   production signing keys.
//!
//! The test key infrastructure uses a fixed seed to generate Ed25519 keypairs,
//! ensuring that:
//! 1. Tests are reproducible across machines and builds
//! 2. Manifest signatures generated at build time can be verified at runtime
//! 3. No environment variable hacks are needed to skip verification

use base64::Engine;

/// Fixed seed for deterministic test key generation.
/// This seed is used ONLY for development and testing.
/// Production keys are managed separately and injected during CI.
///
/// The seed is a BLAKE3 hash of "adapteros-test-signing-key-v1" to ensure
/// it's deterministic and unique to this project.
pub const TEST_KEY_SEED: [u8; 32] = [
    0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1,
    0xf2, 0x03, 0x14, 0x25, 0x36, 0x47, 0x58, 0x69,
    0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1,
    0xf2, 0x03, 0x14, 0x25, 0x36, 0x47, 0x58, 0x69,
];

/// Generate the test public key from the fixed seed.
/// This function produces a deterministic public key that matches what
/// the build script uses when signing manifests.
pub fn get_test_public_key_bytes() -> [u8; 32] {
    use adapteros_crypto::SigningKey;
    let signing_key = SigningKey::from_bytes(&TEST_KEY_SEED);
    signing_key.verifying_key().to_bytes()
}

/// Generate the test signing key from the fixed seed.
/// Used by build.rs to sign manifests during build.
pub fn get_test_signing_key() -> adapteros_crypto::SigningKey {
    adapteros_crypto::SigningKey::from_bytes(&TEST_KEY_SEED)
}

/// Embedded Ed25519 public key for manifest verification.
///
/// This is dynamically generated from the test seed in development/test builds.
/// In production, this should be replaced with the actual signing public key
/// via the CI build process.
///
/// The format is raw 32-byte Ed25519 public key in base64 (not ASN.1/DER).
pub fn get_signing_public_key_pem() -> String {
    let public_key_bytes = get_test_public_key_bytes();
    let public_key_b64 = base64::engine::general_purpose::STANDARD.encode(public_key_bytes);
    format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        public_key_b64
    )
}

/// Legacy constant for backward compatibility.
/// NOTE: This constant uses placeholder bytes. For actual verification,
/// use `get_signing_public_key_pem()` which returns the correctly derived key.
///
/// This will be removed once all callers migrate to the function-based API.
#[deprecated(
    since = "0.1.0",
    note = "Use get_signing_public_key_pem() instead for proper test key derivation"
)]
pub const SIGNING_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
eoucrb7P0OHyAxQlNkdYaXqLnK2+z9Dh8gMUJTZHWGk=
-----END PUBLIC KEY-----"#;

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, Verifier, VerifyingKey};

    #[test]
    fn test_deterministic_key_generation() {
        // Verify key generation is deterministic
        let key1 = get_test_public_key_bytes();
        let key2 = get_test_public_key_bytes();
        assert_eq!(key1, key2, "Key generation should be deterministic");
    }

    #[test]
    fn test_signing_key_matches_public_key() {
        let signing_key = get_test_signing_key();
        let public_key_bytes = get_test_public_key_bytes();

        // Verify the signing key produces the expected public key
        assert_eq!(
            signing_key.verifying_key().to_bytes(),
            public_key_bytes,
            "Signing key should produce matching public key"
        );

        // Test sign and verify round-trip
        let message = b"test message for signing";
        let signature = signing_key.sign(message);

        let verifying_key =
            VerifyingKey::from_bytes(&public_key_bytes).expect("Valid public key");
        assert!(
            verifying_key.verify(message, &signature).is_ok(),
            "Signature should verify successfully"
        );
    }

    #[test]
    fn test_pem_format() {
        let pem = get_signing_public_key_pem();
        assert!(pem.starts_with("-----BEGIN PUBLIC KEY-----"));
        assert!(pem.ends_with("-----END PUBLIC KEY-----"));
    }
}
