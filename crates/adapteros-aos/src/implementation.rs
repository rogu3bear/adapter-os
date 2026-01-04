use std::collections::HashMap;

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

pub use crate::writer::{
    compute_scope_hash, BackendTag, AOS_MAGIC, HAS_INDEX_FLAG, HEADER_SIZE, INDEX_ENTRY_SIZE,
};

#[cfg(feature = "mmap")]
use adapteros_core::derive_seed;
#[cfg(feature = "mmap")]
use memmap2::Mmap;
#[cfg(feature = "mmap")]
use rand::SeedableRng;
#[cfg(feature = "mmap")]
use rand_chacha::ChaCha20Rng;
#[cfg(feature = "mmap")]
use safetensors::SafeTensors;
#[cfg(feature = "mmap")]
use std::fs::File;
#[cfg(feature = "mmap")]
use std::path::Path;
#[cfg(feature = "mmap")]
use tracing::info;

/// Reference to a segment inside a memory-mapped .aos file.
pub struct AosSegmentRef<'a> {
    pub backend_tag: BackendTag,
    pub scope_hash: [u8; 16],
    pub segment_id: u32,
    pub payload: &'a [u8],
    pub weights_hash: [u8; 32],
}

/// Parsed view of an .aos file (manifest + indexed segments)
pub struct AosFileView<'a> {
    pub manifest_bytes: &'a [u8],
    pub segments: Vec<AosSegmentRef<'a>>,
}

/// Open an AOS archive from in-memory bytes, validating header, index, and segment hashes.
pub fn open_aos<'a>(bytes: &'a [u8]) -> Result<AosFileView<'a>> {
    if bytes.len() < HEADER_SIZE {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: file too small for AOS header".to_string(),
        ));
    }

    if bytes[0..4] != AOS_MAGIC {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: invalid AOS magic".to_string(),
        ));
    }

    let flags = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if flags & HAS_INDEX_FLAG == 0 {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: missing segment index".to_string(),
        ));
    }

    let index_offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let index_size = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
    let manifest_offset = u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;
    let manifest_size = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;

    if index_offset != HEADER_SIZE {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: invalid AOS header layout".to_string(),
        ));
    }

    if bytes[40..HEADER_SIZE].iter().any(|b| *b != 0) {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: reserved header bytes non-zero".to_string(),
        ));
    }

    let file_len = bytes.len();
    if index_offset < HEADER_SIZE {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index overlaps header".to_string(),
        ));
    }

    let index_end = index_offset.checked_add(index_size).ok_or_else(|| {
        AosError::Validation("Corrupted / needs retrain: index overflow".to_string())
    })?;
    if index_end > file_len {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index beyond file".to_string(),
        ));
    }

    if !index_size.is_multiple_of(INDEX_ENTRY_SIZE) {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: index size not 80-byte aligned".to_string(),
        ));
    }

    let manifest_end = manifest_offset.checked_add(manifest_size).ok_or_else(|| {
        AosError::Validation("Corrupted / needs retrain: manifest overflow".to_string())
    })?;
    if manifest_end > file_len {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: manifest beyond file".to_string(),
        ));
    }
    if manifest_offset < index_end {
        return Err(AosError::Validation(
            "Corrupted / needs retrain: manifest overlaps index/segments".to_string(),
        ));
    }

    let entry_count = index_size / INDEX_ENTRY_SIZE;
    let index_bytes = &bytes[index_offset..index_end];
    let mut segments = Vec::with_capacity(entry_count);

    for i in 0..entry_count {
        let entry_start = i * INDEX_ENTRY_SIZE;
        let entry = &index_bytes[entry_start..entry_start + INDEX_ENTRY_SIZE];
        let segment_id = u32::from_le_bytes(entry[0..4].try_into().unwrap());
        let backend_tag_raw = u16::from_le_bytes(entry[4..6].try_into().unwrap());
        let backend_tag = BackendTag::try_from(backend_tag_raw)?;
        // Edge case: use try_from to handle u64 -> usize conversion safely on 32-bit systems
        let offset_u64 = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        let len_u64 = u64::from_le_bytes(entry[16..24].try_into().unwrap());
        let offset = usize::try_from(offset_u64).map_err(|_| {
            AosError::Validation(format!(
                "Segment offset {} exceeds platform usize limit (32-bit platform overflow)",
                offset_u64
            ))
        })?;
        let len = usize::try_from(len_u64).map_err(|_| {
            AosError::Validation(format!(
                "Segment length {} exceeds platform usize limit (32-bit platform overflow)",
                len_u64
            ))
        })?;
        let mut scope_hash = [0u8; 16];
        scope_hash.copy_from_slice(&entry[24..40]);
        let mut weights_hash = [0u8; 32];
        weights_hash.copy_from_slice(&entry[40..72]);

        let payload_end = offset.checked_add(len).ok_or_else(|| {
            AosError::Validation("Corrupted / needs retrain: segment overflow".to_string())
        })?;
        if offset < index_end {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment overlaps index".to_string(),
            ));
        }
        if payload_end > manifest_offset {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment overlaps manifest".to_string(),
            ));
        }
        if payload_end > file_len {
            return Err(AosError::Validation(
                "Corrupted / needs retrain: segment beyond file".to_string(),
            ));
        }

        let payload = &bytes[offset..payload_end];
        if B3Hash::hash(payload).as_bytes() != &weights_hash {
            return Err(AosError::SegmentHashMismatch { segment_id });
        }

        segments.push(AosSegmentRef {
            backend_tag,
            scope_hash,
            segment_id,
            payload,
            weights_hash,
        });
    }

    let manifest_bytes = &bytes[manifest_offset..manifest_end];

    Ok(AosFileView {
        manifest_bytes,
        segments,
    })
}

/// AOS Format Loader (Metal + mmap)
#[cfg(feature = "mmap")]
pub struct AosLoader {
    device: metal::Device,
    global_seed: B3Hash,
}

#[cfg(feature = "mmap")]
impl AosLoader {
    pub fn new() -> Result<Self> {
        Self::with_seed(&B3Hash::hash(b"aos_loader_default_seed"))
    }

    pub fn with_seed(global_seed: &B3Hash) -> Result<Self> {
        let device = metal::Device::system_default()
            .ok_or(AosError::Mtl("No Metal device available".to_string()))?;
        Ok(Self {
            device,
            global_seed: *global_seed,
        })
    }

    pub async fn load_from_path(&self, path: &Path) -> Result<LoadedAdapter> {
        info!(path = %path.display(), "Loading AOS adapter");

        let file = File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open AOS file: {}", e)))?;
        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| AosError::Io(format!("Failed to mmap AOS file: {}", e)))?
        };

        let file_view = open_aos(&mmap)?;

        let manifest: AosManifest = serde_json::from_slice(file_view.manifest_bytes)?;
        let scope_path = manifest
            .metadata
            .get("scope_path")
            .cloned()
            .ok_or_else(|| {
                AosError::Validation(
                    "Corrupted / needs retrain: missing scope_path metadata".to_string(),
                )
            })?;
        let scope_hash = compute_scope_hash(&scope_path);

        let canonical = file_view
            .segments
            .iter()
            .filter(|seg| seg.backend_tag == BackendTag::Canonical)
            .find(|seg| seg.scope_hash == scope_hash)
            .or_else(|| {
                file_view
                    .segments
                    .iter()
                    .find(|seg| seg.backend_tag == BackendTag::Canonical)
            })
            .ok_or(AosError::MissingCanonicalSegment)?;

        let safetensors = SafeTensors::deserialize(canonical.payload)
            .map_err(|e| AosError::Validation(format!("Invalid safetensors: {}", e)))?;

        let mut buffers: HashMap<String, metal::Buffer> = HashMap::new();
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
#[cfg(feature = "mmap")]
#[derive(Debug)]
pub struct LoadedAdapter {
    pub manifest: AosManifest,
    pub buffers: HashMap<String, metal::Buffer>,
}

#[cfg(feature = "mmap")]
impl LoadedAdapter {
    pub fn adapter_id(&self) -> &str {
        &self.manifest.adapter_id
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn size_bytes(&self) -> u64 {
        self.buffers.values().map(|buffer| buffer.length()).sum()
    }

    pub fn tensor_count(&self) -> usize {
        self.buffers.len()
    }

    /// Check if this adapter is for an MoE (Mixture of Experts) model
    pub fn is_moe_adapter(&self) -> bool {
        self.manifest.moe_config.is_some()
    }

    /// Get the MoE configuration if this adapter is for an MoE model
    pub fn moe_config(&self) -> Option<&MoEConfigManifest> {
        self.manifest.moe_config.as_ref()
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
    /// Optional per-layer BLAKE3 hashes keyed by canonical logical layer path
    /// (e.g., "transformer.layer_12.attn.q_proj.lora_A").
    #[serde(default)]
    pub per_layer_hashes: Option<HashMap<String, String>>,
    /// Training hyperparameters
    #[serde(default)]
    pub training_config: Option<TrainingConfigManifest>,
    /// MoE configuration (for adapters targeting MoE models)
    #[serde(default)]
    pub moe_config: Option<MoEConfigManifest>,
    /// Arbitrary metadata including scope_path/domain/group/operation
    #[serde(default)]
    pub metadata: HashMap<String, String>,
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

/// MoE (Mixture of Experts) Configuration for adapters targeting MoE models
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MoEConfigManifest {
    /// Number of experts in the base model
    pub num_experts: u32,
    /// Number of experts activated per token
    pub num_experts_per_token: u32,
    /// Number of shared experts (if any)
    #[serde(default)]
    pub num_shared_experts: Option<u32>,
    /// MoE intermediate size per expert
    #[serde(default)]
    pub moe_intermediate_size: Option<u32>,
    /// LoRA strategy: "routing_weighted_shared" or "per_expert"
    #[serde(default = "default_moe_lora_strategy")]
    pub lora_strategy: String,
    /// Whether to use expert routing weights for LoRA scaling
    #[serde(default = "default_use_routing_weights")]
    pub use_routing_weights: bool,
}

fn default_moe_lora_strategy() -> String {
    "routing_weighted_shared".to_string()
}

fn default_use_routing_weights() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "mmap")]
    use tempfile::NamedTempFile;

    #[test]
    fn test_header_constants() {
        assert_eq!(AOS_MAGIC, *b"AOS\0");
        assert_eq!(HEADER_SIZE, 64);
        assert_eq!(INDEX_ENTRY_SIZE, 80);
    }

    #[test]
    fn test_moe_config_manifest_serialization() {
        let moe_config = MoEConfigManifest {
            num_experts: 128,
            num_experts_per_token: 8,
            num_shared_experts: Some(0),
            moe_intermediate_size: Some(768),
            lora_strategy: "routing_weighted_shared".to_string(),
            use_routing_weights: true,
        };

        let json = serde_json::to_string(&moe_config).unwrap();
        let parsed: MoEConfigManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.num_experts, 128);
        assert_eq!(parsed.num_experts_per_token, 8);
        assert_eq!(parsed.num_shared_experts, Some(0));
        assert!(parsed.use_routing_weights);
    }

    #[test]
    fn test_aos_manifest_with_moe_config() {
        let manifest_json = r#"{
            "adapter_id": "test/moe/qwen3-30b/v1",
            "version": "1.0.0",
            "rank": 16,
            "alpha": 32.0,
            "base_model": "Qwen/Qwen3-Coder-30B-A3B",
            "target_modules": ["q_proj", "v_proj", "gate_proj", "up_proj", "down_proj"],
            "moe_config": {
                "num_experts": 128,
                "num_experts_per_token": 8,
                "lora_strategy": "routing_weighted_shared"
            }
        }"#;

        let manifest: AosManifest = serde_json::from_str(manifest_json).unwrap();

        assert_eq!(manifest.adapter_id, "test/moe/qwen3-30b/v1");
        assert!(manifest.moe_config.is_some());

        let moe = manifest.moe_config.unwrap();
        assert_eq!(moe.num_experts, 128);
        assert_eq!(moe.num_experts_per_token, 8);
        assert!(moe.use_routing_weights); // default value
    }

    #[test]
    fn test_aos_manifest_without_moe_config() {
        let manifest_json = r#"{
            "adapter_id": "test/dense/llama/v1",
            "version": "1.0.0",
            "rank": 8,
            "alpha": 16.0,
            "base_model": "meta-llama/Llama-3.1-8B",
            "target_modules": ["q_proj", "v_proj"]
        }"#;

        let manifest: AosManifest = serde_json::from_str(manifest_json).unwrap();

        assert_eq!(manifest.adapter_id, "test/dense/llama/v1");
        assert!(manifest.moe_config.is_none());
    }

    #[cfg(feature = "mmap")]
    #[tokio::test]
    async fn test_validation_too_small() {
        let loader = AosLoader::new().unwrap();
        let temp_root = std::path::PathBuf::from("var/tmp");
        std::fs::create_dir_all(&temp_root).unwrap();
        let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

        std::fs::write(temp_file.path(), b"TINY").unwrap();

        let result = loader.load_from_path(temp_file.path()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too small"));
    }

    #[cfg(feature = "mmap")]
    #[tokio::test]
    async fn test_validation_bad_magic() {
        let loader = AosLoader::new().unwrap();
        let temp_root = std::path::PathBuf::from("var/tmp");
        std::fs::create_dir_all(&temp_root).unwrap();
        let temp_file = NamedTempFile::new_in(&temp_root).unwrap();

        let mut header = vec![0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(b"BADM");
        std::fs::write(temp_file.path(), header).unwrap();

        let result = loader.load_from_path(temp_file.path()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid AOS magic"));
    }
}
