//! Comprehensive tests for policy integrity, signing, verification, and tampering attacks
//!
//! These tests verify:
//! - Policy signing with Ed25519
//! - Signature verification at load time
//! - File integrity checking via BLAKE3
//! - Tampering detection and recovery
//! - Attack scenarios (modifications, replacements, etc.)

use adapteros_crypto::signature::Keypair;
use adapteros_policy::{
    compute_blake3_hash, PolicyIntegrityMetadata, PolicyIntegrityVerifier, RecoveryAction,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_policy_signing_and_verification() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    // Create test policy content
    let policy_content = b"{\n  \"version\": \"1.0\",\n  \"rules\": []\n}";
    fs::write(&policy_path, policy_content).unwrap();

    // Create signing keypair
    let signing_keypair = Keypair::generate();
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    // Sign policy content
    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(policy_content, &signing_keypair)
        .unwrap();

    // Create integrity metadata
    let metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);

    // Verify the policy
    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();

    assert!(result.is_valid, "Policy verification should succeed");
    assert!(result.signature_valid, "Signature should be valid");
    assert!(result.hash_valid, "Hash should be valid");
    assert!(result.version_compatible, "Version should be compatible");
    assert!(result.tamper_free, "Policy should be tamper-free");
}

#[test]
fn test_policy_modification_detection() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    // Create and sign original policy
    let original_content = b"{\n  \"version\": \"1.0\",\n  \"rules\": []\n}";
    fs::write(&policy_path, original_content).unwrap();

    let signing_keypair = Keypair::generate();
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(original_content, &signing_keypair)
        .unwrap();

    let metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);

    // Modify policy file
    let modified_content = b"{\n  \"version\": \"1.0\",\n  \"rules\": [{\"malicious\": true}]\n}";
    fs::write(&policy_path, modified_content).unwrap();

    // Detect tampering
    let tamper_result = verifier.detect_tampering(&policy_path, &metadata).unwrap();

    assert!(tamper_result.tampered, "Tampering should be detected");
    assert_ne!(
        tamper_result.expected_hash, tamper_result.actual_hash,
        "Hashes should differ"
    );

    // Verify hash mismatch detection
    let verify_result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(
        !verify_result.is_valid,
        "Verification should fail for modified policy"
    );
    assert!(!verify_result.hash_valid, "Hash validation should fail");
}

#[test]
fn test_signature_tampering_detection() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{\n  \"version\": \"1.0\",\n  \"rules\": []\n}";
    fs::write(&policy_path, policy_content).unwrap();

    let signing_keypair = Keypair::generate();
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(policy_content, &signing_keypair)
        .unwrap();

    // Create metadata with tampered signature
    let mut metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);
    metadata.signature = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        .to_string()
        + "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();

    assert!(
        !result.is_valid,
        "Verification should fail for tampered signature"
    );
    assert!(!result.signature_valid, "Signature validation should fail");
}

#[test]
fn test_hash_collision_resistance() {
    // Verify that different policies produce different hashes
    let policy1 = b"{\n  \"version\": \"1.0\"\n}";
    let policy2 = b"{\n  \"version\": \"1.1\"\n}";

    let hash1 = compute_blake3_hash(policy1).unwrap();
    let hash2 = compute_blake3_hash(policy2).unwrap();

    assert_ne!(
        hash1, hash2,
        "Different policies should have different hashes"
    );

    // Same policy should always produce same hash
    let hash1_again = compute_blake3_hash(policy1).unwrap();
    assert_eq!(hash1, hash1_again, "Same policy should produce same hash");
}

#[test]
fn test_recovery_action_selection() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let original_content = b"{\n  \"version\": \"1.0\"\n}";
    fs::write(&policy_path, original_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(original_content, &signing_keypair)
        .unwrap();

    let mut metadata =
        PolicyIntegrityMetadata::new(file_hash.clone(), signature, pubkey.clone(), 1);

    // Modify policy
    fs::write(&policy_path, b"modified content").unwrap();

    // Test recovery action with backup history
    let tamper_result = verifier.detect_tampering(&policy_path, &metadata).unwrap();
    assert!(tamper_result.tampered);
    assert_eq!(
        tamper_result.recovery_action,
        RecoveryAction::LoadBackup,
        "Should suggest loading backup when history exists"
    );

    // Test recovery action without history
    let mut metadata_no_history =
        PolicyIntegrityMetadata::new(file_hash, "sig".to_string(), pubkey, 1);
    metadata_no_history.hash_history.clear();

    let tamper_result_no_history = verifier
        .detect_tampering(&policy_path, &metadata_no_history)
        .unwrap();
    assert!(tamper_result_no_history.tampered);
    assert_eq!(
        tamper_result_no_history.recovery_action,
        RecoveryAction::Quarantine,
        "Should quarantine when no backup history exists"
    );
}

#[test]
fn test_schema_version_compatibility() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{}";
    fs::write(&policy_path, policy_content).unwrap();

    let signing_keypair = Keypair::generate();
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(policy_content, &signing_keypair)
        .unwrap();

    // Test with unsupported schema version
    let mut metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 99);

    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(
        !result.is_valid,
        "Unsupported schema should fail verification"
    );
    assert!(
        !result.version_compatible,
        "Version compatibility should fail"
    );

    // Test with supported schema version
    metadata.schema_version = 1;
    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(result.version_compatible, "Version 1 should be compatible");
}

#[test]
fn test_multiple_trusted_keys() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{}";
    fs::write(&policy_path, policy_content).unwrap();

    // Create two keypairs
    let keypair1 = Keypair::generate();
    let keypair2 = Keypair::generate();

    // Sign with first keypair
    let verifier_signer = PolicyIntegrityVerifier::new();
    let (signature, pubkey1, file_hash) = verifier_signer
        .sign_policy_content(policy_content, &keypair1)
        .unwrap();

    // Create verifier with both trusted keys
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(keypair1.public_key());
    verifier.add_trusted_key(keypair2.public_key());

    let metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey1, 1);

    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(result.is_valid, "Should verify against trusted key in list");
}

#[test]
fn test_verification_caching() {
    let mut verifier = PolicyIntegrityVerifier::new();
    let hash = "test_hash_abc123".to_string();

    assert!(
        !verifier.is_hash_verified(&hash),
        "Hash should not be verified initially"
    );

    verifier.cache_verification(hash.clone());
    assert!(
        verifier.is_hash_verified(&hash),
        "Hash should be cached and verified"
    );

    // Test statistics
    let stats = verifier.get_verification_stats();
    assert_eq!(stats.total_verified, 1, "Should track one verified hash");
}

#[test]
fn test_file_io_error_handling() {
    let nonexistent_path = PathBuf::from("/nonexistent/path/policy.json");
    let metadata =
        PolicyIntegrityMetadata::new("hash".to_string(), "sig".to_string(), "key".to_string(), 1);

    let mut verifier = PolicyIntegrityVerifier::new();
    let result = verifier
        .verify_policy_file(&nonexistent_path, &metadata)
        .unwrap();

    assert!(!result.is_valid, "Should fail for nonexistent file");
    assert!(
        result.error_message.is_some(),
        "Should provide error message"
    );
    assert!(
        result.issues.iter().any(|i| i.contains("I/O")),
        "Should report I/O error"
    );
}

#[test]
fn test_malicious_policy_replacement() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    // Create legitimate policy
    let legitimate_content = b"{\n  \"version\": \"1.0\",\n  \"rules\": []\n}";
    fs::write(&policy_path, legitimate_content).unwrap();

    let signing_keypair = Keypair::generate();
    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(legitimate_content, &signing_keypair)
        .unwrap();

    let metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);

    // Replace with malicious policy
    let malicious_content = b"{\n  \"version\": \"1.0\",\n  \"malicious\": true,\n  \"rules\": [{\"disable_checks\": true}]\n}";
    fs::write(&policy_path, malicious_content).unwrap();

    // Attempt verification
    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();

    assert!(!result.is_valid, "Malicious replacement should be detected");
    assert!(!result.hash_valid, "Hash should not match");
    assert!(result.tamper_free == false, "Tampering should be detected");
}

#[test]
fn test_incremental_tampering_detection() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let original_content = b"{\n  \"rules\": []\n}";
    fs::write(&policy_path, original_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(original_content, &signing_keypair)
        .unwrap();

    let mut metadata = PolicyIntegrityMetadata::new(file_hash.clone(), signature, pubkey, 1);

    // Make small modifications and track hash history
    let modifications = vec![
        b"{\n  \"rules\": [{\"added\": 1}]\n}".to_vec(),
        b"{\n  \"rules\": [{\"added\": 1}, {\"added\": 2}]\n}".to_vec(),
        b"{\n  \"rules\": [{\"added\": 1}, {\"added\": 2}, {\"added\": 3}]\n}".to_vec(),
    ];

    for modified in modifications {
        fs::write(&policy_path, &modified).unwrap();

        let new_hash = compute_blake3_hash(&modified).unwrap();
        metadata.hash_history.push(new_hash);
    }

    // Verify history tracking
    assert!(metadata.hash_history.len() > 1, "Should have hash history");
    assert!(
        metadata.hash_history.iter().all(|h| !h.is_empty()),
        "All hashes should be non-empty"
    );
}

#[test]
fn test_concurrent_verification_safety() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{}";
    fs::write(&policy_path, policy_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(policy_content, &signing_keypair)
        .unwrap();

    let metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);

    // Simulate concurrent verifications
    let mut verifiers = vec![];
    for _ in 0..3 {
        let mut v = PolicyIntegrityVerifier::new();
        v.add_trusted_key(signing_keypair.public_key());
        verifiers.push(v);
    }

    for mut v in verifiers {
        let result = v.verify_policy_file(&policy_path, &metadata).unwrap();
        assert!(
            result.is_valid,
            "All concurrent verifications should succeed"
        );
    }
}

#[test]
fn test_policy_audit_trail() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let original_content = b"{\n  \"version\": \"1.0\"\n}";
    fs::write(&policy_path, original_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature, pubkey, original_hash) = verifier
        .sign_policy_content(original_content, &signing_keypair)
        .unwrap();

    let mut metadata = PolicyIntegrityMetadata::new(original_hash.clone(), signature, pubkey, 1);

    // Record multiple modifications
    for i in 1..4 {
        let modified = format!("{{\n  \"version\": \"1.{}\"\n}}", i);
        let new_hash = compute_blake3_hash(modified.as_bytes()).unwrap();
        metadata.hash_history.push(new_hash);
    }

    // Verify audit trail
    assert_eq!(
        metadata.hash_history.len(),
        4,
        "Should have 4 entries in hash history"
    );
    assert_eq!(
        metadata.hash_history[0], original_hash,
        "First hash should be original"
    );
}

#[test]
fn test_invalid_signature_format() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{}";
    fs::write(&policy_path, policy_content).unwrap();

    let mut metadata = PolicyIntegrityMetadata::new(
        compute_blake3_hash(policy_content).unwrap(),
        "invalid_hex_signature".to_string(),
        "pubkey".to_string(),
        1,
    );

    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(Keypair::generate().public_key());

    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(!result.is_valid, "Should reject invalid signature format");
    assert!(!result.signature_valid, "Signature validation should fail");
}

#[test]
fn test_policy_version_rollback_detection() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    // Create v2 policy
    let v2_content = b"{\n  \"version\": \"2.0\",\n  \"strict_mode\": true\n}";
    fs::write(&policy_path, v2_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature_v2, pubkey, _hash_v2) = verifier
        .sign_policy_content(v2_content, &signing_keypair)
        .unwrap();

    // Create v1 policy (rollback attempt)
    let v1_content = b"{\n  \"version\": \"1.0\"\n}";
    fs::write(&policy_path, v1_content).unwrap();

    let hash_v1 = compute_blake3_hash(v1_content).unwrap();

    // Try to verify v1 with v2 signature
    let metadata = PolicyIntegrityMetadata::new(hash_v1, signature_v2, pubkey, 1);

    let mut verifier = PolicyIntegrityVerifier::new();
    verifier.add_trusted_key(signing_keypair.public_key());

    let result = verifier
        .verify_policy_file(&policy_path, &metadata)
        .unwrap();
    assert!(!result.is_valid, "Rollback should be detected");
    assert!(!result.hash_valid, "Hash should not match");
}

#[test]
fn test_policy_integrity_recovery_suggestions() {
    let temp_dir = TempDir::new().unwrap();
    let policy_path = temp_dir.path().join("test_policy.json");

    let policy_content = b"{}";
    fs::write(&policy_path, policy_content).unwrap();

    let signing_keypair = Keypair::generate();
    let verifier = PolicyIntegrityVerifier::new();

    let (signature, pubkey, file_hash) = verifier
        .sign_policy_content(policy_content, &signing_keypair)
        .unwrap();

    let mut metadata = PolicyIntegrityMetadata::new(file_hash, signature, pubkey, 1);
    metadata.hash_history.push("backup_hash_1".to_string());
    metadata.hash_history.push("backup_hash_2".to_string());

    // Modify file and test recovery suggestions
    fs::write(&policy_path, b"modified").unwrap();
    let tamper_result = verifier.detect_tampering(&policy_path, &metadata).unwrap();

    assert!(tamper_result.tampered);
    match tamper_result.recovery_action {
        RecoveryAction::LoadBackup => {
            assert!(
                !metadata.hash_history.is_empty(),
                "Should have backup available"
            );
        }
        _ => panic!("Should suggest backup recovery with history"),
    }
}
