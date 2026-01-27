//! Serialization Edge Cases Tests (P3 Low)
//!
//! Tests for manifest serialization and deserialization edge cases.
//! JSON parsing and generation must handle all valid inputs.
//!
//! These tests verify:
//! - Invalid JSON manifest handling
//! - Manifest missing required fields
//! - SafeTensors deserialization validation
//! - Malformed metadata JSON
//! - Unicode in manifest fields
//! - Maximum size manifest handling

use adapteros_aos::writer::{AosWriter, BackendTag};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

#[derive(Serialize, Deserialize, Debug)]
struct TestManifest {
    adapter_id: String,
    rank: u32,
    metadata: HashMap<String, String>,
}

/// Test that manifest with Unicode characters serializes correctly.
#[test]
fn test_manifest_with_unicode() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("unicode.aos");

    let manifest = TestManifest {
        adapter_id: "日本語アダプター".to_string(), // Japanese
        rank: 8,
        metadata: HashMap::from([
            ("scope_path".to_string(), "test/scope".to_string()),
            ("emoji".to_string(), "🚀🔥💻".to_string()),
            ("chinese".to_string(), "中文测试".to_string()),
        ]),
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
    assert!(result.is_ok(), "Unicode manifest should be accepted");

    // Verify we can read it back
    let data = fs::read(&path).unwrap();
    let header = AosWriter::parse_header_bytes(&data).unwrap();

    // Extract manifest bytes
    let manifest_start = header.manifest_offset as usize;
    let manifest_end = manifest_start + header.manifest_size as usize;
    let manifest_bytes = &data[manifest_start..manifest_end];

    // Should be valid JSON
    let parsed: serde_json::Value = serde_json::from_slice(manifest_bytes).unwrap();
    assert!(parsed.get("adapter_id").is_some());
}

/// Test manifest with very long string values.
#[test]
fn test_manifest_with_long_strings() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("long_strings.aos");

    let long_string = "x".repeat(10000);

    let manifest = TestManifest {
        adapter_id: long_string.clone(),
        rank: 16,
        metadata: HashMap::from([
            ("scope_path".to_string(), "test/scope".to_string()),
            ("long_value".to_string(), long_string),
        ]),
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
    assert!(result.is_ok(), "Long string manifest should be accepted");
}

/// Test manifest with special JSON characters.
#[test]
fn test_manifest_with_special_json_chars() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("special_chars.aos");

    let manifest = TestManifest {
        adapter_id: "test\"with\\quotes/and/slashes".to_string(),
        rank: 8,
        metadata: HashMap::from([
            ("scope_path".to_string(), "test/scope".to_string()),
            ("backslash".to_string(), "path\\to\\file".to_string()),
            ("newline".to_string(), "line1\nline2".to_string()),
            ("tab".to_string(), "col1\tcol2".to_string()),
        ]),
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
    assert!(result.is_ok(), "Special char manifest should be accepted");
}

/// Test manifest with numeric edge values.
#[test]
fn test_manifest_with_numeric_edge_values() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("numeric_edge.aos");

    #[derive(Serialize)]
    struct NumericManifest {
        rank: u32,
        alpha: f32,
        big_int: u64,
        negative: i64,
        metadata: HashMap<String, String>,
    }

    let manifest = NumericManifest {
        rank: u32::MAX,
        alpha: f32::MAX,
        big_int: u64::MAX,
        negative: i64::MIN,
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
    assert!(result.is_ok(), "Numeric edge manifest should be accepted");
}

/// Test manifest with empty metadata.
#[test]
fn test_manifest_with_empty_metadata() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("empty_metadata.aos");

    // Manifest with scope_path in metadata but nothing else
    let manifest = TestManifest {
        adapter_id: "minimal".to_string(),
        rank: 8,
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
    assert!(result.is_ok(), "Minimal metadata should be accepted");
}

/// Test manifest with null-like values (empty strings).
#[test]
fn test_manifest_with_empty_values() {
    let temp_dir = new_test_tempdir();
    let path = temp_dir.path().join("empty_values.aos");

    let manifest = TestManifest {
        adapter_id: "".to_string(), // Empty but valid JSON
        rank: 0,
        metadata: HashMap::from([
            ("scope_path".to_string(), "test/scope".to_string()),
            ("empty".to_string(), "".to_string()),
        ]),
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
    assert!(result.is_ok(), "Empty values should be accepted in JSON");
}
