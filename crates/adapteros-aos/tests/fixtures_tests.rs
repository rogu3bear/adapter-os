//! Tests using actual fixture files
//!
//! This module tests the AOS parser using pre-generated fixture files.

mod fixture_generator;

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, Result};
use fixture_generator::{
    generate_corrupted_aos, generate_empty_weights_aos, generate_invalid_header_aos,
    generate_large_aos, generate_missing_manifest_aos, generate_valid_aos,
    generate_wrong_version_aos, TestManifest,
};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to get fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Setup: Generate all test fixtures before running tests
fn setup_fixtures() -> Result<TempDir> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Generate all test fixtures
    generate_valid_aos(temp_dir.path().join("test_v2.aos"))?;
    generate_corrupted_aos(temp_dir.path().join("corrupted.aos"))?;
    generate_wrong_version_aos(temp_dir.path().join("wrong_version.aos"))?;
    generate_invalid_header_aos(temp_dir.path().join("invalid_header.aos"))?;
    generate_missing_manifest_aos(temp_dir.path().join("missing_manifest.aos"))?;
    generate_empty_weights_aos(temp_dir.path().join("empty_weights.aos"))?;
    generate_large_aos(temp_dir.path().join("large.aos"))?;

    Ok(temp_dir)
}

#[test]
fn test_valid_fixture_loads() -> Result<()> {
    let temp_dir = setup_fixtures()?;
    let path = temp_dir.path().join("test_v2.aos");

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;

    assert!(manifest_offset > 8, "Offset should be past header");
    assert!(manifest_len > 0, "Manifest should have content");

    // Parse manifest
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    assert_eq!(manifest.version, "2.0", "Version should be 2.0");
    assert!(!manifest.adapter_id.is_empty(), "Should have adapter_id");

    Ok(())
}

#[test]
fn test_corrupted_fixture_detected() {
    let temp_dir = setup_fixtures().unwrap();
    let path = temp_dir.path().join("corrupted.aos");

    // Should still read header (it's valid)
    let header_result = AOS2Writer::read_header(&path);
    assert!(
        header_result.is_ok(),
        "Header should parse even if corrupted"
    );

    // But manifest parsing should fail or produce unexpected data
    let (manifest_offset, manifest_len) = header_result.unwrap();

    let file_result = File::open(&path);
    assert!(file_result.is_ok(), "File should open");

    let mut file = file_result.unwrap();
    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];

    // Reading should work
    let read_result = file.read_exact(&mut buffer);

    // But JSON parsing should fail due to corruption
    if read_result.is_ok() {
        let manifest_bytes = &buffer[manifest_offset as usize..];
        let parse_result: serde_json::Result<TestManifest> = serde_json::from_slice(manifest_bytes);

        assert!(
            parse_result.is_err(),
            "Corrupted manifest should fail to parse"
        );
    }
}

#[test]
fn test_invalid_header_fixture() {
    let temp_dir = setup_fixtures().unwrap();
    let path = temp_dir.path().join("invalid_header.aos");

    let result = AOS2Writer::read_header(&path);

    assert!(result.is_err(), "Invalid header should fail to parse");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("Failed to read header"),
            "Error should mention header: {}",
            e
        );
    }
}

#[test]
fn test_missing_manifest_fixture() {
    let temp_dir = setup_fixtures().unwrap();
    let path = temp_dir.path().join("missing_manifest.aos");

    // Header should parse (it's valid)
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path).unwrap();

    // But reading manifest should fail (offset beyond file)
    let mut file = File::open(&path).unwrap();
    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];

    let read_result = file.read_exact(&mut buffer);

    assert!(read_result.is_err(), "Should fail to read beyond file size");
}

#[test]
fn test_empty_weights_fixture() -> Result<()> {
    let temp_dir = setup_fixtures()?;
    let path = temp_dir.path().join("empty_weights.aos");

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;

    // With empty weights, manifest should start at byte 8
    assert_eq!(
        manifest_offset, 8,
        "Empty weights means manifest starts at byte 8"
    );
    assert!(manifest_len > 0, "Manifest should still exist");

    Ok(())
}

#[test]
fn test_large_fixture_performance() -> Result<()> {
    let temp_dir = setup_fixtures()?;
    let path = temp_dir.path().join("large.aos");

    // Measure time to parse header (should be fast even with 1MB file)
    let start = std::time::Instant::now();
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 100,
        "Header parsing should be fast: {:?}",
        duration
    );

    // Manifest should be at the end after 1MB of weights
    assert!(
        manifest_offset > 1024 * 1024,
        "Manifest should be after 1MB weights"
    );
    assert!(manifest_len > 0, "Manifest should exist");

    Ok(())
}

#[test]
fn test_wrong_version_fixture() -> Result<()> {
    let temp_dir = setup_fixtures()?;
    let path = temp_dir.path().join("wrong_version.aos");

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;

    // Parse manifest
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    // Verify version is wrong
    assert_ne!(
        manifest.version, "2.0",
        "Version should not be 2.0 in this fixture"
    );
    assert_eq!(manifest.version, "1.0", "Version should be 1.0");

    Ok(())
}

#[test]
fn test_all_fixtures_exist() {
    let temp_dir = setup_fixtures().unwrap();

    let expected_fixtures = [
        "test_v2.aos",
        "corrupted.aos",
        "wrong_version.aos",
        "invalid_header.aos",
        "missing_manifest.aos",
        "empty_weights.aos",
        "large.aos",
    ];

    for fixture in &expected_fixtures {
        let path = temp_dir.path().join(fixture);
        assert!(path.exists(), "Fixture should exist: {}", path.display());
    }
}

#[test]
fn test_fixture_file_sizes() {
    let temp_dir = setup_fixtures().unwrap();

    // Verify file size ranges
    let large_path = temp_dir.path().join("large.aos");
    let metadata = std::fs::metadata(&large_path).unwrap();

    assert!(
        metadata.len() > 1024 * 1024,
        "Large fixture should be > 1MB: {} bytes",
        metadata.len()
    );

    let empty_weights_path = temp_dir.path().join("empty_weights.aos");
    let metadata = std::fs::metadata(&empty_weights_path).unwrap();

    assert!(
        metadata.len() < 1024,
        "Empty weights fixture should be small: {} bytes",
        metadata.len()
    );
}
