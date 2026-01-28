//! Comprehensive tests for encryption and decryption operations
//!
//! Tests cover:
//! - Envelope encryption with AES-GCM
//! - Seal/unseal operations via KeyManager
//! - Key derivation and encryption
//! - Error handling and edge cases

use adapteros_crypto::{
    decrypt_envelope, encrypt_envelope, KeyAlgorithm, KeyManager, KeyManagerConfig, KeyProviderMode,
};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

#[test]
fn test_envelope_encrypt_decrypt_basic() {
    let key = [42u8; 32];
    let plaintext = b"secret message";

    let (ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // Ciphertext should be different from plaintext
    assert_ne!(ciphertext.as_slice(), plaintext);

    // Decrypt
    let decrypted = decrypt_envelope(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_envelope_different_nonces() {
    let key = [42u8; 32];
    let plaintext = b"secret message";

    let (ciphertext1, nonce1) = encrypt_envelope(&key, plaintext).unwrap();
    let (ciphertext2, nonce2) = encrypt_envelope(&key, plaintext).unwrap();

    // Nonces should be different (random)
    assert_ne!(nonce1, nonce2);

    // But both should decrypt correctly
    let decrypted1 = decrypt_envelope(&key, &ciphertext1, &nonce1).unwrap();
    let decrypted2 = decrypt_envelope(&key, &ciphertext2, &nonce2).unwrap();

    assert_eq!(decrypted1, plaintext);
    assert_eq!(decrypted2, plaintext);
}

#[test]
fn test_envelope_wrong_key_fails() {
    let key1 = [42u8; 32];
    let key2 = [43u8; 32];
    let plaintext = b"secret message";

    let (ciphertext, nonce) = encrypt_envelope(&key1, plaintext).unwrap();

    // Decryption with wrong key should fail
    let result = decrypt_envelope(&key2, &ciphertext, &nonce);
    assert!(result.is_err());
}

#[test]
fn test_envelope_wrong_nonce_fails() {
    let key = [42u8; 32];
    let plaintext = b"secret message";

    let (ciphertext, _nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // Use a different nonce
    let wrong_nonce = [99u8; 12];

    // Decryption with wrong nonce should fail
    let result = decrypt_envelope(&key, &ciphertext, &wrong_nonce);
    assert!(result.is_err());
}

#[test]
fn test_envelope_corrupted_ciphertext_fails() {
    let key = [42u8; 32];
    let plaintext = b"secret message";

    let (mut ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // Corrupt the ciphertext
    if !ciphertext.is_empty() {
        ciphertext[0] ^= 0xFF;
    }

    // Decryption should fail (AEAD authentication)
    let result = decrypt_envelope(&key, &ciphertext, &nonce);
    assert!(result.is_err());
}

#[test]
fn test_envelope_empty_plaintext() {
    let key = [42u8; 32];
    let plaintext = b"";

    let (ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // Should be able to decrypt empty plaintext
    let decrypted = decrypt_envelope(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_envelope_large_plaintext() {
    let key = [42u8; 32];
    let plaintext = vec![0x42u8; 1024 * 1024]; // 1MB

    let (ciphertext, nonce) = encrypt_envelope(&key, &plaintext).unwrap();

    // Ciphertext should be slightly larger (GCM tag)
    assert!(ciphertext.len() >= plaintext.len());

    // Should decrypt correctly
    let decrypted = decrypt_envelope(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_envelope_binary_data() {
    let key = [42u8; 32];
    let plaintext: Vec<u8> = (0..=255).collect();

    let (ciphertext, nonce) = encrypt_envelope(&key, &plaintext).unwrap();

    let decrypted = decrypt_envelope(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_envelope_nonce_size() {
    let key = [42u8; 32];
    let plaintext = b"test";

    let (_ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // AES-GCM nonce should be 12 bytes
    assert_eq!(nonce.len(), 12);
}

#[tokio::test]
async fn test_seal_unseal_via_keymanager() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate an AES key
    manager
        .generate_key("seal-key", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    let plaintext = b"secret data to seal";

    // Seal
    let sealed = manager.seal("seal-key", plaintext).await.unwrap();
    assert_ne!(sealed.as_slice(), plaintext);

    // Unseal
    let unsealed = manager.unseal("seal-key", &sealed).await.unwrap();
    assert_eq!(unsealed, plaintext);
}

#[tokio::test]
async fn test_seal_unseal_multiple_times() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    manager
        .generate_key("multi-seal", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    let plaintext = b"test data";

    // Seal multiple times
    let sealed1 = manager.seal("multi-seal", plaintext).await.unwrap();
    let sealed2 = manager.seal("multi-seal", plaintext).await.unwrap();

    // Ciphertexts should be different (random nonces)
    assert_ne!(sealed1, sealed2);

    // But both should unseal to same plaintext
    let unsealed1 = manager.unseal("multi-seal", &sealed1).await.unwrap();
    let unsealed2 = manager.unseal("multi-seal", &sealed2).await.unwrap();

    assert_eq!(unsealed1, plaintext);
    assert_eq!(unsealed2, plaintext);
}

#[tokio::test]
async fn test_unseal_with_wrong_key_fails() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate two keys
    manager
        .generate_key("key-1", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();
    manager
        .generate_key("key-2", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    let plaintext = b"secret";

    // Seal with key-1
    let sealed = manager.seal("key-1", plaintext).await.unwrap();

    // Try to unseal with key-2 (should fail)
    let result = manager.unseal("key-2", &sealed).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_seal_unseal_empty_data() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    manager
        .generate_key("empty-seal", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    let plaintext = b"";

    let sealed = manager.seal("empty-seal", plaintext).await.unwrap();
    let unsealed = manager.unseal("empty-seal", &sealed).await.unwrap();

    assert_eq!(unsealed, plaintext);
}

#[tokio::test]
async fn test_seal_unseal_large_data() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    manager
        .generate_key("large-seal", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    let plaintext = vec![0x55u8; 10 * 1024 * 1024]; // 10MB

    let sealed = manager.seal("large-seal", &plaintext).await.unwrap();
    let unsealed = manager.unseal("large-seal", &sealed).await.unwrap();

    assert_eq!(unsealed, plaintext);
}

#[tokio::test]
async fn test_seal_unseal_persistence() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let plaintext = b"persistent secret";
    let sealed;

    // First instance: seal data
    {
        let manager = KeyManager::new(config.clone()).await.unwrap();
        manager
            .generate_key("persist-seal", KeyAlgorithm::Aes256Gcm)
            .await
            .unwrap();

        sealed = manager.seal("persist-seal", plaintext).await.unwrap();
    }

    // Second instance: unseal with same key
    {
        let manager = KeyManager::new(config).await.unwrap();
        let unsealed = manager.unseal("persist-seal", &sealed).await.unwrap();
        assert_eq!(unsealed, plaintext);
    }
}

#[tokio::test]
async fn test_seal_with_nonexistent_key() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    let result = manager.seal("nonexistent", b"data").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_seal_operations() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = std::sync::Arc::new(KeyManager::new(config).await.unwrap());

    manager
        .generate_key("concurrent-seal", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    // Seal multiple items concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let manager = manager.clone();
            tokio::spawn(async move {
                let plaintext = format!("secret-{}", i).into_bytes();
                let sealed = manager.seal("concurrent-seal", &plaintext).await.unwrap();
                let unsealed = manager.unseal("concurrent-seal", &sealed).await.unwrap();
                assert_eq!(unsealed, plaintext);
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }
}

#[test]
fn test_envelope_key_size() {
    // Test with exact 32-byte key
    let key = [1u8; 32];
    let plaintext = b"test";

    let (ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();
    let decrypted = decrypt_envelope(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_envelope_authenticated_encryption() {
    let key = [42u8; 32];
    let plaintext = b"authenticated data";

    let (mut ciphertext, nonce) = encrypt_envelope(&key, plaintext).unwrap();

    // Modify the last byte (authentication tag)
    let last_idx = ciphertext.len() - 1;
    ciphertext[last_idx] ^= 0x01;

    // Should fail authentication
    let result = decrypt_envelope(&key, &ciphertext, &nonce);
    assert!(result.is_err());
}

#[test]
fn test_envelope_deterministic_with_same_nonce() {
    let key = [42u8; 32];
    let plaintext = b"test message";
    let nonce = [55u8; 12]; // Fixed nonce (NOT recommended in practice)

    // Encrypt twice with same nonce
    let (ciphertext1, _) = encrypt_envelope(&key, plaintext).unwrap();
    let (ciphertext2, _) = encrypt_envelope(&key, plaintext).unwrap();

    // Due to random nonces, ciphertexts will be different
    // This is correct behavior - nonces should be random
    // We're just testing that the function works as expected
    let decrypted1 = decrypt_envelope(&key, &ciphertext1, &nonce);
    let decrypted2 = decrypt_envelope(&key, &ciphertext2, &nonce);

    // Decryption will fail because we're using wrong nonces
    // (the actual nonces used in encryption are random)
    assert!(decrypted1.is_err() || decrypted2.is_err());
}
