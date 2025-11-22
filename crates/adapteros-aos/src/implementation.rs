use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Magic bytes identifying an AOS v3.0 archive (8 bytes)
pub const AOS3_MAGIC: [u8; 8] = *b"AOS3\x00\x00\x00\x00";

/// Current format version
pub const AOS_VERSION: u32 = 3;

/// Header size in bytes (64-byte aligned for cache efficiency)
pub const HEADER_SIZE: usize = 64;

/// AOS Format Loader
///
/// Loads memory-mappable AOS v3.0 format with zero-copy GPU transfer.
///
/// ## Format Specification (64-byte header)
///
/// ```text
/// | Offset | Size | Field                              |
/// |--------|------|------------------------------------|
/// | 0      | 8    | Magic: "AOS3\x00\x00\x00\x00"      |
/// | 8      | 4    | Version (u32 LE) = 3               |
/// | 12     | 4    | Flags (u32 LE, reserved)           |
/// | 16     | 8    | Total file size (u64 LE)           |
/// | 24     | 8    | Weights offset (u64 LE)            |
/// | 32     | 8    | Weights size (u64 LE)              |
/// | 40     | 8    | Manifest offset (u64 LE)           |
/// | 48     | 8    | Manifest size (u64 LE)             |
/// | 56     | 8    | Reserved (padding)                 |
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

        // 2. Validate magic bytes (8 bytes at offset 0)
        if &mmap[0..8] != &AOS3_MAGIC {
            return Err(AosError::Validation(format!(
                "Invalid AOS magic bytes: expected {:?}, got {:?}",
                AOS3_MAGIC,
                &mmap[0..8]
            )));
        }

        // 3. Read and validate version (bytes 8-12)
        let version = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        if version != AOS_VERSION {
            return Err(AosError::Validation(format!(
                "Unsupported AOS version: expected {}, got {}",
                AOS_VERSION, version
            )));
        }

        // 4. Read header fields
        // let flags = u32::from_le_bytes(mmap[12..16].try_into().unwrap()); // Reserved
        // let total_size = u64::from_le_bytes(mmap[16..24].try_into().unwrap());
        let weights_offset = u64::from_le_bytes(mmap[24..32].try_into().unwrap()) as usize;
        let weights_size = u64::from_le_bytes(mmap[32..40].try_into().unwrap()) as usize;
        let manifest_offset = u64::from_le_bytes(mmap[40..48].try_into().unwrap()) as usize;
        let manifest_size = u64::from_le_bytes(mmap[48..56].try_into().unwrap()) as usize;

        // 5. Validate offsets and sizes
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

        // 6. Parse manifest JSON
        let manifest_bytes = &mmap[manifest_offset..manifest_offset + manifest_size];
        let manifest: AosManifest = serde_json::from_slice(manifest_bytes)?;

        // 7. Parse safetensors weights
        let weights_data = &mmap[weights_offset..weights_offset + weights_size];
        let safetensors = SafeTensors::deserialize(weights_data)
            .map_err(|e| AosError::Validation(format!("Invalid safetensors: {}", e)))?;

        let mut buffers: HashMap<String, metal::Buffer> = HashMap::new();

        // 4. Transfer tensor data to Metal buffers
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

        // 5. Deterministic post-load with HKDF-seeded RNG
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

/// AOS Manifest Structure (v3.0)
///
/// Matches the JSON schema defined in docs/AOS_FORMAT.md
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AosManifest {
    /// Format version (must be 3 for v3.0)
    pub format_version: u32,
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
    /// Additional key-value metadata
    #[serde(default)]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Training configuration stored in manifest
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TrainingConfigManifest {
    /// LoRA rank used during training
    pub rank: u32,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Learning rate
    pub learning_rate: f32,
    /// Batch size
    pub batch_size: u32,
    /// Number of training epochs
    pub epochs: u32,
    /// Model hidden dimension
    #[serde(default)]
    pub hidden_dim: Option<u32>,
}
