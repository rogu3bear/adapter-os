// Regression test: Telemetry bundle verification from metadata files only
//
// This test suite verifies that telemetry bundles can be cryptographically
// verified using ONLY the metadata files persisted to disk, without requiring
// access to the original signing keypair in memory.
//
// Expected failure modes (if public key is missing from metadata):
// - Verification after BundleWriter is dropped
// - Verification in a fresh process/session
// - Chain verification across bundle boundaries

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_telemetry::bundle::{BundleWriter, SignatureMetadata};
use adapteros_telemetry::bundle_store::{BundleStore, RetentionPolicy};
use adapteros_telemetry::unified_events::TelemetryEvent;
use adapteros_telemetry::verify_bundle_signature;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Helper: Create a test event
fn create_test_event(seq: u64, data: &str) -> TelemetryEvent {
    use adapteros_telemetry::unified_events::LogLevel;
    use chrono::Utc;

    TelemetryEvent {
        id: format!("test_event_{}", seq),
        timestamp: Utc::now(),
        event_type: "test_event".to_string(),
        level: LogLevel::Info,
        message: format!("Test event {}", seq),
        component: Some("test".to_string()),
        tenant_id: None,
        user_id: None,
        metadata: Some(json!({
            "sequence": seq,
            "data": data,
        })),
        trace_id: None,
        span_id: None,
        event_hash: None,
    }
}

// Helper: Load signature metadata from .ndjson.sig file
fn load_signature_metadata(sig_file: &Path) -> Result<SignatureMetadata> {
    let contents = fs::read_to_string(sig_file)
        .map_err(|e| AosError::Io(format!("Failed to read sig file: {}", e)))?;

    serde_json::from_str(&contents).map_err(AosError::Serialization)
}

/// Test 1: Two sequential bundles with in-memory verification (baseline)
///
/// This test verifies that bundle signing and verification works when the
/// BundleWriter is still in scope and we have access to the keypair.
#[tokio::test]
async fn test_two_bundles_in_memory_verification() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    eprintln!("Creating BundleWriter with generated keypair");
    let mut writer = BundleWriter::new(
        bundle_dir.clone(),
        5,           // max_events per bundle
        1024 * 1024, // max_bytes
    )?;

    eprintln!("Writing 6 events (will create 2 bundles)");
    for i in 0..6 {
        writer.write_event(&create_test_event(i, &format!("event_{}", i)))?;
    }

    // Force rotation to ensure we have 2 complete bundles
    writer.rotate_bundle()?;

    eprintln!("Verifying bundles while BundleWriter is in scope");

    // Find all .ndjson.sig files
    let sig_files: Vec<PathBuf> = fs::read_dir(&bundle_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "sig" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert_eq!(sig_files.len(), 2, "Expected 2 signature files");

    // Verify each bundle signature
    for sig_file in &sig_files {
        eprintln!("Verifying signature file: {:?}", sig_file);
        let sig_meta = load_signature_metadata(sig_file)?;

        let merkle_root = B3Hash::from_hex(&sig_meta.merkle_root)
            .map_err(|e| AosError::Validation(format!("Invalid merkle_root hex: {}", e)))?;

        let is_valid =
            verify_bundle_signature(&merkle_root, &sig_meta.signature, &sig_meta.public_key)?;

        assert!(
            is_valid,
            "Bundle signature verification failed for {:?}",
            sig_file
        );
        eprintln!("✓ Bundle verified successfully");
    }

    eprintln!("✅ All bundles verified with in-memory keypair");
    Ok(())
}

/// Test 2: Verification after BundleWriter is dropped (no in-memory keypair)
///
/// This test demonstrates whether bundle verification works when:
/// 1. Bundles are created and signed
/// 2. BundleWriter is dropped (keypair no longer in memory)
/// 3. We attempt to verify bundles using ONLY persisted metadata
///
/// Expected to FAIL if public_key is not properly persisted to metadata files.
#[tokio::test]
async fn test_verification_after_writer_drop() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    let public_key_hex: String;
    let sig_files: Vec<PathBuf>;

    // Scope 1: Create bundles and let BundleWriter drop
    {
        eprintln!("Creating BundleWriter and generating bundles");
        let mut writer = BundleWriter::new(
            bundle_dir.clone(),
            3, // max_events per bundle
            1024 * 1024,
        )?;

        // Capture public key before writer is dropped
        public_key_hex = writer.public_key();

        // Write 7 events to create 3 bundles
        for i in 0..7 {
            writer.write_event(&create_test_event(i, &format!("data_{}", i)))?;
        }

        writer.rotate_bundle()?;

        // Find signature files before drop
        sig_files = fs::read_dir(&bundle_dir)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension()? == "sig" {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        eprintln!("Created {} bundles", sig_files.len());
        assert!(sig_files.len() >= 2, "Expected at least 2 bundles");
    } // BundleWriter dropped here - keypair no longer in memory

    eprintln!("BundleWriter dropped - attempting verification from metadata files only");

    // Scope 2: Verify bundles without BundleWriter (keypair not in memory)
    for sig_file in &sig_files {
        eprintln!(
            "Verifying {:?} from metadata only",
            sig_file.file_name().unwrap()
        );

        let sig_meta = load_signature_metadata(sig_file)?;

        // Check if public_key is present in signature metadata
        if sig_meta.public_key.is_empty() {
            return Err(AosError::Validation(
                "❌ REGRESSION DETECTED: public_key field is EMPTY in .ndjson.sig file".to_string(),
            ));
        }

        // Verify that the persisted public key matches the original
        assert_eq!(
            sig_meta.public_key, public_key_hex,
            "Public key in metadata doesn't match original keypair"
        );

        let merkle_root = B3Hash::from_hex(&sig_meta.merkle_root)
            .map_err(|e| AosError::Validation(format!("Invalid merkle_root: {}", e)))?;

        // Attempt verification using ONLY metadata (no in-memory keypair)
        let is_valid =
            verify_bundle_signature(&merkle_root, &sig_meta.signature, &sig_meta.public_key)?;

        if !is_valid {
            return Err(AosError::Validation(format!(
                "❌ REGRESSION: Bundle verification FAILED from metadata for {:?}",
                sig_file
            )));
        }

        eprintln!("✓ Bundle verified successfully from metadata");
    }

    eprintln!("✅ All bundles verified after BundleWriter drop (metadata-only verification)");
    Ok(())
}

/// Test 3: Chain verification from metadata only
///
/// Verifies that:
/// 1. Multiple bundles link correctly via prev_bundle_hash
/// 2. All signatures can be verified from metadata
/// 3. Chain integrity is maintained
#[tokio::test]
async fn test_chain_verification_from_metadata() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    eprintln!("Creating chain of 3 bundles");

    let sig_files: Vec<PathBuf>;

    // Create bundles
    {
        let mut writer = BundleWriter::new(
            bundle_dir.clone(),
            2, // max_events per bundle (create more bundles)
            1024 * 1024,
        )?;

        // Write 7 events to create 4 bundles
        for i in 0..7 {
            writer.write_event(&create_test_event(i, &format!("chain_event_{}", i)))?;
        }

        writer.rotate_bundle()?;

        sig_files = fs::read_dir(&bundle_dir)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.extension()? == "sig" {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        eprintln!("Created {} bundles in chain", sig_files.len());
    } // Drop writer

    eprintln!("Verifying chain from metadata files only");

    // Load all signature metadata
    let mut sig_metadata: Vec<(PathBuf, SignatureMetadata)> = sig_files
        .iter()
        .map(|path| {
            let meta = load_signature_metadata(path)?;
            Ok((path.clone(), meta))
        })
        .collect::<Result<Vec<_>>>()?;

    // Sort by sequence number to verify chain order
    sig_metadata.sort_by_key(|(_, meta)| meta.sequence_no);

    assert!(sig_metadata.len() >= 3, "Expected at least 3 bundles");

    // Verify each bundle's signature
    for (path, meta) in &sig_metadata {
        let merkle_root = B3Hash::from_hex(&meta.merkle_root)
            .map_err(|e| AosError::Validation(format!("Invalid merkle_root: {}", e)))?;

        if meta.public_key.is_empty() {
            return Err(AosError::Validation(format!(
                "❌ REGRESSION: public_key missing in {:?}",
                path.file_name().unwrap()
            )));
        }

        let is_valid = verify_bundle_signature(&merkle_root, &meta.signature, &meta.public_key)?;

        assert!(
            is_valid,
            "Signature verification failed for sequence {}",
            meta.sequence_no
        );
        eprintln!("✓ Bundle {} signature verified", meta.sequence_no);
    }

    // Verify chain links
    for i in 1..sig_metadata.len() {
        let (_, current) = &sig_metadata[i];
        let (_, prev) = &sig_metadata[i - 1];

        if let Some(prev_hash) = &current.prev_bundle_hash {
            let prev_merkle = B3Hash::from_hex(&prev.merkle_root)
                .map_err(|e| AosError::Validation(format!("Invalid prev merkle_root: {}", e)))?;

            assert_eq!(
                prev_hash, &prev_merkle,
                "Chain link broken between bundle {} and {}",
                prev.sequence_no, current.sequence_no
            );
            eprintln!(
                "✓ Chain link verified: {} → {}",
                prev.sequence_no, current.sequence_no
            );
        } else {
            return Err(AosError::Validation(format!(
                "❌ REGRESSION: prev_bundle_hash missing for bundle {}",
                current.sequence_no
            )));
        }
    }

    eprintln!("✅ Complete chain verified from metadata only");
    Ok(())
}

/// Test 4: Simulate fresh process (no in-memory state)
///
/// Simulates starting a new process with only the bundle files on disk:
/// 1. Create bundles in temp directory
/// 2. Create a NEW BundleStore instance (fresh process simulation)
/// 3. Load bundles and verify signatures
///
/// This proves that verification works in a realistic production scenario
/// where the original signing keypair is not available.
#[tokio::test]
async fn test_fresh_process_verification() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    let cpid = "test-cpid";
    let tenant_id = "tenant-a";

    eprintln!("Phase 1: Create bundles (simulating original process)");

    // Phase 1: Original process creates bundles
    {
        let mut writer = BundleWriter::new(bundle_dir.clone(), 3, 1024 * 1024)?;

        for i in 0..8 {
            writer.write_event(&create_test_event(i, &format!("fresh_process_{}", i)))?;
        }

        writer.rotate_bundle()?;
        eprintln!("Original process created bundles");
    } // Original process ends, all in-memory state lost

    eprintln!("Phase 2: Fresh process loads bundles from disk");

    // Phase 2: Fresh process starts, creates new BundleStore
    let store = BundleStore::new(temp_dir.path().to_path_buf(), RetentionPolicy::default())?;

    // Manually register bundles from signature files
    // (In production, this would be done during initialization)
    let sig_files: Vec<PathBuf> = fs::read_dir(&bundle_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "sig" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    eprintln!("Found {} bundles in storage", sig_files.len());
    assert!(sig_files.len() >= 2, "Expected at least 2 bundles");

    // Verify each bundle from metadata
    for sig_file in &sig_files {
        eprintln!(
            "Fresh process verifying {:?}",
            sig_file.file_name().unwrap()
        );

        let sig_meta = load_signature_metadata(sig_file)?;

        // Critical: public_key must be available in persisted metadata
        if sig_meta.public_key.is_empty() {
            return Err(AosError::Validation(
                "❌ FATAL: Cannot verify bundle in fresh process - public_key missing from metadata".to_string()
            ));
        }

        let merkle_root = B3Hash::from_hex(&sig_meta.merkle_root)
            .map_err(|e| AosError::Validation(format!("Invalid merkle_root: {}", e)))?;

        // Fresh process has NO access to original keypair
        // Verification must work from metadata alone
        let is_valid =
            verify_bundle_signature(&merkle_root, &sig_meta.signature, &sig_meta.public_key)?;

        if !is_valid {
            return Err(AosError::Validation(
                "❌ REGRESSION: Fresh process cannot verify bundle signature from metadata"
                    .to_string(),
            ));
        }

        eprintln!("✓ Fresh process verified bundle successfully");
    }

    eprintln!("✅ Fresh process verified all bundles from persisted metadata");
    Ok(())
}

/// Test 5: Document current metadata coverage
///
/// Inspects the actual metadata files to document what fields are persisted
/// and whether they contain all necessary verification data.
#[tokio::test]
async fn test_document_metadata_coverage() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    eprintln!("Creating test bundle to inspect metadata coverage");

    let public_key_from_writer: Vec<u8>;

    {
        let mut writer = BundleWriter::new(bundle_dir.clone(), 5, 1024 * 1024)?;

        public_key_from_writer = hex::decode(&writer.public_key())
            .map_err(|e| AosError::Validation(format!("Invalid public_key hex: {}", e)))?;

        for i in 0..3 {
            writer.write_event(&create_test_event(i, &format!("coverage_test_{}", i)))?;
        }

        writer.rotate_bundle()?;
    }

    eprintln!("\n=== METADATA COVERAGE REPORT ===\n");

    // Find signature file
    let sig_file = fs::read_dir(&bundle_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "sig" {
                Some(path)
            } else {
                None
            }
        })
        .next()
        .expect("No signature file found");

    eprintln!("Inspecting: {:?}", sig_file.file_name().unwrap());

    let sig_meta = load_signature_metadata(&sig_file)?;

    eprintln!("SignatureMetadata fields:");
    eprintln!(
        "  ✓ merkle_root: {} (length: {})",
        sig_meta.merkle_root,
        sig_meta.merkle_root.len()
    );
    eprintln!(
        "  ✓ signature: {} (length: {})",
        sig_meta.signature,
        sig_meta.signature.len()
    );
    eprintln!(
        "  ✓ public_key: {} (length: {})",
        sig_meta.public_key,
        sig_meta.public_key.len()
    );
    eprintln!("  ✓ event_count: {}", sig_meta.event_count);
    eprintln!("  ✓ sequence_no: {}", sig_meta.sequence_no);
    eprintln!("  ✓ prev_bundle_hash: {:?}", sig_meta.prev_bundle_hash);

    // Verify public_key is not empty
    assert!(!sig_meta.public_key.is_empty(), "public_key field is EMPTY");

    // Verify public_key matches the writer's keypair
    let public_key_from_metadata = hex::decode(&sig_meta.public_key)
        .map_err(|e| AosError::Validation(format!("Invalid public_key hex: {}", e)))?;

    assert_eq!(
        public_key_from_metadata, public_key_from_writer,
        "Public key in metadata doesn't match writer's keypair"
    );

    // Verify signature is valid length (64 bytes for Ed25519)
    let signature_bytes = hex::decode(&sig_meta.signature)
        .map_err(|e| AosError::Validation(format!("Invalid signature hex: {}", e)))?;
    assert_eq!(signature_bytes.len(), 64, "Signature should be 64 bytes");

    // Verify merkle_root is valid BLAKE3 hash (32 bytes)
    let merkle_bytes = hex::decode(&sig_meta.merkle_root)
        .map_err(|e| AosError::Validation(format!("Invalid merkle_root hex: {}", e)))?;
    assert_eq!(merkle_bytes.len(), 32, "Merkle root should be 32 bytes");

    eprintln!("\n=== VERIFICATION TEST ===\n");

    // Attempt verification using persisted metadata
    let merkle_root = B3Hash::from_hex(&sig_meta.merkle_root)?;
    let is_valid =
        verify_bundle_signature(&merkle_root, &sig_meta.signature, &sig_meta.public_key)?;

    if is_valid {
        eprintln!("✅ METADATA COVERAGE: COMPLETE");
        eprintln!("   All necessary fields are persisted for verification:");
        eprintln!("   - merkle_root (BLAKE3 hash)");
        eprintln!("   - signature (Ed25519 signature)");
        eprintln!("   - public_key (Ed25519 public key)");
        eprintln!("   - Chain metadata (prev_bundle_hash, sequence_no)");
        eprintln!("\n   Bundle verification from metadata: WORKING ✓");
    } else {
        return Err(AosError::Validation(
            "❌ METADATA COVERAGE: INCOMPLETE - Verification failed".to_string(),
        ));
    }

    eprintln!("\n================================\n");

    Ok(())
}

/// Test 6: Negative test - Modified signature should fail verification
///
/// Ensures that tampered bundles are properly rejected.
#[tokio::test]
async fn test_tampered_signature_fails_verification() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    eprintln!("Creating bundle for tampering test");

    {
        let mut writer = BundleWriter::new(bundle_dir.clone(), 5, 1024 * 1024)?;

        for i in 0..3 {
            writer.write_event(&create_test_event(i, "tamper_test"))?;
        }

        writer.rotate_bundle()?;
    }

    let sig_file = fs::read_dir(&bundle_dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "sig" {
                Some(path)
            } else {
                None
            }
        })
        .next()
        .unwrap();

    let sig_meta = load_signature_metadata(&sig_file)?;

    // Tamper with the merkle root
    let tampered_merkle = B3Hash::hash(b"tampered_data");

    eprintln!("Attempting verification with tampered merkle root");

    let result =
        verify_bundle_signature(&tampered_merkle, &sig_meta.signature, &sig_meta.public_key);

    // Verification should fail for tampered data
    assert!(
        result.is_err(),
        "❌ SECURITY ISSUE: Tampered signature passed verification!"
    );

    eprintln!("✅ Tampered signature correctly rejected");
    Ok(())
}
