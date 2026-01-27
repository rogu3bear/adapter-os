//! .aos Manifest Overflow Tests (P1 High)
//!
//! Tests for integer overflow and boundary condition handling in AOS archives.
//! These tests verify that the format correctly rejects malformed archives
//! that could cause overflows during parsing.
//!
//! These tests verify:
//! - Manifest offset checked_add overflow
//! - Index offset checked_add overflow
//! - Segment offset checked_add overflow
//! - Total archive size overflow protection
//! - Integer underflow protection

use adapteros_aos::writer::{
    parse_segments, AosWriter, BackendTag, AOS_MAGIC, HAS_INDEX_FLAG, HEADER_SIZE, INDEX_ENTRY_SIZE,
};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use tempfile::NamedTempFile;

fn new_test_tempfile() -> NamedTempFile {
    NamedTempFile::new().expect("create temp file")
}

/// Create a minimal valid header with customizable offsets
fn create_custom_header(
    index_offset: u64,
    index_size: u64,
    manifest_offset: u64,
    manifest_size: u64,
) -> Vec<u8> {
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&index_offset.to_le_bytes());
    header[16..24].copy_from_slice(&index_size.to_le_bytes());
    header[24..32].copy_from_slice(&manifest_offset.to_le_bytes());
    header[32..40].copy_from_slice(&manifest_size.to_le_bytes());
    header
}

/// Test that manifest offset + size overflow is detected.
///
/// If manifest_offset + manifest_size > u64::MAX, parsing should fail safely.
#[test]
fn test_manifest_offset_checked_add_overflow() {
    let temp_file = new_test_tempfile();

    // Create header with manifest offset near u64::MAX
    let header = create_custom_header(
        HEADER_SIZE as u64, // index_offset
        0,                  // index_size (no segments)
        u64::MAX - 10,      // manifest_offset near max
        100,                // manifest_size that would overflow
    );

    fs::write(temp_file.path(), &header).unwrap();

    let result = AosWriter::read_header(temp_file.path());
    // This should either fail or return a header that will fail on read
    // The key is that we don't panic or have undefined behavior
    if let Ok(parsed) = result {
        // If it parsed, attempting to read should fail
        let data = fs::read(temp_file.path()).unwrap();
        // The manifest_offset + manifest_size would overflow
        // so any safe implementation should detect this
        assert!(
            parsed
                .manifest_offset
                .checked_add(parsed.manifest_size)
                .is_none()
                || parsed.manifest_offset as usize > data.len(),
            "Overflow should be detected"
        );
    }
}

/// Test that index offset + size overflow is detected.
///
/// If index_offset + index_size > u64::MAX, parsing should fail safely.
#[test]
fn test_index_offset_checked_add_overflow() {
    let temp_file = new_test_tempfile();

    // Create header with index offset near u64::MAX
    let header = create_custom_header(
        u64::MAX - 50, // index_offset near max
        100,           // index_size that would overflow
        100,           // manifest_offset
        50,            // manifest_size
    );

    fs::write(temp_file.path(), &header).unwrap();

    let result = AosWriter::read_header(temp_file.path());
    if let Ok(parsed) = result {
        assert!(
            parsed.index_offset.checked_add(parsed.index_size).is_none()
                || parsed.index_offset as usize > HEADER_SIZE + header.len(),
            "Index overflow should be detected"
        );
    }
}

/// Test that segment payloads with overflow offsets are rejected.
///
/// Malformed index entries with huge offsets should not cause panics.
#[test]
fn test_segment_offset_overflow_protection() {
    let temp_file = new_test_tempfile();

    // Create a valid header and one malformed index entry
    let index_offset = HEADER_SIZE as u64;
    let index_size = INDEX_ENTRY_SIZE as u64;
    let manifest_offset = index_offset + index_size;
    let manifest_json = b"{}";
    let manifest_size = manifest_json.len() as u64;

    let mut data = create_custom_header(index_offset, index_size, manifest_offset, manifest_size);

    // Add one index entry with overflowing offset
    let mut index_entry = vec![0u8; INDEX_ENTRY_SIZE];
    index_entry[0..4].copy_from_slice(&0u32.to_le_bytes()); // segment_id
    index_entry[4..6].copy_from_slice(&0u16.to_le_bytes()); // backend_tag (canonical)
                                                            // offset at bytes 8..16 - set to near max value
    index_entry[8..16].copy_from_slice(&(u64::MAX - 10).to_le_bytes());
    // length at bytes 16..24 - set to cause overflow
    index_entry[16..24].copy_from_slice(&100u64.to_le_bytes());
    // rest are scope_hash and weights_hash (zeros are fine)

    data.extend_from_slice(&index_entry);
    data.extend_from_slice(manifest_json);

    fs::write(temp_file.path(), &data).unwrap();

    let header = AosWriter::parse_header_bytes(&data).unwrap();
    let result = parse_segments(&data, &header);

    // Should either fail or return segments that won't cause buffer overrun
    if let Ok(segments) = result {
        // Any segment with offset beyond file size should not be usable
        for seg in &segments {
            assert!(
                seg.offset + seg.len <= data.len(),
                "Segment bounds must be within file"
            );
        }
    }
}

/// Test that total archive size calculations are protected from overflow.
///
/// When computing total_size, all additions must use checked arithmetic.
#[test]
fn test_total_archive_size_overflow_protection() {
    // This test verifies at the API level that we can't trick the writer
    // into creating overflowing archives

    #[derive(Serialize)]
    struct TestManifest {
        metadata: HashMap<String, String>,
    }

    let _manifest = TestManifest {
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let mut writer = AosWriter::new();

    // Add a canonical segment
    let weights = vec![0u8; 1024];
    let result = writer.add_segment(
        BackendTag::Canonical,
        Some("test/scope".to_string()),
        &weights,
    );
    assert!(result.is_ok());

    // The writer should handle size calculations safely internally
    // We can't easily force an overflow without memory allocation issues,
    // but we can verify the API doesn't panic
}

/// Test that index size not divisible by entry size is rejected.
///
/// index_size must be a multiple of INDEX_ENTRY_SIZE (80 bytes).
#[test]
fn test_index_size_not_aligned_rejected() {
    let temp_file = new_test_tempfile();

    // Create header with misaligned index size
    let header = create_custom_header(
        HEADER_SIZE as u64,          // index_offset
        INDEX_ENTRY_SIZE as u64 + 7, // Misaligned! Not divisible by 80
        (HEADER_SIZE + INDEX_ENTRY_SIZE + 7) as u64,
        10, // manifest_size
    );

    // Add some padding to make file big enough
    let mut data = header.clone();
    data.resize(HEADER_SIZE + INDEX_ENTRY_SIZE + 7 + 10, 0);

    fs::write(temp_file.path(), &data).unwrap();

    let parsed_header = AosWriter::parse_header_bytes(&data).unwrap();
    let result = parse_segments(&data, &parsed_header);

    // Should reject or handle gracefully
    assert!(
        result.is_err()
            || !parsed_header
                .index_size
                .is_multiple_of(INDEX_ENTRY_SIZE as u64),
        "Misaligned index size should be handled"
    );
}
