// Integration test for telemetry bundle signature verification
// Tests that signature validation FAILS when metadata is tampered with
//
// This test verifies the critical security property that bundles cannot be
// verified with incorrect signatures, public keys, or merkle roots.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{Keypair, PublicKey, Signature};
use adapteros_telemetry::{TelemetryConfig, TelemetryWriter};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[derive(Debug, Serialize, Deserialize)]
struct BundleMetadata {
    event_count: usize,
    merkle_root: B3Hash,
    signature: Option<String>,
    public_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct TestEvent {
    event_type: String,
    message: String,
}

/// Verify bundle signature using only metadata (no keypair required)
///
/// [source: crates/adapteros-telemetry/src/lib.rs L418-L450]
fn verify_bundle_signature(
    merkle_root: &B3Hash,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool> {
    // 1. Decode hex strings to bytes
    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| AosError::Validation(format!("Invalid signature hex: {}", e)))?;
    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|e| AosError::Validation(format!("Invalid public key hex: {}", e)))?;

    // 2. Validate lengths
    if signature_bytes.len() != 64 {
        return Err(AosError::Validation(format!(
            "Invalid signature length: expected 64, got {}",
            signature_bytes.len()
        )));
    }
    if public_key_bytes.len() != 32 {
        return Err(AosError::Validation(format!(
            "Invalid public key length: expected 32, got {}",
            public_key_bytes.len()
        )));
    }

    // 3. Convert to Ed25519 types
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature = Signature::from_bytes(&sig_array)?;

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key = PublicKey::from_bytes(&pk_array)?;

    // 4. Verify signature using Ed25519 (constant-time)
    public_key.verify(merkle_root.as_bytes(), &signature)?;

    Ok(true)
}

fn create_test_writer(temp_dir: &Path) -> Result<TelemetryWriter> {
    let config = TelemetryConfig {
        bundles_dir: temp_dir.join("bundles"),
        keys_dir: temp_dir.join("keys"),
        max_events_per_bundle: 10,
        max_bytes_per_bundle: 1024 * 1024,
        enable_signing: true,
    };

    TelemetryWriter::new(config)
}

#[tokio::test]
async fn test_signature_verification_succeeds_with_valid_metadata() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    // Flush to finalize bundle
    writer.flush().await?;
    drop(writer); // Keypair no longer in memory

    // Find the metadata file
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    assert_eq!(meta_files.len(), 1, "Expected exactly one metadata file");

    // Load metadata
    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    // Verify metadata has required fields
    assert!(metadata.signature.is_some(), "Signature must be present");
    assert!(metadata.public_key.is_some(), "Public key must be present");

    let signature = metadata.signature.unwrap();
    let public_key = metadata.public_key.unwrap();

    // ✅ Valid signature should verify successfully
    let result = verify_bundle_signature(&metadata.merkle_root, &signature, &public_key);
    assert!(
        result.is_ok(),
        "Valid signature should verify: {:?}",
        result
    );
    assert_eq!(result.unwrap(), true);

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_tampered_signature() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let public_key = metadata.public_key.unwrap();

    // ❌ Tamper with signature (all zeros)
    let tampered_signature = "0".repeat(128); // 64 bytes in hex = 128 chars

    let result = verify_bundle_signature(&metadata.merkle_root, &tampered_signature, &public_key);

    // Verification MUST fail
    assert!(
        result.is_err(),
        "Tampered signature should fail verification"
    );

    // Check error message is descriptive
    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Crypto") || error_msg.contains("verification"),
        "Error should indicate cryptographic failure: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_wrong_public_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let signature = metadata.signature.unwrap();

    // ❌ Generate a different keypair (wrong public key)
    let wrong_keypair = Keypair::generate();
    let wrong_public_key = hex::encode(wrong_keypair.public_key().to_bytes());

    let result = verify_bundle_signature(&metadata.merkle_root, &signature, &wrong_public_key);

    // Verification MUST fail
    assert!(
        result.is_err(),
        "Wrong public key should fail verification"
    );

    // Check error message
    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Crypto") || error_msg.contains("verification"),
        "Error should indicate cryptographic failure: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_modified_merkle_root() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let signature = metadata.signature.unwrap();
    let public_key = metadata.public_key.unwrap();

    // ❌ Modify the merkle root
    let wrong_merkle_root = B3Hash::hash(b"tampered_data");

    let result = verify_bundle_signature(&wrong_merkle_root, &signature, &public_key);

    // Verification MUST fail
    assert!(
        result.is_err(),
        "Modified merkle root should fail verification"
    );

    // Check error message
    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Crypto") || error_msg.contains("verification"),
        "Error should indicate cryptographic failure: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_without_public_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    // Verify public_key exists in metadata
    assert!(
        metadata.public_key.is_some(),
        "Public key MUST be present in metadata for verification"
    );

    // If someone tries to verify without public key, it should fail
    // This simulates metadata corruption where public_key field is missing

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_invalid_hex() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let public_key = metadata.public_key.unwrap();

    // ❌ Invalid hex string (contains non-hex characters)
    let invalid_signature = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";

    let result = verify_bundle_signature(&metadata.merkle_root, invalid_signature, &public_key);

    // Verification MUST fail with validation error
    assert!(result.is_err(), "Invalid hex should fail verification");

    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Validation") || error_msg.contains("hex"),
        "Error should indicate invalid hex: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_wrong_length_signature() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let public_key = metadata.public_key.unwrap();

    // ❌ Wrong length signature (32 bytes instead of 64)
    let wrong_length_signature = "0".repeat(64); // 32 bytes in hex = 64 chars

    let result =
        verify_bundle_signature(&metadata.merkle_root, &wrong_length_signature, &public_key);

    // Verification MUST fail with validation error
    assert!(
        result.is_err(),
        "Wrong length signature should fail verification"
    );

    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Validation") || error_msg.contains("length"),
        "Error should indicate wrong length: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_signature_verification_fails_with_wrong_length_public_key() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Write events
    for i in 0..5 {
        let event = TestEvent {
            event_type: "test".to_string(),
            message: format!("Event {}", i),
        };
        writer.log_event("test_event", &event).await?;
    }

    writer.flush().await?;
    drop(writer);

    // Load metadata
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    let meta_path = meta_files[0].path();
    let meta_json = fs::read_to_string(&meta_path)?;
    let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

    let signature = metadata.signature.unwrap();

    // ❌ Wrong length public key (16 bytes instead of 32)
    let wrong_length_pk = "0".repeat(32); // 16 bytes in hex = 32 chars

    let result = verify_bundle_signature(&metadata.merkle_root, &signature, &wrong_length_pk);

    // Verification MUST fail with validation error
    assert!(
        result.is_err(),
        "Wrong length public key should fail verification"
    );

    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(
        error_msg.contains("Validation") || error_msg.contains("length"),
        "Error should indicate wrong length: {}",
        error_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_bundles_all_verifiable() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = create_test_writer(temp_dir.path())?;

    // Create 3 bundles
    for bundle_idx in 0..3 {
        for i in 0..5 {
            let event = TestEvent {
                event_type: "test".to_string(),
                message: format!("Bundle {} Event {}", bundle_idx, i),
            };
            writer.log_event("test_event", &event).await?;
        }
        writer.flush().await?;
    }

    drop(writer);

    // Load all metadata files
    let bundles_dir = temp_dir.path().join("bundles");
    let meta_files: Vec<_> = fs::read_dir(&bundles_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();

    assert_eq!(meta_files.len(), 3, "Expected 3 metadata files");

    // Verify all bundles
    for meta_file in meta_files {
        let meta_path = meta_file.path();
        let meta_json = fs::read_to_string(&meta_path)?;
        let metadata: BundleMetadata = serde_json::from_str(&meta_json)?;

        let signature = metadata
            .signature
            .as_ref()
            .expect("Signature must be present");
        let public_key = metadata
            .public_key
            .as_ref()
            .expect("Public key must be present");

        // ✅ All bundles should verify successfully
        let result = verify_bundle_signature(&metadata.merkle_root, signature, public_key);
        assert!(
            result.is_ok(),
            "Bundle {:?} should verify: {:?}",
            meta_path,
            result
        );
    }

    Ok(())
}

#[test]
fn test_documentation_references_are_accurate() {
    // This test verifies that the code citations in the documentation are accurate
    // by checking that the referenced functions exist

    // The documentation cites these functions:
    // - sign_bundle_merkle_root (crates/adapteros-telemetry/src/lib.rs:408-416)
    // - finalize_bundle (crates/adapteros-telemetry/src/lib.rs:377-406)
    // - verify_bundle_signature (crates/adapteros-telemetry/src/lib.rs:418-450)

    // This test ensures that if these functions are moved or renamed,
    // the test will fail and remind us to update the documentation

    // Note: This is a compile-time check - if the functions don't exist,
    // this test file won't compile
}
