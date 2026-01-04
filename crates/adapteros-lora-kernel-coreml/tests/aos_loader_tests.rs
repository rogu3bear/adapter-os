//! Tests for .aos archive loader functionality
//!
//! Tests the aos_loader module which handles multiple archive formats:
//! - AOS format: 268-byte header with AOS magic
//! - Simple format: 8-byte header with manifest_offset/manifest_len
//! - Raw/NotAos: Plain file paths or unrecognized formats
//!
//! These tests use mock data fixtures and do not require actual .aos files.

#![cfg(target_os = "macos")]

use adapteros_lora_kernel_coreml::aos_loader::{
    detect_format, extract_simple_manifest, parse_aos_header, parse_simple_aos_header,
    read_coreml_sections, AosFormat, CoremlTrainingSection, PlacementRecord, AOS_HEADER_SIZE,
    AOS_MAGIC, MIN_AOS_HEADER_SIZE,
};

// =============================================================================
// Test Fixtures
// =============================================================================

/// Create a valid simple .aos format archive with the given manifest
fn create_simple_aos(manifest: &[u8]) -> Vec<u8> {
    let manifest_offset: u32 = MIN_AOS_HEADER_SIZE as u32;
    let manifest_len: u32 = manifest.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(manifest);
    bytes
}

/// Create a valid simple .aos format with weights data after manifest
fn create_simple_aos_with_weights(manifest: &[u8], weights: &[u8]) -> Vec<u8> {
    // Place manifest after header, then weights after manifest
    let manifest_offset: u32 = MIN_AOS_HEADER_SIZE as u32;
    let manifest_len: u32 = manifest.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(manifest);
    bytes.extend_from_slice(weights);
    bytes
}

/// Create a valid AOS format archive
fn create_aos_archive(metadata: &[u8], weights: &[u8]) -> Vec<u8> {
    let version: u32 = 2;
    let weights_offset: u64 = AOS_HEADER_SIZE as u64;
    let weights_size: u64 = weights.len() as u64;
    let metadata_offset: u64 = weights_offset + weights_size;
    let metadata_size: u64 = metadata.len() as u64;
    let total_size: u64 = metadata_offset + metadata_size;

    let mut bytes = vec![0u8; AOS_HEADER_SIZE];

    // Magic bytes (0-7)
    bytes[..8].copy_from_slice(AOS_MAGIC);

    // Version (8-11)
    bytes[8..12].copy_from_slice(&version.to_le_bytes());

    // Total size (12-19)
    bytes[12..20].copy_from_slice(&total_size.to_le_bytes());

    // Weights offset (20-27)
    bytes[20..28].copy_from_slice(&weights_offset.to_le_bytes());

    // Weights size (28-35)
    bytes[28..36].copy_from_slice(&weights_size.to_le_bytes());

    // Metadata offset (36-43)
    bytes[36..44].copy_from_slice(&metadata_offset.to_le_bytes());

    // Metadata size (44-51)
    bytes[44..52].copy_from_slice(&metadata_size.to_le_bytes());

    // Append weights and metadata
    bytes.extend_from_slice(weights);
    bytes.extend_from_slice(metadata);

    bytes
}

/// Create minimal valid manifest JSON
fn minimal_manifest() -> Vec<u8> {
    br#"{"version": "1.0"}"#.to_vec()
}

/// Create a full manifest JSON with typical fields
fn full_manifest() -> Vec<u8> {
    br#"{
    "version": "1.0",
    "name": "test-adapter",
    "rank": 16,
    "alpha": 32.0,
    "target_modules": ["q_proj", "v_proj"],
    "hidden_size": 768
}"#
    .to_vec()
}

/// Create mock weights data (just deterministic bytes for testing)
fn mock_weights(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

// =============================================================================
// Format Detection Tests
// =============================================================================

#[test]
fn test_aos_format_detection_aos() {
    // Test AOS format detection
    let metadata = minimal_manifest();
    let weights = mock_weights(1024);
    let archive = create_aos_archive(&metadata, &weights);

    let format = detect_format(&archive);
    assert_eq!(format, AosFormat::Aos, "Should detect AOS format");
}

#[test]
fn test_aos_format_detection_simple() {
    // Test simple format detection
    let manifest = minimal_manifest();
    let archive = create_simple_aos(&manifest);

    let format = detect_format(&archive);
    assert_eq!(format, AosFormat::Simple, "Should detect simple format");
}

#[test]
fn test_aos_format_detection_simple_with_weights() {
    // Test simple format with weights data appended
    let manifest = full_manifest();
    let weights = mock_weights(4096);
    let archive = create_simple_aos_with_weights(&manifest, &weights);

    let format = detect_format(&archive);
    assert_eq!(
        format,
        AosFormat::Simple,
        "Should detect simple format with weights"
    );
}

#[test]
fn test_aos_format_detection_not_aos_path() {
    // Test that file paths are detected as NotAos
    let path_bytes = b"/path/to/model.mlmodelc";
    let format = detect_format(path_bytes);
    assert_eq!(format, AosFormat::NotAos, "File path should be NotAos");
}

#[test]
fn test_aos_format_detection_not_aos_utf8() {
    // Test that valid UTF-8 text is detected as NotAos
    let text = b"This is just some random text, not an archive.";
    let format = detect_format(text);
    assert_eq!(format, AosFormat::NotAos, "Plain text should be NotAos");
}

#[test]
fn test_aos_format_detection_not_aos_empty() {
    // Test that empty bytes are detected as NotAos
    let format = detect_format(&[]);
    assert_eq!(format, AosFormat::NotAos, "Empty input should be NotAos");
}

#[test]
fn test_aos_format_detection_not_aos_too_short() {
    // Test that very short input is detected as NotAos
    let short_bytes = &[1, 2, 3, 4];
    let format = detect_format(short_bytes);
    assert_eq!(
        format,
        AosFormat::NotAos,
        "Input shorter than header should be NotAos"
    );
}

#[test]
fn test_aos_format_detection_not_aos_invalid_json() {
    // Create a simple header that points to non-JSON data
    let non_json = b"not json content here";
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = non_json.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(non_json);

    let format = detect_format(&bytes);
    assert_eq!(
        format,
        AosFormat::NotAos,
        "Non-JSON manifest should be NotAos"
    );
}

#[test]
fn test_aos_format_detection_aos_priority_over_simple() {
    // AOS magic should take priority even if simple format would also match
    let mut archive = vec![0u8; 300];
    archive[..8].copy_from_slice(AOS_MAGIC);

    let format = detect_format(&archive);
    assert_eq!(
        format,
        AosFormat::Aos,
        "AOS magic should take priority over simple format heuristics"
    );
}

// =============================================================================
// AOS Header Parsing Tests
// =============================================================================

#[test]
fn test_aos_header_parsing_valid() {
    let metadata = full_manifest();
    let weights = mock_weights(2048);
    let archive = create_aos_archive(&metadata, &weights);

    let result = parse_aos_header(&archive);
    assert!(
        result.is_ok(),
        "Valid AOS header should parse successfully"
    );

    let (version, total_size, weights_offset, weights_size, metadata_offset, metadata_size) =
        result.unwrap();

    assert_eq!(version, 2, "Version should be 2");
    assert_eq!(weights_offset, AOS_HEADER_SIZE, "Weights at header end");
    assert_eq!(weights_size, 2048, "Weights size should match");
    assert_eq!(
        metadata_offset,
        AOS_HEADER_SIZE + 2048,
        "Metadata after weights"
    );
    assert_eq!(metadata_size, metadata.len(), "Metadata size should match");
    assert_eq!(total_size, archive.len(), "Total size should match");
}

#[test]
fn test_aos_header_parsing_empty_weights() {
    let metadata = minimal_manifest();
    let weights: Vec<u8> = vec![];
    let archive = create_aos_archive(&metadata, &weights);

    let result = parse_aos_header(&archive);
    assert!(
        result.is_ok(),
        "AOS with empty weights should parse successfully"
    );

    let (_, _, _, weights_size, _, _) = result.unwrap();
    assert_eq!(weights_size, 0, "Weights size should be 0");
}

#[test]
fn test_aos_header_parsing_large_archive() {
    let metadata = full_manifest();
    let weights = mock_weights(1024 * 1024); // 1MB weights
    let archive = create_aos_archive(&metadata, &weights);

    let result = parse_aos_header(&archive);
    assert!(
        result.is_ok(),
        "Large AOS archive should parse successfully"
    );

    let (_, total_size, _, weights_size, _, _) = result.unwrap();
    assert_eq!(weights_size, 1024 * 1024, "Large weights size should match");
    assert!(
        total_size > 1024 * 1024,
        "Total size should be larger than weights"
    );
}

#[test]
fn test_aos_header_parsing_too_short() {
    // Create buffer that's too short for AOS header
    let short_bytes = vec![0u8; 100];

    let result = parse_aos_header(&short_bytes);
    assert!(result.is_err(), "Short buffer should fail to parse");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("too short"),
        "Error should mention file is too short"
    );
}

#[test]
fn test_aos_header_parsing_wrong_version() {
    // Create archive with wrong version
    let mut archive = vec![0u8; AOS_HEADER_SIZE + 100];
    let archive_len = archive.len() as u64;
    archive[..8].copy_from_slice(AOS_MAGIC);
    // Set version to 3 instead of 2
    archive[8..12].copy_from_slice(&3u32.to_le_bytes());
    // Set total_size to match buffer length
    archive[12..20].copy_from_slice(&archive_len.to_le_bytes());

    let result = parse_aos_header(&archive);
    assert!(result.is_err(), "Wrong version should fail to parse");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Unsupported AOS version"),
        "Error should mention unsupported version"
    );
}

#[test]
fn test_aos_header_parsing_truncated() {
    let metadata = minimal_manifest();
    let weights = mock_weights(1024);
    let mut archive = create_aos_archive(&metadata, &weights);

    // Truncate the archive
    archive.truncate(AOS_HEADER_SIZE + 512);

    let result = parse_aos_header(&archive);
    assert!(result.is_err(), "Truncated archive should fail to parse");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("truncated"),
        "Error should mention file is truncated"
    );
}

#[test]
fn test_aos_header_parsing_invalid_metadata_offset() {
    // Create archive where metadata offset points beyond file
    let mut archive = vec![0u8; AOS_HEADER_SIZE + 100];
    let archive_len = archive.len() as u64;
    archive[..8].copy_from_slice(AOS_MAGIC);
    archive[8..12].copy_from_slice(&2u32.to_le_bytes()); // version
    archive[12..20].copy_from_slice(&archive_len.to_le_bytes()); // total_size
    archive[20..28].copy_from_slice(&(AOS_HEADER_SIZE as u64).to_le_bytes()); // weights_offset
    archive[28..36].copy_from_slice(&50u64.to_le_bytes()); // weights_size
    archive[36..44].copy_from_slice(&(9999u64).to_le_bytes()); // invalid metadata_offset
    archive[44..52].copy_from_slice(&100u64.to_le_bytes()); // metadata_size

    let result = parse_aos_header(&archive);
    assert!(
        result.is_err(),
        "Invalid metadata offset should fail to parse"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Invalid AOS metadata offset"),
        "Error should mention invalid offset"
    );
}

// =============================================================================
// Simple Format Header Parsing Tests
// =============================================================================

#[test]
fn test_simple_format_parsing_valid() {
    let manifest = minimal_manifest();
    let archive = create_simple_aos(&manifest);

    let result = parse_simple_aos_header(&archive);
    assert!(
        result.is_ok(),
        "Valid simple header should parse successfully"
    );

    let (offset, len) = result.unwrap();
    assert_eq!(offset, 8, "Manifest offset should be 8");
    assert_eq!(len, manifest.len(), "Manifest length should match");
}

#[test]
fn test_simple_format_parsing_with_weights() {
    let manifest = full_manifest();
    let weights = mock_weights(8192);
    let archive = create_simple_aos_with_weights(&manifest, &weights);

    let result = parse_simple_aos_header(&archive);
    assert!(
        result.is_ok(),
        "Simple header with weights should parse successfully"
    );

    let (offset, len) = result.unwrap();
    assert_eq!(offset, 8);
    assert_eq!(len, manifest.len());
}

#[test]
fn test_simple_format_parsing_offset_beyond_header() {
    // Create archive where manifest is placed further into the file
    let manifest = minimal_manifest();
    let padding = mock_weights(256); // Extra padding before manifest

    let manifest_offset: u32 = (MIN_AOS_HEADER_SIZE + padding.len()) as u32;
    let manifest_len: u32 = manifest.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(&padding);
    bytes.extend_from_slice(&manifest);

    let result = parse_simple_aos_header(&bytes);
    assert!(result.is_ok(), "Offset beyond header should work");

    let (offset, len) = result.unwrap();
    assert_eq!(offset, manifest_offset as usize);
    assert_eq!(len, manifest.len());
}

#[test]
fn test_simple_format_parsing_too_short() {
    let short_bytes = vec![1, 2, 3, 4]; // Only 4 bytes, need 8

    let result = parse_simple_aos_header(&short_bytes);
    assert!(result.is_err(), "Short buffer should fail to parse");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("too short"),
        "Error should mention file is too short"
    );
}

#[test]
fn test_simple_format_parsing_invalid_bounds() {
    // Create header where manifest_offset + manifest_len > file_size
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = 1000; // Much larger than actual file

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(b"short"); // Only 5 bytes of content

    let result = parse_simple_aos_header(&bytes);
    assert!(result.is_err(), "Invalid bounds should fail to parse");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Invalid .aos header"),
        "Error should mention invalid header"
    );
}

#[test]
fn test_simple_format_parsing_zero_length() {
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = 0;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());

    let result = parse_simple_aos_header(&bytes);
    // Zero length should succeed as a valid header (empty manifest)
    assert!(result.is_ok(), "Zero-length manifest should parse");

    let (offset, len) = result.unwrap();
    assert_eq!(len, 0);
    assert_eq!(offset, 8);
}

// =============================================================================
// Manifest Extraction Tests
// =============================================================================

#[test]
fn test_manifest_extraction_minimal() {
    let manifest = minimal_manifest();
    let archive = create_simple_aos(&manifest);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok(), "Should extract minimal manifest");

    let (json, offset, len) = result.unwrap();
    assert_eq!(offset, 8);
    assert_eq!(len, manifest.len());
    assert!(json.is_object(), "Manifest should be a JSON object");
    assert_eq!(json["version"], "1.0", "Version should match");
}

#[test]
fn test_manifest_extraction_full() {
    let manifest = full_manifest();
    let archive = create_simple_aos(&manifest);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok(), "Should extract full manifest");

    let (json, _, _) = result.unwrap();
    assert_eq!(json["version"], "1.0");
    assert_eq!(json["name"], "test-adapter");
    assert_eq!(json["rank"], 16);
    assert_eq!(json["alpha"], 32.0);
    assert!(json["target_modules"].is_array());
    assert_eq!(json["hidden_size"], 768);
}

#[test]
fn test_manifest_extraction_with_weights() {
    let manifest = full_manifest();
    let weights = mock_weights(4096);
    let archive = create_simple_aos_with_weights(&manifest, &weights);

    let result = extract_simple_manifest(&archive);
    assert!(
        result.is_ok(),
        "Should extract manifest from archive with weights"
    );

    let (json, offset, len) = result.unwrap();
    assert_eq!(offset, 8);
    assert_eq!(len, manifest.len());
    assert_eq!(json["name"], "test-adapter");
}

#[test]
fn test_manifest_extraction_nested_json() {
    let manifest = br#"{
    "version": "2.0",
    "config": {
        "nested": {
            "deeply": {
                "value": 42
            }
        }
    },
    "array": [1, 2, {"inner": true}]
}"#;
    let archive = create_simple_aos(manifest);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok(), "Should extract nested JSON manifest");

    let (json, _, _) = result.unwrap();
    assert_eq!(json["config"]["nested"]["deeply"]["value"], 42);
    assert_eq!(json["array"][2]["inner"], true);
}

#[test]
fn test_manifest_extraction_unicode() {
    let manifest = br#"{"name": "test-\u0041dapter", "description": "UTF-8 works!"}"#;
    let archive = create_simple_aos(manifest);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok(), "Should extract manifest with unicode");

    let (json, _, _) = result.unwrap();
    assert!(json["name"].is_string());
}

// =============================================================================
// Invalid Archive Handling Tests
// =============================================================================

#[test]
fn test_invalid_archive_empty_input() {
    let result = parse_simple_aos_header(&[]);
    assert!(result.is_err(), "Empty input should fail");
}

#[test]
fn test_invalid_archive_garbage_data() {
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00];
    let format = detect_format(&garbage);
    assert_eq!(
        format,
        AosFormat::NotAos,
        "Garbage data should be detected as NotAos"
    );
}

#[test]
fn test_invalid_archive_corrupted_magic() {
    // Create AOS-like archive with corrupted magic
    let mut archive = vec![0u8; AOS_HEADER_SIZE + 100];
    archive[..8].copy_from_slice(b"AOS3\x00\x00\x00\x00"); // Wrong magic

    let format = detect_format(&archive);
    // Should not be detected as AOS due to wrong magic
    assert_ne!(
        format,
        AosFormat::Aos,
        "Corrupted magic should not be AOS"
    );
}

#[test]
fn test_invalid_archive_non_json_manifest() {
    // Create simple format pointing to binary data
    let binary_data = mock_weights(50);
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = binary_data.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(&binary_data);

    // detect_format should return NotAos because manifest isn't JSON
    let format = detect_format(&bytes);
    assert_eq!(format, AosFormat::NotAos);

    // But if we force parse, extraction should fail
    let result = extract_simple_manifest(&bytes);
    assert!(result.is_err(), "Non-JSON manifest should fail extraction");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Failed to parse"),
        "Error should mention parse failure"
    );
}

#[test]
fn test_invalid_archive_truncated_manifest() {
    // Create valid header but truncate the manifest
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = 100; // Claim 100 bytes

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(br#"{"version":"#); // Only 11 bytes

    let result = parse_simple_aos_header(&bytes);
    assert!(
        result.is_err(),
        "Truncated manifest should fail header parse"
    );
}

#[test]
fn test_invalid_archive_invalid_utf8_manifest() {
    // Create manifest with invalid UTF-8
    let invalid_utf8 = vec![0xFF, 0xFE, 0x00, 0x01, 0x7B, 0x7D]; // Invalid UTF-8 + {}
    let manifest_offset: u32 = 8;
    let manifest_len: u32 = invalid_utf8.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(&invalid_utf8);

    // This might fail at JSON parse stage if UTF-8 is invalid
    let format = detect_format(&bytes);
    assert_eq!(format, AosFormat::NotAos, "Invalid UTF-8 should be NotAos");
}

#[test]
fn test_invalid_archive_manifest_json_array() {
    // JSON arrays are valid JSON but not valid manifests (should be objects)
    let manifest = b"[1, 2, 3]";
    let archive = create_simple_aos(manifest);

    // detect_format checks for object starting with '{'
    let format = detect_format(&archive);
    assert_eq!(
        format,
        AosFormat::NotAos,
        "JSON array should not be detected as valid manifest"
    );
}

#[test]
fn test_invalid_archive_manifest_json_primitive() {
    // JSON primitives are not valid manifests
    let manifest = b"42";

    let manifest_offset: u32 = 8;
    let manifest_len: u32 = manifest.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(manifest);

    let format = detect_format(&bytes);
    assert_eq!(
        format,
        AosFormat::NotAos,
        "JSON primitive should not be detected as valid manifest"
    );
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
fn test_edge_case_minimum_valid_simple() {
    // Smallest possible valid simple format archive
    let manifest = b"{}";
    let archive = create_simple_aos(manifest);

    assert_eq!(archive.len(), 10); // 8 byte header + 2 byte manifest

    let format = detect_format(&archive);
    assert_eq!(format, AosFormat::Simple);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok());
    let (json, _, _) = result.unwrap();
    assert!(json.is_object());
}

#[test]
fn test_edge_case_minimum_valid_aos() {
    // Smallest possible valid AOS archive
    let metadata = b"{}";
    let weights: Vec<u8> = vec![];
    let archive = create_aos_archive(metadata, &weights);

    assert_eq!(archive.len(), AOS_HEADER_SIZE + 2); // Header + empty weights + 2 byte metadata

    let format = detect_format(&archive);
    assert_eq!(format, AosFormat::Aos);

    let result = parse_aos_header(&archive);
    assert!(result.is_ok());
}

#[test]
fn test_edge_case_exactly_header_size() {
    // Buffer exactly MIN_AOS_HEADER_SIZE bytes with no manifest
    let bytes = vec![0u8; MIN_AOS_HEADER_SIZE];

    let format = detect_format(&bytes);
    // All zeros means offset=0, len=0, which points before valid manifest position
    assert_eq!(format, AosFormat::NotAos);
}

#[test]
fn test_edge_case_manifest_at_max_offset() {
    // Manifest at a high offset within the file
    let padding = mock_weights(10000);
    let manifest = minimal_manifest();

    let manifest_offset: u32 = (MIN_AOS_HEADER_SIZE + padding.len()) as u32;
    let manifest_len: u32 = manifest.len() as u32;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&manifest_offset.to_le_bytes());
    bytes.extend_from_slice(&manifest_len.to_le_bytes());
    bytes.extend_from_slice(&padding);
    bytes.extend_from_slice(&manifest);

    let format = detect_format(&bytes);
    assert_eq!(format, AosFormat::Simple);

    let result = extract_simple_manifest(&bytes);
    assert!(result.is_ok());
}

#[test]
fn test_edge_case_whitespace_in_manifest() {
    // Manifest with lots of whitespace (should still parse)
    let manifest = b"  \n\t  {  \"version\"  :  \"1.0\"  }  \n  ";
    let archive = create_simple_aos(manifest);

    let format = detect_format(&archive);
    assert_eq!(format, AosFormat::Simple);

    let result = extract_simple_manifest(&archive);
    assert!(result.is_ok());
}

#[test]
fn test_read_coreml_sections_with_placement() {
    let manifest = serde_json::json!({
        "coreml": {
            "coreml_used": true,
            "coreml_device_type": "ane",
            "coreml_precision_mode": "q15"
        },
        "placement": {
            "records": [{
                "graph_target": "transformer.layer_0.attn.q_proj",
                "rank": 8,
                "direction": "q_proj",
                "alpha_override": 0.75
            }]
        }
    });

    let (coreml, placement) = read_coreml_sections(&manifest);
    assert_eq!(
        coreml,
        Some(CoremlTrainingSection {
            coreml_used: true,
            coreml_device_type: Some("ane".to_string()),
            coreml_precision_mode: Some("q15".to_string()),
            coreml_compile_config_id: None
        })
    );
    assert_eq!(
        placement,
        vec![PlacementRecord {
            graph_target: "transformer.layer_0.attn.q_proj".to_string(),
            rank: 8,
            direction: "q_proj".to_string(),
            alpha_override: Some(0.75)
        }]
    );
}

#[test]
fn test_read_coreml_sections_when_absent() {
    let manifest = serde_json::json!({
        "version": "2.0",
        "metadata": { "scope_path": "domain/group/scope/op" }
    });

    let (coreml, placement) = read_coreml_sections(&manifest);
    assert!(coreml.is_none(), "coreml section should be optional");
    assert!(
        placement.is_empty(),
        "placement records should default empty"
    );
}

// =============================================================================
// Constants Verification Tests
// =============================================================================

#[test]
fn test_constants_aos_magic() {
    assert_eq!(AOS_MAGIC.len(), 8);
    assert_eq!(&AOS_MAGIC[..4], b"AOS\0");
    assert_eq!(&AOS_MAGIC[4..], &[0, 0, 0, 0]);
}

#[test]
fn test_constants_header_sizes() {
    assert_eq!(MIN_AOS_HEADER_SIZE, 8, "Simple header is 8 bytes");
    assert_eq!(AOS_HEADER_SIZE, 268, "AOS header is 268 bytes");
    assert!(
        AOS_HEADER_SIZE > MIN_AOS_HEADER_SIZE,
        "AOS header is larger"
    );
}

#[test]
fn test_constants_aos_header_field_positions() {
    // Verify the documented field positions in AOS header
    // [0-7]: Magic
    // [8-11]: Version
    // [12-19]: Total size
    // [20-27]: Weights offset
    // [28-35]: Weights size
    // [36-43]: Metadata offset
    // [44-51]: Metadata size
    // [52-267]: Reserved

    let field_end = 52; // Last documented field ends at byte 52
    assert!(
        field_end <= AOS_HEADER_SIZE,
        "All fields fit within header"
    );
}

// =============================================================================
// Format Enum Tests
// =============================================================================

#[test]
fn test_aos_format_enum_equality() {
    assert_eq!(AosFormat::Aos, AosFormat::Aos);
    assert_eq!(AosFormat::Simple, AosFormat::Simple);
    assert_eq!(AosFormat::NotAos, AosFormat::NotAos);

    assert_ne!(AosFormat::Aos, AosFormat::Simple);
    assert_ne!(AosFormat::Aos, AosFormat::NotAos);
    assert_ne!(AosFormat::Simple, AosFormat::NotAos);
}

#[test]
fn test_aos_format_enum_debug() {
    // Ensure Debug trait is implemented
    let debug_str = format!("{:?}", AosFormat::Aos);
    assert!(debug_str.contains("Aos"));
}

#[test]
fn test_aos_format_enum_clone() {
    // Ensure Clone trait is implemented
    let format = AosFormat::Simple;
    let cloned = format;
    assert_eq!(format, cloned);
}

#[test]
fn test_aos_format_enum_copy() {
    // Ensure Copy trait is implemented
    let format = AosFormat::Aos;
    let copied: AosFormat = format;
    assert_eq!(format, copied);
}
