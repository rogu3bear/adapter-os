//! AOS 2.0: Memory-Mappable Single-File Adapter Format
//!
//! This module provides the AOS 2.0 binary format implementation with
//! fixed-offset sections for zero-copy weight loading via memory mapping.

use crate::format::{AdapterManifest, LineageInfo, SingleFileAdapter};
use crate::training::TrainingConfig;
use crate::weights::{WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, Result};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// AOS 2.0 file header (268 bytes, fixed size for mmap compatibility)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Aos2Header {
    /// Magic bytes: "AOS2\x00\x00\x00\x00"
    pub magic: [u8; 8],
    /// Format version (2 for AOS 2.0)
    pub version: u32,
    /// Total file size
    pub total_size: u64,
    /// Weights section offset
    pub weights_offset: u64,
    /// Weights section size
    pub weights_size: u64,
    /// Metadata section offset
    pub metadata_offset: u64,
    /// Metadata section size
    pub metadata_size: u64,
    /// Signatures section offset
    pub signatures_offset: u64,
    /// Signatures section size
    pub signatures_size: u64,
    /// BLAKE3 checksum of header (excluding this field)
    pub header_checksum: [u8; 32],
    /// Reserved for future use (padded to fill 268 bytes: 100 bytes used, reserved = 168)
    pub _reserved: [u8; 168],
}

impl Aos2Header {
    pub const MAGIC: &[u8; 8] = b"AOS2\x00\x00\x00\x00";
    pub const SIZE: usize = 268;

    /// Validate header magic and version
    pub fn validate(&self) -> Result<()> {
        if &self.magic != Self::MAGIC {
            return Err(AosError::Parse("Invalid AOS 2.0 magic bytes".to_string()));
        }
        if self.version != 2 {
            return Err(AosError::Parse(format!(
                "Unsupported AOS version: {} (expected 2)",
                self.version
            )));
        }
        Ok(())
    }

    /// Convert header to bytes for writing
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        unsafe { std::ptr::read(self as *const Self as *const [u8; Self::SIZE]) }
    }

    /// Create header from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(AosError::Parse("Header too short".to_string()));
        }
        unsafe { Ok(std::ptr::read(data.as_ptr() as *const Self)) }
    }
}

/// AOS 2.0 adapter loader with memory-mapped access
pub struct Aos2Adapter {
    /// Memory-mapped file
    mmap: Arc<Mmap>,
    /// Parsed header
    header: Aos2Header,
    /// Cached adapter (loaded on demand)
    adapter: parking_lot::RwLock<Option<Arc<SingleFileAdapter>>>,
}

impl Aos2Adapter {
    /// Load AOS 2.0 adapter from file path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open AOS 2.0 file: {}", e)))?;

        let mmap = unsafe {
            Mmap::map(&file)
                .map_err(|e| AosError::Io(format!("Failed to memory-map AOS 2.0 file: {}", e)))?
        };

        if mmap.len() < Aos2Header::SIZE {
            return Err(AosError::Parse(
                "AOS 2.0 file too short for header".to_string(),
            ));
        }

        let header = Aos2Header::from_bytes(&mmap[..Aos2Header::SIZE])?;
        header.validate()?;

        if mmap.len() < header.total_size as usize {
            return Err(AosError::Parse(format!(
                "File size {} < expected {}",
                mmap.len(),
                header.total_size
            )));
        }

        Ok(Self {
            mmap: Arc::new(mmap),
            header,
            adapter: parking_lot::RwLock::new(None),
        })
    }

    /// Load the full adapter (lazy loading)
    pub fn to_single_file_adapter(&self) -> Result<Arc<SingleFileAdapter>> {
        // Check cache first
        {
            let guard = self.adapter.read();
            if let Some(adapter) = guard.as_ref() {
                return Ok(Arc::clone(adapter));
            }
        }

        // Load from mmap
        let weights_data = &self.mmap[self.header.weights_offset as usize
            ..(self.header.weights_offset + self.header.weights_size) as usize];

        let metadata_data = &self.mmap[self.header.metadata_offset as usize
            ..(self.header.metadata_offset + self.header.metadata_size) as usize];

        // Decompress metadata
        let metadata_bytes = zstd::decode_all(metadata_data)
            .map_err(|e| AosError::Parse(format!("Failed to decompress metadata: {}", e)))?;

        let metadata: Aos2Metadata = serde_json::from_slice(&metadata_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse metadata: {}", e)))?;

        // Parse weights from AOS 2.0 format
        let weights = parse_weights_from_aos2(weights_data)?;

        // Build adapter
        let adapter = SingleFileAdapter {
            manifest: metadata.manifest,
            weights,
            training_data: metadata.training_data,
            config: metadata.config,
            lineage: metadata.lineage,
            signature: metadata.signature,
        };

        // Cache and return
        let adapter = Arc::new(adapter);
        *self.adapter.write() = Some(Arc::clone(&adapter));
        Ok(adapter)
    }

    /// Get header for inspection
    pub fn header(&self) -> &Aos2Header {
        &self.header
    }
}

/// Metadata bundle stored in AOS 2.0 format
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Aos2Metadata {
    pub manifest: AdapterManifest,
    pub config: TrainingConfig,
    pub lineage: LineageInfo,
    pub training_data: Vec<crate::training::TrainingExample>,
    pub signature: Option<crate::format::AosSignature>,
}

/// Weights structure for AOS 2.0 format
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Aos2Weights {
    pub positive: Vec<u8>,
    pub negative: Vec<u8>,
    pub combined: Option<Vec<u8>>,
    pub manifest: WeightGroupsManifest,
}

/// Parse weights from AOS 2.0 format
fn parse_weights_from_aos2(data: &[u8]) -> Result<crate::format::AdapterWeights> {
    use crate::format::WeightGroupType;
    use crate::weights::deserialize_weight_group;

    let weights: Aos2Weights = serde_json::from_slice(data)
        .map_err(|e| AosError::Parse(format!("Failed to parse AOS 2.0 weights: {}", e)))?;

    let positive = deserialize_weight_group(
        &weights.positive,
        disk_metadata_to_runtime(&weights.manifest.positive, WeightGroupType::Positive),
    )?;

    let negative = deserialize_weight_group(
        &weights.negative,
        disk_metadata_to_runtime(&weights.manifest.negative, WeightGroupType::Negative),
    )?;

    let combined = match (&weights.combined, &weights.manifest.combined) {
        (Some(bytes), Some(meta)) => Some(deserialize_weight_group(
            bytes,
            disk_metadata_to_runtime(meta, WeightGroupType::Combined),
        )?),
        _ => None,
    };

    Ok(crate::format::AdapterWeights {
        positive,
        negative,
        combined,
    })
}

/// Convert disk metadata to runtime metadata
fn disk_metadata_to_runtime(
    info: &WeightGroupDiskInfo,
    group_type: crate::format::WeightGroupType,
) -> crate::format::WeightMetadata {
    crate::format::WeightMetadata {
        example_count: info.example_count,
        avg_loss: info.avg_loss,
        training_time_ms: info.training_time_ms,
        group_type,
        created_at: info.created_at.clone(),
    }
}

/// Page alignment helper
pub fn align_to_page(size: u64, page_size: usize) -> u64 {
    let page_size = page_size as u64;
    size.div_ceil(page_size) * page_size
}
