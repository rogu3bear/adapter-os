//! Single-file adapter loader
//!
//! Loads .aos files in AOS binary format (64-byte header with segment index).

use super::format::*;
use super::training::TrainingConfig;
use adapteros_core::{AosError, Result};
use std::path::Path;

/// Load options for .aos files
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Skip integrity verification (faster but unsafe) — DEV ONLY
    pub skip_verification: bool,
    /// Skip signature verification even if present — DEV ONLY
    pub skip_signature_check: bool,
}

pub(crate) fn production_mode_enabled() -> bool {
    std::env::var("AOS_SERVER_PRODUCTION_MODE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Single-file adapter loader
pub struct SingleFileAdapterLoader;

impl SingleFileAdapterLoader {
    /// Load adapter from .aos file with default options
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<SingleFileAdapter> {
        Self::load_with_options(path, LoadOptions::default()).await
    }

    /// Load adapter from .aos file with custom options
    pub async fn load_with_options<P: AsRef<Path>>(
        path: P,
        options: LoadOptions,
    ) -> Result<SingleFileAdapter> {
        let path = path.as_ref();

        // Disallow unsafe skips when production_mode is enabled
        let production_mode = production_mode_enabled();
        if production_mode && (options.skip_verification || options.skip_signature_check) {
            return Err(AosError::PolicyViolation(
                "Adapter load skips are disabled when production_mode is enabled".to_string(),
            ));
        }
        if options.skip_verification || options.skip_signature_check {
            tracing::warn!(
                production_mode,
                path = %path.display(),
                skip_verification = options.skip_verification,
                skip_signature_check = options.skip_signature_check,
                "DEV-ONLY adapter load bypass requested"
            );
        }

        // Load AOS format (64-byte header with segment index)
        Self::load_aos_format(path, options).await
    }

    /// Load AOS format adapter (64-byte header with segment index)
    async fn load_aos_format(path: &Path, options: LoadOptions) -> Result<SingleFileAdapter> {
        use crate::{open_aos, BackendTag};

        // Read entire file
        let data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read AOS file: {}", e)))?;

        // Parse AOS indexed format
        let aos_view = open_aos(&data)?;

        // Parse manifest
        let manifest: AdapterManifest = serde_json::from_slice(aos_view.manifest_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Verify integrity hash if present (backward compatible with older .aos files)
        if !options.skip_verification {
            manifest.verify_integrity()?;
        }

        // Find the canonical weights segment
        let weights_segment = aos_view
            .segments
            .iter()
            .find(|s| s.backend_tag == BackendTag::Canonical)
            .ok_or_else(|| AosError::Parse("No canonical segment found in AOS file".to_string()))?;

        // Deserialize weights from safetensors
        let weights = Self::deserialize_aos_weights(weights_segment.payload, &manifest)?;

        // Build training config from manifest
        let rank = manifest.rank as usize;
        let config = TrainingConfig {
            rank,
            alpha: manifest.alpha,
            hidden_dim: 768, // Default, will be inferred from weights if possible
            ..Default::default()
        };

        // Create a default lineage for AOS-loaded adapters
        let lineage = LineageInfo {
            adapter_id: manifest.adapter_id.clone(),
            version: manifest.version.clone(),
            parent_version: None,
            parent_hash: None,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: manifest.created_at.clone(),
        };

        let adapter = SingleFileAdapter {
            manifest: manifest.clone(),
            weights,
            config,
            lineage,
            training_data: vec![],
            signature: None,
        };

        tracing::info!(
            "Loaded AOS format adapter from: {} (adapter_id={}, signed: false)",
            path.display(),
            manifest.adapter_id
        );

        // Note: Integrity verification happens at manifest parse time above
        // The AOS format also has segment hash verification in open_aos()
        let _ = options; // consumed for skip_verification above

        Ok(adapter)
    }

    /// Deserialize weights from AOS format segment (safetensors)
    fn deserialize_aos_weights(data: &[u8], manifest: &AdapterManifest) -> Result<AdapterWeights> {
        use safetensors::SafeTensors;

        let tensors = SafeTensors::deserialize(data)
            .map_err(|e| AosError::Parse(format!("Failed to deserialize safetensors: {}", e)))?;

        // Extract lora_a and lora_b tensors
        let lora_a_data = tensors
            .tensor("lora_a")
            .or_else(|_| tensors.tensor("lora.a"))
            .map_err(|_| AosError::Parse("Missing lora_a tensor".to_string()))?;
        let lora_b_data = tensors
            .tensor("lora_b")
            .or_else(|_| tensors.tensor("lora.b"))
            .map_err(|_| AosError::Parse("Missing lora_b tensor".to_string()))?;

        // Determine dtype and convert to f32
        let lora_a_flat = Self::tensor_to_f32_vec(&lora_a_data)?;
        let lora_b_flat = Self::tensor_to_f32_vec(&lora_b_data)?;

        // Get dimensions from tensor shapes
        let lora_a_shape = lora_a_data.shape();

        // lora_a: [rank, hidden_dim], lora_b: [hidden_dim, rank]
        let (rank, hidden_dim) = if lora_a_shape.len() == 2 {
            (lora_a_shape[0], lora_a_shape[1])
        } else {
            (manifest.rank as usize, 768) // fallback
        };

        // Reshape flat vectors to 2D
        let lora_a_2d: Vec<Vec<f32>> = lora_a_flat
            .chunks(hidden_dim)
            .map(|chunk| chunk.to_vec())
            .collect();

        let lora_b_2d: Vec<Vec<f32>> = lora_b_flat
            .chunks(rank)
            .map(|chunk| chunk.to_vec())
            .collect();

        let created_at = manifest.created_at.clone();

        let positive = WeightGroup {
            lora_a: lora_a_2d.clone(),
            lora_b: lora_b_2d.clone(),
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Positive,
                created_at: created_at.clone(),
            },
        };

        let negative = WeightGroup {
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Negative,
                created_at: created_at.clone(),
            },
        };

        let combined = WeightGroup {
            lora_a: lora_a_2d,
            lora_b: lora_b_2d,
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Combined,
                created_at,
            },
        };

        Ok(AdapterWeights {
            positive,
            negative,
            combined: Some(combined),
        })
    }

    /// Convert safetensors data to f32 vec, handling F16 and F32 dtypes
    fn tensor_to_f32_vec(tensor: &safetensors::tensor::TensorView<'_>) -> Result<Vec<f32>> {
        use safetensors::Dtype;

        match tensor.dtype() {
            Dtype::F16 => Ok(tensor
                .data()
                .chunks(2)
                .map(|chunk| {
                    let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                    half::f16::from_bits(bits).to_f32()
                })
                .collect()),
            Dtype::F32 => Ok(tensor
                .data()
                .chunks(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()),
            Dtype::BF16 => Ok(tensor
                .data()
                .chunks(2)
                .map(|chunk| {
                    let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                    half::bf16::from_bits(bits).to_f32()
                })
                .collect()),
            other => Err(AosError::Parse(format!(
                "Unsupported tensor dtype: {:?}",
                other
            ))),
        }
    }

    /// Load only the manifest without extracting full weights (fast)
    pub async fn load_manifest_only<P: AsRef<Path>>(path: P) -> Result<AdapterManifest> {
        use crate::open_aos;

        let path = path.as_ref();

        // Read entire file
        let data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read AOS file: {}", e)))?;

        // Parse AOS indexed format
        let aos_view = open_aos(&data)?;

        // Parse manifest
        let manifest: AdapterManifest = serde_json::from_slice(aos_view.manifest_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Verify format version
        verify_format_version(manifest.format_version)?;

        tracing::debug!("Loaded manifest for adapter: {}", manifest.adapter_id);
        Ok(manifest)
    }

    /// Extract a specific component from .aos file without loading everything
    pub async fn extract_component<P: AsRef<Path>>(path: P, component: &str) -> Result<Vec<u8>> {
        use crate::{open_aos, BackendTag};

        let path = path.as_ref();

        // Read entire file
        let data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read AOS file: {}", e)))?;

        // Parse AOS indexed format
        let aos_view = open_aos(&data)?;

        match component {
            "manifest" => {
                tracing::debug!(
                    "Extracted component 'manifest' ({} bytes)",
                    aos_view.manifest_bytes.len()
                );
                Ok(aos_view.manifest_bytes.to_vec())
            }
            "weights" | "weights_combined" => {
                // Find canonical segment
                let segment = aos_view
                    .segments
                    .iter()
                    .find(|s| s.backend_tag == BackendTag::Canonical)
                    .ok_or_else(|| {
                        AosError::Training("Missing canonical weights in AOS file".to_string())
                    })?;
                tracing::debug!(
                    "Extracted component '{}' ({} bytes)",
                    component,
                    segment.payload.len()
                );
                Ok(segment.payload.to_vec())
            }
            _ => Err(AosError::Training(format!(
                "Component '{}' not available in AOS format. Available: manifest, weights",
                component
            ))),
        }
    }
}
