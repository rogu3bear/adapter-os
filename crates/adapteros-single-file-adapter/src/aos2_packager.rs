//! AOS 2.0 format packager
//!
//! Provides functionality to create AOS 2.0 format files with fixed-offset
//! sections for memory-mappable weight loading.

use crate::aos2_format::{align_to_page, Aos2Header, Aos2Metadata, Aos2Weights};
use crate::format::SingleFileAdapter;
use crate::weights::{serialize_weight_group, WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, B3Hash, Result};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// Page size for memory alignment (typically 4096 bytes)
const PAGE_SIZE: usize = 4096;

/// Options for AOS 2.0 packaging
#[derive(Debug, Clone)]
pub struct Aos2PackageOptions {
    /// Whether to compress metadata section
    pub compress_metadata: bool,
    /// Whether to compress weights section
    pub compress_weights: bool,
    /// Compression level (1-22 for zstd)
    pub compression_level: i32,
    /// Whether to include combined weights
    pub include_combined_weights: bool,
}

impl Default for Aos2PackageOptions {
    fn default() -> Self {
        Self {
            compress_metadata: true,
            compress_weights: false, // Keep weights uncompressed for mmap
            compression_level: 3,
            include_combined_weights: true,
        }
    }
}

/// AOS 2.0 packager
pub struct Aos2Packager;

impl Aos2Packager {
    /// Save adapter to AOS 2.0 format
    pub async fn save<P: AsRef<Path>>(adapter: &SingleFileAdapter, path: P) -> Result<()> {
        Self::save_with_options(adapter, path, Aos2PackageOptions::default()).await
    }

    /// Save adapter to AOS 2.0 format with options
    pub async fn save_with_options<P: AsRef<Path>>(
        adapter: &SingleFileAdapter,
        path: P,
        options: Aos2PackageOptions,
    ) -> Result<()> {
        let path = path.as_ref();

        // Create file
        let mut file = File::create(path)
            .map_err(|e| AosError::Io(format!("Failed to create AOS 2.0 file: {}", e)))?;

        // Reserve space for header
        file.set_len(Aos2Header::SIZE as u64)
            .map_err(|e| AosError::Io(format!("Failed to set file length: {}", e)))?;

        // Prepare weights section
        let positive_weights = serialize_weight_group(&adapter.weights.positive)?;
        let negative_weights = serialize_weight_group(&adapter.weights.negative)?;
        let combined_weights = if options.include_combined_weights {
            adapter
                .weights
                .combined
                .as_ref()
                .and_then(|c| serialize_weight_group(c).ok())
        } else {
            None
        };

        // Build weights manifest
        let weight_manifest = WeightGroupsManifest {
            positive: WeightGroupDiskInfo {
                example_count: adapter.weights.positive.metadata.example_count,
                avg_loss: adapter.weights.positive.metadata.avg_loss,
                training_time_ms: adapter.weights.positive.metadata.training_time_ms,
                created_at: adapter.weights.positive.metadata.created_at.clone(),
            },
            negative: WeightGroupDiskInfo {
                example_count: adapter.weights.negative.metadata.example_count,
                avg_loss: adapter.weights.negative.metadata.avg_loss,
                training_time_ms: adapter.weights.negative.metadata.training_time_ms,
                created_at: adapter.weights.negative.metadata.created_at.clone(),
            },
            combined: combined_weights.as_ref().map(|_| {
                let combined = adapter.weights.combined.as_ref().unwrap();
                WeightGroupDiskInfo {
                    example_count: combined.metadata.example_count,
                    avg_loss: combined.metadata.avg_loss,
                    training_time_ms: combined.metadata.training_time_ms,
                    created_at: combined.metadata.created_at.clone(),
                }
            }),
            combination_strategy: adapter.manifest.weight_groups.combination_strategy.clone(),
            use_separate_weights: adapter.manifest.weight_groups.use_separate_weights,
        };

        // Build weights structure
        let weights = Aos2Weights {
            positive: positive_weights,
            negative: negative_weights,
            combined: combined_weights,
            manifest: weight_manifest,
        };

        let weights_json = serde_json::to_vec(&weights)
            .map_err(|e| AosError::Training(format!("Failed to serialize weights: {}", e)))?;

        // Compress weights if requested
        let weights_data = if options.compress_weights {
            zstd::encode_all(weights_json.as_slice(), options.compression_level)
                .map_err(|e| AosError::Io(format!("Failed to compress weights: {}", e)))?
        } else {
            weights_json
        };

        // Prepare metadata section
        let metadata = Aos2Metadata {
            manifest: adapter.manifest.clone(),
            config: adapter.config.clone(),
            lineage: adapter.lineage.clone(),
            training_data: adapter.training_data.clone(),
            signature: adapter.signature.clone(),
        };

        let metadata_json = serde_json::to_vec(&metadata)
            .map_err(|e| AosError::Training(format!("Failed to serialize metadata: {}", e)))?;

        // Compress metadata
        let metadata_data = if options.compress_metadata {
            zstd::encode_all(metadata_json.as_slice(), options.compression_level)
                .map_err(|e| AosError::Io(format!("Failed to compress metadata: {}", e)))?
        } else {
            metadata_json
        };

        // Calculate offsets with page alignment for weights section
        let weights_offset = align_to_page(Aos2Header::SIZE as u64, PAGE_SIZE);

        // Write weights section
        file.seek(SeekFrom::Start(weights_offset))
            .map_err(|e| AosError::Io(format!("Failed to seek to weights section: {}", e)))?;
        file.write_all(&weights_data)
            .map_err(|e| AosError::Io(format!("Failed to write weights section: {}", e)))?;
        let weights_size = weights_data.len() as u64;

        // Write metadata section
        let metadata_offset = weights_offset + weights_size;
        file.seek(SeekFrom::Start(metadata_offset))
            .map_err(|e| AosError::Io(format!("Failed to seek to metadata section: {}", e)))?;
        file.write_all(&metadata_data)
            .map_err(|e| AosError::Io(format!("Failed to write metadata section: {}", e)))?;
        let metadata_size = metadata_data.len() as u64;

        // Prepare signature section
        let signature_data = if let Some(sig) = &adapter.signature {
            serde_json::to_vec(sig)
                .map_err(|e| AosError::Training(format!("Failed to serialize signature: {}", e)))?
        } else {
            Vec::new()
        };

        // Write signature section
        let signatures_offset = metadata_offset + metadata_size;
        file.seek(SeekFrom::Start(signatures_offset))
            .map_err(|e| AosError::Io(format!("Failed to seek to signatures section: {}", e)))?;
        file.write_all(&signature_data)
            .map_err(|e| AosError::Io(format!("Failed to write signatures section: {}", e)))?;
        let signatures_size = signature_data.len() as u64;

        // Calculate total size and compute checksum
        let total_size = signatures_offset + signatures_size;

        // Compute header checksum (excluding the checksum field itself)
        let header_checksum = {
            let temp_header = Aos2Header {
                magic: *Aos2Header::MAGIC,
                version: 2,
                total_size,
                weights_offset,
                weights_size,
                metadata_offset,
                metadata_size,
                signatures_offset,
                signatures_size,
                header_checksum: [0; 32],
                _reserved: [0; 168],
            };
            let header_bytes = temp_header.to_bytes();
            // Hash all but the checksum field (first 68 bytes are before checksum)
            B3Hash::hash(&header_bytes[..68]).to_bytes()
        };

        // Create and write header
        let header = Aos2Header {
            magic: *Aos2Header::MAGIC,
            version: 2,
            total_size,
            weights_offset,
            weights_size,
            metadata_offset,
            metadata_size,
            signatures_offset,
            signatures_size,
            header_checksum,
            _reserved: [0; 168],
        };

        file.seek(SeekFrom::Start(0))
            .map_err(|e| AosError::Io(format!("Failed to seek to header: {}", e)))?;
        file.write_all(&header.to_bytes())
            .map_err(|e| AosError::Io(format!("Failed to write header: {}", e)))?;

        // Sync file
        file.sync_all()
            .map_err(|e| AosError::Io(format!("Failed to sync file: {}", e)))?;

        Ok(())
    }
}
