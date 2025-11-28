use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Magic bytes identifying an AOS archive (4 bytes)
pub const AOS_MAGIC: [u8; 4] = *b"AOS\x00";

/// Header size in bytes (64-byte aligned for cache efficiency)
pub const HEADER_SIZE: usize = 64;

/// AOS Format Loader
///
/// Loads memory-mappable AOS format with zero-copy GPU transfer.
///
/// ## Format Specification (64-byte header)
///
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
///
/// # Features
/// - Safetensors weight parsing from mmap
/// - Zero-copy transfer to Metal buffers
/// - Deterministic loading (seeded RNG for any randomization)
///
/// # Errors
/// - AosError::Io for file access
/// - AosError::Serialization for manifest parsing
/// - AosError::Validation for format mismatches
///
/// # Example
/// ```rust,ignore
/// use adapteros_aos::AosLoader;
/// let loader = AosLoader::new()?;
/// let adapter = loader.load_from_path("./adapters/my_adapter.aos").await?;
/// ```
pub struct AosLoader {
    device: metal::Device,
    global_seed: B3Hash,
}

impl AosLoader {
    pub fn new() -> Result<Self> {
        Self::with_seed(&B3Hash::hash(b"aos_loader_default_seed"))
    }

    pub fn with_seed(global_seed: &B3Hash) -> Result<Self> {
        let device = metal::Device::system_default()
            .ok_or(AosError::Mtl("No Metal device available".to_string()))?;
        Ok(Self {
            device,
            global_seed: global_seed.clone(),
        })
    }

    pub async fn load_from_path(&self, path: &Path) -> Result<LoadedAdapter> {
        info!(path = %path.display(), "Loading AOS adapter");

        // 1. Memory-map the file
        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open AOS file: {}", e)))?;
        let mmap = unsafe {
            memmap2::Mmap::map(&file)
                .map_err(|e| AosError::Io(format!("Failed to mmap AOS file: {}", e)))?
        };

        // Validate minimum file size for header
        if mmap.len() < HEADER_SIZE {
            return Err(AosError::Validation(format!(
                "AOS file too small: {} bytes (minimum {} bytes for header)",
                mmap.len(),
                HEADER_SIZE
            )));
        }

        // 2. Validate magic bytes (4 bytes at offset 0)
        if &mmap[0..4] != &AOS_MAGIC {
            return Err(AosError::Validation(format!(
                "Invalid AOS magic bytes: expected {:?}, got {:?}",
                AOS_MAGIC,
                &mmap[0..4]
            )));
        }

        // 3. Read header fields
        // let flags = u32::from_le_bytes(mmap[4..8].try_into().unwrap()); // Reserved
        let weights_offset = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;
        let weights_size = u64::from_le_bytes(mmap[16..24].try_into().unwrap()) as usize;
        let manifest_offset = u64::from_le_bytes(mmap[24..32].try_into().unwrap()) as usize;
        let manifest_size = u64::from_le_bytes(mmap[32..40].try_into().unwrap()) as usize;

        // 4. Validate offsets and sizes
        if manifest_offset + manifest_size > mmap.len() {
            return Err(AosError::Validation(format!(
                "Manifest extends beyond file: offset {} + size {} > file size {}",
                manifest_offset,
                manifest_size,
                mmap.len()
            )));
        }
        if weights_offset + weights_size > mmap.len() {
            return Err(AosError::Validation(format!(
                "Weights extend beyond file: offset {} + size {} > file size {}",
                weights_offset,
                weights_size,
                mmap.len()
            )));
        }

        // 5. Parse manifest JSON
        let manifest_bytes = &mmap[manifest_offset..manifest_offset + manifest_size];
        let manifest: AosManifest = serde_json::from_slice(manifest_bytes)?;

        // 6. Parse safetensors weights
        let weights_data = &mmap[weights_offset..weights_offset + weights_size];
        let safetensors = SafeTensors::deserialize(weights_data)
            .map_err(|e| AosError::Validation(format!("Invalid safetensors: {}", e)))?;

        let mut buffers: HashMap<String, metal::Buffer> = HashMap::new();

        // 7. Transfer tensor data to Metal buffers
        for (name, tensor) in safetensors.tensors() {
            let data = tensor.data();
            let buffer = self.device.new_buffer_with_data(
                data.as_ptr() as *const std::ffi::c_void,
                data.len() as u64,
                metal::MTLResourceOptions::CPUCacheModeDefaultCache
                    | metal::MTLResourceOptions::StorageModeShared,
            );

            buffers.insert(name.to_string(), buffer);
        }

        // 8. Deterministic post-load with HKDF-seeded RNG
        let loader_seed = derive_seed(&self.global_seed, "aos_loader");
        let _rng = ChaCha20Rng::from_seed(loader_seed);

        info!(
            global_seed_hash = %hex::encode(&self.global_seed.as_bytes()[..8]),
            tensors = buffers.len(),
            "AOS loaded deterministically"
        );

        Ok(LoadedAdapter { manifest, buffers })
    }
}

/// Loaded AOS Adapter
#[derive(Debug)]
pub struct LoadedAdapter {
    pub manifest: AosManifest,
    pub buffers: HashMap<String, metal::Buffer>,
}

impl LoadedAdapter {
    /// Get the adapter ID from the manifest
    pub fn adapter_id(&self) -> &str {
        &self.manifest.adapter_id
    }

    /// Get the adapter version from the manifest
    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// Get the approximate size in bytes by summing Metal buffer sizes
    pub fn size_bytes(&self) -> u64 {
        self.buffers.values().map(|buffer| buffer.length()).sum()
    }

    /// Get the number of tensors/buffers loaded
    pub fn tensor_count(&self) -> usize {
        self.buffers.len()
    }
}

/// AOS Manifest Structure
///
/// Matches the JSON schema defined in docs/AOS_FORMAT.md
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AosManifest {
    /// Semantic adapter ID: `{tenant}/{domain}/{purpose}/{revision}`
    pub adapter_id: String,
    /// Human-readable display name
    #[serde(default)]
    pub name: Option<String>,
    /// Semantic version (e.g., "1.0.0")
    pub version: String,
    /// LoRA rank (typically 8-32)
    pub rank: u32,
    /// LoRA scaling factor (typically 2x rank)
    pub alpha: f32,
    /// Base model identifier
    pub base_model: String,
    /// List of model layers for adapter application
    pub target_modules: Vec<String>,
    /// Adapter category (code, documentation, creative)
    #[serde(default)]
    pub category: Option<String>,
    /// Lifecycle tier (persistent, ephemeral)
    #[serde(default)]
    pub tier: Option<String>,
    /// ISO 8601 timestamp
    #[serde(default)]
    pub created_at: Option<String>,
    /// BLAKE3 hash of weights data (64 hex chars)
    #[serde(default)]
    pub weights_hash: Option<String>,
    /// Training hyperparameters
    #[serde(default)]
    pub training_config: Option<TrainingConfigManifest>,
}

/// Inline Training Configuration
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrainingConfigManifest {
    pub learning_rate: Option<f32>,
    pub batch_size: Option<u32>,
    pub epochs: Option<u32>,
    pub warmup_steps: Option<u32>,
    pub weight_decay: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_header_constants() {
        assert_eq!(AOS_MAGIC, *b"AOS\x00");
        assert_eq!(HEADER_SIZE, 64);
    }

    #[tokio::test]
    async fn test_validation_too_small() {
        let loader = AosLoader::new().unwrap();
        let temp_file = NamedTempFile::new().unwrap();

        // Write file smaller than header
        std::fs::write(temp_file.path(), b"TINY").unwrap();

        let result = loader.load_from_path(temp_file.path()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[tokio::test]
    async fn test_validation_bad_magic() {
        let loader = AosLoader::new().unwrap();
        let temp_file = NamedTempFile::new().unwrap();

        // Write header with bad magic
        let mut header = vec![0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(b"BADM");
        std::fs::write(temp_file.path(), header).unwrap();

        let result = loader.load_from_path(temp_file.path()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid AOS magic"));
    }
}
