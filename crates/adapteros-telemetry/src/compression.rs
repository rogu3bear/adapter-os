//! Telemetry data compression
//!
//! Provides efficient compression of telemetry bundles to reduce storage
//! and network overhead while maintaining integrity verification.
//!
//! Compression strategies:
//! - zstd: Fast compression with good ratios (default)
//! - gzip: Wide compatibility
//! - lz4: Extremely fast, lower compression
//! - none: No compression (for already-compressed data)
//!
//! Per PRD-08: Implement telemetry data compression

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Compression algorithm
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// Zstandard compression (fast, good ratio)
    Zstd,
    /// Gzip compression (compatible)
    Gzip,
    /// LZ4 compression (extremely fast)
    Lz4,
}

impl CompressionAlgorithm {
    /// Get the file extension for this compression algorithm
    pub fn extension(&self) -> &'static str {
        match self {
            CompressionAlgorithm::None => "",
            CompressionAlgorithm::Zstd => ".zst",
            CompressionAlgorithm::Gzip => ".gz",
            CompressionAlgorithm::Lz4 => ".lz4",
        }
    }

    /// Get the MIME type for this compression algorithm
    pub fn mime_type(&self) -> &'static str {
        match self {
            CompressionAlgorithm::None => "application/octet-stream",
            CompressionAlgorithm::Zstd => "application/zstd",
            CompressionAlgorithm::Gzip => "application/gzip",
            CompressionAlgorithm::Lz4 => "application/x-lz4",
        }
    }

    /// Parse from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "zst" => Some(CompressionAlgorithm::Zstd),
            "gz" => Some(CompressionAlgorithm::Gzip),
            "lz4" => Some(CompressionAlgorithm::Lz4),
            _ => None,
        }
    }
}

/// Compression level (1-22 for zstd, 0-9 for gzip)
#[derive(Debug, Clone, Copy)]
pub struct CompressionLevel(pub i32);

impl CompressionLevel {
    /// Fastest compression (lowest ratio)
    pub const FASTEST: Self = Self(1);
    /// Default compression (balanced)
    pub const DEFAULT: Self = Self(3);
    /// Best compression (highest ratio, slowest)
    pub const BEST: Self = Self(22);

    /// Create a compression level, clamping to valid range
    pub fn new(level: i32) -> Self {
        Self(level.clamp(1, 22))
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Telemetry bundle compressor
pub struct TelemetryCompressor {
    algorithm: CompressionAlgorithm,
    level: CompressionLevel,
}

impl TelemetryCompressor {
    /// Create a new compressor with default settings (Zstd, level 3)
    pub fn new() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            level: CompressionLevel::DEFAULT,
        }
    }

    /// Create a compressor with specific algorithm and level
    pub fn with_config(algorithm: CompressionAlgorithm, level: CompressionLevel) -> Self {
        Self { algorithm, level }
    }

    /// Compress data
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Zstd => self.compress_zstd(data),
            CompressionAlgorithm::Gzip => self.compress_gzip(data),
            CompressionAlgorithm::Lz4 => self.compress_lz4(data),
        }
    }

    /// Decompress data
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Zstd => Self::decompress_zstd(data),
            CompressionAlgorithm::Gzip => Self::decompress_gzip(data),
            CompressionAlgorithm::Lz4 => Self::decompress_lz4(data),
        }
    }

    /// Compress using Zstandard
    fn compress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.level.0)
            .map_err(|e| AosError::Telemetry(format!("Zstd compression failed: {}", e)))
    }

    /// Decompress using Zstandard
    fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>> {
        zstd::decode_all(data)
            .map_err(|e| AosError::Telemetry(format!("Zstd decompression failed: {}", e)))
    }

    /// Compress using Gzip
    fn compress_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.level.0 as u32));
        encoder
            .write_all(data)
            .map_err(|e| AosError::Telemetry(format!("Gzip compression failed: {}", e)))?;

        encoder
            .finish()
            .map_err(|e| AosError::Telemetry(format!("Gzip compression failed: {}", e)))
    }

    /// Decompress using Gzip
    fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
        use flate2::read::GzDecoder;

        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();

        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| AosError::Telemetry(format!("Gzip decompression failed: {}", e)))?;

        Ok(decompressed)
    }

    /// Compress using LZ4
    fn compress_lz4(&self, data: &[u8]) -> Result<Vec<u8>> {
        lz4_flex::compress_prepend_size(data)
            .map_err(|e| AosError::Telemetry(format!("LZ4 compression failed: {}", e)))
            .map(|v| v.into())
    }

    /// Decompress using LZ4
    fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>> {
        lz4_flex::decompress_size_prepended(data)
            .map_err(|e| AosError::Telemetry(format!("LZ4 decompression failed: {}", e)))
    }

    /// Get compression ratio (compressed_size / original_size)
    pub fn compression_ratio(&self, original: &[u8], compressed: &[u8]) -> f64 {
        compressed.len() as f64 / original.len() as f64
    }

    /// Get compression algorithm
    pub fn algorithm(&self) -> CompressionAlgorithm {
        self.algorithm
    }
}

impl Default for TelemetryCompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compressed telemetry bundle metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedBundleMetadata {
    /// Compression algorithm used
    pub algorithm: CompressionAlgorithm,
    /// Original size in bytes
    pub original_size: usize,
    /// Compressed size in bytes
    pub compressed_size: usize,
    /// Compression ratio (compressed / original)
    pub compression_ratio: f64,
    /// Checksum of original data (BLAKE3)
    pub original_checksum: String,
    /// Checksum of compressed data (BLAKE3)
    pub compressed_checksum: String,
}

impl CompressedBundleMetadata {
    /// Create metadata for a compressed bundle
    pub fn new(
        algorithm: CompressionAlgorithm,
        original: &[u8],
        compressed: &[u8],
    ) -> Self {
        let original_size = original.len();
        let compressed_size = compressed.len();
        let compression_ratio = compressed_size as f64 / original_size as f64;

        let original_checksum = blake3::hash(original).to_hex().to_string();
        let compressed_checksum = blake3::hash(compressed).to_hex().to_string();

        Self {
            algorithm,
            original_size,
            compressed_size,
            compression_ratio,
            original_checksum,
            compressed_checksum,
        }
    }

    /// Verify checksums
    pub fn verify(&self, original: &[u8], compressed: &[u8]) -> bool {
        let original_ok = blake3::hash(original).to_hex().to_string() == self.original_checksum;
        let compressed_ok =
            blake3::hash(compressed).to_hex().to_string() == self.compressed_checksum;

        original_ok && compressed_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_test_data(size: usize) -> Vec<u8> {
        // Generate compressible data (repeated patterns)
        let pattern = b"AdapterOS telemetry event data with repeated patterns for compression testing. ";
        let mut data = Vec::with_capacity(size);

        while data.len() < size {
            data.extend_from_slice(pattern);
        }

        data.truncate(size);
        data
    }

    #[test]
    fn test_zstd_compression() {
        let compressor = TelemetryCompressor::new();
        let data = generate_test_data(10000);

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());

        let ratio = compressor.compression_ratio(&data, &compressed);
        println!("Zstd compression ratio: {:.2}%", ratio * 100.0);
    }

    #[test]
    fn test_gzip_compression() {
        let compressor =
            TelemetryCompressor::with_config(CompressionAlgorithm::Gzip, CompressionLevel::DEFAULT);
        let data = generate_test_data(10000);

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());

        let ratio = compressor.compression_ratio(&data, &compressed);
        println!("Gzip compression ratio: {:.2}%", ratio * 100.0);
    }

    #[test]
    fn test_lz4_compression() {
        let compressor =
            TelemetryCompressor::with_config(CompressionAlgorithm::Lz4, CompressionLevel::DEFAULT);
        let data = generate_test_data(10000);

        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(data, decompressed);
        assert!(compressed.len() < data.len());

        let ratio = compressor.compression_ratio(&data, &compressed);
        println!("LZ4 compression ratio: {:.2}%", ratio * 100.0);
    }

    #[test]
    fn test_no_compression() {
        let compressor =
            TelemetryCompressor::with_config(CompressionAlgorithm::None, CompressionLevel::DEFAULT);
        let data = generate_test_data(1000);

        let compressed = compressor.compress(&data).unwrap();
        assert_eq!(data, compressed);
    }

    #[test]
    fn test_compression_levels() {
        let data = generate_test_data(10000);

        let fastest = TelemetryCompressor::with_config(
            CompressionAlgorithm::Zstd,
            CompressionLevel::FASTEST,
        );
        let default =
            TelemetryCompressor::with_config(CompressionAlgorithm::Zstd, CompressionLevel::DEFAULT);
        let best = TelemetryCompressor::with_config(CompressionAlgorithm::Zstd, CompressionLevel::BEST);

        let fastest_compressed = fastest.compress(&data).unwrap();
        let default_compressed = default.compress(&data).unwrap();
        let best_compressed = best.compress(&data).unwrap();

        // Best compression should produce smallest output
        assert!(best_compressed.len() <= default_compressed.len());
        assert!(default_compressed.len() <= fastest_compressed.len());

        println!("Fastest: {} bytes", fastest_compressed.len());
        println!("Default: {} bytes", default_compressed.len());
        println!("Best: {} bytes", best_compressed.len());
    }

    #[test]
    fn test_compressed_bundle_metadata() {
        let compressor = TelemetryCompressor::new();
        let data = generate_test_data(10000);

        let compressed = compressor.compress(&data).unwrap();

        let metadata = CompressedBundleMetadata::new(
            CompressionAlgorithm::Zstd,
            &data,
            &compressed,
        );

        assert_eq!(metadata.original_size, data.len());
        assert_eq!(metadata.compressed_size, compressed.len());
        assert!(metadata.compression_ratio < 1.0);
        assert!(metadata.verify(&data, &compressed));
    }

    #[test]
    fn test_algorithm_extension() {
        assert_eq!(CompressionAlgorithm::Zstd.extension(), ".zst");
        assert_eq!(CompressionAlgorithm::Gzip.extension(), ".gz");
        assert_eq!(CompressionAlgorithm::Lz4.extension(), ".lz4");
        assert_eq!(CompressionAlgorithm::None.extension(), "");
    }

    #[test]
    fn test_algorithm_from_extension() {
        assert_eq!(
            CompressionAlgorithm::from_extension("zst"),
            Some(CompressionAlgorithm::Zstd)
        );
        assert_eq!(
            CompressionAlgorithm::from_extension("gz"),
            Some(CompressionAlgorithm::Gzip)
        );
        assert_eq!(
            CompressionAlgorithm::from_extension("lz4"),
            Some(CompressionAlgorithm::Lz4)
        );
        assert_eq!(CompressionAlgorithm::from_extension("unknown"), None);
    }
}
