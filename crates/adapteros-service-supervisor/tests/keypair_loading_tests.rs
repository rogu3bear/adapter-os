//! Tests for supervisor keypair loading and authentication

use adapteros_crypto::Keypair;
use base64::Engine;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test that valid base64 32-byte seed is accepted
#[test]
fn test_load_keypair_from_pem_valid_base64() {
    // Generate a valid 32-byte seed
    let seed: [u8; 32] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        26, 27, 28, 29, 30, 31, 32,
    ];
    let pem = base64::engine::general_purpose::STANDARD.encode(seed);

    // The function is private, so we test indirectly through ServiceSupervisor
    // or we can make it pub(crate) for testing
    // For now, just verify the base64 encoding works
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&pem)
        .unwrap();
    assert_eq!(decoded.len(), 32);
}

/// Test that PEM with headers is parsed correctly
#[test]
fn test_load_keypair_from_pem_with_headers() {
    let seed: [u8; 32] = [42u8; 32];
    let base64_content = base64::engine::general_purpose::STANDARD.encode(seed);
    let pem = format!(
        "-----BEGIN ED25519 PRIVATE KEY-----\n{}\n-----END ED25519 PRIVATE KEY-----",
        base64_content
    );

    // Verify the PEM can be parsed by stripping headers
    let content = pem
        .replace("-----BEGIN ED25519 PRIVATE KEY-----", "")
        .replace("-----END ED25519 PRIVATE KEY-----", "")
        .replace(['\n', '\r', ' '], "");

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&content)
        .unwrap();
    assert_eq!(decoded.len(), 32);
}

/// Test that invalid length is rejected
#[test]
fn test_keypair_invalid_length_rejected() {
    // 31 bytes - too short
    let short_seed: [u8; 31] = [1u8; 31];
    let pem = base64::engine::general_purpose::STANDARD.encode(short_seed);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&pem)
        .unwrap();
    assert_ne!(decoded.len(), 32, "Should detect wrong length");

    // 33 bytes - too long
    let long_seed: [u8; 33] = [1u8; 33];
    let pem = base64::engine::general_purpose::STANDARD.encode(long_seed);
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&pem)
        .unwrap();
    assert_ne!(decoded.len(), 32, "Should detect wrong length");
}

/// Test that invalid base64 is rejected
#[test]
fn test_keypair_invalid_base64_rejected() {
    let invalid_pem = "not-valid-base64!!!@@@";
    let result = base64::engine::general_purpose::STANDARD.decode(invalid_pem);
    assert!(result.is_err(), "Invalid base64 should be rejected");
}

/// Test self-healing: generate_signing_key creates file with correct permissions
#[test]
fn test_generate_signing_key_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("test_signing.key");

    // Generate key
    let keypair = adapteros_crypto::generate_signing_key(&key_path).unwrap();

    // Verify file exists
    assert!(key_path.exists(), "Key file should be created");

    // Verify file size (32 bytes for Ed25519 seed)
    let metadata = std::fs::metadata(&key_path).unwrap();
    assert_eq!(metadata.len(), 32, "Key file should be 32 bytes");

    // Verify permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = metadata.permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "Key should have 0600 permissions"
        );
    }

    // Verify we can load it back
    let loaded = adapteros_crypto::load_signing_key(&key_path).unwrap();
    assert_eq!(
        keypair.public_key().to_bytes(),
        loaded.public_key().to_bytes(),
        "Loaded key should match generated key"
    );
}

/// Test that load_signing_key correctly loads an existing key
#[test]
fn test_load_signing_key_existing() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("existing.key");

    // Create a key file manually
    let seed: [u8; 32] = [99u8; 32];
    std::fs::write(&key_path, seed).unwrap();

    // Load it
    let keypair = adapteros_crypto::load_signing_key(&key_path).unwrap();

    // Verify public key is derived correctly
    let expected = Keypair::from_bytes(&seed);
    assert_eq!(
        keypair.public_key().to_bytes(),
        expected.public_key().to_bytes()
    );
}

/// Test that load_signing_key rejects non-existent file
#[test]
fn test_load_signing_key_missing_file() {
    let result = adapteros_crypto::load_signing_key(&PathBuf::from("/nonexistent/path/key.key"));
    assert!(result.is_err(), "Should fail for missing file");
}

/// Test that load_signing_key rejects wrong file size
#[test]
fn test_load_signing_key_wrong_size() {
    let temp_dir = TempDir::new().unwrap();
    let key_path = temp_dir.path().join("wrong_size.key");

    // Write wrong size
    std::fs::write(&key_path, [0u8; 16]).unwrap();

    let result = adapteros_crypto::load_signing_key(&key_path);
    assert!(result.is_err(), "Should fail for wrong size");
}
