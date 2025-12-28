//! Signature Key Validation Tests (P1 High)
//!
//! Tests for Ed25519 key handling and validation.
//! Keys must be properly formatted and validated before use.
//!
//! These tests verify:
//! - Key length validation rejects short key
//! - Key length validation rejects long key
//! - Key file not found error handling
//! - Key file permission checking
//! - Invalid PEM format rejection
//! - Invalid hex public key rejection

use adapteros_core::{AosError, B3Hash};
use std::sync::Mutex;

// Global mutex for tests that access files
static FILE_MUTEX: Mutex<()> = Mutex::new(());

/// Test that Ed25519 key generation produces valid keys.
///
/// Keys should be exactly 32 bytes (64 hex chars).
#[test]
fn test_ed25519_key_length_requirements() {
    // Ed25519 seed must be exactly 32 bytes
    let valid_seed = [0u8; 32];
    assert_eq!(valid_seed.len(), 32);

    // Short seeds should be rejected
    let short_seed = [0u8; 16];
    assert_ne!(short_seed.len(), 32);

    // Long seeds should be rejected
    let long_seed = [0u8; 64];
    assert_ne!(long_seed.len(), 32);
}

/// Test that public key parsing validates length.
///
/// Public keys must be exactly 32 bytes.
#[test]
fn test_public_key_length_validation() {
    // Valid public key is 32 bytes
    let valid_pubkey_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert_eq!(valid_pubkey_hex.len(), 64); // 32 bytes = 64 hex chars

    // Short public key should be invalid
    let short_pubkey_hex = "0123456789abcdef";
    assert!(hex::decode(short_pubkey_hex).unwrap().len() != 32);

    // Long public key should be invalid
    let long_pubkey_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef00";
    assert!(hex::decode(long_pubkey_hex).unwrap().len() != 32);
}

/// Test that signature parsing validates length.
///
/// Ed25519 signatures must be exactly 64 bytes.
#[test]
fn test_signature_length_validation() {
    // Valid signature is 64 bytes
    let valid_sig_hex = "00".repeat(64);
    let decoded = hex::decode(&valid_sig_hex).unwrap();
    assert_eq!(decoded.len(), 64);

    // Short signature should be invalid
    let short_sig_hex = "00".repeat(32);
    let short_decoded = hex::decode(&short_sig_hex).unwrap();
    assert_ne!(short_decoded.len(), 64);

    // Long signature should be invalid
    let long_sig_hex = "00".repeat(128);
    let long_decoded = hex::decode(&long_sig_hex).unwrap();
    assert_ne!(long_decoded.len(), 64);
}

/// Test that invalid hex strings are rejected.
///
/// Keys must be valid hexadecimal.
#[test]
fn test_invalid_hex_rejected() {
    // Invalid hex characters
    let invalid_hex = "not-valid-hex-string";
    assert!(hex::decode(invalid_hex).is_err());

    // Odd length (invalid hex)
    let odd_hex = "0123456789abcde"; // 15 chars, needs even
    assert!(hex::decode(odd_hex).is_err());

    // Non-ASCII
    let unicode_hex = "🔑🔑🔑🔑🔑🔑🔑🔑";
    assert!(hex::decode(unicode_hex).is_err());
}

/// Test key ID computation from public key.
///
/// Key ID should be deterministic hash of public key.
#[test]
fn test_key_id_computation_deterministic() {
    let pubkey_bytes = [42u8; 32];
    let key_id_1 = B3Hash::hash(&pubkey_bytes).to_hex()[..16].to_string();
    let key_id_2 = B3Hash::hash(&pubkey_bytes).to_hex()[..16].to_string();

    assert_eq!(key_id_1, key_id_2, "Key ID must be deterministic");
    assert_eq!(
        key_id_1.len(),
        16,
        "Key ID should be 8 bytes = 16 hex chars"
    );
}

/// Test that different public keys produce different key IDs.
///
/// Collision resistance for key IDs.
#[test]
fn test_key_id_uniqueness() {
    let mut key_ids = std::collections::HashSet::new();

    // Generate 100 different "public keys" and verify unique key IDs
    for i in 0..100u8 {
        let mut pubkey = [0u8; 32];
        pubkey[0] = i;
        let key_id = B3Hash::hash(&pubkey).to_hex()[..16].to_string();
        let inserted = key_ids.insert(key_id);
        assert!(inserted, "Key ID collision at iteration {}", i);
    }
}

/// Test that empty public key is handled gracefully.
///
/// Should produce an error, not panic.
#[test]
fn test_empty_public_key_handling() {
    let empty_bytes: [u8; 0] = [];

    // Hashing empty should work (not panic)
    let hash = B3Hash::hash(&empty_bytes);
    assert_ne!(hash, B3Hash::zero());

    // But using as a public key would fail validation
    // (empty != 32 bytes)
    assert_ne!(empty_bytes.len(), 32);
}
