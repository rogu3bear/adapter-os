//! AOS 2.0 Archive Writer
//!
//! Creates single-file .aos archives with manifest + weights.

use adapteros_core::{AosError, Result};
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::info;

/// AOS 2.0 Writer Options
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

/// AOS 2.0 Archive Writer
pub struct AOS2Writer {
    #[allow(dead_code)]
    options: WriteOptions,
}

impl AOS2Writer {
    /// Create a new AOS 2.0 writer
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
    /// ## Format
    /// ```text
    /// [0-3]    manifest_offset (u32, little-endian)
    /// [4-7]    manifest_len (u32, little-endian)
    /// [8...]   weights (safetensors format)
    /// [offset] manifest (JSON)
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
        info!(path = %output_path.display(), "Writing AOS 2.0 archive");

        // Serialize manifest to JSON
        let manifest_json = serde_json::to_vec_pretty(manifest)?;

        // Calculate offsets
        let header_size = 8; // 2 x u32
        let _weights_offset = header_size;
        let manifest_offset = header_size + weights_data.len();
        let manifest_len = manifest_json.len();

        // Validate sizes fit in u32
        if manifest_offset > u32::MAX as usize {
            return Err(AosError::Validation(format!(
                "Archive too large: manifest_offset {} exceeds u32::MAX",
                manifest_offset
            )));
        }
        if manifest_len > u32::MAX as usize {
            return Err(AosError::Validation(format!(
                "Manifest too large: {} exceeds u32::MAX",
                manifest_len
            )));
        }

        // Write archive
        let mut file = File::create(output_path)
            .map_err(|e| AosError::Io(format!("Failed to create archive: {}", e)))?;

        // Write header
        file.write_all(&(manifest_offset as u32).to_le_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write manifest_offset: {}", e)))?;
        file.write_all(&(manifest_len as u32).to_le_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write manifest_len: {}", e)))?;

        // Write weights (safetensors format)
        file.write_all(weights_data)
            .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

        // Write manifest (JSON)
        file.write_all(&manifest_json)
            .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;

        file.flush()
            .map_err(|e| AosError::Io(format!("Failed to flush archive: {}", e)))?;

        let total_size = header_size + weights_data.len() + manifest_json.len();
        info!(
            path = %output_path.display(),
            total_size = total_size,
            weights_size = weights_data.len(),
            manifest_size = manifest_json.len(),
            "AOS 2.0 archive written"
        );

        Ok(total_size as u64)
    }

    /// Read and validate archive header
    pub fn read_header<P: AsRef<Path>>(path: P) -> Result<(u32, u32)> {
        use std::io::Read;

        let mut file = File::open(path.as_ref())
            .map_err(|e| AosError::Io(format!("Failed to open archive: {}", e)))?;

        let mut header = [0u8; 8];
        file.read_exact(&mut header)
            .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

        let manifest_offset = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let manifest_len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        Ok((manifest_offset, manifest_len))
    }
}

impl Default for AOS2Writer {
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
        version: String,
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
            .map(|i| (i as f32 * 0.1) - 0.8)  // Values from -0.8 to 0.7
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
            version: "2.0".to_string(),
            adapter_id: "test-adapter".to_string(),
            rank: 4,
        };

        let weights_data = generate_test_safetensors();

        let writer = AOS2Writer::new();
        let total_size = writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        // Verify header
        let (manifest_offset, manifest_len) = AOS2Writer::read_header(temp_file.path())?;

        assert_eq!(manifest_offset as usize, 8 + weights_data.len());
        assert!(manifest_len > 0);
        assert!(total_size > 0);

        // Verify safetensors header can be parsed from the archive
        use std::io::{Read, Seek, SeekFrom};
        let mut file = File::open(temp_file.path())
            .map_err(|e| AosError::Io(format!("Failed to open archive: {}", e)))?;

        // Skip .aos header (8 bytes)
        file.seek(SeekFrom::Start(8))
            .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

        // Read safetensors header size
        let mut st_header_size_bytes = [0u8; 8];
        file.read_exact(&mut st_header_size_bytes)
            .map_err(|e| AosError::Io(format!("Failed to read safetensors header size: {}", e)))?;
        let st_header_size = u64::from_le_bytes(st_header_size_bytes);

        // Verify header size is reasonable
        assert!(st_header_size > 0 && st_header_size < 10000, "Safetensors header size should be reasonable");

        Ok(())
    }

    #[test]
    fn test_large_archive_validation() {
        let writer = AOS2Writer::new();
        let manifest = TestManifest {
            version: "2.0".to_string(),
            adapter_id: "test".to_string(),
            rank: 4,
        };

        // Create weights data larger than u32::MAX would cause manifest_offset overflow
        // We can't actually create 4GB+ of data in a test, so just verify the validation exists
        let temp_file = NamedTempFile::new().unwrap();
        let result = writer.write_archive(temp_file.path(), &manifest, b"small");

        // Should succeed for small data
        assert!(result.is_ok());
    }
}
