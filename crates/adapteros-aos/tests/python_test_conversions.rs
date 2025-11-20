//! Python Test Conversions - Rust port of test_aos_loading.py and test_production_aos.py
//!
//! This test suite converts the Python test files to idiomatic Rust integration tests.
//!
//! Original Python tests:
//! - tests/test_aos_loading.py - Tests loading of .aos adapter files
//! - tests/test_production_aos.py - Tests production-ready .aos files with hashing
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

mod fixture_generator;

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tempfile::TempDir;

// ============================================================================
// Test Manifest Structures
// ============================================================================

/// Extended manifest structure for production tests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductionManifest {
    pub format_version: u32,
    pub adapter_id: String,
    pub name: String,
    pub category: String,
    pub rank: u32,
    pub alpha: u32,
    pub base_model: String,
    pub target_modules: Vec<String>,
    pub weights_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_config: Option<TrainingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingConfig {
    pub rank: u32,
    pub alpha: u32,
    pub learning_rate: f64,
    pub epochs: u32,
}

/// Catalog structure for adapter inventory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCatalog {
    pub total_adapters: usize,
    pub total_size_mb: f64,
    pub format: String,
    pub adapters: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub file: String,
    pub name: String,
    pub rank: u32,
    pub size_mb: f64,
}

// ============================================================================
// Test Fixture Generators
// ============================================================================

/// Create a production-ready .aos file with full metadata
fn create_production_aos<P: AsRef<Path>>(
    path: P,
    adapter_id: &str,
    name: &str,
    category: &str,
    rank: u32,
    weights_size_kb: usize,
) -> Result<()> {
    let weights_data = create_fake_safetensors_data(weights_size_kb * 1024);

    // Compute BLAKE3 hash
    let hash = B3Hash::hash(&weights_data);
    let weights_hash = hex::encode(hash.as_bytes());

    // Generate mock Ed25519 signature and public key (64-char hex strings)
    let signature = "a".repeat(128); // 64 bytes = 128 hex chars
    let public_key = "b".repeat(64); // 32 bytes = 64 hex chars

    let manifest = ProductionManifest {
        format_version: 2,
        adapter_id: adapter_id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        rank,
        alpha: rank * 2,
        base_model: "llama-3.2-1b".to_string(),
        target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
        weights_hash,
        signature: Some(signature),
        public_key: Some(public_key),
        training_config: Some(TrainingConfig {
            rank,
            alpha: rank * 2,
            learning_rate: 0.0001,
            epochs: 3,
        }),
        metadata: Some({
            let mut meta = HashMap::new();
            meta.insert(
                "use_cases".to_string(),
                serde_json::json!(["general", "specialized"]),
            );
            meta.insert("description".to_string(), serde_json::json!(name));
            meta
        }),
    };

    let writer = AOS2Writer::new();
    writer.write_archive(path, &manifest, &weights_data)?;

    Ok(())
}

/// Create fake safetensors-like binary data
fn create_fake_safetensors_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);

    // Add minimal safetensors header
    let header_json = serde_json::json!({
        "lora_A": {
            "dtype": "F32",
            "shape": [768, 8],
            "data_offsets": [0, 24576]
        }
    });
    let header_bytes = serde_json::to_vec(&header_json).unwrap();
    let header_len = header_bytes.len() as u64;

    data.extend_from_slice(&header_len.to_le_bytes());
    data.extend_from_slice(&header_bytes);

    // Fill rest with deterministic pattern
    let pattern = b"SAFETENSORS_TEST_DATA";
    while data.len() < size {
        let remaining = size - data.len();
        let chunk_size = remaining.min(pattern.len());
        data.extend_from_slice(&pattern[..chunk_size]);
    }

    data
}

/// Load and parse manifest from .aos file (Python equivalent)
fn load_aos_manifest<P: AsRef<Path>>(aos_path: P) -> Result<ProductionManifest> {
    let mut file = File::open(aos_path.as_ref())
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    // Read header (8 bytes)
    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

    // Read from beginning to extract full archive
    use std::io::Seek;
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset + manifest_len];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    // Extract and parse manifest
    let manifest_bytes = &buffer[manifest_offset..];
    let manifest: ProductionManifest = serde_json::from_slice(manifest_bytes)?;

    Ok(manifest)
}

// ============================================================================
// Test Suite: test_aos_loading.py Conversion
// ============================================================================

#[test]
fn test_adapter_loading_code_assistant() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("code-assistant.aos");

    // Create adapter (equivalent to Python's code-assistant.aos)
    create_production_aos(
        &path,
        "default/code/assistant/r001",
        "Code Assistant",
        "code",
        16,
        1024, // 1MB+
    )?;

    // Test file exists
    assert!(path.exists(), "File should exist");

    // Test file size (should be at least 1MB)
    let metadata = std::fs::metadata(&path)
        .map_err(|e| AosError::Io(format!("Failed to get metadata: {}", e)))?;
    let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
    assert!(
        size_mb >= 1.0,
        "File should be at least 1MB, got {:.2}MB",
        size_mb
    );

    // Load manifest
    let manifest = load_aos_manifest(&path)?;

    // Verify format version
    assert_eq!(manifest.format_version, 2, "Format version should be 2");

    // Verify rank
    assert_eq!(manifest.rank, 16, "Rank should be 16");

    // Verify name/id contains expected string
    // In production manifests, ID uses semantic naming like "default/code/assistant/r001"
    // so we check for "assistant" or "adapter" in either field
    assert!(
        manifest.adapter_id.to_lowercase().contains("adapter")
            || manifest.adapter_id.to_lowercase().contains("assistant")
            || manifest.name.to_lowercase().contains("adapter")
            || manifest.name.to_lowercase().contains("assistant"),
        "Name or ID should contain 'adapter' or 'assistant', got id='{}' name='{}'",
        manifest.adapter_id,
        manifest.name
    );

    // Verify required fields
    assert!(
        !manifest.weights_hash.is_empty(),
        "weights_hash should be present"
    );
    assert!(
        !manifest.base_model.is_empty(),
        "base_model should be present"
    );
    assert!(
        !manifest.target_modules.is_empty(),
        "target_modules should be present"
    );
    assert!(manifest.alpha > 0, "alpha should be present");

    Ok(())
}

#[test]
fn test_adapter_loading_readme_writer() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("readme-writer.aos");

    // Create adapter (smaller rank = 8)
    create_production_aos(
        &path,
        "default/documentation/readme-writer/r001",
        "README Writer",
        "documentation",
        8,
        128, // 128KB (smaller)
    )?;

    // Test file exists and size
    assert!(path.exists());
    let metadata = std::fs::metadata(&path)
        .map_err(|e| AosError::Io(format!("Failed to get metadata: {}", e)))?;
    let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
    assert!(size_mb >= 0.1, "File should be at least 0.1MB");

    // Load and verify manifest
    let manifest = load_aos_manifest(&path)?;
    assert_eq!(manifest.format_version, 2);
    assert_eq!(manifest.rank, 8);
    assert!(!manifest.weights_hash.is_empty());

    Ok(())
}

#[test]
fn test_adapter_loading_creative_writer() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("creative-writer.aos");

    create_production_aos(
        &path,
        "default/creative/story-writer/r001",
        "Creative Writer",
        "creative",
        12,
        1024,
    )?;

    let manifest = load_aos_manifest(&path)?;
    assert_eq!(manifest.format_version, 2);
    assert_eq!(manifest.rank, 12);
    assert!(!manifest.weights_hash.is_empty());

    Ok(())
}

#[test]
fn test_catalog_integrity() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Create adapters directory
    let adapters_dir = temp_dir.path().join("adapters");
    std::fs::create_dir(&adapters_dir)
        .map_err(|e| AosError::Io(format!("Failed to create adapters dir: {}", e)))?;

    // Create three adapter files
    let files = vec![
        ("code-assistant.aos", 16, 1024),
        ("readme-writer.aos", 8, 128),
        ("creative-writer.aos", 12, 1024),
    ];

    let mut total_size_mb = 0.0;
    let mut entries = Vec::new();

    for (filename, rank, size_kb) in &files {
        let path = adapters_dir.join(filename);
        create_production_aos(
            &path,
            &format!("test/{}", filename),
            filename,
            "test",
            *rank,
            *size_kb,
        )?;

        let metadata = std::fs::metadata(&path)
            .map_err(|e| AosError::Io(format!("Failed to get metadata: {}", e)))?;
        let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
        total_size_mb += size_mb;

        entries.push(CatalogEntry {
            file: filename.to_string(),
            name: filename.to_string(),
            rank: *rank,
            size_mb,
        });
    }

    // Create catalog
    let catalog = AdapterCatalog {
        total_adapters: 3,
        total_size_mb,
        format: "AOS 2.0".to_string(),
        adapters: entries,
    };

    // Write catalog
    let catalog_path = adapters_dir.join("catalog.json");
    let catalog_json = serde_json::to_string_pretty(&catalog)?;
    std::fs::write(&catalog_path, catalog_json)
        .map_err(|e| AosError::Io(format!("Failed to write catalog: {}", e)))?;

    // Verify catalog exists
    assert!(catalog_path.exists(), "Catalog should exist");

    // Load and verify catalog
    let catalog_content = std::fs::read_to_string(&catalog_path)
        .map_err(|e| AosError::Io(format!("Failed to read catalog: {}", e)))?;
    let loaded_catalog: AdapterCatalog = serde_json::from_str(&catalog_content)?;

    // Check adapter count
    assert_eq!(loaded_catalog.total_adapters, 3, "Should have 3 adapters");

    // Check each referenced file exists
    for adapter in &loaded_catalog.adapters {
        let file_path = adapters_dir.join(&adapter.file);
        assert!(
            file_path.exists(),
            "Referenced file should exist: {}",
            adapter.file
        );
    }

    Ok(())
}

#[test]
fn test_invalid_file_handling() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Test 1: Non-existent file
    let nonexistent = temp_dir.path().join("nonexistent.aos");
    let result = load_aos_manifest(&nonexistent);
    assert!(result.is_err(), "Should fail for non-existent file");

    // Test 2: File too small
    let too_small = temp_dir.path().join("too-small.aos");
    std::fs::write(&too_small, b"small")
        .map_err(|e| AosError::Io(format!("Failed to write: {}", e)))?;
    let result = load_aos_manifest(&too_small);
    assert!(result.is_err(), "Should fail for file too small");

    // Test 3: Invalid header
    let invalid_header = temp_dir.path().join("invalid-header.aos");
    std::fs::write(&invalid_header, &[0xFF; 8])
        .map_err(|e| AosError::Io(format!("Failed to write: {}", e)))?;
    let result = load_aos_manifest(&invalid_header);
    assert!(result.is_err(), "Should fail for invalid header");

    Ok(())
}

// ============================================================================
// Test Suite: test_production_aos.py Conversion
// ============================================================================

#[test]
fn test_production_code_assistant() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("code-assistant.aos");

    create_production_aos(
        &path,
        "default/code/assistant/r001",
        "Code Assistant",
        "code",
        16,
        1024,
    )?;

    let manifest = load_aos_manifest(&path)?;

    // Check format version
    assert_eq!(manifest.format_version, 2, "Format version should be 2");

    // Check semantic ID
    assert_eq!(
        manifest.adapter_id, "default/code/assistant/r001",
        "Semantic ID mismatch"
    );

    // Check name
    assert_eq!(manifest.name, "Code Assistant", "Name mismatch");

    // Check category
    assert_eq!(manifest.category, "code", "Category mismatch");

    // Check rank
    assert_eq!(manifest.rank, 16, "Rank mismatch");

    // Check BLAKE3 hash is present
    assert_eq!(
        manifest.weights_hash.len(),
        64,
        "BLAKE3 hash should be 64 hex chars"
    );

    // Check Ed25519 signature is present
    let signature = manifest.signature.expect("Signature should be present");
    assert_eq!(
        signature.len(),
        128,
        "Ed25519 signature should be 128 hex chars"
    );

    // Check public key is present
    let public_key = manifest.public_key.expect("Public key should be present");
    assert_eq!(public_key.len(), 64, "Public key should be 64 hex chars");

    // Check training config
    let training_config = manifest
        .training_config
        .expect("Training config should be present");
    assert_eq!(training_config.rank, 16, "Training config rank mismatch");

    // Check metadata
    let metadata = manifest.metadata.expect("Metadata should be present");
    assert!(
        metadata.contains_key("use_cases"),
        "Metadata should contain use_cases"
    );

    Ok(())
}

#[test]
fn test_production_readme_writer() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("readme-writer.aos");

    create_production_aos(
        &path,
        "default/documentation/readme-writer/r001",
        "README Writer",
        "documentation",
        8,
        128,
    )?;

    let manifest = load_aos_manifest(&path)?;

    assert_eq!(manifest.format_version, 2);
    assert_eq!(
        manifest.adapter_id,
        "default/documentation/readme-writer/r001"
    );
    assert_eq!(manifest.name, "README Writer");
    assert_eq!(manifest.category, "documentation");
    assert_eq!(manifest.rank, 8);
    assert_eq!(manifest.weights_hash.len(), 64);

    Ok(())
}

#[test]
fn test_production_creative_writer() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("creative-writer.aos");

    create_production_aos(
        &path,
        "default/creative/story-writer/r001",
        "Creative Writer",
        "creative",
        12,
        1024,
    )?;

    let manifest = load_aos_manifest(&path)?;

    assert_eq!(manifest.format_version, 2);
    assert_eq!(manifest.adapter_id, "default/creative/story-writer/r001");
    assert_eq!(manifest.name, "Creative Writer");
    assert_eq!(manifest.category, "creative");
    assert_eq!(manifest.rank, 12);

    Ok(())
}

#[test]
fn test_unique_semantic_ids() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Create three adapters with unique IDs
    let adapters = vec![
        ("default/code/assistant/r001", "Code Assistant", "code", 16),
        (
            "default/documentation/readme-writer/r001",
            "README Writer",
            "documentation",
            8,
        ),
        (
            "default/creative/story-writer/r001",
            "Creative Writer",
            "creative",
            12,
        ),
    ];

    let mut ids = std::collections::HashSet::new();

    for (id, name, category, rank) in &adapters {
        let path = temp_dir
            .path()
            .join(format!("{}.aos", name.replace(' ', "-")));
        create_production_aos(&path, id, name, category, *rank, 128)?;

        let manifest = load_aos_manifest(&path)?;

        // Verify ID is unique
        assert!(
            ids.insert(manifest.adapter_id.clone()),
            "Duplicate adapter ID: {}",
            manifest.adapter_id
        );
    }

    assert_eq!(ids.len(), 3, "Should have 3 unique adapter IDs");

    Ok(())
}

#[test]
fn test_hash_verification() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("hash-test.aos");

    // Create adapter with known weights
    let weights_data = create_fake_safetensors_data(1024);
    let expected_hash = B3Hash::hash(&weights_data);
    let expected_hash_hex = hex::encode(expected_hash.as_bytes());

    // Manually create manifest with computed hash
    let manifest = ProductionManifest {
        format_version: 2,
        adapter_id: "test/hash/verify/r001".to_string(),
        name: "Hash Test".to_string(),
        category: "test".to_string(),
        rank: 8,
        alpha: 16,
        base_model: "test-model".to_string(),
        target_modules: vec!["q_proj".to_string()],
        weights_hash: expected_hash_hex.clone(),
        signature: Some("a".repeat(128)),
        public_key: Some("b".repeat(64)),
        training_config: None,
        metadata: None,
    };

    let writer = AOS2Writer::new();
    writer.write_archive(&path, &manifest, &weights_data)?;

    // Load and verify hash matches
    let loaded_manifest = load_aos_manifest(&path)?;
    assert_eq!(
        loaded_manifest.weights_hash, expected_hash_hex,
        "Hash should match"
    );

    Ok(())
}

#[test]
fn test_format_detection() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("format-test.aos");
    create_production_aos(&path, "test/format/r001", "Format Test", "test", 8, 128)?;

    // Read header to detect format
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

    // Valid AOS 2.0 files have:
    // - manifest_offset > 8 (header size)
    // - manifest_len > 0
    assert!(manifest_offset > 8, "Valid offset for AOS 2.0");
    assert!(manifest_len > 0, "Valid manifest length");

    // Load manifest and verify version field
    let manifest = load_aos_manifest(&path)?;
    assert_eq!(manifest.format_version, 2, "Format version should be 2");

    Ok(())
}

#[test]
fn test_weights_extraction() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("weights-test.aos");

    // Create with known weights
    let weights_data = create_fake_safetensors_data(2048);
    let manifest = ProductionManifest {
        format_version: 2,
        adapter_id: "test/weights/r001".to_string(),
        name: "Weights Test".to_string(),
        category: "test".to_string(),
        rank: 8,
        alpha: 16,
        base_model: "test".to_string(),
        target_modules: vec!["q_proj".to_string()],
        weights_hash: hex::encode(B3Hash::hash(&weights_data).as_bytes()),
        signature: Some("a".repeat(128)),
        public_key: Some("b".repeat(64)),
        training_config: None,
        metadata: None,
    };

    let writer = AOS2Writer::new();
    writer.write_archive(&path, &manifest, &weights_data)?;

    // Extract weights
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut header = [0u8; 8];
    file.read_exact(&mut header)
        .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

    let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;

    // Weights are from byte 8 to manifest_offset
    let weights_size = manifest_offset - 8;
    let mut extracted_weights = vec![0u8; weights_size];
    file.read_exact(&mut extracted_weights)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    // Verify weights match
    assert_eq!(extracted_weights.len(), weights_data.len());
    assert_eq!(
        extracted_weights, weights_data,
        "Extracted weights should match"
    );

    Ok(())
}

#[test]
fn test_manifest_parsing() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("manifest-test.aos");

    create_production_aos(&path, "test/manifest/r001", "Manifest Test", "test", 8, 128)?;

    // Parse with custom parser
    let manifest = load_aos_manifest(&path)?;

    // Verify all expected fields are present and correct
    assert_eq!(manifest.format_version, 2);
    assert_eq!(manifest.adapter_id, "test/manifest/r001");
    assert_eq!(manifest.name, "Manifest Test");
    assert_eq!(manifest.category, "test");
    assert_eq!(manifest.rank, 8);
    assert_eq!(manifest.alpha, 16); // alpha = rank * 2
    assert!(!manifest.base_model.is_empty());
    assert!(!manifest.target_modules.is_empty());
    assert!(!manifest.weights_hash.is_empty());

    Ok(())
}

#[test]
fn test_error_handling_corrupted_manifest() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("corrupted.aos");

    // Create a valid file first
    create_production_aos(&path, "test/corrupt/r001", "Corrupt Test", "test", 8, 128)?;

    // Corrupt the manifest by overwriting the end
    use std::fs::OpenOptions;
    use std::io::{Seek, Write};

    let mut file = OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| AosError::Io(format!("Failed to open for corruption: {}", e)))?;

    file.seek(std::io::SeekFrom::End(-10))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;
    file.write_all(b"CORRUPTED!")
        .map_err(|e| AosError::Io(format!("Failed to corrupt: {}", e)))?;

    // Try to load corrupted file
    let result = load_aos_manifest(&path);
    assert!(result.is_err(), "Should fail to parse corrupted manifest");

    Ok(())
}

// ============================================================================
// Additional Integration Tests
// ============================================================================

#[test]
fn test_roundtrip_with_parser() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("roundtrip.aos");

    create_production_aos(
        &path,
        "test/roundtrip/r001",
        "Roundtrip Test",
        "test",
        16,
        512,
    )?;

    // Load with both methods
    let manifest1 = load_aos_manifest(&path)?;

    // Use AOS2Writer to read header
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;
    assert!(manifest_offset > 8);
    assert!(manifest_len > 0);

    // Verify consistency
    assert_eq!(manifest1.adapter_id, "test/roundtrip/r001");
    assert_eq!(manifest1.rank, 16);

    Ok(())
}

#[test]
fn test_file_size_validation() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let sizes = vec![
        ("tiny", 64, 0.0, 0.1),
        ("small", 256, 0.0, 0.5),
        ("medium", 1024, 0.5, 2.0),
        ("large", 2048, 1.5, 3.0),
    ];

    for (name, size_kb, min_mb, max_mb) in sizes {
        let path = temp_dir.path().join(format!("{}.aos", name));
        create_production_aos(
            &path,
            &format!("test/{}/r001", name),
            name,
            "test",
            8,
            size_kb,
        )?;

        let metadata = std::fs::metadata(&path)
            .map_err(|e| AosError::Io(format!("Failed to get metadata: {}", e)))?;
        let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;

        assert!(
            size_mb >= min_mb && size_mb <= max_mb,
            "{}: size {:.2}MB not in range {:.2}-{:.2}MB",
            name,
            size_mb,
            min_mb,
            max_mb
        );
    }

    Ok(())
}
