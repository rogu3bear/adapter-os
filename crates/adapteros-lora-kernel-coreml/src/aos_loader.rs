//! .aos archive loading utilities for CoreML backend
//!
//! Provides support for loading adapters from .aos archives in multiple formats:
//! - Simple .aos format: manifest_offset/manifest_len header
//! - AOS format: 268-byte header with AOS magic

use adapteros_core::{AosError, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// AOS magic bytes (8 bytes for AOS 2.0 format with 268-byte header)
pub const AOS_MAGIC: &[u8; 8] = b"AOS\x00\x00\x00\x00\x00";

/// Minimum size for .aos header (manifest_offset + manifest_len)
pub const MIN_AOS_HEADER_SIZE: usize = 8;

/// AOS header size
pub const AOS_HEADER_SIZE: usize = 268;

/// Detect if bytes are a raw .aos archive
///
/// Returns the detected format type if the bytes appear to be an .aos archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AosFormat {
    /// AOS format with 268-byte magic header
    Aos,
    /// Simple format with manifest_offset/manifest_len
    Simple,
    /// Not an .aos archive (likely a UTF-8 path)
    NotAos,
}

/// Minimal view of CoreML training metadata in manifest.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CoremlTrainingSection {
    pub coreml_used: bool,
    #[serde(default)]
    pub coreml_device_type: Option<String>,
    #[serde(default)]
    pub coreml_precision_mode: Option<String>,
    #[serde(default)]
    pub coreml_compile_config_id: Option<String>,
}

/// Placement section containing CoreML graph placement hints.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PlacementSection {
    #[serde(default)]
    pub records: Vec<PlacementRecord>,
}

/// Placement record describing how LoRA maps to CoreML graph targets.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PlacementRecord {
    pub graph_target: String,
    pub rank: u32,
    pub direction: String,
    #[serde(default)]
    pub alpha_override: Option<f32>,
}

/// Extract CoreML training and placement metadata from manifest JSON value.
pub fn read_coreml_sections(
    manifest: &serde_json::Value,
) -> (Option<CoremlTrainingSection>, Vec<PlacementRecord>) {
    let coreml = manifest
        .get("coreml")
        .and_then(|v| serde_json::from_value::<CoremlTrainingSection>(v.clone()).ok());

    let placement = manifest
        .get("placement")
        .and_then(|v| serde_json::from_value::<PlacementSection>(v.clone()).ok())
        .map(|p| p.records)
        .unwrap_or_default();

    (coreml, placement)
}

/// Detect the format of plan_bytes
pub fn detect_format(plan_bytes: &[u8]) -> AosFormat {
    // Check for AOS magic bytes first (8-byte magic for AOS 2.0)
    if plan_bytes.len() >= 8 && &plan_bytes[..8] == AOS_MAGIC {
        return AosFormat::Aos;
    }

    // Check for simple .aos format (manifest_offset + manifest_len header)
    if plan_bytes.len() >= MIN_AOS_HEADER_SIZE {
        let manifest_offset =
            u32::from_le_bytes([plan_bytes[0], plan_bytes[1], plan_bytes[2], plan_bytes[3]])
                as usize;
        let manifest_len =
            u32::from_le_bytes([plan_bytes[4], plan_bytes[5], plan_bytes[6], plan_bytes[7]])
                as usize;

        // Validate: offset should be >= 8 (after header), and offset+len should fit in plan_bytes
        let is_simple_aos = manifest_offset >= MIN_AOS_HEADER_SIZE
            && manifest_len > 0
            && manifest_len < 1024 * 1024 // Reasonable manifest size limit (1MB)
            && manifest_offset.saturating_add(manifest_len) <= plan_bytes.len();

        // Additional check: the manifest should be valid JSON
        if is_simple_aos {
            let manifest_slice = &plan_bytes[manifest_offset..manifest_offset + manifest_len];
            if let Ok(manifest_str) = std::str::from_utf8(manifest_slice) {
                if manifest_str.trim_start().starts_with('{') {
                    return AosFormat::Simple;
                }
            }
        }
    }

    AosFormat::NotAos
}

/// Parse AOS header from bytes
///
/// Returns (version, total_size, weights_offset, weights_size, metadata_offset, metadata_size)
pub fn parse_aos_header(plan_bytes: &[u8]) -> Result<(u32, usize, usize, usize, usize, usize)> {
    if plan_bytes.len() < AOS_HEADER_SIZE {
        return Err(AosError::Kernel(format!(
            "AOS file too short: {} bytes (need at least {} for header)",
            plan_bytes.len(),
            AOS_HEADER_SIZE
        )));
    }

    // Parse header fields
    let version =
        u32::from_le_bytes([plan_bytes[8], plan_bytes[9], plan_bytes[10], plan_bytes[11]]);

    if version != 2 {
        return Err(AosError::Kernel(format!(
            "Unsupported AOS version: {} (expected 2)",
            version
        )));
    }

    let total_size = u64::from_le_bytes([
        plan_bytes[12],
        plan_bytes[13],
        plan_bytes[14],
        plan_bytes[15],
        plan_bytes[16],
        plan_bytes[17],
        plan_bytes[18],
        plan_bytes[19],
    ]) as usize;

    let weights_offset = u64::from_le_bytes([
        plan_bytes[20],
        plan_bytes[21],
        plan_bytes[22],
        plan_bytes[23],
        plan_bytes[24],
        plan_bytes[25],
        plan_bytes[26],
        plan_bytes[27],
    ]) as usize;

    let weights_size = u64::from_le_bytes([
        plan_bytes[28],
        plan_bytes[29],
        plan_bytes[30],
        plan_bytes[31],
        plan_bytes[32],
        plan_bytes[33],
        plan_bytes[34],
        plan_bytes[35],
    ]) as usize;

    let metadata_offset = u64::from_le_bytes([
        plan_bytes[36],
        plan_bytes[37],
        plan_bytes[38],
        plan_bytes[39],
        plan_bytes[40],
        plan_bytes[41],
        plan_bytes[42],
        plan_bytes[43],
    ]) as usize;

    let metadata_size = u64::from_le_bytes([
        plan_bytes[44],
        plan_bytes[45],
        plan_bytes[46],
        plan_bytes[47],
        plan_bytes[48],
        plan_bytes[49],
        plan_bytes[50],
        plan_bytes[51],
    ]) as usize;

    // Validate sizes
    if plan_bytes.len() < total_size {
        return Err(AosError::Kernel(format!(
            "AOS file truncated: {} bytes (expected {})",
            plan_bytes.len(),
            total_size
        )));
    }

    if metadata_offset + metadata_size > plan_bytes.len() {
        return Err(AosError::Kernel(format!(
            "Invalid AOS metadata offset/size: {}+{} > {}",
            metadata_offset,
            metadata_size,
            plan_bytes.len()
        )));
    }

    Ok((
        version,
        total_size,
        weights_offset,
        weights_size,
        metadata_offset,
        metadata_size,
    ))
}

/// Parse simple .aos header from bytes
///
/// Returns (manifest_offset, manifest_len)
pub fn parse_simple_aos_header(plan_bytes: &[u8]) -> Result<(usize, usize)> {
    if plan_bytes.len() < MIN_AOS_HEADER_SIZE {
        return Err(AosError::Kernel(format!(
            "Simple .aos file too short: {} bytes",
            plan_bytes.len()
        )));
    }

    let manifest_offset =
        u32::from_le_bytes([plan_bytes[0], plan_bytes[1], plan_bytes[2], plan_bytes[3]]) as usize;
    let manifest_len =
        u32::from_le_bytes([plan_bytes[4], plan_bytes[5], plan_bytes[6], plan_bytes[7]]) as usize;

    // Validate bounds
    if manifest_offset + manifest_len > plan_bytes.len() {
        return Err(AosError::Kernel(format!(
            "Invalid .aos header: manifest_offset({}) + manifest_len({}) > file_size({})",
            manifest_offset,
            manifest_len,
            plan_bytes.len()
        )));
    }

    Ok((manifest_offset, manifest_len))
}

/// Extract and parse manifest JSON from simple .aos format
pub fn extract_simple_manifest(plan_bytes: &[u8]) -> Result<(serde_json::Value, usize, usize)> {
    let (manifest_offset, manifest_len) = parse_simple_aos_header(plan_bytes)?;

    let manifest_bytes = &plan_bytes[manifest_offset..manifest_offset + manifest_len];
    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
        .map_err(|e| AosError::Kernel(format!("Failed to parse .aos manifest JSON: {}", e)))?;

    Ok((manifest, manifest_offset, manifest_len))
}

/// Write weights data to a temp file and return the path
pub fn write_weights_to_temp(weights_data: &[u8]) -> Result<PathBuf> {
    use adapteros_storage::platform::common::PlatformUtils;
    use std::io::Write;

    let temp_root = PlatformUtils::temp_dir().join("aos");
    std::fs::create_dir_all(&temp_root).map_err(|e| {
        AosError::Io(format!(
            "Failed to create adapterOS temp directory {}: {}",
            temp_root.display(),
            e
        ))
    })?;

    let mut temp_file = tempfile::NamedTempFile::new_in(&temp_root)
        .map_err(|e| AosError::Io(format!("Failed to create temp file for .aos: {}", e)))?;

    temp_file
        .write_all(weights_data)
        .map_err(|e| AosError::Io(format!("Failed to write .aos weights to temp file: {}", e)))?;

    let temp_path = temp_file.into_temp_path();
    temp_path
        .keep()
        .map_err(|e| AosError::Io(format!("Failed to persist .aos temp weights file: {}", e)))
}

/// Memory-map a file and return the mapped bytes
pub fn mmap_file(path: &PathBuf) -> Result<memmap2::Mmap> {
    use memmap2::Mmap;
    use std::fs::File;

    let file = File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open .aos file {}: {}",
            path.display(),
            e
        ))
    })?;

    let mmap = unsafe {
        Mmap::map(&file).map_err(|e| {
            AosError::Io(format!(
                "Failed to memory-map .aos file {}: {}",
                path.display(),
                e
            ))
        })?
    };

    Ok(mmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_not_aos() {
        let path_bytes = b"/path/to/model.mlmodelc";
        assert_eq!(detect_format(path_bytes), AosFormat::NotAos);
    }

    #[test]
    fn test_detect_format_aos() {
        let mut bytes = vec![0u8; 300];
        bytes[..8].copy_from_slice(AOS_MAGIC);
        assert_eq!(detect_format(&bytes), AosFormat::Aos);
    }

    #[test]
    fn test_detect_format_simple() {
        // Create a simple .aos format with manifest at offset 8
        let manifest = br#"{"version": "1.0"}"#;
        let manifest_offset: u32 = 8;
        let manifest_len: u32 = manifest.len() as u32;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&manifest_offset.to_le_bytes());
        bytes.extend_from_slice(&manifest_len.to_le_bytes());
        bytes.extend_from_slice(manifest);

        assert_eq!(detect_format(&bytes), AosFormat::Simple);
    }

    #[test]
    fn test_parse_simple_aos_header() {
        let manifest = br#"{"version": "1.0"}"#;
        let manifest_offset: u32 = 8;
        let manifest_len: u32 = manifest.len() as u32;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&manifest_offset.to_le_bytes());
        bytes.extend_from_slice(&manifest_len.to_le_bytes());
        bytes.extend_from_slice(manifest);

        let (offset, len) = parse_simple_aos_header(&bytes).unwrap();
        assert_eq!(offset, 8);
        assert_eq!(len, manifest.len());
    }
}
