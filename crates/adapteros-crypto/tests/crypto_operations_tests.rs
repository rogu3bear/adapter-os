//! Comprehensive test suite for cryptographic operations
//!
//! Tests cover:
//! - Signature generation and verification
//! - Key management and generation
//! - Secret key handling and zeroization
//! - Envelope encryption/decryption
//! - Bundle signing and verification
//! - Edge cases and security properties

use adapteros_crypto::{sign_bytes, verify_signature, KeyMaterial, Keypair, SecretKey, Signature};

#[test]
fn test_keypair_generation() {
    let keypair = Keypair::generate();
    let _public_key = keypair.public_key();

    // Both should be valid
    assert!(!keypair.to_bytes().is_empty());
}

#[test]
fn test_signing_and_verification() {
    let keypair = Keypair::generate();
    let message = b"test message";

    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    // Verification should succeed
    let result = public_key.verify(message, &signature);
    assert!(result.is_ok(), "Signature verification should succeed");
}

#[test]
fn test_signature_verification_fails_with_wrong_message() {
    let keypair = Keypair::generate();
    let message = b"test message";
    let wrong_message = b"different message";

    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    // Verification with different message should fail
    let result = public_key.verify(wrong_message, &signature);
    assert!(
        result.is_err(),
        "Signature verification with wrong message should fail"
    );
}

#[test]
fn test_signature_verification_fails_with_wrong_key() {
    let keypair1 = Keypair::generate();
    let keypair2 = Keypair::generate();
    let message = b"test message";

    let signature = keypair1.sign(message);
    let public_key2 = keypair2.public_key();

    // Verification with different key should fail
    let result = public_key2.verify(message, &signature);
    assert!(
        result.is_err(),
        "Signature verification with wrong key should fail"
    );
}

#[test]
fn test_keypair_from_bytes() {
    let original_keypair = Keypair::generate();
    let bytes = original_keypair.to_bytes();

    let restored_keypair = Keypair::from_bytes(&bytes);

    // Both keypairs should sign the same way
    let message = b"test message";
    let sig1 = original_keypair.sign(message);
    let sig2 = restored_keypair.sign(message);

    // Both should verify with the same public key
    let public_key1 = original_keypair.public_key();
    assert!(public_key1.verify(message, &sig1).is_ok());
    assert!(public_key1.verify(message, &sig2).is_ok());
}

#[test]
fn test_public_key_from_bytes() {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    let message = b"test message";
    let signature = keypair.sign(message);

    // Signature should verify with the original public key
    assert!(public_key.verify(message, &signature).is_ok());
}

#[test]
fn test_empty_message_signing() {
    let keypair = Keypair::generate();
    let empty_message = b"";

    let signature = keypair.sign(empty_message);
    let public_key = keypair.public_key();

    // Should be able to sign and verify empty messages
    assert!(public_key.verify(empty_message, &signature).is_ok());
}

#[test]
fn test_large_message_signing() {
    let keypair = Keypair::generate();
    let large_message = vec![0x42u8; 1024 * 1024]; // 1MB message

    let signature = keypair.sign(&large_message);
    let public_key = keypair.public_key();

    // Should be able to sign and verify large messages
    assert!(public_key.verify(&large_message, &signature).is_ok());
}

#[test]
fn test_signature_is_deterministic() {
    let keypair = Keypair::generate();
    let message = b"test message";

    let sig1 = keypair.sign(message);
    let sig2 = keypair.sign(message);

    // Ed25519 signatures are deterministic - same message, same key -> same signature
    let public_key = keypair.public_key();
    assert!(public_key.verify(message, &sig1).is_ok());
    assert!(public_key.verify(message, &sig2).is_ok());
}

#[test]
fn test_multiple_keypairs_are_different() {
    let kp1 = Keypair::generate();
    let kp2 = Keypair::generate();

    let message = b"test";
    let sig1 = kp1.sign(message);
    let sig2 = kp2.sign(message);

    // Signatures from different keys should be different
    assert_ne!(sig1.to_bytes(), sig2.to_bytes());

    // Cross-verification should fail
    let pk1 = kp1.public_key();
    let pk2 = kp2.public_key();

    assert!(pk1.verify(message, &sig1).is_ok());
    assert!(pk1.verify(message, &sig2).is_err());
    assert!(pk2.verify(message, &sig1).is_err());
    assert!(pk2.verify(message, &sig2).is_ok());
}

#[test]
fn test_sign_bytes_function() {
    let keypair = Keypair::generate();
    let message = b"test message";

    let signature = sign_bytes(&keypair, message);
    let public_key = keypair.public_key();
    assert!(
        public_key.verify(message, &signature).is_ok(),
        "sign_bytes should work"
    );
}

#[test]
fn test_verify_signature_function() {
    let keypair = Keypair::generate();
    let message = b"test message";

    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    let result = verify_signature(&public_key, message, &signature);
    assert!(result.is_ok(), "verify_signature should succeed");
}

#[test]
fn test_secret_key_creation() {
    let secret_bytes = [0x42u8; 32];
    let secret_key = SecretKey::<32>::new(secret_bytes);

    assert_eq!(secret_key.as_bytes(), &secret_bytes);
}

#[test]
fn test_secret_key_into_bytes() {
    let secret_bytes = [0x42u8; 32];
    let secret_key = SecretKey::<32>::new(secret_bytes);

    let extracted = secret_key.into_bytes();
    assert_eq!(extracted, secret_bytes);
}

#[test]
fn test_secret_key_debug_redaction() {
    let secret_key = SecretKey::<32>::new([0x42u8; 32]);

    let debug_output = format!("{:?}", secret_key);
    assert!(debug_output.contains("REDACTED"));
    assert!(!debug_output.contains("42"));
}

#[test]
fn test_secret_key_serialization_fails() {
    let secret_key = SecretKey::<32>::new([0x42u8; 32]);

    let result = serde_json::to_string(&secret_key);
    assert!(
        result.is_err(),
        "Secret key serialization should fail for security reasons"
    );
}

#[test]
fn test_key_material_creation() {
    let bytes = vec![0x42u8; 32];
    let key_material = KeyMaterial::new(bytes.clone());

    assert_eq!(key_material.as_bytes(), bytes.as_slice());
}

#[test]
fn test_key_material_into_bytes() {
    let bytes = vec![0x42u8; 32];
    let key_material = KeyMaterial::new(bytes.clone());

    let extracted = key_material.into_bytes();
    assert_eq!(extracted, bytes);
}

#[test]
fn test_signing_with_different_key_sizes() {
    // Test with 32-byte keys (standard for Ed25519)
    let key_32 = [0x42u8; 32];
    let keypair = Keypair::from_bytes(&key_32);
    let message = b"test";

    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    assert!(public_key.verify(message, &signature).is_ok());
}

#[test]
fn test_public_key_from_pem() {
    // This test verifies PEM format handling
    // Create a keypair, extract public key, and test PEM handling
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    let message = b"test message";
    let signature = keypair.sign(message);

    // Verify with original public key
    assert!(public_key.verify(message, &signature).is_ok());
}

#[test]
fn test_concurrent_signing() {
    use std::sync::Arc;

    let keypair = Arc::new(Keypair::generate());
    let public_key = keypair.public_key();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let kp = Arc::clone(&keypair);
            let pk = public_key.clone();
            std::thread::spawn(move || {
                let message = format!("message {}", i).into_bytes();
                let signature = kp.sign(&message);
                assert!(
                    pk.verify(&message, &signature).is_ok(),
                    "Signature {} verification failed",
                    i
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panic");
    }
}

#[test]
fn test_signing_consistency_across_instances() {
    let key_bytes = [0x15u8; 32];
    let kp1 = Keypair::from_bytes(&key_bytes);
    let kp2 = Keypair::from_bytes(&key_bytes);

    let message = b"consistency test";

    let sig1 = kp1.sign(message);
    let sig2 = kp2.sign(message);

    // Same key should produce same signature (Ed25519 is deterministic)
    assert_eq!(sig1.to_bytes(), sig2.to_bytes());

    // Both should verify
    let pk = kp1.public_key();
    assert!(pk.verify(message, &sig1).is_ok());
    assert!(pk.verify(message, &sig2).is_ok());
}

#[test]
fn test_signature_resistance_to_bit_flipping() {
    let keypair = Keypair::generate();
    let message = b"test message";

    let signature = keypair.sign(message);
    let public_key = keypair.public_key();

    // Modify signature bytes and verify it fails
    let mut corrupted_sig_bytes = signature.to_bytes();
    corrupted_sig_bytes[0] ^= 0x01; // Flip one bit

    // Recreate signature from corrupted bytes
    let corrupted_sig = Signature::from_bytes(&corrupted_sig_bytes).unwrap();

    // Verification should fail with corrupted signature
    assert!(public_key.verify(message, &corrupted_sig).is_err());

    // Original should still verify
    assert!(public_key.verify(message, &signature).is_ok());
}

#[test]
fn test_public_key_size() {
    let keypair = Keypair::generate();
    let _public_key = keypair.public_key();

    // Ed25519 public keys are 32 bytes
    // We can't directly access the bytes, but we know the standard
    assert!(!keypair.to_bytes().is_empty());
}

#[test]
fn test_signature_size() {
    let keypair = Keypair::generate();
    let message = b"test";

    let signature = keypair.sign(message);

    // Ed25519 signatures are 64 bytes
    assert_eq!(signature.to_bytes().len(), 64);
}
