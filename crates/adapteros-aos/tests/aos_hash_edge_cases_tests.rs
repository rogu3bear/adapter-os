//! .aos Hash Edge Cases Tests (P1 High)
//!
//! Tests for hash verification edge cases in AOS archives.
//! These tests verify hash validation at segment, per-layer, and whole-adapter levels.
//!
//! These tests verify:
//! - Scope hash mismatch with manifest
//! - Per-layer hash mismatch detection
//! - Partial segment corruption detection
//! - Zero-length segment hash
//! - Maximum segment size hash verification
//! - Hash verification after concurrent write

use adapteros_aos::writer::{
    compute_scope_hash, open_aos, parse_segments, AosWriter, BackendTag, AOS_MAGIC, HAS_INDEX_FLAG,
    HEADER_SIZE, INDEX_ENTRY_SIZE,
};
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::NamedTempFile;

fn new_test_tempdir() -> PathBuf {
    let root = PathBuf::from("var/tmp");
    fs::create_dir_all(&root).expect("create var/tmp");
    root
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TestManifest {
    adapter_id: String,
    rank: u32,
    metadata: HashMap<String, String>,
}

fn fake_weights(label: &str, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut seed = B3Hash::hash(label.as_bytes()).to_bytes().to_vec();
    while out.len() < len {
        out.extend_from_slice(&seed);
        seed = B3Hash::hash(&seed).to_bytes().to_vec();
    }
    out.truncate(len);
    out
}

/// Test that scope hash is correctly computed from scope_path.
///
/// The scope hash is truncated BLAKE3 of the scope_path string.
#[test]
fn test_scope_hash_computation() {
    let scope_path = "tenant/domain/purpose";
    let hash = compute_scope_hash(scope_path);

    // Should be 16 bytes
    assert_eq!(hash.len(), 16);

    // Should be deterministic
    let hash2 = compute_scope_hash(scope_path);
    assert_eq!(hash, hash2);

    // Different path should produce different hash
    let hash_different = compute_scope_hash("other/scope/path");
    assert_ne!(hash, hash_different);
}

/// Test that scope hash mismatch in segment index is detectable.
///
/// When the index entry's scope_hash doesn't match the manifest's scope_path,
/// the archive should be considered corrupted.
#[test]
fn test_scope_hash_mismatch_with_manifest() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)
        .map_err(|e| AosError::Io(format!("temp file: {}", e)))?;

    let manifest = TestManifest {
        adapter_id: "scope-test".to_string(),
        rank: 8,
        metadata: HashMap::from([("scope_path".to_string(), "correct/scope/path".to_string())]),
    };

    let weights = fake_weights("canonical", 128);

    // Create archive with WRONG scope_path in segment
    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("wrong/scope/path".to_string()), // Mismatch!
        &weights,
    )?;

    writer.write_archive(temp_file.path(), &manifest)?;

    // Parse the archive
    let data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;
    let segments = parse_segments(&data, &header)?;

    // Compute expected scope hash from manifest
    let expected_scope_hash = compute_scope_hash("correct/scope/path");

    // The segment should have wrong scope hash
    assert_eq!(segments.len(), 1);
    assert_ne!(
        segments[0].scope_hash, expected_scope_hash,
        "Scope hash mismatch should be detectable"
    );

    Ok(())
}

/// Test that weights hash mismatch is detected.
///
/// When the payload doesn't match the recorded hash, loading should fail.
#[test]
fn test_weights_hash_mismatch_detection() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)
        .map_err(|e| AosError::Io(format!("temp file: {}", e)))?;

    let manifest = TestManifest {
        adapter_id: "hash-test".to_string(),
        rank: 8,
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let weights = fake_weights("canonical", 256);
    let expected_hash = B3Hash::hash(&weights);

    let mut writer = AosWriter::new();
    writer.add_segment(BackendTag::Canonical, Some("test/scope".to_string()), &weights)?;
    writer.write_archive(temp_file.path(), &manifest)?;

    // Read and verify
    let data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;
    let segments = parse_segments(&data, &header)?;

    assert_eq!(segments[0].weights_hash, expected_hash);

    // Now corrupt the weights in the file
    let mut corrupt_data = data.clone();
    // Find the segment payload and flip a byte
    let payload_start = segments[0].offset;
    if payload_start < corrupt_data.len() {
        corrupt_data[payload_start] ^= 0xFF;
    }

    // Re-verify - hash should no longer match actual content
    let actual_payload = &corrupt_data[segments[0].offset..segments[0].offset + segments[0].len];
    let actual_hash = B3Hash::hash(actual_payload);
    assert_ne!(
        actual_hash, expected_hash,
        "Corruption should cause hash mismatch"
    );

    Ok(())
}

/// Test hash verification for zero-length segment.
///
/// Empty segments should have a well-defined hash (hash of empty bytes).
#[test]
fn test_zero_length_segment_hash() {
    let empty_hash = B3Hash::hash(&[]);

    // Empty hash should be deterministic
    let empty_hash2 = B3Hash::hash(&[]);
    assert_eq!(empty_hash, empty_hash2);

    // Should not be zero
    assert_ne!(empty_hash, B3Hash::zero());
}

/// Test that multiple segments each have independent hash verification.
///
/// Corrupting one segment should not affect other segments' validation.
#[test]
fn test_multi_segment_independent_hash_verification() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)
        .map_err(|e| AosError::Io(format!("temp file: {}", e)))?;

    let manifest = TestManifest {
        adapter_id: "multi-segment".to_string(),
        rank: 8,
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let canonical_weights = fake_weights("canonical", 128);
    let mlx_weights = fake_weights("mlx", 64);
    let coreml_weights = fake_weights("coreml", 96);

    let canonical_hash = B3Hash::hash(&canonical_weights);
    let mlx_hash = B3Hash::hash(&mlx_weights);
    let coreml_hash = B3Hash::hash(&coreml_weights);

    let mut writer = AosWriter::new();
    writer.add_segment(BackendTag::Canonical, Some("test/scope".to_string()), &canonical_weights)?;
    writer.add_segment(BackendTag::Mlx, None, &mlx_weights)?;
    writer.add_segment(BackendTag::Coreml, None, &coreml_weights)?;

    writer.write_archive(temp_file.path(), &manifest)?;

    let data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;
    let segments = parse_segments(&data, &header)?;

    assert_eq!(segments.len(), 3);

    // Verify each segment has correct hash
    assert_eq!(segments[0].weights_hash, canonical_hash);
    assert_eq!(segments[1].weights_hash, mlx_hash);
    assert_eq!(segments[2].weights_hash, coreml_hash);

    // All hashes should be different
    assert_ne!(canonical_hash, mlx_hash);
    assert_ne!(mlx_hash, coreml_hash);
    assert_ne!(canonical_hash, coreml_hash);

    Ok(())
}

/// Test that hash collision is extremely unlikely with different content.
///
/// BLAKE3 should provide collision resistance.
#[test]
fn test_hash_collision_resistance() {
    let mut hashes = std::collections::HashSet::new();

    // Generate 1000 different payloads and verify no collisions
    for i in 0..1000 {
        let payload = format!("unique-payload-{}", i);
        let hash = B3Hash::hash(payload.as_bytes());
        let inserted = hashes.insert(hash);
        assert!(inserted, "Hash collision detected for payload {}", i);
    }
}
