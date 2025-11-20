//! AOS v2 Parser - Production Implementation
//!
//! Parses the actual AOS v2 format with proper safetensors support.
//!
//! ## Format Specification
//!
//! ```text
//! [0-3]    manifest_offset (u32, little-endian)
//! [4-7]    manifest_len (u32, little-endian)
//! [8...]   weights (safetensors format)
//! [offset] manifest (JSON)
//! ```
//!
//! ## Features
//!
//! - Memory-mapped file access (zero-copy)
//! - Safetensors tensor metadata extraction
//! - BLAKE3 hash verification
//! - Proper error handling with AosError
//!
//! ## Example
//!
//! ```rust
//! use adapteros_aos::aos_v2_parser::AosV2Parser;
//!
//! let parser = AosV2Parser::open("adapter.aos")?;
//! let manifest = parser.manifest()?;
//! let tensor_info = parser.tensor_metadata()?;
//!
//! // Extract specific tensor
//! if let Some(data) = parser.tensor("lora_A")? {
//!     println!("Tensor shape: {:?}", data.shape());
//! }
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use tracing::{debug, info};

/// AOS v2 Parser
///
/// Provides access to AOS v2 archive contents through memory-mapped I/O.
#[derive(Debug)]
pub struct AosV2Parser {
    /// Memory-mapped file
    mmap: memmap2::Mmap,
    /// Parsed manifest offset
    manifest_offset: usize,
    /// Parsed manifest length
    manifest_len: usize,
    /// Parsed safetensors view (lazily initialized)
    safetensors: Option<safetensors::SafeTensors<'static>>,
}

impl AosV2Parser {
    /// Open and parse an AOS v2 file
    ///
    /// Args:
    /// - `path`: Path to .aos file
    ///
    /// Errors:
    /// - `AosError::Io` if file cannot be opened or mapped
    /// - `AosError::Validation` if header is invalid
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!(path = %path.display(), "Opening AOS v2 archive");

        // Open and memory-map the file
        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open AOS v2 file: {}", e)))?;

        let mmap = unsafe {
            memmap2::Mmap::map(&file)
                .map_err(|e| AosError::Io(format!("Failed to mmap AOS v2 file: {}", e)))?
        };

        // Validate minimum size (8-byte header)
        if mmap.len() < 8 {
            return Err(AosError::Validation(format!(
                "File too small: {} bytes (minimum 8 bytes required)",
                mmap.len()
            )));
        }

        // Parse header
        let manifest_offset = u32::from_le_bytes([mmap[0], mmap[1], mmap[2], mmap[3]]) as usize;
        let manifest_len = u32::from_le_bytes([mmap[4], mmap[5], mmap[6], mmap[7]]) as usize;

        // Validate offsets
        if manifest_offset + manifest_len > mmap.len() {
            return Err(AosError::Validation(format!(
                "Invalid manifest bounds: offset={}, len={}, file_size={}",
                manifest_offset,
                manifest_len,
                mmap.len()
            )));
        }

        debug!(
            manifest_offset = manifest_offset,
            manifest_len = manifest_len,
            file_size = mmap.len(),
            "Parsed AOS v2 header"
        );

        Ok(Self {
            mmap,
            manifest_offset,
            manifest_len,
            safetensors: None,
        })
    }

    /// Get the raw manifest bytes
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.mmap[self.manifest_offset..self.manifest_offset + self.manifest_len]
    }

    /// Parse the manifest as JSON
    ///
    /// Errors:
    /// - `AosError::Serialization` if manifest is not valid JSON
    pub fn manifest<M: for<'de> Deserialize<'de>>(&self) -> Result<M> {
        let bytes = self.manifest_bytes();
        serde_json::from_slice(bytes).map_err(|e| AosError::Serialization(e))
    }

    /// Get the weights section bounds (safetensors data)
    pub fn weights_section(&self) -> (usize, usize) {
        let start = 8; // After header
        let end = self.manifest_offset;
        (start, end)
    }

    /// Get raw weights bytes (safetensors format)
    pub fn weights_bytes(&self) -> &[u8] {
        let (start, end) = self.weights_section();
        &self.mmap[start..end]
    }

    /// Initialize safetensors parser (lazy initialization)
    fn ensure_safetensors(&mut self) -> Result<&safetensors::SafeTensors<'static>> {
        if self.safetensors.is_none() {
            let weights_data = self.weights_bytes();

            // SAFETY: We need to transmute the lifetime because SafeTensors expects 'static
            // This is safe because:
            // 1. The mmap is owned by this struct and won't be dropped while safetensors exists
            // 2. SafeTensors doesn't modify the data
            // 3. We only expose references tied to &self lifetime
            let static_data: &'static [u8] = unsafe { std::mem::transmute(weights_data) };

            let st = safetensors::SafeTensors::deserialize(static_data)
                .map_err(|e| AosError::Other(format!("Failed to parse safetensors: {}", e)))?;

            self.safetensors = Some(st);
        }

        Ok(self.safetensors.as_ref().unwrap())
    }

    /// Get tensor metadata (names, shapes, dtypes) without copying data
    ///
    /// Errors:
    /// - `AosError::Other` if safetensors format is invalid
    pub fn tensor_metadata(&mut self) -> Result<HashMap<String, TensorInfo>> {
        let st = self.ensure_safetensors()?;
        let mut metadata = HashMap::new();

        for (name, tensor_view) in st.tensors() {
            metadata.insert(
                name.clone(),
                TensorInfo {
                    name: name.clone(),
                    shape: tensor_view.shape().to_vec(),
                    dtype: format!("{:?}", tensor_view.dtype()),
                    offset: tensor_view.data().as_ptr() as usize,
                    size: tensor_view.data().len(),
                },
            );
        }

        debug!(tensor_count = metadata.len(), "Extracted tensor metadata");
        Ok(metadata)
    }

    /// Get tensor names
    ///
    /// Errors:
    /// - `AosError::Other` if safetensors format is invalid
    pub fn tensor_names(&mut self) -> Result<Vec<String>> {
        let st = self.ensure_safetensors()?;
        Ok(st.names().iter().map(|s| s.to_string()).collect())
    }

    /// Get a specific tensor's data by name
    ///
    /// Args:
    /// - `name`: Tensor name (e.g., "lora_A", "lora_B")
    ///
    /// Returns:
    /// - `Some(TensorView)` if tensor exists
    /// - `None` if tensor not found
    ///
    /// Errors:
    /// - `AosError::Other` if safetensors format is invalid
    pub fn tensor(&mut self, name: &str) -> Result<Option<TensorView<'_>>> {
        let st = self.ensure_safetensors()?;

        if let Ok(tensor_view) = st.tensor(name) {
            Ok(Some(TensorView {
                name: name.to_string(),
                shape: tensor_view.shape().to_vec(),
                dtype: format!("{:?}", tensor_view.dtype()),
                data: tensor_view.data(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Verify the archive's BLAKE3 hash if present in manifest
    ///
    /// Args:
    /// - `expected_hash`: Expected BLAKE3 hash from manifest
    ///
    /// Errors:
    /// - `AosError::Verification` if hash mismatch
    pub fn verify_hash(&self, expected_hash: &B3Hash) -> Result<()> {
        let weights_data = self.weights_bytes();
        let actual_hash = B3Hash::hash(weights_data);

        if &actual_hash != expected_hash {
            return Err(AosError::Verification(format!(
                "Hash mismatch: expected {}, got {}",
                expected_hash.to_hex(),
                actual_hash.to_hex()
            )));
        }

        debug!(
            hash = %expected_hash.to_short_hex(),
            "Verified weights hash"
        );
        Ok(())
    }

    /// Get the total file size
    pub fn file_size(&self) -> usize {
        self.mmap.len()
    }

    /// Extract all tensors to a HashMap (copies data)
    ///
    /// Errors:
    /// - `AosError::Other` if safetensors format is invalid
    pub fn extract_all_tensors(&mut self) -> Result<HashMap<String, Vec<u8>>> {
        let st = self.ensure_safetensors()?;
        let mut tensors = HashMap::new();

        for name in st.names() {
            if let Ok(tensor_view) = st.tensor(name) {
                tensors.insert(name.to_string(), tensor_view.data().to_vec());
            }
        }

        debug!(tensor_count = tensors.len(), "Extracted all tensors");
        Ok(tensors)
    }
}

/// Tensor metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorInfo {
    /// Tensor name
    pub name: String,
    /// Tensor shape (e.g., [768, 16])
    pub shape: Vec<usize>,
    /// Data type (e.g., "F32", "F16")
    pub dtype: String,
    /// Byte offset in file
    pub offset: usize,
    /// Size in bytes
    pub size: usize,
}

impl TensorInfo {
    /// Get the number of elements
    pub fn num_elements(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get the element size in bytes (derived from total size)
    pub fn element_size(&self) -> usize {
        let num_elements = self.num_elements();
        if num_elements == 0 {
            0
        } else {
            self.size / num_elements
        }
    }
}

/// Tensor view (borrowed from mmap)
#[derive(Debug)]
pub struct TensorView<'a> {
    /// Tensor name
    pub name: String,
    /// Tensor shape
    pub shape: Vec<usize>,
    /// Data type
    pub dtype: String,
    /// Raw tensor data (borrowed from mmap)
    pub data: &'a [u8],
}

impl<'a> TensorView<'a> {
    /// Get the number of elements
    pub fn num_elements(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get the element size in bytes
    pub fn element_size(&self) -> usize {
        let num_elements = self.num_elements();
        if num_elements == 0 {
            0
        } else {
            self.data.len() / num_elements
        }
    }

    /// Get tensor shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get raw data
    pub fn as_bytes(&self) -> &[u8] {
        self.data
    }

    /// Copy data to owned Vec
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }
}

/// Standard AOS v2 manifest structure
///
/// This matches the format written by AOS2Writer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AosV2Manifest {
    /// Format version (always "2.0")
    pub version: String,
    /// Adapter ID
    pub adapter_id: String,
    /// LoRA rank
    pub rank: u32,
    /// BLAKE3 hash of weights (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights_hash: Option<B3Hash>,
    /// Tensor shapes (for validation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tensor_shapes: Option<HashMap<String, Vec<usize>>>,
    /// Additional metadata
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AosV2Manifest {
    /// Validate manifest format
    pub fn validate(&self) -> Result<()> {
        if self.version != "2.0" {
            return Err(AosError::Validation(format!(
                "Invalid AOS version: expected 2.0, got {}",
                self.version
            )));
        }

        if self.adapter_id.is_empty() {
            return Err(AosError::Validation(
                "Adapter ID cannot be empty".to_string(),
            ));
        }

        if self.rank == 0 {
            return Err(AosError::Validation(
                "LoRA rank must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aos2_writer::AOS2Writer;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_aos_v2_archive() -> Result<()> {
        // Create a test archive
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test-adapter".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: HashMap::new(),
        };

        // Create fake safetensors data (just enough to parse header)
        // Real safetensors has: [header_size:u64][header_json][data]
        let header_json = serde_json::json!({
            "lora_A": {
                "dtype": "F32",
                "shape": [768, 8],
                "data_offsets": [0, 24576]
            }
        });
        let header_bytes = serde_json::to_vec(&header_json).unwrap();
        let header_size = header_bytes.len() as u64;

        let mut weights_data = Vec::new();
        weights_data.extend_from_slice(&header_size.to_le_bytes());
        weights_data.extend_from_slice(&header_bytes);
        weights_data.extend_from_slice(&vec![0u8; 24576]); // Fake tensor data

        // Write archive
        let writer = AOS2Writer::new();
        writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        // Parse archive
        let mut parser = AosV2Parser::open(temp_file.path())?;

        // Verify header
        assert_eq!(parser.manifest_offset, 8 + weights_data.len());
        assert!(parser.manifest_len > 0);

        // Parse manifest
        let parsed_manifest: AosV2Manifest = parser.manifest()?;
        assert_eq!(parsed_manifest.version, "2.0");
        assert_eq!(parsed_manifest.adapter_id, "test-adapter");
        assert_eq!(parsed_manifest.rank, 8);

        // Get tensor metadata
        let tensor_info = parser.tensor_metadata()?;
        assert!(tensor_info.contains_key("lora_A"));
        assert_eq!(tensor_info["lora_A"].shape, vec![768, 8]);

        Ok(())
    }

    #[test]
    fn test_invalid_file_size() {
        // Create file with less than 8 bytes
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"short").unwrap();

        let result = AosV2Parser::open(temp_file.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AosError::Validation(_)));
    }

    #[test]
    fn test_hash_verification() -> Result<()> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let weights_data = b"test_weights_data";
        let weights_hash = B3Hash::hash(weights_data);

        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test-hash".to_string(),
            rank: 4,
            weights_hash: Some(weights_hash.clone()),
            tensor_shapes: None,
            metadata: HashMap::new(),
        };

        let writer = AOS2Writer::new();
        writer.write_archive(temp_file.path(), &manifest, weights_data)?;

        let parser = AosV2Parser::open(temp_file.path())?;
        parser.verify_hash(&weights_hash)?;

        Ok(())
    }

    #[test]
    fn test_manifest_validation() {
        // Invalid version
        let manifest = AosV2Manifest {
            version: "1.0".to_string(),
            adapter_id: "test".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: HashMap::new(),
        };
        assert!(manifest.validate().is_err());

        // Empty adapter ID
        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: HashMap::new(),
        };
        assert!(manifest.validate().is_err());

        // Zero rank
        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test".to_string(),
            rank: 0,
            weights_hash: None,
            tensor_shapes: None,
            metadata: HashMap::new(),
        };
        assert!(manifest.validate().is_err());

        // Valid manifest
        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: HashMap::new(),
        };
        assert!(manifest.validate().is_ok());
    }
}
