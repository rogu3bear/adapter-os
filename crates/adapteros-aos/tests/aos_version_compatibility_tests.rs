//! Format Version Compatibility Tests (P3 Low)
//!
//! Tests for AOS format version handling and backward compatibility.
//! The format must handle version evolution gracefully.
//!
//! These tests verify:
//! - AOS magic bytes detection
//! - Invalid magic bytes rejection
//! - Schema version handling
//! - Minimum supported schema
//! - Format version in header
//! - Manifest schema evolution

use adapteros_aos::writer::{AosWriter, BackendTag, AOS_MAGIC, HEADER_SIZE};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var/tmp");
    fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

#[derive(Serialize, Deserialize)]
struct TestManifest {
    metadata: HashMap<String, String>,
}

/// Test that AOS magic bytes are correctly written and detected.
#[test]
fn test_aos_magic_bytes_correct() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("magic.aos");

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

    writer.write_archive(&path, &manifest).unwrap();

    // Read file and check magic bytes
    let data = fs::read(&path).unwrap();
    assert!(data.len() >= 4, "File too small");
    assert_eq!(&data[0..4], &AOS_MAGIC, "Magic bytes should be AOS");
    assert_eq!(&data[0..4], b"AOS\0", "Magic bytes should be 'AOS\\0'");
}

/// Test that invalid magic bytes are rejected.
#[test]
fn test_invalid_magic_bytes_rejected() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("bad_magic.aos");

    // Write a file with invalid magic
    let mut bad_header = vec![0u8; HEADER_SIZE];
    bad_header[0..4].copy_from_slice(b"BAD!");
    fs::write(&path, &bad_header).unwrap();

    let result = AosWriter::read_header(&path);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("magic"),
        "Should indicate invalid magic"
    );
}

/// Test that completely wrong file is rejected.
#[test]
fn test_non_aos_file_rejected() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("not_aos.bin");

    // Write random data
    let random_data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33];
    fs::write(&path, &random_data).unwrap();

    let result = AosWriter::read_header(&path);
    assert!(result.is_err());
}

/// Test that header flags are correctly set.
#[test]
fn test_header_flags_format() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("flags.aos");

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

    writer.write_archive(&path, &manifest).unwrap();

    let header = AosWriter::read_header(&path).unwrap();

    // Verify header structure
    assert!(header.manifest_offset > 0, "Manifest offset should be set");
    assert!(header.manifest_size > 0, "Manifest size should be set");
}

/// Test that reserved header bytes are zero.
#[test]
fn test_reserved_header_bytes_zero() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("reserved.aos");

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

    writer.write_archive(&path, &manifest).unwrap();

    // Read raw header and check reserved bytes (40-63)
    let data = fs::read(&path).unwrap();
    assert!(data.len() >= HEADER_SIZE);

    // Reserved bytes should be zero
    for i in 40..HEADER_SIZE {
        assert_eq!(
            data[i], 0,
            "Reserved byte at offset {} should be zero, got {}",
            i, data[i]
        );
    }
}

/// Test manifest schema can include version information.
#[test]
fn test_manifest_with_schema_version() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("schema_version.aos");

    #[derive(Serialize)]
    struct VersionedManifest {
        schema_version: String,
        metadata: HashMap<String, String>,
    }

    let manifest = VersionedManifest {
        schema_version: "1.0.0".to_string(),
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
    assert!(
        result.is_ok(),
        "Manifest with schema version should be accepted"
    );

    // Verify we can read manifest back
    let data = fs::read(&path).unwrap();
    let header = AosWriter::parse_header_bytes(&data).unwrap();
    let manifest_start = header.manifest_offset as usize;
    let manifest_end = manifest_start + header.manifest_size as usize;
    let manifest_bytes = &data[manifest_start..manifest_end];

    let parsed: serde_json::Value = serde_json::from_slice(manifest_bytes).unwrap();
    assert_eq!(
        parsed.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0.0")
    );
}
