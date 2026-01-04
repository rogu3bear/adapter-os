//! AOS: Memory-Mappable Single-File Adapter Format
//!
//! This module provides the AOS binary format implementation with
//! fixed-offset sections for zero-copy weight loading via memory mapping.

use crate::format::{AdapterManifest, LineageInfo, SingleFileAdapter};
use crate::training::TrainingConfig;
use crate::weights::{WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, Result};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// AOS file header (268 bytes, fixed size for mmap compatibility)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AosHeader {
    /// Magic bytes: "AOS\x00\x00\x00\x00\x00"
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

impl AosHeader {
    pub const MAGIC: &[u8; 8] = b"AOS\x00\x00\x00\x00\x00";
    pub const SIZE: usize = 268;

    /// Validate header magic and version
    pub fn validate(&self) -> Result<()> {
        if &self.magic != Self::MAGIC {
            return Err(AosError::Parse("Invalid AOS magic bytes".to_string()));
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

/// AOS adapter loader with memory-mapped access
pub struct AosAdapter {
    /// Memory-mapped file
    mmap: Arc<Mmap>,
    /// Parsed header
    header: AosHeader,
    /// Cached adapter (loaded on demand)
    adapter: parking_lot::RwLock<Option<Arc<SingleFileAdapter>>>,
}

impl AosAdapter {
    /// Load AOS adapter from file path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open AOS file: {}", e)))?;

        let mmap = unsafe {
            Mmap::map(&file)
                .map_err(|e| AosError::Io(format!("Failed to memory-map AOS file: {}", e)))?
        };

        if mmap.len() < AosHeader::SIZE {
            return Err(AosError::Parse("AOS file too short for header".to_string()));
        }

        let header = AosHeader::from_bytes(&mmap[..AosHeader::SIZE])?;
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

        let metadata: AosMetadata = serde_json::from_slice(&metadata_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse metadata: {}", e)))?;

        // Parse weights from AOS format
        let weights = parse_weights_from_aos(weights_data)?;

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
    pub fn header(&self) -> &AosHeader {
        &self.header
    }
}

/// Metadata bundle stored in AOS format
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AosMetadata {
    pub manifest: AdapterManifest,
    pub config: TrainingConfig,
    pub lineage: LineageInfo,
    pub training_data: Vec<crate::training::TrainingExample>,
    pub signature: Option<crate::format::AosSignature>,
}

/// Weights structure for AOS format
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AosWeights {
    pub positive: Vec<u8>,
    pub negative: Vec<u8>,
    pub combined: Option<Vec<u8>>,
    pub manifest: WeightGroupsManifest,
}

/// Parse weights from AOS format
fn parse_weights_from_aos(data: &[u8]) -> Result<crate::format::AdapterWeights> {
    use crate::format::WeightGroupType;
    use crate::weights::deserialize_weight_group;

    let weights: AosWeights = serde_json::from_slice(data)
        .map_err(|e| AosError::Parse(format!("Failed to parse AOS weights: {}", e)))?;

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
