//! AOS Archive Writer
//!
//! Creates single-file .aos archives with manifest + weights.
//!
//! ## Format Specification
//!
//! See docs/AOS_FORMAT.md for full specification.

use adapteros_core::{AosError, Result};
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::info;

/// Magic bytes identifying an AOS archive (4 bytes)
pub const AOS_MAGIC: [u8; 4] = *b"AOS\x00";

/// Current header size in bytes (64-byte aligned for cache efficiency)
pub const HEADER_SIZE: usize = 64;

/// AOS Writer Options
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// Include signature (default: true)
    pub include_signature: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            include_signature: true,
        }
    }
}

/// AOS Archive Writer
///
/// Creates .aos archive files with a 64-byte fixed header.
pub struct AosWriter {
    #[allow(dead_code)]
    options: WriteOptions,
}

/// Parsed AOS archive header
///
/// ## Binary Layout (64 bytes)
/// ```text
/// | Offset | Size | Field                              |
/// |--------|------|------------------------------------|
/// | 0      | 4    | Magic: "AOS\x00"                   |
/// | 4      | 4    | Flags (u32 LE, reserved)           |
/// | 8      | 8    | Weights offset (u64 LE)            |
/// | 16     | 8    | Weights size (u64 LE)              |
/// | 24     | 8    | Manifest offset (u64 LE)           |
/// | 32     | 8    | Manifest size (u64 LE)             |
/// | 40     | 24   | Reserved (padding)                 |
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AosHeader {
    /// Flags (reserved for future use)
    pub flags: u32,
    /// Offset to weights data
    pub weights_offset: u64,
    /// Size of weights data in bytes
    pub weights_size: u64,
    /// Offset to manifest JSON
    pub manifest_offset: u64,
    /// Size of manifest JSON in bytes
    pub manifest_size: u64,
}

impl AosWriter {
    /// Create a new AOS writer
    pub fn new() -> Self {
        Self {
            options: WriteOptions::default(),
        }
    }

    /// Create with custom options
    pub fn with_options(options: WriteOptions) -> Self {
        Self { options }
    }

    /// Write adapter archive to file
    ///
    /// ## Format (64-byte header)
    /// ```text
    /// | Offset | Size | Field                              |
    /// |--------|------|------------------------------------|
    /// | 0      | 4    | Magic: "AOS\x00"                   |
    /// | 4      | 4    | Flags (u32 LE, reserved)           |
    /// | 8      | 8    | Weights offset (u64 LE)            |
    /// | 16     | 8    | Weights size (u64 LE)              |
    /// | 24     | 8    | Manifest offset (u64 LE)           |
    /// | 32     | 8    | Manifest size (u64 LE)             |
    /// | 40     | 24   | Reserved (padding)                 |
    /// [64...]   weights (safetensors format)
    /// [...]     manifest (JSON)
    /// ```
    pub fn write_archive<P, M>(
        &self,
        output_path: P,
        manifest: &M,
        weights_data: &[u8],
    ) -> Result<u64>
    where
        P: AsRef<Path>,
        M: Serialize,
    {
        let output_path = output_path.as_ref();
        info!(path = %output_path.display(), "Writing AOS archive");

        // Serialize manifest to JSON
        let manifest_json = serde_json::to_vec_pretty(manifest)?;

        // Calculate offsets (v3.0 layout)
        let weights_offset = HEADER_SIZE as u64;
        let weights_size = weights_data.len() as u64;
        let manifest_offset = weights_offset + weights_size;
        let manifest_size = manifest_json.len() as u64;
        let total_size = manifest_offset + manifest_size;

        // Write archive
        let mut file = File::create(output_path)
            .map_err(|e| AosError::Io(format!("Failed to create archive: {}", e)))?;

        // Write 64-byte header (v3.0 format)
        let mut header = [0u8; HEADER_SIZE];

        // Magic bytes [0-7]
        header[0..8].copy_from_slice(&AOS_MAGIC);

        // Version [8-11]
        header[8..12].copy_from_slice(&AOS_VERSION.to_le_bytes());

        // Flags [12-15] - reserved, zeroed
        header[12..16].copy_from_slice(&0u32.to_le_bytes());

        // Total file size [16-23]
        header[16..24].copy_from_slice(&total_size.to_le_bytes());

        // Weights offset [24-31]
        header[24..32].copy_from_slice(&weights_offset.to_le_bytes());

        // Weights size [32-39]
        header[32..40].copy_from_slice(&weights_size.to_le_bytes());

        // Manifest offset [40-47]
        header[40..48].copy_from_slice(&manifest_offset.to_le_bytes());

        // Manifest size [48-55]
        header[48..56].copy_from_slice(&manifest_size.to_le_bytes());

        // Reserved [56-63] - already zeroed

        file.write_all(&header)
            .map_err(|e| AosError::Io(format!("Failed to write header: {}", e)))?;

        // Write weights (safetensors format)
        file.write_all(weights_data)
            .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

        // Write manifest (JSON)
        file.write_all(&manifest_json)
            .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;

        file.flush()
            .map_err(|e| AosError::Io(format!("Failed to flush archive: {}", e)))?;

        info!(
            path = %output_path.display(),
            total_size = total_size,
            weights_size = weights_size,
            manifest_size = manifest_size,
            "AOS archive written"
        );

        Ok(total_size)
    }

    /// Read and validate archive header
    ///
    /// Returns the parsed AosHeader with all offset and length fields.
    pub fn read_header<P: AsRef<Path>>(path: P) -> Result<AosHeader> {
        use std::io::Read;

        let mut file = File::open(path.as_ref())
            .map_err(|e| AosError::Io(format!("Failed to open archive: {}", e)))?;

        let mut header = [0u8; HEADER_SIZE];
        file.read_exact(&mut header)
            .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

        // Validate magic bytes (4 bytes)
        if header[0..4] != AOS_MAGIC {
            return Err(AosError::Validation(format!(
                "Invalid magic bytes: expected {:?}, got {:?}",
                AOS_MAGIC,
                &header[0..4]
            )));
        }

        let flags = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let weights_offset = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let weights_size = u64::from_le_bytes(header[16..24].try_into().unwrap());
        let manifest_offset = u64::from_le_bytes(header[24..32].try_into().unwrap());
        let manifest_size = u64::from_le_bytes(header[32..40].try_into().unwrap());

        Ok(AosHeader {
            flags,
            weights_offset,
            weights_size,
            manifest_offset,
            manifest_size,
        })
    }
}

impl Default for AosWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::NamedTempFile;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestManifest {
        adapter_id: String,
        rank: u32,
    }

    /// Generate valid safetensors format data for testing
    ///
    /// Safetensors format:
    /// - [0-7]: header_size (u64, little-endian)
    /// - [8..8+header_size]: JSON header with tensor metadata
    /// - [8+header_size..]: raw tensor data
    fn generate_test_safetensors() -> Vec<u8> {
        use std::collections::HashMap;

        // Create minimal tensor metadata
        let mut metadata = HashMap::new();

        // LoRA weight tensor metadata (4x4 float32 = 64 bytes)
        let tensor_meta = serde_json::json!({
            "dtype": "F32",
            "shape": [4, 4],
            "data_offsets": [0, 64]
        });
        metadata.insert("lora_A.weight", tensor_meta);

        // Add __metadata__ for adapter info
        let file_meta = serde_json::json!({
            "format": "pt",
            "framework": "adapteros"
        });
        metadata.insert("__metadata__", file_meta);

        // Serialize header to JSON
        let header_json = serde_json::to_string(&metadata).unwrap();
        let header_bytes = header_json.as_bytes();
        let header_size = header_bytes.len() as u64;

        // Build safetensors buffer
        let mut buffer = Vec::new();

        // Write header size (u64, little-endian)
        buffer.extend_from_slice(&header_size.to_le_bytes());

        // Write JSON header
        buffer.extend_from_slice(header_bytes);

        // Write tensor data (4x4 float32 matrix with deterministic values)
        // Using HKDF-style deterministic values for reproducibility
        let tensor_data: Vec<f32> = (0..16)
            .map(|i| (i as f32 * 0.1) - 0.8) // Values from -0.8 to 0.7
            .collect();

        for val in tensor_data {
            buffer.extend_from_slice(&val.to_le_bytes());
        }

        buffer
    }

    #[test]
    fn test_write_and_read_archive() -> Result<()> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let manifest = TestManifest {
            adapter_id: "test-adapter".to_string(),
            rank: 4,
        };

        let weights_data = generate_test_safetensors();

        let writer = AosWriter::new();
        let total_size = writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        // Verify header
        let header = AosWriter::read_header(temp_file.path())?;

        assert_eq!(header.version, AOS_VERSION);
        assert_eq!(header.weights_offset, HEADER_SIZE as u64);
        assert_eq!(header.weights_size, weights_data.len() as u64);
        assert_eq!(
            header.manifest_offset,
            HEADER_SIZE as u64 + weights_data.len() as u64
        );
        assert!(header.manifest_size > 0);
        assert_eq!(header.total_size, total_size);

        // Verify safetensors header can be parsed from the archive
        use std::io::{Read, Seek, SeekFrom};
        let mut file = File::open(temp_file.path())
            .map_err(|e| AosError::Io(format!("Failed to open archive: {}", e)))?;

        // Skip .aos header (64 bytes)
        file.seek(SeekFrom::Start(HEADER_SIZE as u64))
            .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

        // Read safetensors header size
        let mut st_header_size_bytes = [0u8; 8];
        file.read_exact(&mut st_header_size_bytes)
            .map_err(|e| AosError::Io(format!("Failed to read safetensors header size: {}", e)))?;
        let st_header_size = u64::from_le_bytes(st_header_size_bytes);

        // Verify header size is reasonable
        assert!(
            st_header_size > 0 && st_header_size < 10000,
            "Safetensors header size should be reasonable"
        );

        Ok(())
    }

    #[test]
    fn test_magic_validation() {
        let temp_file = NamedTempFile::new().unwrap();

        // Write invalid magic bytes (file too small for full header)
        std::fs::write(
            temp_file.path(),
            b"BADMAGICBADMAGICBADMAGICBADMAGICBADMAGICBADMAGICBADMAGICBADMAGIC",
        )
        .unwrap();

        let result = AosWriter::read_header(temp_file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid magic"));
    }

    #[test]
    fn test_header_size_is_64_bytes() {
        assert_eq!(HEADER_SIZE, 64);
    }

    #[test]
    fn test_small_archive() {
        let writer = AosWriter::new();
        let manifest = TestManifest {
            adapter_id: "test".to_string(),
            rank: 4,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let result = writer.write_archive(temp_file.path(), &manifest, b"small");

        // Should succeed for small data
        assert!(result.is_ok());
    }
}
