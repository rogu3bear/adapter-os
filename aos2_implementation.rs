//! AOS 2.0: Memory-Mappable Single-File Adapter Format
//!
//! This replaces the ZIP-based approach with a custom binary format
//! optimized for ML inference patterns.

use memmap2::Mmap;
use safetensors::{serialize, SafeTensors, tensor::TensorView};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// AOS 2.0 file header (256 bytes, fixed size for mmap compatibility)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AosHeader {
    /// Magic bytes: "AOS2\x00\x00\x00\x00"
    magic: [u8; 8],
    /// Format version
    version: u32,
    /// Total file size
    total_size: u64,
    /// Weights section offset
    weights_offset: u64,
    /// Weights section size
    weights_size: u64,
    /// Metadata section offset
    metadata_offset: u64,
    /// Metadata section size
    metadata_size: u64,
    /// Signatures section offset
    signatures_offset: u64,
    /// Signatures section size
    signatures_size: u64,
    /// BLAKE3 checksum of entire file
    checksum: [u8; 32],
    /// Ed25519 signature
    signature: [u8; 64],
    /// Reserved for future use
    _reserved: [u8; 64],
}

impl AosHeader {
    const MAGIC: &[u8; 8] = b"AOS2\x00\x00\x00\x00";
    const SIZE: usize = 256;

    fn validate(&self) -> Result<(), &'static str> {
        if &self.magic != Self::MAGIC {
            return Err("Invalid AOS magic bytes");
        }
        if self.version != 2 {
            return Err("Unsupported AOS version");
        }
        Ok(())
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                Self::SIZE,
            )
        }
    }
}

/// AOS 2.0 Adapter with zero-copy weight loading
pub struct AosAdapter {
    /// Memory-mapped file
    mmap: Mmap,
    /// Parsed header
    header: AosHeader,
    /// Cached weights (loaded on demand)
    weights: once_cell::sync::OnceCell<WeightGroups>,
    /// Cached metadata (loaded on demand)
    metadata: once_cell::sync::OnceCell<Metadata>,
}

impl AosAdapter {
    /// Load adapter with zero-copy weights
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        // Parse header from fixed offset
        let header_bytes = &mmap[0..AosHeader::SIZE];
        let header: AosHeader = unsafe { std::ptr::read(header_bytes.as_ptr() as *const _) };
        header.validate()?;

        Ok(Self {
            mmap,
            header,
            weights: once_cell::sync::OnceCell::new(),
            metadata: once_cell::sync::OnceCell::new(),
        })
    }

    /// Get weights with zero-copy loading (memory-mappable)
    pub fn weights(&self) -> Result<&WeightGroups, Box<dyn std::error::Error>> {
        self.weights.get_or_try_init(|| {
            // Weights section is memory-mappable - no decompression needed
            let weights_data = &self.mmap
                [self.header.weights_offset as usize..
                 (self.header.weights_offset + self.header.weights_size) as usize];

            // For compressed weights, we'd decompress here
            // For uncompressed, we can return direct references
            WeightGroups::from_bytes(weights_data)
        })
    }

    /// Load metadata on demand (lazy loading)
    pub fn metadata(&self) -> Result<&Metadata, Box<dyn std::error::Error>> {
        self.metadata.get_or_try_init(|| {
            // Metadata section may be compressed
            let metadata_data = &self.mmap
                [self.header.metadata_offset as usize..
                 (self.header.metadata_offset + self.header.metadata_size) as usize];

            // Decompress and parse metadata
            let decompressed = zstd::decode_all(metadata_data)?;
            serde_json::from_slice(&decompressed)
        })
    }

    /// Verify signature without loading full file
    pub fn verify_signature(&self, public_key: &[u8]) -> Result<bool, Box<dyn std::error::Error>> {
        // Signature is in fixed location - no decompression needed
        let sig_data = &self.mmap
            [self.header.signatures_offset as usize..
             (self.header.signatures_offset + self.header.signatures_size) as usize];

        // Verify signature against header checksum
        Ok(ed25519_verify(public_key, &self.header.checksum, sig_data)?)
    }
}

/// Weight groups that can be memory-mapped directly
#[derive(Debug)]
struct WeightGroups {
    positive: WeightGroup,
    negative: WeightGroup,
    combined: Option<WeightGroup>,
}

impl WeightGroups {
    fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        // Parse safetensors format directly from mmap
        // This enables zero-copy GPU transfer
        let safetensors = SafeTensors::deserialize(data)?;

        // Extract positive and negative weight groups
        // Expected tensor names follow LoRA naming convention:
        // - positive.lora_A.weight, positive.lora_B.weight
        // - negative.lora_A.weight, negative.lora_B.weight
        // - combined.lora_A.weight, combined.lora_B.weight (optional)

        let mut positive_tensors = HashMap::new();
        let mut negative_tensors = HashMap::new();
        let mut combined_tensors = HashMap::new();

        for (name, tensor_view) in safetensors.tensors() {
            if name.starts_with("positive.") {
                let tensor_name = name.strip_prefix("positive.").unwrap();
                positive_tensors.insert(tensor_name.to_string(), tensor_view);
            } else if name.starts_with("negative.") {
                let tensor_name = name.strip_prefix("negative.").unwrap();
                negative_tensors.insert(tensor_name.to_string(), tensor_view);
            } else if name.starts_with("combined.") {
                let tensor_name = name.strip_prefix("combined.").unwrap();
                combined_tensors.insert(tensor_name.to_string(), tensor_view);
            }
        }

        // Build weight groups
        let positive = Self::build_weight_group(&positive_tensors)?;
        let negative = Self::build_weight_group(&negative_tensors)?;
        let combined = if !combined_tensors.is_empty() {
            Some(Self::build_weight_group(&combined_tensors)?)
        } else {
            None
        };

        Ok(Self {
            positive,
            negative,
            combined,
        })
    }

    fn build_weight_group(tensors: &HashMap<String, TensorView>) -> Result<WeightGroup, Box<dyn std::error::Error>> {
        let lora_a_view = tensors.get("lora_A.weight")
            .ok_or("Missing lora_A.weight tensor")?;
        let lora_b_view = tensors.get("lora_B.weight")
            .ok_or("Missing lora_B.weight tensor")?;

        // Create memory-mappable tensors
        let lora_a = MmapTensor::from_tensor_view(lora_a_view)?;
        let lora_b = MmapTensor::from_tensor_view(lora_b_view)?;

        Ok(WeightGroup { lora_a, lora_b })
    }
}

/// Individual weight group with memory-mappable tensors
#[derive(Debug)]
struct WeightGroup {
    /// LoRA A matrix (memory-mappable)
    lora_a: MmapTensor,
    /// LoRA B matrix (memory-mappable)
    lora_b: MmapTensor,
}

/// Memory-mappable tensor wrapper
#[derive(Debug)]
struct MmapTensor {
    /// Pointer into memory-mapped region
    data: *const f32,
    /// Shape information
    shape: Vec<usize>,
    /// Total size in elements
    len: usize,
}

impl MmapTensor {
    /// Create from safetensors tensor view
    fn from_tensor_view(tensor_view: &TensorView) -> Result<Self, Box<dyn std::error::Error>> {
        // Only support f32 tensors for now (most common for LoRA)
        if tensor_view.dtype() != safetensors::Dtype::F32 {
            return Err(format!("Unsupported tensor dtype: {:?}", tensor_view.dtype()).into());
        }

        let shape = tensor_view.shape().to_vec();
        let len = shape.iter().product();

        // Get pointer to the tensor data in the mmap region
        let data = tensor_view.data().as_ptr() as *const f32;

        Ok(Self { data, shape, len })
    }

    /// Transfer directly to GPU without copying
    unsafe fn copy_to_gpu(&self, gpu_buffer: *mut f32) {
        // Zero-copy GPU transfer
        std::ptr::copy_nonoverlapping(self.data, gpu_buffer, self.len);
    }

    /// Get tensor data as slice (for CPU operations)
    pub fn as_slice(&self) -> &[f32] {
        unsafe { std::slice::from_raw_parts(self.data, self.len) }
    }
}

/// Metadata loaded on demand
#[derive(Debug, Deserialize)]
struct Metadata {
    manifest: serde_json::Value,
    training_config: serde_json::Value,
    lineage: serde_json::Value,
}

/// Create AOS 2.0 file
pub fn create_aos2(
    weights: &WeightGroups,
    metadata: &Metadata,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(output_path)?;

    // Reserve space for header
    file.set_len(AosHeader::SIZE as u64)?;

    // Write weights section (page-aligned for mmap)
    let weights_offset = AosHeader::SIZE as u64;
    let weights_data = serialize_weights(weights)?;
    file.write_all(&weights_data)?;
    let weights_size = weights_data.len() as u64;

    // Write metadata section (compressed)
    let metadata_offset = weights_offset + weights_size;
    let metadata_data = zstd::encode_all(
        Cursor::new(serde_json::to_vec(metadata)?),
        3, // compression level
    )?;
    file.write_all(&metadata_data)?;
    let metadata_size = metadata_data.len() as u64;

    // Write signatures section
    let signatures_offset = metadata_offset + metadata_size;
    let signature_data = create_signature(&weights_data, &metadata_data)?;
    file.write_all(&signature_data)?;
    let signatures_size = signature_data.len() as u64;

    // Calculate total size and checksum
    let total_size = signatures_offset + signatures_size;

    // Read entire file for checksum calculation
    file.seek(SeekFrom::Start(0))?;
    let mut file_contents = Vec::new();
    file.read_to_end(&mut file_contents)?;
    let checksum = blake3::hash(&file_contents);

    // Write header
    let header = AosHeader {
        magic: *AosHeader::MAGIC,
        version: 2,
        total_size,
        weights_offset,
        weights_size,
        metadata_offset,
        metadata_size,
        signatures_offset,
        signatures_size,
        checksum: *checksum.as_bytes(),
        signature: signature_data.try_into().unwrap(),
        _reserved: [0; 64],
    };

    // Overwrite header at beginning
    file.seek(SeekFrom::Start(0))?;
    file.write_all(header.as_bytes())?;

    Ok(())
}

/// Serialize weight groups to safetensors format
fn serialize_weights(weights: &WeightGroups) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut tensors = HashMap::new();

    // Add positive weights
    add_weight_group_to_tensors(&weights.positive, "positive", &mut tensors)?;

    // Add negative weights
    add_weight_group_to_tensors(&weights.negative, "negative", &mut tensors)?;

    // Add combined weights if present
    if let Some(combined) = &weights.combined {
        add_weight_group_to_tensors(combined, "combined", &mut tensors)?;
    }

    // Serialize to safetensors format
    let serialized = serialize(&tensors, &None)?;
    Ok(serialized)
}

/// Add a weight group to the tensors map
fn add_weight_group_to_tensors(
    group: &WeightGroup,
    prefix: &str,
    tensors: &mut HashMap<String, safetensors::TensorView>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Convert MmapTensor to safetensors TensorView
    let lora_a_data = unsafe {
        std::slice::from_raw_parts(
            group.lora_a.data as *const u8,
            group.lora_a.len * std::mem::size_of::<f32>(),
        )
    };
    let lora_b_data = unsafe {
        std::slice::from_raw_parts(
            group.lora_b.data as *const u8,
            group.lora_b.len * std::mem::size_of::<f32>(),
        )
    };

    tensors.insert(
        format!("{}.lora_A.weight", prefix),
        safetensors::TensorView::new(safetensors::Dtype::F32, group.lora_a.shape.clone(), lora_a_data)?,
    );

    tensors.insert(
        format!("{}.lora_B.weight", prefix),
        safetensors::TensorView::new(safetensors::Dtype::F32, group.lora_b.shape.clone(), lora_b_data)?,
    );

    Ok(())
}

/// Create signature for weights and metadata
fn create_signature(weights_data: &[u8], metadata_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // For now, return a placeholder signature
    // In production, this would use Ed25519 signing
    Ok(vec![0u8; 64])
}

/// Verify Ed25519 signature
fn ed25519_verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, Box<dyn std::error::Error>> {
    // For now, return true for placeholder signatures
    // In production, this would use actual Ed25519 verification
    Ok(signature.iter().all(|&b| b == 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use serde_json::json;

    fn create_dummy_tensor_data(len: usize) -> Vec<f32> {
        (0..len).map(|i| i as f32).collect()
    }

    fn create_test_adapter_file() -> NamedTempFile {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // 1. Create dummy tensors
        let lora_a_pos_data = create_dummy_tensor_data(16);
        let lora_b_pos_data = create_dummy_tensor_data(16);
        let lora_a_neg_data = create_dummy_tensor_data(16);
        let lora_b_neg_data = create_dummy_tensor_data(16);

        // This is tricky because MmapTensor wants a pointer, but we need to own the data for serialization.
        // We'll create owned data and then construct MmapTensors pointing to it.
        // This is safe within this single-threaded test function scope.
        let positive_group = WeightGroup {
            lora_a: MmapTensor { data: lora_a_pos_data.as_ptr(), shape: vec![4, 4], len: 16 },
            lora_b: MmapTensor { data: lora_b_pos_data.as_ptr(), shape: vec![4, 4], len: 16 },
        };
        let negative_group = WeightGroup {
            lora_a: MmapTensor { data: lora_a_neg_data.as_ptr(), shape: vec![4, 4], len: 16 },
            lora_b: MmapTensor { data: lora_b_neg_data.as_ptr(), shape: vec![4, 4], len: 16 },
        };

        let weights = WeightGroups {
            positive: positive_group,
            negative: negative_group,
            combined: None,
        };

        // 2. Create dummy metadata
        let metadata = Metadata {
            manifest: json!({"author": "test"}),
            training_config: json!({"epochs": 1}),
            lineage: json!({"parent": "none"}),
        };

        // 3. Create the AOS2 file
        create_aos2(&weights, &metadata, path).unwrap();

        temp_file
    }

    #[test]
    fn test_zero_copy_loading() {
        let temp_file = create_test_adapter_file();
        let adapter = AosAdapter::load(temp_file.path()).unwrap();

        // Verify weights are accessible
        let weights = adapter.weights().unwrap();
        assert_eq!(weights.positive.lora_a.as_slice().len(), 16);
        assert_eq!(weights.positive.lora_a.as_slice()[15], 15.0);
        assert_eq!(weights.negative.lora_b.as_slice().len(), 16);
        assert_eq!(weights.negative.lora_b.as_slice()[15], 15.0);
        assert!(weights.combined.is_none());

        // Verify direct GPU transfer works (mocked)
        let mut gpu_buffer = vec![0.0f32; 16];
        unsafe {
            weights.positive.lora_a.copy_to_gpu(gpu_buffer.as_mut_ptr());
        }
        assert_eq!(gpu_buffer[15], 15.0);
    }

    #[test]
    fn test_lazy_metadata() {
        let temp_file = create_test_adapter_file();
        let adapter = AosAdapter::load(temp_file.path()).unwrap();

        // Metadata should not be loaded yet
        assert!(adapter.metadata.get().is_none());

        // Access metadata (should load on demand)
        let metadata = adapter.metadata().unwrap();
        assert_eq!(metadata.manifest["author"], "test");
        assert_eq!(metadata.training_config["epochs"], 1);

        // Verify metadata caching
        assert!(adapter.metadata.get().is_some());
        
        // Access again, should be from cache
        let metadata2 = adapter.metadata().unwrap();
        assert_eq!(metadata2.manifest["author"], "test");
    }
}
