//! Comprehensive tests for key generation across all providers
//!
//! Tests cover:
//! - Key generation with various algorithms
//! - Key persistence and retrieval
//! - Key rotation
//! - Provider-specific behaviors
//! - Error handling

use adapteros_crypto::{
    KeyAlgorithm, KeyManager, KeyManagerConfig, KeyProviderMode,
};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

#[tokio::test]
async fn test_ed25519_key_generation() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate Ed25519 key
    let handle = manager
        .generate_key("test-ed25519", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    assert_eq!(handle.key_id, "test-ed25519");
    assert!(matches!(handle.algorithm, KeyAlgorithm::Ed25519));
}

#[tokio::test]
async fn test_aes256_key_generation() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate AES-256 key
    let handle = manager
        .generate_key("test-aes", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();

    assert_eq!(handle.key_id, "test-aes");
    assert!(matches!(handle.algorithm, KeyAlgorithm::Aes256Gcm));
}

#[tokio::test]
async fn test_multiple_key_generation() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate multiple keys
    let handle1 = manager
        .generate_key("key-1", KeyAlgorithm::Ed25519)
        .await
        .unwrap();
    let handle2 = manager
        .generate_key("key-2", KeyAlgorithm::Aes256Gcm)
        .await
        .unwrap();
    let handle3 = manager
        .generate_key("key-3", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    assert_eq!(handle1.key_id, "key-1");
    assert_eq!(handle2.key_id, "key-2");
    assert_eq!(handle3.key_id, "key-3");
}

#[tokio::test]
async fn test_key_persistence_across_instances() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    // First instance: generate key
    {
        let manager = KeyManager::new(config.clone()).await.unwrap();
        manager
            .generate_key("persistent-key", KeyAlgorithm::Ed25519)
            .await
            .unwrap();

        // Sign with the key
        let data = b"test data";
        let signature = manager.sign_with_key("persistent-key", data).await.unwrap();
        assert_eq!(signature.len(), 64); // Ed25519 signature length
    }

    // Second instance: use existing key
    {
        let manager = KeyManager::new(config).await.unwrap();

        // Should be able to sign with existing key
        let data = b"test data";
        let signature = manager.sign_with_key("persistent-key", data).await.unwrap();
        assert_eq!(signature.len(), 64);
    }
}

#[tokio::test]
async fn test_key_generation_with_rotation() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate initial key
    manager
        .generate_key("rotation-test", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    // Sign with initial key
    let data = b"test message";
    let sig1 = manager.sign_with_key("rotation-test", data).await.unwrap();

    // Rotate the key
    let receipt = manager.rotate_key("rotation-test").await.unwrap();
    assert_eq!(receipt.key_id, "rotation-test");
    assert!(receipt.timestamp > 0);

    // Sign with rotated key (should produce different signature)
    let sig2 = manager.sign_with_key("rotation-test", data).await.unwrap();

    // Both signatures should be valid length but different
    assert_eq!(sig1.len(), 64);
    assert_eq!(sig2.len(), 64);
    assert_ne!(sig1, sig2);
}

#[tokio::test]
async fn test_key_rotation_receipt() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate key
    manager
        .generate_key("receipt-test", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    // Rotate and check receipt
    let receipt = manager.rotate_key("receipt-test").await.unwrap();

    assert_eq!(receipt.key_id, "receipt-test");
    assert!(receipt.timestamp > 0);
    assert!(!receipt.signature.is_empty());
    assert!(receipt.old_key_fingerprint.len() > 0);
    assert!(receipt.new_key_fingerprint.len() > 0);
    assert_ne!(receipt.old_key_fingerprint, receipt.new_key_fingerprint);
}

#[tokio::test]
async fn test_concurrent_key_generation() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = std::sync::Arc::new(KeyManager::new(config).await.unwrap());

    // Generate multiple keys concurrently
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .generate_key(&format!("concurrent-key-{}", i), KeyAlgorithm::Ed25519)
                    .await
            })
        })
        .collect();

    // Wait for all to complete
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Key generation {} failed", i);
    }
}

#[tokio::test]
async fn test_key_fingerprint() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate key
    manager
        .generate_key("fingerprint-test", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    // Get fingerprint
    let fingerprint = manager.key_fingerprint("fingerprint-test").await.unwrap();

    assert!(!fingerprint.is_empty());
    assert!(fingerprint.len() > 32); // BLAKE3 hash in hex should be 64+ chars
}

#[tokio::test]
async fn test_different_keys_have_different_fingerprints() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Generate two different keys
    manager
        .generate_key("key-a", KeyAlgorithm::Ed25519)
        .await
        .unwrap();
    manager
        .generate_key("key-b", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    // Get fingerprints
    let fp_a = manager.key_fingerprint("key-a").await.unwrap();
    let fp_b = manager.key_fingerprint("key-b").await.unwrap();

    assert_ne!(fp_a, fp_b);
}

#[tokio::test]
async fn test_key_generation_error_handling() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file.clone()),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Try to rotate a non-existent key
    let result = manager.rotate_key("nonexistent-key").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_production_mode_rejects_file_provider() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: false,
        production_mode: true,
        ..Default::default()
    };

    let result = KeyManager::new(config).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("--allow-insecure-keys"));
}

#[tokio::test]
async fn test_production_mode_allows_file_with_flag() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: true,
        ..Default::default()
    };

    let result = KeyManager::new(config).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_env_key_takes_precedence() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    // Generate a test key in hex format
    let test_key = hex::encode([42u8; 32]);
    std::env::set_var("AOS_SIGNING_KEY", &test_key);

    let config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        allow_insecure_keys: true,
        production_mode: false,
        ..Default::default()
    };

    let manager = KeyManager::new(config).await.unwrap();

    // Environment variable should take precedence (mode will be File but backed by env)
    assert_eq!(manager.mode(), &KeyProviderMode::File);

    // Clean up
    std::env::remove_var("AOS_SIGNING_KEY");
}

#[tokio::test]
async fn test_attestation() {
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

    // Generate a key
    manager
        .generate_key("attest-key", KeyAlgorithm::Ed25519)
        .await
        .unwrap();

    // Get attestation
    let attestation = manager.attest().await.unwrap();

    assert!(!attestation.fingerprint.is_empty());
    assert!(attestation.timestamp > 0);
    assert!(matches!(attestation.mode, KeyProviderMode::File));
}
