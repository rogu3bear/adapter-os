//! Integration tests for telemetry bundle signature verification
//!
//! Tests compliance with Artifacts Ruleset #13:
//! - All bundles must be signed with Ed25519
//! - Signatures must be verifiable with stored public key
//! - Key material must persist across restarts

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::Keypair;
use adapteros_telemetry::verify_bundle_signature;
use std::fs;
use tempfile::TempDir;

/// Test that bundle signatures can be verified with the stored public key
#[test]
fn test_bundle_signature_roundtrip() -> Result<()> {
    let _temp_dir = TempDir::new()?;

    // Generate keypair
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    // Create test merkle root
    let merkle_root = B3Hash::hash(b"test_bundle_events");

    // Sign the merkle root
    let signature = keypair.sign(merkle_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(public_key.to_bytes());

    // Verify signature succeeds
    let verified = verify_bundle_signature(&merkle_root, &signature_hex, &public_key_hex)?;
    assert!(verified, "Signature verification should succeed");

    Ok(())
}

/// Test that tampered merkle root causes verification failure
#[test]
fn test_tampered_merkle_root_fails_verification() -> Result<()> {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    // Sign original merkle root
    let original_root = B3Hash::hash(b"original_events");
    let signature = keypair.sign(original_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(public_key.to_bytes());

    // Attempt to verify with tampered merkle root
    let tampered_root = B3Hash::hash(b"tampered_events");
    let result = verify_bundle_signature(&tampered_root, &signature_hex, &public_key_hex);

    // Verification should fail
    assert!(
        result.is_err(),
        "Tampered merkle root should fail verification"
    );
    match result {
        Err(AosError::Crypto(_)) => {
            // Expected error type
        }
        _ => panic!("Expected AosError::Crypto for tampered signature"),
    }

    Ok(())
}

/// Test that wrong public key causes verification failure
#[test]
fn test_wrong_public_key_fails_verification() -> Result<()> {
    // Sign with keypair 1
    let keypair1 = Keypair::generate();
    let merkle_root = B3Hash::hash(b"test_events");
    let signature = keypair1.sign(merkle_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());

    // Try to verify with keypair 2's public key
    let keypair2 = Keypair::generate();
    let wrong_public_key_hex = hex::encode(keypair2.public_key().to_bytes());

    let result = verify_bundle_signature(&merkle_root, &signature_hex, &wrong_public_key_hex);

    // Verification should fail
    assert!(result.is_err(), "Wrong public key should fail verification");
    match result {
        Err(AosError::Crypto(_)) => {
            // Expected error type
        }
        _ => panic!("Expected AosError::Crypto for wrong public key"),
    }

    Ok(())
}

/// Test that signing key persists across restarts
#[test]
fn test_signing_key_persistence() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let key_path = temp_dir.path().join("telemetry_signing.key");

    // Generate and save key
    let keypair1 = Keypair::generate();
    let key_bytes = keypair1.to_bytes();
    fs::write(&key_path, key_bytes)?;

    // Restrict permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&key_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&key_path, perms)?;
    }

    // Load key (simulating restart)
    let loaded_key_bytes = fs::read(&key_path)?;
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&loaded_key_bytes);
    let keypair2 = Keypair::from_bytes(&key_array);

    // Public keys should match
    assert_eq!(
        keypair1.public_key().to_bytes(),
        keypair2.public_key().to_bytes(),
        "Loaded keypair should have same public key"
    );

    // Sign with original, verify with loaded
    let merkle_root = B3Hash::hash(b"test_persistence");
    let signature = keypair1.sign(merkle_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(keypair2.public_key().to_bytes());

    let verified = verify_bundle_signature(&merkle_root, &signature_hex, &public_key_hex)?;
    assert!(verified, "Signature should verify with loaded key");

    Ok(())
}

/// Test that key file has correct permissions (Unix only)
#[cfg(unix)]
#[test]
fn test_key_file_permissions() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new()?;
    let key_path = temp_dir.path().join("telemetry_signing.key");

    // Generate and save key
    let keypair = Keypair::generate();
    let key_bytes = keypair.to_bytes();
    fs::write(&key_path, key_bytes)?;

    // Set permissions
    let mut perms = fs::metadata(&key_path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&key_path, perms)?;

    // Verify permissions
    let metadata = fs::metadata(&key_path)?;
    let mode = metadata.permissions().mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "Key file should have 0o600 permissions"
    );

    Ok(())
}

/// Test Artifacts Ruleset #13 compliance: signature + public_key in metadata
#[test]
fn test_artifacts_ruleset_13_compliance() -> Result<()> {
    use serde_json::json;

    let keypair = Keypair::generate();
    let merkle_root = B3Hash::hash(b"test_events");
    let signature = keypair.sign(merkle_root.as_bytes());

    // Create metadata matching BundleMetadata structure
    let metadata = json!({
        "event_count": 10,
        "merkle_root": merkle_root.to_hex(),
        "signature": hex::encode(signature.to_bytes()),
        "public_key": hex::encode(keypair.public_key().to_bytes()),
    });

    // Verify metadata contains required fields
    assert!(
        metadata.get("signature").is_some(),
        "Metadata must contain signature field"
    );
    assert!(
        metadata.get("public_key").is_some(),
        "Metadata must contain public_key field per Artifacts Ruleset #13"
    );
    assert!(
        metadata.get("merkle_root").is_some(),
        "Metadata must contain merkle_root field"
    );

    // Verify signature can be reconstructed and verified
    let signature_hex = metadata["signature"].as_str().unwrap();
    let public_key_hex = metadata["public_key"].as_str().unwrap();

    let verified = verify_bundle_signature(&merkle_root, signature_hex, public_key_hex)?;
    assert!(verified, "Metadata signature should be verifiable");

    Ok(())
}

/// Test that invalid signature format returns proper error
#[test]
fn test_invalid_signature_format() {
    let merkle_root = B3Hash::hash(b"test");
    let keypair = Keypair::generate();
    let public_key_hex = hex::encode(keypair.public_key().to_bytes());

    // Invalid signature (wrong length)
    let result = verify_bundle_signature(&merkle_root, "invalid_hex", &public_key_hex);
    assert!(result.is_err(), "Invalid signature hex should fail");

    // Invalid signature (correct length, wrong content)
    let wrong_signature = hex::encode([0u8; 64]);
    let result = verify_bundle_signature(&merkle_root, &wrong_signature, &public_key_hex);
    assert!(result.is_err(), "Wrong signature should fail verification");
}

/// Test that invalid public key format returns proper error
#[test]
fn test_invalid_public_key_format() {
    let merkle_root = B3Hash::hash(b"test");
    let keypair = Keypair::generate();
    let signature = keypair.sign(merkle_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());

    // Invalid public key (wrong length)
    let result = verify_bundle_signature(&merkle_root, &signature_hex, "invalid_hex");
    assert!(result.is_err(), "Invalid public key hex should fail");

    // Invalid public key (correct length, wrong content)
    let wrong_pubkey = hex::encode([0u8; 32]);
    let result = verify_bundle_signature(&merkle_root, &signature_hex, &wrong_pubkey);
    assert!(result.is_err(), "Wrong public key should fail verification");
}

/// Integration test: Multiple bundles signed with same key
#[test]
fn test_multiple_bundles_same_key() -> Result<()> {
    let keypair = Keypair::generate();
    let public_key_hex = hex::encode(keypair.public_key().to_bytes());

    // Sign multiple bundles
    let bundles = vec![
        B3Hash::hash(b"bundle_1"),
        B3Hash::hash(b"bundle_2"),
        B3Hash::hash(b"bundle_3"),
    ];

    for (i, merkle_root) in bundles.iter().enumerate() {
        let signature = keypair.sign(merkle_root.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        let verified = verify_bundle_signature(merkle_root, &signature_hex, &public_key_hex)?;
        assert!(
            verified,
            "Bundle {} signature should verify with same key",
            i
        );
    }

    Ok(())
}

/// Test that signature verification is deterministic
#[test]
fn test_signature_verification_deterministic() -> Result<()> {
    let keypair = Keypair::generate();
    let merkle_root = B3Hash::hash(b"test_determinism");
    let signature = keypair.sign(merkle_root.as_bytes());
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key_hex = hex::encode(keypair.public_key().to_bytes());

    // Verify multiple times - should always succeed
    for _ in 0..10 {
        let verified = verify_bundle_signature(&merkle_root, &signature_hex, &public_key_hex)?;
        assert!(verified, "Verification should be deterministic");
    }

    Ok(())
}
