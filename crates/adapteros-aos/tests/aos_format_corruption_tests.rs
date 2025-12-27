//! Comprehensive corruption detection tests for AOS archive format
//!
//! Tests verify:
//! 1. AOS file structure (header, index, segments, manifest)
//! 2. Hash validation for all segments
//! 3. Corruption detection (bad magic, truncated files, hash mismatches)
//! 4. Scope hash validation
//! 5. Multi-backend segment handling

use adapteros_aos::writer::{
    compute_scope_hash, open_aos, parse_segments, select_segment, AosWriter, BackendTag, AOS_MAGIC,
    HAS_INDEX_FLAG, HEADER_SIZE, INDEX_ENTRY_SIZE,
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

#[test]
fn test_aos_format_valid_archive() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)
        .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

    let manifest = TestManifest {
        adapter_id: "test-adapter".to_string(),
        rank: 8,
        metadata: HashMap::from([("scope_path".to_string(), "domain/group/scope".to_string())]),
    };

    let canonical_weights = fake_weights("canonical", 128);
    let mlx_weights = fake_weights("mlx", 64);

    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("domain/group/scope".to_string()),
        &canonical_weights,
    )?;
    writer.add_segment(BackendTag::Mlx, None, &mlx_weights)?;

    let total_size = writer.write_archive(temp_file.path(), &manifest)?;

    // Verify archive structure
    let data = fs::read(temp_file.path())?;
    assert_eq!(data.len(), total_size as usize);

    // Verify header
    let header = AosWriter::read_header(temp_file.path())?;
    assert_eq!(header.flags & HAS_INDEX_FLAG, HAS_INDEX_FLAG);
    assert_eq!(header.index_offset, HEADER_SIZE as u64);
    assert_eq!(header.index_size, 2 * INDEX_ENTRY_SIZE as u64);

    // Parse segments
    let segments = parse_segments(&data, &header)?;
    assert_eq!(segments.len(), 2);

    // Verify canonical segment
    assert_eq!(segments[0].backend_tag, BackendTag::Canonical);
    assert_eq!(segments[0].weights_hash, B3Hash::hash(&canonical_weights));

    // Verify MLX segment
    assert_eq!(segments[1].backend_tag, BackendTag::Mlx);
    assert_eq!(segments[1].weights_hash, B3Hash::hash(&mlx_weights));

    Ok(())
}

#[test]
fn test_aos_corrupt_magic_bytes() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Write invalid magic
    let mut corrupt_header = vec![0u8; HEADER_SIZE];
    corrupt_header[0..4].copy_from_slice(b"BAD!");
    fs::write(temp_file.path(), &corrupt_header).unwrap();

    let result = AosWriter::read_header(temp_file.path());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("invalid AOS magic"));
}

#[test]
fn test_aos_corrupt_truncated_header() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Write truncated header (only 32 bytes instead of 64)
    let mut incomplete_header = vec![0u8; 32];
    incomplete_header[0..4].copy_from_slice(&AOS_MAGIC);
    fs::write(temp_file.path(), &incomplete_header).unwrap();

    let result = AosWriter::read_header(temp_file.path());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Failed to read header"));
}

#[test]
fn test_aos_corrupt_missing_index_flag() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Create valid header but with missing index flag
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    // flags at [4..8] are zeros (no HAS_INDEX_FLAG)
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes());
    fs::write(temp_file.path(), &header).unwrap();

    let data = fs::read(temp_file.path()).unwrap();
    let parsed_header = AosWriter::parse_header_bytes(&data).unwrap();
    let result = parse_segments(&data, &parsed_header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("missing segment index"));
}

#[test]
fn test_aos_corrupt_hash_mismatch() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)?;

    let manifest = TestManifest {
        adapter_id: "test-adapter".to_string(),
        rank: 4,
        metadata: HashMap::from([("scope_path".to_string(), "domain/group".to_string())]),
    };

    let weights = fake_weights("test", 128);
    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("domain/group".to_string()),
        &weights,
    )?;
    writer.write_archive(temp_file.path(), &manifest)?;

    // Read and corrupt the segment data
    let mut data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;

    // Find segment payload and corrupt it
    let segment_offset = HEADER_SIZE + INDEX_ENTRY_SIZE;
    if segment_offset < data.len() {
        data[segment_offset] ^= 0xFF; // Flip byte in segment payload
        fs::write(temp_file.path(), &data)?;
    }

    // Re-read corrupted file
    let corrupted_data = fs::read(temp_file.path())?;
    let result = parse_segments(&corrupted_data, &header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("hash mismatch")
            || err.to_string().contains("Corrupted / needs retrain")
    );

    Ok(())
}

#[test]
fn test_aos_corrupt_index_overlap() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Create header with overlapping index and manifest
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes()); // index offset
    header[16..24].copy_from_slice(&(INDEX_ENTRY_SIZE as u64).to_le_bytes()); // index size
    header[24..32].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes()); // manifest overlaps index!
    header[32..40].copy_from_slice(&100u64.to_le_bytes()); // manifest size

    // Write some dummy data
    let mut data = header.clone();
    data.extend(vec![0u8; 500]);
    fs::write(temp_file.path(), &data).unwrap();

    let file_data = fs::read(temp_file.path()).unwrap();
    let parsed_header = AosWriter::parse_header_bytes(&file_data).unwrap();
    let result = parse_segments(&file_data, &parsed_header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("overlaps"));
}

#[test]
fn test_aos_corrupt_segment_beyond_file() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Create header with segment extending beyond file
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes());
    header[16..24].copy_from_slice(&(INDEX_ENTRY_SIZE as u64).to_le_bytes());
    header[24..32].copy_from_slice(&1000u64.to_le_bytes()); // manifest at offset 1000
    header[32..40].copy_from_slice(&100u64.to_le_bytes());

    // Create index entry with segment beyond file bounds
    let mut index_entry = vec![0u8; INDEX_ENTRY_SIZE];
    index_entry[0..4].copy_from_slice(&0u32.to_le_bytes()); // segment_id
    index_entry[4..6].copy_from_slice(&BackendTag::Canonical.as_u16().to_le_bytes());
    index_entry[8..16].copy_from_slice(&200u64.to_le_bytes()); // offset
    index_entry[16..24].copy_from_slice(&5000u64.to_le_bytes()); // length extends beyond file

    let mut data = header;
    data.extend(index_entry);
    data.extend(vec![0u8; 300]); // File is only ~450 bytes total
    fs::write(temp_file.path(), &data).unwrap();

    let file_data = fs::read(temp_file.path()).unwrap();
    let parsed_header = AosWriter::parse_header_bytes(&file_data).unwrap();
    let result = parse_segments(&file_data, &parsed_header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("beyond file") || err.to_string().contains("overflow"));
}

#[test]
fn test_aos_corrupt_invalid_backend_tag() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes());
    header[16..24].copy_from_slice(&(INDEX_ENTRY_SIZE as u64).to_le_bytes());
    header[24..32].copy_from_slice(&300u64.to_le_bytes()); // manifest offset
    header[32..40].copy_from_slice(&50u64.to_le_bytes());

    // Create index with invalid backend tag
    let mut index_entry = vec![0u8; INDEX_ENTRY_SIZE];
    index_entry[0..4].copy_from_slice(&0u32.to_le_bytes());
    index_entry[4..6].copy_from_slice(&999u16.to_le_bytes()); // Invalid backend tag
    index_entry[8..16].copy_from_slice(&200u64.to_le_bytes());
    index_entry[16..24].copy_from_slice(&10u64.to_le_bytes());

    let mut data = header;
    data.extend(index_entry);
    data.extend(vec![0u8; 200]); // Segment data
    data.extend(b"{\"test\":\"manifest\"}"); // Manifest
    fs::write(temp_file.path(), &data).unwrap();

    let file_data = fs::read(temp_file.path()).unwrap();
    let parsed_header = AosWriter::parse_header_bytes(&file_data).unwrap();
    let result = parse_segments(&file_data, &parsed_header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unknown backend tag"));
}

#[test]
fn test_aos_scope_hash_validation() -> Result<()> {
    let scope1 = "domain/group/scope1";
    let scope2 = "domain/group/scope2";

    let hash1 = compute_scope_hash(scope1);
    let hash2 = compute_scope_hash(scope2);

    // Different scopes should have different hashes
    assert_ne!(hash1, hash2);

    // Same scope should produce same hash (deterministic)
    let hash1_again = compute_scope_hash(scope1);
    assert_eq!(hash1, hash1_again);

    Ok(())
}

#[test]
fn test_aos_segment_selection_by_backend() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)?;

    let manifest = TestManifest {
        adapter_id: "multi-backend".to_string(),
        rank: 4,
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let scope_path = "test/scope";
    let canonical_weights = fake_weights("canonical", 100);
    let mlx_weights = fake_weights("mlx", 100);
    let metal_weights = fake_weights("metal", 100);

    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some(scope_path.to_string()),
        &canonical_weights,
    )?;
    writer.add_segment(BackendTag::Mlx, Some(scope_path.to_string()), &mlx_weights)?;
    writer.add_segment(
        BackendTag::Metal,
        Some(scope_path.to_string()),
        &metal_weights,
    )?;

    writer.write_archive(temp_file.path(), &manifest)?;

    // Load and test segment selection
    let data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;
    let segments = parse_segments(&data, &header)?;

    let scope_hash = compute_scope_hash(scope_path);

    // Select canonical backend
    let selected = select_segment(&segments, scope_hash, Some(BackendTag::Canonical));
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().backend_tag, BackendTag::Canonical);

    // Select MLX backend
    let selected = select_segment(&segments, scope_hash, Some(BackendTag::Mlx));
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().backend_tag, BackendTag::Mlx);

    // Select Metal backend
    let selected = select_segment(&segments, scope_hash, Some(BackendTag::Metal));
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().backend_tag, BackendTag::Metal);

    Ok(())
}

#[test]
fn test_aos_segment_selection_fallback() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)?;

    let manifest = TestManifest {
        adapter_id: "fallback-test".to_string(),
        rank: 4,
        metadata: HashMap::from([("scope_path".to_string(), "test/scope".to_string())]),
    };

    let weights = fake_weights("canonical", 100);
    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("test/scope".to_string()),
        &weights,
    )?;
    writer.write_archive(temp_file.path(), &manifest)?;

    let data = fs::read(temp_file.path())?;
    let header = AosWriter::parse_header_bytes(&data)?;
    let segments = parse_segments(&data, &header)?;

    let scope_hash = compute_scope_hash("test/scope");

    // Request CoreML backend (not available), should fallback to canonical
    let selected = select_segment(&segments, scope_hash, Some(BackendTag::Coreml));
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().backend_tag, BackendTag::Canonical);

    Ok(())
}

#[test]
fn test_aos_open_file_view() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)?;

    let manifest = TestManifest {
        adapter_id: "view-test".to_string(),
        rank: 8,
        metadata: HashMap::from([("scope_path".to_string(), "view/scope".to_string())]),
    };

    let weights = fake_weights("weights", 256);
    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some("view/scope".to_string()),
        &weights,
    )?;
    writer.write_archive(temp_file.path(), &manifest)?;

    // Open file view
    let data = fs::read(temp_file.path())?;
    let view = open_aos(&data)?;

    // Verify manifest
    let manifest_json: TestManifest = serde_json::from_slice(view.manifest_bytes)?;
    assert_eq!(manifest_json.adapter_id, "view-test");

    // Verify segments
    assert_eq!(view.segments.len(), 1);
    assert_eq!(view.segments[0].backend_tag, BackendTag::Canonical);
    assert_eq!(view.segments[0].payload, weights.as_slice());

    Ok(())
}

#[test]
fn test_aos_reserved_bytes_validation() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Create header with non-zero reserved bytes
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes());
    header[16..24].copy_from_slice(&0u64.to_le_bytes());
    header[24..32].copy_from_slice(&100u64.to_le_bytes());
    header[32..40].copy_from_slice(&50u64.to_le_bytes());
    // Reserved bytes [40..64] - set one to non-zero
    header[45] = 0xFF; // Invalid!

    fs::write(temp_file.path(), &header).unwrap();

    let result = AosWriter::read_header(temp_file.path());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("reserved header bytes non-zero"));
}

#[test]
fn test_aos_index_alignment() {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

    // Create header with misaligned index size (not multiple of 80)
    let mut header = vec![0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&AOS_MAGIC);
    header[4..8].copy_from_slice(&HAS_INDEX_FLAG.to_le_bytes());
    header[8..16].copy_from_slice(&(HEADER_SIZE as u64).to_le_bytes());
    header[16..24].copy_from_slice(&100u64.to_le_bytes()); // Not multiple of 80!
    header[24..32].copy_from_slice(&300u64.to_le_bytes());
    header[32..40].copy_from_slice(&50u64.to_le_bytes());

    let mut data = header;
    data.extend(vec![0u8; 400]);
    fs::write(temp_file.path(), &data).unwrap();

    let file_data = fs::read(temp_file.path()).unwrap();
    let parsed_header = AosWriter::parse_header_bytes(&file_data).unwrap();
    let result = parse_segments(&file_data, &parsed_header);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("80-byte aligned"));
}

#[test]
fn test_aos_multiple_segments_same_scope() -> Result<()> {
    let temp_root = new_test_tempdir();
    let temp_file = NamedTempFile::new_in(&temp_root)?;

    let manifest = TestManifest {
        adapter_id: "multi-segment".to_string(),
        rank: 4,
        metadata: HashMap::from([("scope_path".to_string(), "shared/scope".to_string())]),
    };

    let scope_path = "shared/scope";
    let canonical = fake_weights("canonical", 64);
    let mlx = fake_weights("mlx", 64);

    let mut writer = AosWriter::new();
    writer.add_segment(
        BackendTag::Canonical,
        Some(scope_path.to_string()),
        &canonical,
    )?;
    writer.add_segment(BackendTag::Mlx, Some(scope_path.to_string()), &mlx)?;
    writer.write_archive(temp_file.path(), &manifest)?;

    let data = fs::read(temp_file.path())?;
    let view = open_aos(&data)?;

    assert_eq!(view.segments.len(), 2);

    // Both segments should have the same scope hash
    let scope_hash = compute_scope_hash(scope_path);
    assert_eq!(view.segments[0].scope_hash, scope_hash);
    assert_eq!(view.segments[1].scope_hash, scope_hash);

    Ok(())
}
