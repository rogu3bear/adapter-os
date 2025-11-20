//! Unit tests for AOS 2.0 parser
//!
//! Tests header parsing, manifest extraction, safetensors parsing, and error handling.

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, Write};
use tempfile::NamedTempFile;

/// Test manifest structure
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct TestManifest {
    version: String,
    adapter_id: String,
    rank: u32,
    base_model: String,
    created_at: String,
}

impl TestManifest {
    fn default_test() -> Self {
        Self {
            version: "2.0".to_string(),
            adapter_id: "test-adapter-001".to_string(),
            rank: 8,
            base_model: "llama-7b".to_string(),
            created_at: "2025-01-19T00:00:00Z".to_string(),
        }
    }
}

/// Helper to create a test AOS file
fn create_test_aos_file(manifest: &TestManifest, weights: &[u8]) -> Result<NamedTempFile> {
    let temp_file = NamedTempFile::new()
        .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

    let writer = AOS2Writer::new();
    writer.write_archive(temp_file.path(), manifest, weights)?;

    Ok(temp_file)
}

#[test]
fn test_header_parsing_valid() -> Result<()> {
    let manifest = TestManifest::default_test();
    let weights = b"fake_safetensors_data_12345";

    let temp_file = create_test_aos_file(&manifest, weights)?;

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    // Header is 8 bytes, so manifest starts after header + weights
    let expected_offset = 8 + weights.len();
    assert_eq!(
        manifest_offset as usize, expected_offset,
        "Manifest offset mismatch"
    );
    assert!(manifest_len > 0, "Manifest length should be positive");

    Ok(())
}

#[test]
fn test_header_parsing_invalid_too_small() {
    let temp_file = NamedTempFile::new().unwrap();
    let mut file = File::create(temp_file.path()).unwrap();

    // Write only 4 bytes (incomplete header)
    file.write_all(&[0u8; 4]).unwrap();
    file.flush().unwrap();

    let result = AOS2Writer::read_header(temp_file.path());
    assert!(result.is_err(), "Should fail with incomplete header");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("Failed to read header"),
            "Error should mention header: {}",
            e
        );
    }
}

#[test]
fn test_header_parsing_empty_file() {
    let temp_file = NamedTempFile::new().unwrap();
    // Leave file empty

    let result = AOS2Writer::read_header(temp_file.path());
    assert!(result.is_err(), "Should fail with empty file");
}

#[test]
fn test_manifest_extraction() -> Result<()> {
    let manifest = TestManifest::default_test();
    let weights = b"weights_data";

    let temp_file = create_test_aos_file(&manifest, weights)?;

    // Read header
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    // Extract manifest
    let mut file = File::open(temp_file.path())
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let parsed_manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    assert_eq!(parsed_manifest, manifest, "Manifest should match original");

    Ok(())
}

#[test]
fn test_safetensors_extraction() -> Result<()> {
    let manifest = TestManifest::default_test();
    let weights = b"safetensors_binary_data_here";

    let temp_file = create_test_aos_file(&manifest, weights)?;

    // Read weights section
    let mut file = File::open(temp_file.path())
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    let (manifest_offset, _) = AOS2Writer::read_header(temp_file.path())?;

    // Skip header (8 bytes), read weights
    let mut buffer = vec![0u8; manifest_offset as usize - 8];
    file.seek(std::io::SeekFrom::Start(8))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    assert_eq!(&buffer, weights, "Extracted weights should match original");

    Ok(())
}

#[test]
fn test_corrupted_manifest_json() {
    let temp_file = NamedTempFile::new().unwrap();
    let mut file = File::create(temp_file.path()).unwrap();

    // Write valid header pointing to corrupted JSON
    let manifest_offset = 20u32;
    let manifest_len = 10u32;

    file.write_all(&manifest_offset.to_le_bytes()).unwrap();
    file.write_all(&manifest_len.to_le_bytes()).unwrap();

    // Write some weights
    file.write_all(b"weights_data").unwrap();

    // Write corrupted JSON (not valid JSON)
    file.write_all(b"{corrupt}!").unwrap();
    file.flush().unwrap();

    // Try to parse manifest
    let result: Result<TestManifest> = (|| {
        let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

        let mut file = File::open(temp_file.path())
            .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

        let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

        let manifest_bytes = &buffer[manifest_offset as usize..];
        let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

        Ok(manifest)
    })();

    assert!(result.is_err(), "Should fail with corrupted JSON");
}

#[test]
fn test_wrong_version_in_manifest() -> Result<()> {
    let mut manifest = TestManifest::default_test();
    manifest.version = "1.0".to_string(); // Wrong version

    let weights = b"weights";
    let temp_file = create_test_aos_file(&manifest, weights)?;

    // Read and parse
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    let mut file =
        File::open(temp_file.path()).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let parsed: TestManifest = serde_json::from_slice(manifest_bytes)?;

    // Validate version
    assert_ne!(
        parsed.version, "2.0",
        "Version should not be 2.0 for this test"
    );
    assert_eq!(parsed.version, "1.0", "Version should be 1.0");

    Ok(())
}

#[test]
fn test_oversized_manifest_offset() {
    let temp_file = NamedTempFile::new().unwrap();
    let mut file = File::create(temp_file.path()).unwrap();

    // Write header with offset beyond file size
    let manifest_offset = 1_000_000u32; // 1MB offset
    let manifest_len = 100u32;

    file.write_all(&manifest_offset.to_le_bytes()).unwrap();
    file.write_all(&manifest_len.to_le_bytes()).unwrap();

    // Write only small weights
    file.write_all(b"small").unwrap();
    file.flush().unwrap();

    // Try to read manifest
    let result: Result<Vec<u8>> = (|| {
        let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

        let mut file = File::open(temp_file.path())
            .map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

        let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read beyond EOF: {}", e)))?;

        Ok(buffer)
    })();

    assert!(
        result.is_err(),
        "Should fail when trying to read beyond file size"
    );
}

#[test]
fn test_zero_manifest_length() {
    let temp_file = NamedTempFile::new().unwrap();
    let mut file = File::create(temp_file.path()).unwrap();

    // Write header with zero manifest length
    let manifest_offset = 20u32;
    let manifest_len = 0u32;

    file.write_all(&manifest_offset.to_le_bytes()).unwrap();
    file.write_all(&manifest_len.to_le_bytes()).unwrap();
    file.write_all(b"weights_data").unwrap();
    file.flush().unwrap();

    let (_, len) = AOS2Writer::read_header(temp_file.path()).unwrap();

    assert_eq!(len, 0, "Manifest length should be zero");
}

#[test]
fn test_multiple_archives_same_session() -> Result<()> {
    let manifest1 = TestManifest {
        adapter_id: "adapter-1".to_string(),
        ..TestManifest::default_test()
    };
    let manifest2 = TestManifest {
        adapter_id: "adapter-2".to_string(),
        ..TestManifest::default_test()
    };

    let weights1 = b"weights_for_adapter_1";
    let weights2 = b"weights_for_adapter_2_longer";

    let temp1 = create_test_aos_file(&manifest1, weights1)?;
    let temp2 = create_test_aos_file(&manifest2, weights2)?;

    // Verify both files are independent and correct
    let (offset1, _) = AOS2Writer::read_header(temp1.path())?;
    let (offset2, _) = AOS2Writer::read_header(temp2.path())?;

    assert_ne!(
        offset1, offset2,
        "Different weight sizes should produce different offsets"
    );

    Ok(())
}

#[test]
fn test_large_manifest() -> Result<()> {
    // Create a manifest with lots of fields
    let manifest = TestManifest::default_test();

    // Create a large weights section
    let weights = vec![0u8; 1024 * 100]; // 100KB

    let temp_file = create_test_aos_file(&manifest, &weights)?;

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    assert_eq!(
        manifest_offset as usize,
        8 + weights.len(),
        "Offset should account for large weights"
    );
    assert!(manifest_len > 0, "Manifest should be present");

    Ok(())
}

#[test]
fn test_nonexistent_file() {
    let result = AOS2Writer::read_header("/nonexistent/path/to/file.aos");
    assert!(result.is_err(), "Should fail with nonexistent file");

    if let Err(e) = result {
        assert!(
            e.to_string().contains("Failed to open"),
            "Error should mention file opening: {}",
            e
        );
    }
}

#[test]
fn test_header_little_endian_encoding() -> Result<()> {
    let manifest = TestManifest::default_test();
    let weights = b"test_weights";

    let temp_file = create_test_aos_file(&manifest, weights)?;

    // Manually read and verify little-endian encoding
    let mut file =
        File::open(temp_file.path()).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    // Verify values make sense
    assert_eq!(
        manifest_offset,
        (8 + weights.len()) as u32,
        "Offset should be header + weights"
    );
    assert!(manifest_len > 50, "Manifest should be reasonable size");

    Ok(())
}

#[test]
fn test_minimal_valid_archive() -> Result<()> {
    let manifest = TestManifest::default_test();
    let weights = b""; // Empty weights

    let temp_file = create_test_aos_file(&manifest, weights)?;

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    assert_eq!(
        manifest_offset as usize, 8,
        "With no weights, manifest should start at byte 8"
    );
    assert!(manifest_len > 0, "Manifest should still exist");

    Ok(())
}

#[test]
fn test_roundtrip_consistency() -> Result<()> {
    let original_manifest = TestManifest::default_test();
    let original_weights = b"consistent_weights_data";

    // Write archive
    let temp_file = create_test_aos_file(&original_manifest, original_weights)?;

    // Read back and verify
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

    let mut file =
        File::open(temp_file.path()).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    // Read weights
    let mut weights_buffer = vec![0u8; manifest_offset as usize - 8];
    file.seek(std::io::SeekFrom::Start(8))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;
    file.read_exact(&mut weights_buffer)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    // Read manifest
    let mut manifest_buffer = vec![0u8; manifest_len as usize];
    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
        .map_err(|e| AosError::Io(format!("Failed to seek to manifest: {}", e)))?;
    file.read_exact(&mut manifest_buffer)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

    let parsed_manifest: TestManifest = serde_json::from_slice(&manifest_buffer)?;

    // Verify consistency
    assert_eq!(
        &weights_buffer, original_weights,
        "Weights should match original"
    );
    assert_eq!(
        parsed_manifest, original_manifest,
        "Manifest should match original"
    );

    Ok(())
}
