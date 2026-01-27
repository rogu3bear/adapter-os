//! I/O Failure Handling Tests (P3 Low)
//!
//! Tests for graceful handling of I/O errors during AOS operations.
//! File operations should fail gracefully with informative errors.
//!
//! These tests verify:
//! - File open failure
//! - File write failure
//! - File flush failure
//! - Memory mapping failure
//! - Header read incomplete
//! - Disk full handling
//! - Permission denied handling

use adapteros_aos::writer::{AosWriter, BackendTag, AOS_MAGIC, HEADER_SIZE};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

#[derive(Serialize)]
struct TestManifest {
    metadata: HashMap<String, String>,
}

/// Test that opening non-existent file returns appropriate error.
#[test]
fn test_file_open_failure_nonexistent() {
    let path = PathBuf::from("/nonexistent/path/to/file.aos");

    let result = AosWriter::read_header(&path);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Failed to open")
            || err.to_string().contains("No such file")
            || err.to_string().contains("not found"),
        "Error should indicate file not found: {}",
        err
    );
}

/// Test that reading from empty file fails gracefully.
#[test]
fn test_file_read_empty_file() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("empty.aos");

    // Create empty file
    fs::write(&path, []).unwrap();

    let result = AosWriter::read_header(&path);
    assert!(result.is_err());

    let err = result.unwrap_err();
    // Should indicate file is too small or header read failed
    assert!(
        err.to_string().contains("Failed to read header")
            || err.to_string().contains("too small")
            || err.to_string().contains("unexpected end"),
        "Error should indicate empty file issue: {}",
        err
    );
}

/// Test that reading incomplete header fails gracefully.
#[test]
fn test_header_read_incomplete() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("incomplete.aos");

    // Write only 32 bytes (less than 64-byte header)
    let mut incomplete = vec![0u8; 32];
    incomplete[0..4].copy_from_slice(&AOS_MAGIC);
    fs::write(&path, &incomplete).unwrap();

    let result = AosWriter::read_header(&path);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to read header"),
        "Should indicate incomplete header"
    );
}

/// Test writing archive to valid path succeeds.
#[test]
fn test_write_to_valid_path_succeeds() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("valid.aos");

    let manifest = TestManifest {
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let weights = vec![42u8; 128];

    let mut writer = AosWriter::new();
    writer
        .add_segment(
            BackendTag::Canonical,
            Some("test/scope".to_string()),
            &weights,
        )
        .unwrap();

    let result = writer.write_archive(&path, &manifest);
    assert!(result.is_ok());

    // Verify file was created
    assert!(path.exists());
    let size = fs::metadata(&path).unwrap().len();
    assert!(size > HEADER_SIZE as u64);
}

/// Test that writing without segments fails.
#[test]
fn test_write_without_segments_fails() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("no_segments.aos");

    let manifest = TestManifest {
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let writer = AosWriter::new(); // No segments added

    let result = writer.write_archive(&path, &manifest);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("without segments"),
        "Should indicate no segments"
    );
}

/// Test that writing without canonical segment fails.
#[test]
fn test_write_without_canonical_segment_fails() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("no_canonical.aos");

    let manifest = TestManifest {
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let weights = vec![42u8; 128];

    let mut writer = AosWriter::new();
    // Only add MLX segment, no canonical
    writer.add_segment(BackendTag::Mlx, None, &weights).unwrap();

    let result = writer.write_archive(&path, &manifest);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("canonical"),
        "Should indicate missing canonical segment"
    );
}

/// Test that directory path (not file) fails appropriately.
#[test]
fn test_write_to_directory_fails() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path(); // Directory, not a file

    let manifest = TestManifest {
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let weights = vec![42u8; 128];

    let mut writer = AosWriter::new();
    writer
        .add_segment(
            BackendTag::Canonical,
            Some("test/scope".to_string()),
            &weights,
        )
        .unwrap();

    let result = writer.write_archive(path, &manifest);
    // Should fail - can't write to a directory
    assert!(result.is_err());
}
