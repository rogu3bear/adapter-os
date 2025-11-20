//! Test fixture generator for AOS 2.0 format
//!
//! This module provides utilities to generate test .aos files for testing.

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Standard test manifest structure
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TestManifest {
    pub version: String,
    pub adapter_id: String,
    pub rank: u32,
    pub base_model: String,
    pub created_at: String,
    pub hash: Option<String>,
}

impl TestManifest {
    pub fn new_valid() -> Self {
        Self {
            version: "2.0".to_string(),
            adapter_id: "test-adapter-valid".to_string(),
            rank: 8,
            base_model: "llama-7b".to_string(),
            created_at: "2025-01-19T00:00:00Z".to_string(),
            hash: None,
        }
    }

    pub fn with_hash(mut self, data: &[u8]) -> Self {
        let hash = B3Hash::hash(data);
        self.hash = Some(hex::encode(hash.as_bytes()));
        self
    }
}

/// Generate a valid test .aos file
pub fn generate_valid_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let manifest = TestManifest::new_valid();
    let weights = create_fake_safetensors_data(256); // Small test weights

    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, &weights)?;

    Ok(())
}

/// Generate a corrupted .aos file with bad checksum
pub fn generate_corrupted_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let manifest = TestManifest::new_valid();
    let weights = create_fake_safetensors_data(128);

    // Write valid archive first
    let writer = AOS2Writer::new();
    writer.write_archive(&path, &manifest, &weights)?;

    // Corrupt the manifest by modifying bytes
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(path.as_ref())
        .map_err(|e| AosError::Io(format!("Failed to open for corruption: {}", e)))?;

    // Seek to near the end and corrupt some bytes
    use std::io::Seek;
    file.seek(std::io::SeekFrom::End(-10))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;
    file.write_all(b"CORRUPTED!")
        .map_err(|e| AosError::Io(format!("Failed to corrupt: {}", e)))?;

    Ok(())
}

/// Generate .aos file with wrong version
pub fn generate_wrong_version_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut manifest = TestManifest::new_valid();
    manifest.version = "1.0".to_string(); // Wrong version

    let weights = create_fake_safetensors_data(128);

    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, &weights)?;

    Ok(())
}

/// Generate .aos file with invalid header
pub fn generate_invalid_header_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut file = File::create(path.as_ref())
        .map_err(|e| AosError::Io(format!("Failed to create: {}", e)))?;

    // Write invalid header (only 6 bytes instead of 8)
    file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
        .map_err(|e| AosError::Io(format!("Failed to write: {}", e)))?;

    Ok(())
}

/// Generate .aos file with missing manifest
pub fn generate_missing_manifest_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut file = File::create(path.as_ref())
        .map_err(|e| AosError::Io(format!("Failed to create: {}", e)))?;

    // Write header pointing to non-existent manifest
    let manifest_offset = 1000u32;
    let manifest_len = 100u32;

    file.write_all(&manifest_offset.to_le_bytes())
        .map_err(|e| AosError::Io(format!("Failed to write offset: {}", e)))?;
    file.write_all(&manifest_len.to_le_bytes())
        .map_err(|e| AosError::Io(format!("Failed to write len: {}", e)))?;

    // Write minimal weights (not enough to contain manifest)
    file.write_all(b"small")
        .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

    Ok(())
}

/// Generate .aos file with empty weights
pub fn generate_empty_weights_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let manifest = TestManifest::new_valid();
    let weights = b""; // Empty weights

    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, weights)?;

    Ok(())
}

/// Generate large .aos file (1MB weights)
pub fn generate_large_aos<P: AsRef<Path>>(path: P) -> Result<()> {
    let manifest = TestManifest::new_valid();
    let weights = create_fake_safetensors_data(1024 * 1024); // 1MB

    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, &weights)?;

    Ok(())
}

/// Create fake safetensors-like binary data
fn create_fake_safetensors_data(size: usize) -> Vec<u8> {
    // Real safetensors has a header, but for testing we just need binary data
    let mut data = Vec::with_capacity(size);

    // Add a minimal safetensors-like header (JSON length + empty JSON)
    let header_json = b"{}";
    let header_len = header_json.len() as u64;

    data.extend_from_slice(&header_len.to_le_bytes());
    data.extend_from_slice(header_json);

    // Fill rest with deterministic pattern
    let pattern = b"SAFETENSORS_TEST_DATA";
    while data.len() < size {
        let remaining = size - data.len();
        let chunk_size = remaining.min(pattern.len());
        data.extend_from_slice(&pattern[..chunk_size]);
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_valid_fixture() -> Result<()> {
        let temp_dir = TempDir::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

        let path = temp_dir.path().join("test_valid.aos");
        generate_valid_aos(&path)?;

        assert!(path.exists(), "File should be created");

        // Verify it's readable
        let (offset, len) = AOS2Writer::read_header(&path)?;
        assert!(offset > 0, "Should have valid offset");
        assert!(len > 0, "Should have valid length");

        Ok(())
    }

    #[test]
    fn test_generate_corrupted_fixture() -> Result<()> {
        let temp_dir = TempDir::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

        let path = temp_dir.path().join("test_corrupted.aos");
        generate_corrupted_aos(&path)?;

        assert!(path.exists(), "Corrupted file should be created");

        Ok(())
    }

    #[test]
    fn test_fake_safetensors_generation() {
        let data = create_fake_safetensors_data(1024);
        assert_eq!(data.len(), 1024, "Should generate exact size");

        // Verify it has the header structure
        let header_len = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        assert_eq!(header_len, 2, "Header should indicate 2-byte JSON");
    }
}
