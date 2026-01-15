//! Content Addressing Integrity Verification Tests
//!
//! This test suite verifies that adapterOS's content addressing system
//! properly detects tampering and ensures data integrity through BLAKE3 hashing.

use adapteros_artifacts::CasStore;
use adapteros_core::{AosError, B3Hash};
use std::fs;
use std::io::Write as IoWrite;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

/// Test that the CAS store returns the correct BLAKE3 hash on storage
#[test]
fn test_cas_store_returns_blake3_hash() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data = b"test adapter bundle data";
    let hash = store.store("adapter", data).expect("store data");

    // Verify the returned hash matches BLAKE3 of the data
    let expected_hash = B3Hash::hash(data);
    assert_eq!(hash, expected_hash, "Store should return BLAKE3 hash");
}

/// Test that loading verifies the hash and succeeds for untampered data
#[test]
fn test_cas_load_verifies_hash_success() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data = b"my adapter weights";
    let hash = store.store("adapter", data).expect("store data");

    // Load should succeed and return the original data
    let loaded = store.load("adapter", &hash).expect("load data");
    assert_eq!(loaded, data, "Loaded data should match original");
}

/// Test that loading fails when the stored file has been tampered with
#[test]
fn test_cas_load_fails_on_tampered_data() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    // Store original data
    let original_data = b"original adapter data";
    let hash = store.store("adapter", original_data).expect("store data");

    // Find the stored file and tamper with it
    let hex = hash.to_hex();
    let file_path = temp
        .path()
        .join("adapter")
        .join(&hex[..2])
        .join(&hex[2..4])
        .join(&hex);

    assert!(file_path.exists(), "Stored file should exist");

    // Tamper with the file by modifying its contents
    let tampered_data = b"TAMPERED adapter data";
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&file_path)
        .expect("open file for tampering");
    file.write_all(tampered_data).expect("write tampered data");
    drop(file);

    // Attempt to load - should fail with hash mismatch
    let result = store.load("adapter", &hash);
    assert!(result.is_err(), "Load should fail on tampered data");

    // Verify the error is specifically a hash mismatch
    let err = result.unwrap_err();
    match err {
        AosError::Artifact(msg) => {
            assert!(
                msg.contains("Hash mismatch"),
                "Error should indicate hash mismatch, got: {}",
                msg
            );
        }
        _ => panic!("Expected Artifact error, got: {:?}", err),
    }
}

/// Test that partial corruption is detected
#[test]
fn test_cas_detects_partial_corruption() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    // Store a larger chunk of data
    let original_data = vec![42u8; 10000]; // 10KB
    let hash = store.store("adapter", &original_data).expect("store data");

    // Corrupt just one byte in the middle
    let hex = hash.to_hex();
    let file_path = temp
        .path()
        .join("adapter")
        .join(&hex[..2])
        .join(&hex[2..4])
        .join(&hex);

    let mut data = fs::read(&file_path).expect("read file");
    data[5000] ^= 0xFF; // Flip all bits in one byte
    fs::write(&file_path, data).expect("write corrupted data");

    // Load should fail
    let result = store.load("adapter", &hash);
    assert!(
        result.is_err(),
        "Even single-byte corruption should be detected"
    );
}

/// Test that hash collisions are handled (BLAKE3 should make this impossible)
#[test]
fn test_cas_different_content_different_hash() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data1 = b"adapter bundle v1";
    let data2 = b"adapter bundle v2";

    let hash1 = store.store("adapter", data1).expect("store data1");
    let hash2 = store.store("adapter", data2).expect("store data2");

    // Hashes must be different
    assert_ne!(
        hash1, hash2,
        "Different content must produce different hashes"
    );

    // Both should load correctly
    let loaded1 = store.load("adapter", &hash1).expect("load data1");
    let loaded2 = store.load("adapter", &hash2).expect("load data2");

    assert_eq!(loaded1, data1);
    assert_eq!(loaded2, data2);
}

/// Test that BLAKE3 hashing is deterministic
#[test]
fn test_blake3_deterministic() {
    let data = b"test data for hashing";

    let hash1 = B3Hash::hash(data);
    let hash2 = B3Hash::hash(data);
    let hash3 = B3Hash::hash(data);

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);
    assert_eq!(hash1.to_hex(), hash2.to_hex());
}

/// Test that the CAS exists() method works correctly
#[test]
fn test_cas_exists_check() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data = b"existence test";
    let hash = store.store("adapter", data).expect("store data");

    // Should exist
    assert!(
        store.exists("adapter", &hash),
        "Stored artifact should exist"
    );

    // Random hash should not exist
    let random_hash = B3Hash::hash(b"nonexistent");
    assert!(
        !store.exists("adapter", &random_hash),
        "Non-stored hash should not exist"
    );
}

/// Test that loading non-existent hash fails appropriately
#[test]
fn test_cas_load_nonexistent() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let nonexistent_hash = B3Hash::hash(b"does not exist");
    let result = store.load("adapter", &nonexistent_hash);

    assert!(result.is_err(), "Loading non-existent hash should fail");

    let err = result.unwrap_err();
    match err {
        AosError::Artifact(msg) => {
            assert!(
                msg.contains("not found"),
                "Error should indicate artifact not found, got: {}",
                msg
            );
        }
        _ => panic!("Expected Artifact error, got: {:?}", err),
    }
}

/// Test multi-hash (hashing multiple slices together)
#[test]
fn test_blake3_multi_hash_integrity() {
    let manifest = b"manifest data";
    let weights = b"weights data";

    // Hash separately then together
    let hash_separate = B3Hash::hash_multi(&[manifest, weights]);

    // Hash as concatenated data
    let mut combined = Vec::new();
    combined.extend_from_slice(manifest);
    combined.extend_from_slice(weights);
    let hash_concat = B3Hash::hash(&combined);

    // Should be identical
    assert_eq!(
        hash_separate, hash_concat,
        "Multi-hash should match concatenated hash"
    );
}

/// Test that CAS organizes files by class correctly
#[test]
fn test_cas_class_isolation() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data = b"shared data";
    let hash1 = store
        .store("adapter", data)
        .expect("store in adapter class");
    let hash2 = store.store("model", data).expect("store in model class");

    // Hashes should be the same (same content)
    assert_eq!(hash1, hash2, "Same content should have same hash");

    // But files should exist in different class directories
    assert!(store.exists("adapter", &hash1));
    assert!(store.exists("model", &hash2));

    // Both should load correctly
    let loaded1 = store.load("adapter", &hash1).expect("load from adapter");
    let loaded2 = store.load("model", &hash2).expect("load from model");

    assert_eq!(loaded1, data);
    assert_eq!(loaded2, data);
}

/// Test atomic writes (via temp file rename)
#[test]
fn test_cas_atomic_write() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    // Store data
    let data = b"atomic test data";
    let hash = store.store("adapter", data).expect("store data");

    // Verify no .tmp files are left behind
    let hex = hash.to_hex();
    let dir_path = temp.path().join("adapter").join(&hex[..2]).join(&hex[2..4]);

    let entries: Vec<_> = fs::read_dir(&dir_path).expect("read directory").collect();

    for entry in entries {
        let entry = entry.expect("valid entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        assert!(
            !name_str.ends_with(".tmp"),
            "No temporary files should remain"
        );
    }
}

/// Test zero-length data
#[test]
fn test_cas_empty_data() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    let data = b"";
    let hash = store.store("adapter", data).expect("store empty data");

    // Should be able to load it back
    let loaded = store.load("adapter", &hash).expect("load empty data");
    assert_eq!(loaded, data, "Empty data should round-trip");
}

/// Test large data (stress test)
#[test]
fn test_cas_large_data() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    // 10MB of data
    let data = vec![0xAB; 10 * 1024 * 1024];
    let hash = store.store("adapter", &data).expect("store large data");

    // Verify it loads correctly
    let loaded = store.load("adapter", &hash).expect("load large data");
    assert_eq!(
        loaded.len(),
        data.len(),
        "Large data should load completely"
    );
    assert_eq!(loaded, data, "Large data should match");
}

/// Test that hash hex encoding/decoding works
#[test]
fn test_hash_hex_roundtrip() {
    let data = b"hex test";
    let hash = B3Hash::hash(data);

    let hex = hash.to_hex();
    let parsed = B3Hash::from_hex(&hex).expect("parse hex");

    assert_eq!(hash, parsed, "Hash should survive hex roundtrip");
}

/// Integration test: Full adapter bundle workflow
#[test]
fn test_adapter_bundle_integrity_workflow() {
    let temp = new_test_tempdir();
    let store = CasStore::new(temp.path()).expect("create store");

    // Simulate adapter bundle with manifest and weights
    let manifest = b"manifest content";
    let weights = b"weights content";

    // Store manifest and weights separately
    let manifest_hash = store.store("manifest", manifest).expect("store manifest");
    let weights_hash = store.store("weights", weights).expect("store weights");

    // Create a bundle hash from both
    let bundle_hash = B3Hash::hash_multi(&[manifest, weights]);

    // Later, verify integrity by loading and re-hashing
    let loaded_manifest = store
        .load("manifest", &manifest_hash)
        .expect("load manifest");
    let loaded_weights = store.load("weights", &weights_hash).expect("load weights");

    let verified_hash = B3Hash::hash_multi(&[&loaded_manifest, &loaded_weights]);

    assert_eq!(
        bundle_hash, verified_hash,
        "Recomputed bundle hash should match"
    );
}
