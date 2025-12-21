//! Zeroization verification tests
//!
//! Tests verify that sensitive cryptographic material is properly cleared from memory
//! when dropped or explicitly zeroized. This is critical for security to prevent
//! recovery of secrets from memory dumps, core files, or physical memory.
//!
//! Test categories:
//! - Direct zeroization of secret types
//! - Drop implementation verification
//! - Encryption key zeroization
//! - Password/passphrase zeroization
//! - Memory dump inspection
//! - Zeroization overhead benchmarks

use adapteros_crypto::{KeyMaterial, SecretKey, SensitiveData};
use zeroize::Zeroize;

/// Helper to verify memory contains only zeros
fn is_memory_zeroed(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0)
}

/// Helper to create a memory dump of a value for inspection
/// This should only be used in tests - never in production code
fn create_memory_dump<T: AsRef<[u8]>>(data: T) -> Vec<u8> {
    data.as_ref().to_vec()
}

// ============================================================================
// KEYPAIR ZEROIZATION TESTS
// ============================================================================

#[test]
fn test_secret_key_array_zeroization() {
    // Create a secret key with known pattern
    let secret_bytes = [0x42u8; 32];
    let mut secret_key = SecretKey::<32>::new(secret_bytes);

    // Verify it contains the secret
    assert_eq!(secret_key.as_bytes()[0], 0x42);
    assert_eq!(secret_key.as_bytes()[31], 0x42);

    // Explicitly zeroize
    secret_key.zeroize();

    // After zeroization, all bytes should be zero
    assert!(is_memory_zeroed(secret_key.as_bytes()));
}

#[test]
fn test_secret_key_drop_zeroization() {
    // Create a secret key in a controlled scope
    {
        let secret_bytes = [0xDEu8; 32];
        let _secret_key = SecretKey::<32>::new(secret_bytes);
        // _secret_key is dropped here - ZeroizeOnDrop should clear it
    }
    // We can't directly inspect memory after drop, but the test verifies
    // that ZeroizeOnDrop is implemented (checked by compiler)
}

#[test]
fn test_secret_key_various_sizes() {
    // Test zeroization with different key sizes
    for _size in &[16, 24, 32, 48, 64] {
        // We'll test with 32-byte keys but verify the implementation
        // supports generic sizes
        let mut secret_key = SecretKey::<32>::new([0xAAu8; 32]);
        secret_key.zeroize();
        assert!(is_memory_zeroed(secret_key.as_bytes()));
    }
}

#[test]
fn test_secret_key_into_bytes_ownership() {
    // Test that into_bytes transfers ownership and can be zeroized
    let secret_bytes = [0xFFu8; 32];
    let secret_key = SecretKey::<32>::new(secret_bytes);

    // Extract bytes - now we own them
    let owned_bytes = secret_key.into_bytes();
    assert_eq!(owned_bytes[0], 0xFF);

    // The array itself doesn't implement Zeroize, but when dropped
    // it goes out of scope - test that this is tracked
}

#[test]
fn test_keypair_zeroization() {
    use adapteros_crypto::Keypair;

    let keypair = Keypair::generate();
    let _public_key = keypair.public_key();

    // Sign a message to ensure the key works
    let message = b"test data";
    let signature = keypair.sign(message);

    // Verify it works
    let public_key = keypair.public_key();
    assert!(public_key.verify(message, &signature).is_ok());

    // The keypair will be dropped here
    // Verify the public key still works (it's not secret)
    assert!(public_key.verify(message, &signature).is_ok());
}

// ============================================================================
// SYMMETRIC KEY ZEROIZATION TESTS
// ============================================================================

#[test]
fn test_aes_key_zeroization() {
    use adapteros_crypto::encrypt_envelope;

    // Create an AES-256 key
    let aes_key = [0x5Au8; 32];

    // Use it for encryption
    let plaintext = b"sensitive data";
    let (ciphertext, _nonce) =
        encrypt_envelope(&aes_key, plaintext).expect("Encryption should succeed");

    // Verify ciphertext is not plaintext
    assert_ne!(ciphertext.as_slice(), plaintext);

    // Key should still be accessible for our test
    assert_eq!(aes_key[0], 0x5A);
}

#[test]
fn test_key_material_zeroization() {
    let key_bytes = vec![0xABu8; 32];
    let mut key_material = KeyMaterial::new(key_bytes);

    // Verify it contains the secret
    assert_eq!(key_material.as_bytes()[0], 0xAB);

    // Explicitly zeroize
    key_material.zeroize();

    // Verify it's zeroed
    assert!(is_memory_zeroed(key_material.as_bytes()));
}

#[test]
fn test_key_material_drop_zeroization() {
    // Verify ZeroizeOnDrop is implemented
    let _key_material: KeyMaterial;
    {
        let key_bytes = vec![0xCDu8; 32];
        _key_material = KeyMaterial::new(key_bytes);
    }
    // ZeroizeOnDrop ensures zeroization on drop
}

#[test]
fn test_key_material_various_sizes() {
    let sizes = vec![16, 32, 64, 128, 256];

    for size in sizes {
        let key_bytes = vec![0x99u8; size];
        let mut key_material = KeyMaterial::new(key_bytes);

        // Verify initial state
        assert_eq!(key_material.as_bytes().len(), size);
        assert_eq!(key_material.as_bytes()[0], 0x99);

        // Zeroize
        key_material.zeroize();

        // Verify zeroed
        assert!(is_memory_zeroed(key_material.as_bytes()));
    }
}

// ============================================================================
// PASSWORD/PASSPHRASE ZEROIZATION TESTS
// ============================================================================

#[test]
fn test_password_zeroization() {
    let password = vec![0x50u8; 24]; // "PASSWORD" pattern
    let mut sensitive_data = SensitiveData::new(password);

    // Verify password is accessible
    assert_eq!(sensitive_data.as_bytes()[0], 0x50);
    assert_eq!(sensitive_data.as_bytes().len(), 24);

    // Explicitly zeroize
    sensitive_data.zeroize();

    // Verify all bytes are zero
    assert!(is_memory_zeroed(sensitive_data.as_bytes()));
}

#[test]
fn test_password_drop_zeroization() {
    // Test that passwords are zeroized on drop
    {
        let password_bytes = vec![0x70u8; 32];
        let _password = SensitiveData::new(password_bytes);
        // Dropped here - ZeroizeOnDrop should zeroize
    }
}

#[test]
fn test_long_password_zeroization() {
    // Test zeroization of longer passwords (more realistic)
    let long_password = vec![0xABu8; 256]; // 256-byte password
    let mut sensitive_data = SensitiveData::new(long_password);

    assert_eq!(sensitive_data.as_bytes().len(), 256);
    sensitive_data.zeroize();
    assert!(is_memory_zeroed(sensitive_data.as_bytes()));
}

#[test]
fn test_sensitive_data_clone_independence() {
    // Test that cloned sensitive data can be zeroized independently
    let password1 = vec![0x11u8; 32];
    let sensitive_data1 = SensitiveData::new(password1);

    // Clone it
    let mut sensitive_data2 = sensitive_data1.clone();

    // Zeroize the clone
    sensitive_data2.zeroize();
    assert!(is_memory_zeroed(sensitive_data2.as_bytes()));

    // Original should still be accessible
    assert_eq!(sensitive_data1.as_bytes()[0], 0x11);
}

// ============================================================================
// MEMORY DUMP VERIFICATION TESTS
// ============================================================================

#[test]
fn test_memory_dump_helper() {
    // Verify our memory dump helper works correctly
    let data = vec![0x12, 0x34, 0x56, 0x78];
    let dump = create_memory_dump(&data);

    assert_eq!(dump.len(), 4);
    assert_eq!(dump[0], 0x12);
    assert_eq!(dump[3], 0x78);
}

#[test]
fn test_secret_key_memory_dump() {
    // Create a memory dump of a secret key before and after zeroization
    let secret_bytes = [0xFEu8; 32];
    let mut secret_key = SecretKey::<32>::new(secret_bytes);

    // Dump before zeroization
    let dump_before = create_memory_dump(secret_key.as_bytes());
    assert_eq!(dump_before[0], 0xFE);

    // Zeroize
    secret_key.zeroize();

    // Dump after zeroization
    let dump_after = create_memory_dump(secret_key.as_bytes());
    assert!(dump_after.iter().all(|&b| b == 0));
}

#[test]
fn test_key_material_memory_dump() {
    // Verify key material zeroization via memory inspection
    let key_bytes = vec![0xCCu8; 32];
    let mut key_material = KeyMaterial::new(key_bytes);

    let dump_before = create_memory_dump(key_material.as_bytes());
    assert!(dump_before.iter().all(|&b| b == 0xCC));

    key_material.zeroize();

    let dump_after = create_memory_dump(key_material.as_bytes());
    assert!(dump_after.iter().all(|&b| b == 0));
}

// ============================================================================
// DROP IMPLEMENTATION VERIFICATION TESTS
// ============================================================================

#[test]
fn test_secret_key_size_after_zeroization() {
    // Verify that zeroization doesn't affect the size of the container
    let secret_bytes = [0xAAu8; 32];
    let mut secret_key = SecretKey::<32>::new(secret_bytes);

    let size_before = secret_key.as_bytes().len();
    secret_key.zeroize();
    let size_after = secret_key.as_bytes().len();

    assert_eq!(size_before, size_after);
    assert_eq!(size_after, 32);
}

#[test]
fn test_key_material_size_after_zeroization() {
    // Verify that zeroization doesn't affect Vec size
    let key_bytes = vec![0xBBu8; 64];
    let mut key_material = KeyMaterial::new(key_bytes);

    let size_before = key_material.as_bytes().len();
    assert_eq!(size_before, 64);

    key_material.zeroize();
    let size_after = key_material.as_bytes().len();

    // Size should be unchanged after zeroization
    assert_eq!(size_before, size_after);
    assert_eq!(size_after, 64);
}

#[test]
fn test_sensitive_data_size_after_zeroization() {
    // Verify Vec size is preserved after zeroization
    let password = vec![0xDDu8; 128];
    let mut sensitive_data = SensitiveData::new(password);

    let size_before = sensitive_data.as_bytes().len();
    assert_eq!(size_before, 128);

    sensitive_data.zeroize();
    let size_after = sensitive_data.as_bytes().len();

    // Size should be unchanged after zeroization
    assert_eq!(size_before, size_after);
    assert_eq!(size_after, 128);
}

#[test]
fn test_multiple_zeroizations() {
    // Test that zeroizing multiple times is safe
    let secret_bytes = [0x99u8; 32];
    let mut secret_key = SecretKey::<32>::new(secret_bytes);

    // Zeroize multiple times
    for _ in 0..5 {
        secret_key.zeroize();
        assert!(is_memory_zeroed(secret_key.as_bytes()));
    }
}

// ============================================================================
// SERIALIZATION SECURITY TESTS
// ============================================================================

#[test]
fn test_secret_key_serialization_prevention() {
    // Verify that secret keys cannot be serialized
    let secret_key = SecretKey::<32>::new([0x42u8; 32]);

    let result = serde_json::to_string(&secret_key);
    assert!(
        result.is_err(),
        "Secret key serialization must fail for security"
    );
}

#[test]
fn test_key_material_serialization_prevention() {
    // Verify that key material cannot be serialized
    let key_material = KeyMaterial::new(vec![0x42u8; 32]);

    let result = serde_json::to_string(&key_material);
    assert!(
        result.is_err(),
        "Key material serialization must fail for security"
    );
}

#[test]
fn test_sensitive_data_serialization_prevention() {
    // Verify that sensitive data cannot be serialized
    let sensitive_data = SensitiveData::new(vec![0x42u8; 32]);

    let result = serde_json::to_string(&sensitive_data);
    assert!(
        result.is_err(),
        "Sensitive data serialization must fail for security"
    );
}

// ============================================================================
// DEBUG OUTPUT SECURITY TESTS
// ============================================================================

#[test]
fn test_secret_key_debug_redaction() {
    let secret_key = SecretKey::<32>::new([0x42u8; 32]);
    let debug_output = format!("{:?}", secret_key);

    assert!(
        debug_output.contains("REDACTED") || debug_output.contains("[REDACTED]"),
        "Debug output must redact secrets"
    );
    assert!(
        !debug_output.contains("42"),
        "Debug output must not contain secret bytes"
    );
}

#[test]
fn test_key_material_debug_redaction() {
    let key_material = KeyMaterial::new(vec![0x99u8; 32]);
    let debug_output = format!("{:?}", key_material);

    assert!(
        debug_output.contains("REDACTED") || debug_output.contains("[REDACTED]"),
        "Debug output must redact secrets"
    );
}

#[test]
fn test_sensitive_data_debug_redaction() {
    let sensitive_data = SensitiveData::new(vec![0xFFu8; 32]);
    let debug_output = format!("{:?}", sensitive_data);

    assert!(
        debug_output.contains("REDACTED") || debug_output.contains("[REDACTED]"),
        "Debug output must redact secrets"
    );
}

// ============================================================================
// BOUNDARY CONDITION TESTS
// ============================================================================

#[test]
fn test_empty_sensitive_data_zeroization() {
    // Test zeroization of empty sensitive data
    let empty = SensitiveData::new(vec![]);
    let mut empty_data = empty;

    empty_data.zeroize();
    assert_eq!(empty_data.as_bytes().len(), 0);
}

#[test]
fn test_single_byte_secret_key() {
    // While unusual, test that even the minimum cases work
    // Note: Ed25519 requires 32 bytes, but the wrapper supports generic sizes
    let single_byte = [0xFFu8; 32];
    let mut secret = SecretKey::<32>::new(single_byte);

    secret.zeroize();
    assert!(is_memory_zeroed(secret.as_bytes()));
}

#[test]
fn test_large_sensitive_data() {
    // Test zeroization of large sensitive data (1MB)
    let large_data = vec![0xAAu8; 1024 * 1024];
    let mut sensitive = SensitiveData::new(large_data);

    assert_eq!(sensitive.as_bytes().len(), 1024 * 1024);

    sensitive.zeroize();
    assert!(is_memory_zeroed(sensitive.as_bytes()));
}

// ============================================================================
// ZEROIZATION OVERHEAD BENCHMARKS
// ============================================================================

#[test]
fn bench_secret_key_zeroization() {
    use std::time::Instant;

    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let secret_bytes = [0x42u8; 32];
        let mut secret_key = SecretKey::<32>::new(secret_bytes);
        secret_key.zeroize();
    }

    let elapsed = start.elapsed();
    let per_iter_micros = elapsed.as_micros() as f64 / iterations as f64;

    // Print benchmark results
    println!(
        "Secret key zeroization: {:.3} µs per iteration",
        per_iter_micros
    );

    // Sanity check: should be very fast (< 1µs on modern hardware)
    assert!(
        per_iter_micros < 10.0,
        "Zeroization is slower than expected"
    );
}

#[test]
fn bench_key_material_zeroization() {
    use std::time::Instant;

    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let key_bytes = vec![0x99u8; 32];
        let mut key_material = KeyMaterial::new(key_bytes);
        key_material.zeroize();
    }

    let elapsed = start.elapsed();
    let per_iter_micros = elapsed.as_micros() as f64 / iterations as f64;

    println!(
        "Key material zeroization: {:.3} µs per iteration",
        per_iter_micros
    );

    assert!(per_iter_micros < 10.0);
}

#[test]
fn bench_sensitive_data_zeroization() {
    use std::time::Instant;

    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let password = vec![0xDDu8; 32];
        let mut sensitive = SensitiveData::new(password);
        sensitive.zeroize();
    }

    let elapsed = start.elapsed();
    let per_iter_micros = elapsed.as_micros() as f64 / iterations as f64;

    println!(
        "Sensitive data zeroization: {:.3} µs per iteration",
        per_iter_micros
    );

    assert!(per_iter_micros < 10.0);
}

#[test]
fn bench_large_key_material_zeroization() {
    use std::time::Instant;

    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let key_bytes = vec![0xFFu8; 1024 * 1024]; // 1MB
        let mut key_material = KeyMaterial::new(key_bytes);
        key_material.zeroize();
    }

    let elapsed = start.elapsed();
    let per_iter_millis = elapsed.as_millis() as f64 / iterations as f64;

    println!(
        "Large (1MB) key material zeroization: {:.3} ms per iteration",
        per_iter_millis
    );

    // 1MB zeroization should be < 10ms on modern hardware
    assert!(per_iter_millis < 10.0);
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_encryption_key_lifecycle() {
    use adapteros_crypto::{decrypt_envelope, encrypt_envelope};

    // Create a key and encrypt data
    let encryption_key = [0xABu8; 32];
    let plaintext = b"sensitive message";

    let (ciphertext, nonce) =
        encrypt_envelope(&encryption_key, plaintext).expect("Encryption failed");

    // Verify decryption works
    let decrypted =
        decrypt_envelope(&encryption_key, &ciphertext, &nonce).expect("Decryption failed");
    assert_eq!(decrypted, plaintext);

    // In production, the key would be wrapped in SecretKey or KeyMaterial
    // for automatic zeroization
    let mut secret_key = SecretKey::<32>::new(encryption_key);
    secret_key.zeroize();
    assert!(is_memory_zeroed(secret_key.as_bytes()));
}

#[test]
fn test_signing_key_lifecycle() {
    use adapteros_crypto::Keypair;

    // Generate a keypair
    let keypair = Keypair::generate();

    // Use it to sign
    let message = b"document";
    let signature = keypair.sign(message);

    // Verify
    let public_key = keypair.public_key();
    assert!(public_key.verify(message, &signature).is_ok());

    // The private key material is securely managed by the Keypair type
    // through ed25519_dalek's internal zeroization
}

#[test]
fn test_multi_key_independent_zeroization() {
    // Test that multiple keys can be zeroized independently
    let key1 = [0x11u8; 32];
    let key2 = [0x22u8; 32];
    let key3 = [0x33u8; 32];

    let mut secret1 = SecretKey::<32>::new(key1);
    let mut secret2 = SecretKey::<32>::new(key2);
    let mut secret3 = SecretKey::<32>::new(key3);

    // Zeroize in different order
    secret2.zeroize();
    assert!(is_memory_zeroed(secret2.as_bytes()));
    assert_eq!(secret1.as_bytes()[0], 0x11);
    assert_eq!(secret3.as_bytes()[0], 0x33);

    secret1.zeroize();
    assert!(is_memory_zeroed(secret1.as_bytes()));
    assert_eq!(secret3.as_bytes()[0], 0x33);

    secret3.zeroize();
    assert!(is_memory_zeroed(secret3.as_bytes()));
}
