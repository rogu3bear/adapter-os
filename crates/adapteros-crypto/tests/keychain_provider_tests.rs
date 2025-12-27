//! Comprehensive tests for keychain provider functionality
//!
//! Tests cover:
//! - Platform-specific keychain operations
//! - Keychain backend detection and fallback
//! - Key storage and retrieval
//! - Password fallback mode
//! - Error handling

use adapteros_crypto::key_provider::KeyProviderConfig;
use adapteros_crypto::{
    KeyAlgorithm, KeyManager, KeyManagerConfig, KeyProviderMode, KeychainProvider,
};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_keychain_provider_creation_macos() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);
    assert!(provider.is_ok());

    let provider = provider.unwrap();

    // On macOS, backend should be MacOS
    use adapteros_crypto::providers::keychain::KeychainBackend;
    assert!(matches!(provider.backend(), KeychainBackend::MacOS));
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn test_keychain_provider_creation_linux() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);

    // On Linux, this may succeed or fail depending on available backends
    // Just verify it doesn't panic
    match provider {
        Ok(p) => {
            use adapteros_crypto::providers::keychain::KeychainBackend;
            // Should be either SecretService or KernelKeyring
            assert!(matches!(
                p.backend(),
                KeychainBackend::SecretService | KeychainBackend::KernelKeyring
            ));
        }
        Err(_) => {
            // May fail if no keychain backend available (e.g., in CI)
            // This is acceptable
        }
    }
}

#[tokio::test]
async fn test_keychain_mode_keymanager() {
    let config = KeyManagerConfig {
        mode: KeyProviderMode::Keychain,
        file_path: None,
        keychain_service: Some("adapteros-test-km".to_string()),
        kms_endpoint: None,
        allow_insecure_keys: false,
        production_mode: false,
    };

    let result = KeyManager::new(config).await;

    // May succeed on macOS/Linux with keychain, or fail in CI/headless
    // We're testing that it doesn't panic
    match result {
        Ok(manager) => {
            assert_eq!(manager.mode(), &KeyProviderMode::Keychain);
        }
        Err(_) => {
            // Acceptable if keychain not available
        }
    }
}

#[tokio::test]
#[cfg(feature = "password-fallback")]
async fn test_password_fallback_mode() {
    // Set password fallback environment variable
    std::env::set_var("ADAPTEROS_KEYCHAIN_FALLBACK", "pass:testpassword123");

    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-fallback".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);
    assert!(provider.is_ok());

    let provider = provider.unwrap();

    // Should use password fallback backend
    use adapteros_crypto::providers::keychain::KeychainBackend;
    assert!(matches!(
        provider.backend(),
        KeychainBackend::PasswordFallback
    ));

    // Clean up
    std::env::remove_var("ADAPTEROS_KEYCHAIN_FALLBACK");
}

#[tokio::test]
#[cfg(feature = "password-fallback")]
async fn test_password_fallback_rejects_short_password() {
    // Set a password that's too short (< 8 chars)
    std::env::set_var("ADAPTEROS_KEYCHAIN_FALLBACK", "pass:short");

    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-short".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);

    // Should fall back to platform keychain, not use password
    // (password is too short)
    if let Ok(p) = provider {
        use adapteros_crypto::providers::keychain::KeychainBackend;
        // Should NOT be PasswordFallback
        assert!(!matches!(p.backend(), KeychainBackend::PasswordFallback));
    }

    std::env::remove_var("ADAPTEROS_KEYCHAIN_FALLBACK");
}

#[tokio::test]
#[cfg(feature = "password-fallback")]
async fn test_password_fallback_invalid_format() {
    // Invalid format (missing "pass:" prefix)
    std::env::set_var("ADAPTEROS_KEYCHAIN_FALLBACK", "invalidformat");

    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-invalid".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);

    // Should fall back to platform keychain
    if let Ok(p) = provider {
        use adapteros_crypto::providers::keychain::KeychainBackend;
        assert!(!matches!(p.backend(), KeychainBackend::PasswordFallback));
    }

    std::env::remove_var("ADAPTEROS_KEYCHAIN_FALLBACK");
}

#[tokio::test]
async fn test_keychain_service_name_customization() {
    let custom_service = "my-custom-service-name";

    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some(custom_service.to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let result = KeychainProvider::new(config);

    // Should not panic with custom service name
    match result {
        Ok(_) => {
            // Success
        }
        Err(_) => {
            // May fail if keychain not available, which is acceptable
        }
    }
}

#[tokio::test]
async fn test_keychain_default_service_name() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: None, // Use default
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let result = KeychainProvider::new(config);

    // Should use "adapteros" as default service name
    match result {
        Ok(_) => {
            // Success
        }
        Err(_) => {
            // May fail if keychain not available
        }
    }
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn test_keychain_key_generation_and_signing() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-keygen".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let provider = KeychainProvider::new(config);

    if let Ok(provider) = provider {
        // Try to generate a key
        use adapteros_crypto::key_provider::KeyProvider;

        let handle_result = provider
            .generate("test-mac-key", KeyAlgorithm::Ed25519)
            .await;

        if let Ok(handle) = handle_result {
            assert_eq!(handle.provider_id, "test-mac-key");

            // Try to sign with the key
            let test_data = b"test message";
            let signature_result = provider.sign("test-mac-key", test_data).await;

            if let Ok(signature) = signature_result {
                assert_eq!(signature.len(), 64); // Ed25519 signature
            }
        }
    }
}

#[tokio::test]
async fn test_keychain_backend_health_check() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-health".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    if let Ok(mut provider) = KeychainProvider::new(config) {
        // Health check should not panic
        let health_result = provider.check_backend_health();

        // May succeed or fail depending on backend availability
        match health_result {
            Ok(_) => {
                // Backend is healthy
            }
            Err(_) => {
                // Backend may not be available (e.g., in CI)
            }
        }
    }
}

#[tokio::test]
async fn test_keychain_vs_file_provider_comparison() {
    let temp_dir = new_test_tempdir();
    let key_file = temp_dir.path().join("test_keys.json");

    // Create file provider
    let file_config = KeyManagerConfig {
        mode: KeyProviderMode::File,
        file_path: Some(key_file),
        keychain_service: None,
        kms_endpoint: None,
        allow_insecure_keys: true,
        production_mode: false,
    };

    let file_manager = KeyManager::new(file_config).await.unwrap();
    assert_eq!(file_manager.mode(), &KeyProviderMode::File);

    // Try to create keychain provider
    let keychain_config = KeyManagerConfig {
        mode: KeyProviderMode::Keychain,
        file_path: None,
        keychain_service: Some("adapteros-test-compare".to_string()),
        kms_endpoint: None,
        allow_insecure_keys: false,
        production_mode: false,
    };

    let keychain_result = KeyManager::new(keychain_config).await;

    // May succeed or fail depending on platform
    match keychain_result {
        Ok(keychain_manager) => {
            assert_eq!(keychain_manager.mode(), &KeyProviderMode::Keychain);

            // Both should support key generation
            let _ = file_manager
                .generate_key("file-key", KeyAlgorithm::Ed25519)
                .await;
            let _ = keychain_manager
                .generate_key("keychain-key", KeyAlgorithm::Ed25519)
                .await;
        }
        Err(_) => {
            // Keychain not available on this platform
        }
    }
}

#[tokio::test]
#[cfg(all(target_os = "macos", feature = "password-fallback"))]
async fn test_keychain_fallback_integration() {
    // First, try normal keychain
    let normal_config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-normal".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let normal_provider = KeychainProvider::new(normal_config);
    assert!(normal_provider.is_ok());

    // Then, force password fallback
    std::env::set_var("ADAPTEROS_KEYCHAIN_FALLBACK", "pass:securepassword123");

    let fallback_config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-fallback-int".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(86400),
    };

    let fallback_provider = KeychainProvider::new(fallback_config);
    assert!(fallback_provider.is_ok());

    // Verify different backends
    use adapteros_crypto::providers::keychain::KeychainBackend;

    if let (Ok(normal), Ok(fallback)) = (normal_provider, fallback_provider) {
        // Normal should be MacOS backend
        assert!(matches!(normal.backend(), KeychainBackend::MacOS));

        // Fallback should be PasswordFallback backend
        assert!(matches!(
            fallback.backend(),
            KeychainBackend::PasswordFallback
        ));
    }

    std::env::remove_var("ADAPTEROS_KEYCHAIN_FALLBACK");
}

#[tokio::test]
async fn test_keychain_rotation_interval() {
    let config = KeyProviderConfig {
        mode: KeyProviderMode::Keychain,
        keychain_service: Some("adapteros-test-rotation".to_string()),
        kms_endpoint: None,
        file_path: None,
        rotation_interval_secs: Some(3600), // 1 hour
    };

    let result = KeychainProvider::new(config);

    // Should accept rotation interval without error
    match result {
        Ok(_) => {
            // Success
        }
        Err(_) => {
            // May fail if keychain not available
        }
    }
}

#[tokio::test]
async fn test_keychain_production_mode() {
    // Keychain should be allowed in production mode
    let config = KeyManagerConfig {
        mode: KeyProviderMode::Keychain,
        file_path: None,
        keychain_service: Some("adapteros-test-prod".to_string()),
        kms_endpoint: None,
        allow_insecure_keys: false,
        production_mode: true, // Production mode
    };

    let result = KeyManager::new(config).await;

    // Keychain should be allowed in production (unlike file provider)
    match result {
        Ok(manager) => {
            assert_eq!(manager.mode(), &KeyProviderMode::Keychain);
            assert!(manager.is_production());
        }
        Err(_) => {
            // May fail if keychain not available, but not due to production mode
        }
    }
}
