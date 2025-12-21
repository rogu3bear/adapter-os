//! Tests for replay bundle signing and verification
//!
//! Verifies the cryptographic integrity of replay bundles through Ed25519
//! signature verification and double-sign protection.

use adapteros_crypto::signature::Keypair;
use adapteros_replay::{ReplayBundle, ReplaySignatureMetadata};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_replay_bundle_creation() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    // Create test bundle file
    fs::write(&bundle_path, b"test bundle content").unwrap();

    let bundle = ReplayBundle::new(&bundle_path);

    assert_eq!(bundle.bundle_path(), &bundle_path);
    assert!(!bundle.is_signed());
}

#[test]
fn test_replay_bundle_signing() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    assert!(!bundle.is_signed());

    let result = bundle.sign(&keypair);
    assert!(result.is_ok(), "Signing failed: {:?}", result);
    assert!(bundle.is_signed());

    // Signature file should exist
    let sig_path = bundle.signature_path();
    assert!(sig_path.exists());
}

#[test]
fn test_replay_bundle_double_sign_protection() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // First signing should succeed
    let result = bundle.sign(&keypair);
    assert!(result.is_ok(), "First signing failed: {:?}", result);

    // Second signing should fail
    let result = bundle.sign(&keypair);
    assert!(result.is_err(), "Expected double-sign to fail");

    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("already signed"),
        "Expected 'already signed' error, got: {}",
        error
    );
}

#[test]
fn test_replay_bundle_signature_verification() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Sign the bundle
    bundle.sign(&keypair).unwrap();

    // Verify signature with correct public key
    let public_key = keypair.public_key();
    let result = bundle.verify_signature(&public_key);
    assert!(
        result.is_ok(),
        "Signature verification failed: {:?}",
        result
    );
}

#[test]
fn test_replay_bundle_signature_verification_wrong_key() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();
    let wrong_keypair = Keypair::generate();

    // Sign with one keypair
    bundle.sign(&keypair).unwrap();

    // Verify with different public key should fail
    let wrong_public_key = wrong_keypair.public_key();
    let result = bundle.verify_signature(&wrong_public_key);
    assert!(
        result.is_err(),
        "Expected signature verification to fail with wrong key"
    );
}

#[test]
fn test_replay_bundle_verify_unsigned() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Verify unsigned bundle should fail
    let result = bundle.verify_signature(&keypair.public_key());
    assert!(
        result.is_err(),
        "Expected verification of unsigned bundle to fail"
    );

    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("not signed"),
        "Expected 'not signed' error, got: {}",
        error
    );
}

#[test]
fn test_replay_bundle_merkle_root() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Merkle root should be None before signing
    assert!(bundle.merkle_root().is_none());

    // Sign the bundle
    bundle.sign(&keypair).unwrap();

    // Merkle root should be available after signing
    assert!(bundle.merkle_root().is_some());
}

#[test]
fn test_replay_bundle_signature_persistence() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test bundle content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Sign the bundle
    bundle.sign(&keypair).unwrap();

    // Create a new bundle instance from the same path
    let loaded_bundle = ReplayBundle::new(&bundle_path);

    // Should be detected as signed
    assert!(loaded_bundle.is_signed());

    // Verification should succeed
    let result = loaded_bundle.verify_signature(&keypair.public_key());
    assert!(
        result.is_ok(),
        "Signature verification failed after reload: {:?}",
        result
    );
}

#[test]
fn test_signature_metadata_serialization() {
    let metadata = ReplaySignatureMetadata {
        merkle_root: "abc123".to_string(),
        signature: "def456".to_string(),
        public_key: "ghi789".to_string(),
        bundle_path: "/path/to/bundle.ndjson".to_string(),
        signed_at: 1234567890,
    };

    let serialized = serde_json::to_string(&metadata).expect("Failed to serialize");
    let deserialized: ReplaySignatureMetadata =
        serde_json::from_str(&serialized).expect("Failed to deserialize");

    assert_eq!(deserialized.merkle_root, metadata.merkle_root);
    assert_eq!(deserialized.signature, metadata.signature);
    assert_eq!(deserialized.public_key, metadata.public_key);
    assert_eq!(deserialized.bundle_path, metadata.bundle_path);
    assert_eq!(deserialized.signed_at, metadata.signed_at);
}

#[test]
fn test_replay_bundle_signature_file_location() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("my_bundle.ndjson");

    fs::write(&bundle_path, b"test content").unwrap();

    let bundle = ReplayBundle::new(&bundle_path);
    let sig_path = bundle.signature_path();

    // Signature path should be bundle path with .sig extension
    assert_eq!(
        sig_path,
        temp_dir.path().join("my_bundle.ndjson.sig")
    );
}

#[test]
fn test_replay_bundle_different_content_different_signature() {
    let temp_dir = tempdir().unwrap();
    let bundle_path1 = temp_dir.path().join("bundle1.ndjson");
    let bundle_path2 = temp_dir.path().join("bundle2.ndjson");

    fs::write(&bundle_path1, b"content 1").unwrap();
    fs::write(&bundle_path2, b"content 2").unwrap();

    let mut bundle1 = ReplayBundle::new(&bundle_path1);
    let mut bundle2 = ReplayBundle::new(&bundle_path2);
    let keypair = Keypair::generate();

    bundle1.sign(&keypair).unwrap();
    bundle2.sign(&keypair).unwrap();

    // Read signature files
    let sig1_content = fs::read_to_string(bundle1.signature_path()).unwrap();
    let sig2_content = fs::read_to_string(bundle2.signature_path()).unwrap();

    // Signatures should be different (different content)
    assert_ne!(sig1_content, sig2_content);
}

#[test]
fn test_replay_bundle_same_content_same_signature() {
    let temp_dir = tempdir().unwrap();
    let bundle_path1 = temp_dir.path().join("bundle1.ndjson");
    let bundle_path2 = temp_dir.path().join("bundle2.ndjson");

    let content = b"identical content";
    fs::write(&bundle_path1, content).unwrap();
    fs::write(&bundle_path2, content).unwrap();

    let mut bundle1 = ReplayBundle::new(&bundle_path1);
    let mut bundle2 = ReplayBundle::new(&bundle_path2);
    let keypair = Keypair::generate();

    bundle1.sign(&keypair).unwrap();
    bundle2.sign(&keypair).unwrap();

    // Read signature metadata
    let sig1_content = fs::read_to_string(bundle1.signature_path()).unwrap();
    let sig2_content = fs::read_to_string(bundle2.signature_path()).unwrap();

    let sig1_meta: ReplaySignatureMetadata = serde_json::from_str(&sig1_content).unwrap();
    let sig2_meta: ReplaySignatureMetadata = serde_json::from_str(&sig2_content).unwrap();

    // Merkle roots should be identical (same content)
    assert_eq!(sig1_meta.merkle_root, sig2_meta.merkle_root);

    // Signatures should be identical (same content, same key)
    assert_eq!(sig1_meta.signature, sig2_meta.signature);
}

#[test]
fn test_replay_bundle_tamper_detection() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"original content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Sign the bundle
    bundle.sign(&keypair).unwrap();

    // Tamper with the bundle content
    fs::write(&bundle_path, b"tampered content").unwrap();

    // Create new bundle instance pointing to tampered file
    let tampered_bundle = ReplayBundle::new(&bundle_path);

    // Verification should fail (content was changed)
    let result = tampered_bundle.verify_signature(&keypair.public_key());
    assert!(
        result.is_err(),
        "Expected signature verification to fail after tampering"
    );
}

#[test]
fn test_replay_bundle_signature_metadata_fields() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test content").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    bundle.sign(&keypair).unwrap();

    // Read and parse signature metadata
    let sig_content = fs::read_to_string(bundle.signature_path()).unwrap();
    let metadata: ReplaySignatureMetadata = serde_json::from_str(&sig_content).unwrap();

    // Verify metadata fields are populated
    assert!(!metadata.merkle_root.is_empty());
    assert!(!metadata.signature.is_empty());
    assert!(!metadata.public_key.is_empty());
    assert!(!metadata.bundle_path.is_empty());
    assert!(metadata.signed_at > 0);

    // Verify public key matches
    let expected_pubkey_hex = hex::encode(keypair.public_key().to_bytes());
    assert_eq!(metadata.public_key, expected_pubkey_hex);
}

#[test]
fn test_replay_bundle_concurrent_sign_protection() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("test_bundle.ndjson");

    fs::write(&bundle_path, b"test content").unwrap();

    let mut bundle1 = ReplayBundle::new(&bundle_path);
    let mut bundle2 = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // First bundle signs successfully
    let result1 = bundle1.sign(&keypair);
    assert!(result1.is_ok());

    // Second bundle should detect existing signature
    let result2 = bundle2.sign(&keypair);
    assert!(result2.is_err(), "Expected second sign to fail");
}

#[test]
fn test_replay_bundle_empty_content() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("empty_bundle.ndjson");

    // Create empty bundle
    fs::write(&bundle_path, b"").unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Should be able to sign empty bundle
    let result = bundle.sign(&keypair);
    assert!(result.is_ok(), "Signing empty bundle failed: {:?}", result);

    // Should be able to verify empty bundle
    let result = bundle.verify_signature(&keypair.public_key());
    assert!(
        result.is_ok(),
        "Verifying empty bundle failed: {:?}",
        result
    );
}

#[test]
fn test_replay_bundle_large_content() {
    let temp_dir = tempdir().unwrap();
    let bundle_path = temp_dir.path().join("large_bundle.ndjson");

    // Create large bundle (1MB)
    let large_content = vec![b'X'; 1024 * 1024];
    fs::write(&bundle_path, &large_content).unwrap();

    let mut bundle = ReplayBundle::new(&bundle_path);
    let keypair = Keypair::generate();

    // Should handle large content
    let result = bundle.sign(&keypair);
    assert!(result.is_ok(), "Signing large bundle failed: {:?}", result);

    let result = bundle.verify_signature(&keypair.public_key());
    assert!(
        result.is_ok(),
        "Verifying large bundle failed: {:?}",
        result
    );
}
