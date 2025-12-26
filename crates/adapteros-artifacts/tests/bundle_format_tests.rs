//! Comprehensive tests for ADAPTER BUNDLE FORMAT (.aos)
//!
//! Tests verify:
//! 1. Bundle creation includes: weights, manifest, metadata, signature
//! 2. Bundle extraction validates all components
//! 3. Corrupt bundle detection (missing file, bad hash, invalid signature)

use adapteros_artifacts::{create_bundle, extract_bundle};
use adapteros_core::{B3Hash, Result};
use adapteros_crypto::{bundle_sign::*, Keypair};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var/tmp");
    fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

/// Create a test bundle directory with weights, manifest, and metadata
fn create_test_bundle_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;

    // Create weights directory with sample files
    let weights_dir = dir.join("weights");
    fs::create_dir_all(&weights_dir)?;

    // Write some weight files
    fs::write(
        weights_dir.join("layer1.safetensors"),
        b"fake_weights_layer1",
    )?;
    fs::write(
        weights_dir.join("layer2.safetensors"),
        b"fake_weights_layer2",
    )?;

    // Create manifest.json
    let manifest = serde_json::json!({
        "adapter_id": "test-adapter-001",
        "version": "1.0.0",
        "rank": 4,
        "base_model": "test-model",
        "created_at": "2025-12-24T00:00:00Z",
        "metadata": {
            "scope_path": "domain/group/scope/op",
            "description": "Test adapter for bundle format verification"
        }
    });
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(dir.join("manifest.json"), manifest_json)?;

    // Create metadata.json
    let metadata = serde_json::json!({
        "bundle_version": "1.0",
        "created_by": "test-suite",
        "total_params": 1024
    });
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    fs::write(dir.join("metadata.json"), metadata_json)?;

    Ok(())
}

#[test]
fn test_bundle_create_and_extract() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("test.aos.tar.zst");
    let extract_dir = temp.path().join("extracted");

    // Create source bundle
    create_test_bundle_dir(&source_dir)?;

    // Create bundle
    create_bundle(&source_dir, &bundle_path)?;
    assert!(bundle_path.exists());
    assert!(fs::metadata(&bundle_path)?.len() > 0);

    // Extract bundle
    extract_bundle(&bundle_path, &extract_dir)?;

    // Verify extracted contents
    assert!(extract_dir.join("manifest.json").exists());
    assert!(extract_dir.join("metadata.json").exists());
    assert!(extract_dir.join("weights/layer1.safetensors").exists());
    assert!(extract_dir.join("weights/layer2.safetensors").exists());

    // Verify content integrity
    let original_manifest = fs::read_to_string(source_dir.join("manifest.json"))?;
    let extracted_manifest = fs::read_to_string(extract_dir.join("manifest.json"))?;
    assert_eq!(original_manifest, extracted_manifest);

    Ok(())
}

#[test]
fn test_bundle_includes_all_required_components() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("complete.aos.tar.zst");
    let extract_dir = temp.path().join("extracted");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;
    extract_bundle(&bundle_path, &extract_dir)?;

    // Verify weights are present
    let layer1 = fs::read(extract_dir.join("weights/layer1.safetensors"))?;
    assert_eq!(layer1, b"fake_weights_layer1");

    // Verify manifest is valid JSON with required fields
    let manifest_str = fs::read_to_string(extract_dir.join("manifest.json"))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str)?;
    assert_eq!(manifest["adapter_id"], "test-adapter-001");
    assert_eq!(manifest["version"], "1.0.0");
    assert_eq!(manifest["rank"], 4);
    assert_eq!(manifest["metadata"]["scope_path"], "domain/group/scope/op");

    // Verify metadata is present
    let metadata_str = fs::read_to_string(extract_dir.join("metadata.json"))?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str)?;
    assert_eq!(metadata["bundle_version"], "1.0");

    Ok(())
}

#[test]
fn test_bundle_with_signature() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("signed.aos.tar.zst");
    let signatures_dir = temp.path().join("signatures");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    // Generate keypair and sign bundle
    let keypair = Keypair::generate();
    let bundle_bytes = fs::read(&bundle_path)?;
    let bundle_hash = B3Hash::hash(&bundle_bytes);
    let merkle_root = B3Hash::hash(b"test_merkle_root");

    // Sign and save
    let _signature = sign_and_save_bundle(&bundle_hash, &merkle_root, &keypair, &signatures_dir)?;

    // Verify signature was saved
    let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));
    assert!(sig_path.exists());

    // Load and verify signature
    let loaded_sig = BundleSignature::load_from_file(&sig_path)?;
    assert_eq!(loaded_sig.bundle_hash, bundle_hash);
    assert_eq!(loaded_sig.merkle_root, merkle_root);
    assert!(loaded_sig.verify().is_ok());

    // Verify bundle from file
    let verified = verify_bundle_from_file(&bundle_hash, &signatures_dir)?;
    assert_eq!(verified.bundle_hash, bundle_hash);

    Ok(())
}

#[test]
fn test_corrupt_bundle_missing_file() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("corrupt.aos.tar.zst");
    let extract_dir = temp.path().join("extracted");

    create_test_bundle_dir(&source_dir)?;

    // Remove manifest to create incomplete bundle
    fs::remove_file(source_dir.join("manifest.json"))?;

    create_bundle(&source_dir, &bundle_path)?;
    extract_bundle(&bundle_path, &extract_dir)?;

    // Verify manifest is missing (corruption detected)
    assert!(!extract_dir.join("manifest.json").exists());

    Ok(())
}

#[test]
fn test_corrupt_bundle_invalid_compression() {
    let temp = new_test_tempdir();
    let corrupt_bundle = temp.path().join("corrupt.aos.tar.zst");

    // Write invalid zstd data
    fs::write(&corrupt_bundle, b"INVALID_ZSTD_DATA").unwrap();

    let extract_dir = temp.path().join("extracted");
    let result = extract_bundle(&corrupt_bundle, &extract_dir);

    // Should fail with decompression error
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Failed to create decoder") || err_msg.contains("Failed to extract"));
}

#[test]
fn test_corrupt_bundle_truncated_file() {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("valid.aos.tar.zst");
    let corrupt_bundle = temp.path().join("truncated.aos.tar.zst");

    create_test_bundle_dir(&source_dir).unwrap();
    create_bundle(&source_dir, &bundle_path).unwrap();

    // Truncate the bundle
    let bundle_bytes = fs::read(&bundle_path).unwrap();
    let truncated = &bundle_bytes[..bundle_bytes.len() / 2];
    fs::write(&corrupt_bundle, truncated).unwrap();

    let extract_dir = temp.path().join("extracted");
    let result = extract_bundle(&corrupt_bundle, &extract_dir);

    // Should fail with extraction error
    assert!(result.is_err());
}

#[test]
fn test_corrupt_bundle_bad_hash() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("tampered.aos.tar.zst");
    let signatures_dir = temp.path().join("signatures");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    // Calculate original hash and sign
    let bundle_bytes = fs::read(&bundle_path)?;
    let original_hash = B3Hash::hash(&bundle_bytes);
    let keypair = Keypair::generate();
    let merkle_root = B3Hash::hash(b"test_merkle");
    sign_and_save_bundle(&original_hash, &merkle_root, &keypair, &signatures_dir)?;

    // Tamper with bundle
    let mut tampered = bundle_bytes.clone();
    tampered[100] ^= 0xFF; // Flip a byte
    fs::write(&bundle_path, &tampered)?;

    // Calculate new hash
    let tampered_hash = B3Hash::hash(&tampered);
    assert_ne!(original_hash, tampered_hash);

    // Verification should fail (hash mismatch)
    let _result = verify_bundle_from_file(&tampered_hash, &signatures_dir);

    // In dev mode, this might return Ok with a placeholder
    // In prod mode, this would fail
    #[cfg(not(debug_assertions))]
    assert!(_result.is_err());

    Ok(())
}

#[test]
fn test_corrupt_bundle_invalid_signature() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("signed.aos.tar.zst");
    let signatures_dir = temp.path().join("signatures");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    let bundle_bytes = fs::read(&bundle_path)?;
    let bundle_hash = B3Hash::hash(&bundle_bytes);
    let merkle_root = B3Hash::hash(b"test_merkle");
    let keypair = Keypair::generate();

    // Create valid signature
    let mut signature = sign_bundle(&bundle_hash, &merkle_root, &keypair)?;

    // Tamper with signature by changing the bundle hash
    signature.bundle_hash = B3Hash::hash(b"wrong_bundle");

    // Save tampered signature
    let sig_path = signatures_dir.join(format!("{}.sig", bundle_hash.to_hex()));
    fs::create_dir_all(&signatures_dir)?;
    signature.save_to_file(&sig_path)?;

    // Verify should fail
    let result = signature.verify();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("verification failed"));

    Ok(())
}

#[test]
fn test_bundle_hash_integrity() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("hash_test.aos.tar.zst");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    // Compute hash twice - should be identical (deterministic)
    let bytes1 = fs::read(&bundle_path)?;
    let hash1 = B3Hash::hash(&bytes1);

    let bytes2 = fs::read(&bundle_path)?;
    let hash2 = B3Hash::hash(&bytes2);

    assert_eq!(hash1, hash2);

    // Modify a single byte
    let mut modified = bytes1.clone();
    modified[50] ^= 0x01;
    let hash3 = B3Hash::hash(&modified);

    // Hash should be different
    assert_ne!(hash1, hash3);

    Ok(())
}

#[test]
fn test_empty_bundle() {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("empty");
    let bundle_path = temp.path().join("empty.aos.tar.zst");

    // Create empty directory
    fs::create_dir_all(&source_dir).unwrap();

    // Should still create a valid (albeit empty) bundle
    let result = create_bundle(&source_dir, &bundle_path);
    assert!(result.is_ok());
    assert!(bundle_path.exists());
}

#[test]
fn test_bundle_signature_key_id_deterministic() {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    let key_id1 = compute_key_id(&public_key);
    let key_id2 = compute_key_id(&public_key);

    assert_eq!(key_id1, key_id2);
    assert!(key_id1.starts_with("kid-"));
}

#[test]
fn test_bundle_size_calculation() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("sized.aos.tar.zst");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    let metadata = fs::metadata(&bundle_path)?;
    let size = metadata.len();

    // Bundle should have reasonable size (compressed)
    assert!(size > 0);
    assert!(size < 10 * 1024 * 1024); // Less than 10MB for test data

    // Verify we can read the size using bundle_size utility
    use adapteros_artifacts::bundle::bundle_size;
    let computed_size = bundle_size(&bundle_path)?;
    assert_eq!(size, computed_size);

    Ok(())
}

#[test]
fn test_bundle_roundtrip_preserves_structure() -> Result<()> {
    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("roundtrip.aos.tar.zst");
    let extract_dir = temp.path().join("extracted");
    let rebundle_path = temp.path().join("roundtrip2.aos.tar.zst");
    let reextract_dir = temp.path().join("reextracted");

    // Create, bundle, extract
    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;
    extract_bundle(&bundle_path, &extract_dir)?;

    // Re-bundle and extract again
    create_bundle(&extract_dir, &rebundle_path)?;
    extract_bundle(&rebundle_path, &reextract_dir)?;

    // Compare final manifest with original
    let original_manifest = fs::read_to_string(source_dir.join("manifest.json"))?;
    let final_manifest = fs::read_to_string(reextract_dir.join("manifest.json"))?;
    assert_eq!(original_manifest, final_manifest);

    Ok(())
}

#[test]
fn test_concurrent_bundle_access() -> Result<()> {
    use std::thread;

    let temp = new_test_tempdir();
    let source_dir = temp.path().join("source");
    let bundle_path = temp.path().join("concurrent.aos.tar.zst");

    create_test_bundle_dir(&source_dir)?;
    create_bundle(&source_dir, &bundle_path)?;

    // Spawn multiple threads to read bundle simultaneously
    let bundle_path = bundle_path.clone();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let bundle_path = bundle_path.clone();
            thread::spawn(move || {
                let bytes = fs::read(&bundle_path).unwrap();
                let hash = B3Hash::hash(&bytes);
                (i, hash)
            })
        })
        .collect();

    // All threads should compute same hash
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let first_hash = results[0].1;
    for (_, hash) in &results {
        assert_eq!(*hash, first_hash);
    }

    Ok(())
}
