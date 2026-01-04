//! Single-file adapter loader

use super::format::*;
use super::training::{TrainingConfig, TrainingExample};
use crate::aos2_format::AosAdapter;
use crate::format_detector::{detect_format, FormatVersion};
use crate::weights::{WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, Result};
use serde::Deserialize;
use std::convert::TryInto;
use std::io::{Read, Seek};
use std::path::Path;
use zip::{result::ZipError, ZipArchive};

/// Load options for .aos files
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Skip integrity verification (faster but unsafe) — DEV ONLY
    pub skip_verification: bool,
    /// Skip signature verification even if present — DEV ONLY
    pub skip_signature_check: bool,
    /// Use memory-mapped loading path (zero-copy weights, lazy decompression)
    pub use_mmap: bool,
}

pub(crate) fn production_mode_enabled() -> bool {
    std::env::var("AOS_SERVER_PRODUCTION_MODE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn read_weight_manifest<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
) -> Result<Option<WeightGroupsManifest>> {
    match zip.by_name("weight_groups.json") {
        Ok(mut file) => {
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .map_err(|e| AosError::Io(format!("Failed to read weight_groups.json: {}", e)))?;
            if data.is_empty() {
                return Ok(None);
            }
            match serde_json::from_slice::<WeightGroupsManifest>(&data) {
                Ok(manifest) => Ok(Some(manifest)),
                Err(_) => {
                    let legacy: LegacyWeightGroupsInfo =
                        serde_json::from_slice(&data).map_err(|e| {
                            AosError::Parse(format!("Failed to parse weight_groups.json: {}", e))
                        })?;
                    Ok(Some(legacy.into_manifest()))
                }
            }
        }
        Err(ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(AosError::Io(format!(
            "Failed to open weight_groups.json: {}",
            err
        ))),
    }
}

fn disk_metadata_to_runtime(
    info: &WeightGroupDiskInfo,
    group_type: WeightGroupType,
) -> WeightMetadata {
    WeightMetadata {
        example_count: info.example_count,
        avg_loss: info.avg_loss,
        training_time_ms: info.training_time_ms,
        group_type,
        created_at: info.created_at.clone(),
    }
}

#[derive(Debug, Deserialize)]
struct LegacyWeightGroupsInfo {
    positive: LegacyWeightGroupInfo,
    negative: LegacyWeightGroupInfo,
    #[serde(default)]
    combined: Option<LegacyWeightGroupInfo>,
    #[serde(default)]
    combination_strategy: Option<LegacyCombinationStrategy>,
    #[serde(default)]
    use_separate_weights: Option<bool>,
    #[serde(default)]
    positive_scale: Option<f32>,
    #[serde(default)]
    negative_scale: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct LegacyWeightGroupInfo {
    example_count: usize,
    avg_loss: f32,
    training_time_ms: u64,
    created_at: String,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum LegacyCombinationStrategy {
    Difference,
    WeightedDifference,
    Separate,
}

impl LegacyWeightGroupsInfo {
    fn into_manifest(self) -> WeightGroupsManifest {
        let strategy = match self
            .combination_strategy
            .unwrap_or(LegacyCombinationStrategy::WeightedDifference)
        {
            LegacyCombinationStrategy::Difference => CombinationStrategy::Difference,
            LegacyCombinationStrategy::WeightedDifference => {
                CombinationStrategy::WeightedDifference {
                    positive_scale: self.positive_scale.unwrap_or(1.0),
                    negative_scale: self.negative_scale.unwrap_or(1.0),
                }
            }
            LegacyCombinationStrategy::Separate => CombinationStrategy::Separate,
        };

        WeightGroupsManifest {
            positive: self.positive.into(),
            negative: self.negative.into(),
            combined: self.combined.map(Into::into),
            combination_strategy: strategy,
            use_separate_weights: self.use_separate_weights.unwrap_or(true),
        }
    }
}

impl From<LegacyWeightGroupInfo> for WeightGroupDiskInfo {
    fn from(info: LegacyWeightGroupInfo) -> Self {
        Self {
            example_count: info.example_count,
            avg_loss: info.avg_loss,
            training_time_ms: info.training_time_ms,
            created_at: info.created_at,
        }
    }
}

fn read_zip_bytes<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let mut file = zip
        .by_name(name)
        .map_err(|_| AosError::Training(format!("Missing {} in .aos file", name)))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| AosError::Io(format!("Failed to read {}: {}", name, e)))?;
    Ok(data)
}

fn deserialize_legacy_weights(
    bytes: &[u8],
    config: &TrainingConfig,
) -> Result<(AdapterWeights, WeightGroupConfig)> {
    let rank = config.rank;
    let hidden_dim = config.hidden_dim;
    let expected_floats = rank * hidden_dim * 2;
    let expected_bytes = expected_floats * std::mem::size_of::<f32>();

    if bytes.len() < expected_bytes {
        return Err(AosError::Training(format!(
            "Legacy weights payload too small: expected at least {} bytes, found {}",
            expected_bytes,
            bytes.len()
        )));
    }

    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();

    if floats.len() < expected_floats {
        return Err(AosError::Training(
            "Legacy weights payload truncated".to_string(),
        ));
    }

    let (a_slice, b_slice) = floats.split_at(rank * hidden_dim);

    let mut lora_a = Vec::with_capacity(rank);
    for r in 0..rank {
        let start = r * hidden_dim;
        let end = start + hidden_dim;
        lora_a.push(a_slice[start..end].to_vec());
    }

    let mut lora_b = Vec::with_capacity(hidden_dim);
    for h in 0..hidden_dim {
        let start = h * rank;
        let end = start + rank;
        lora_b.push(b_slice[start..end].to_vec());
    }

    let created_at = chrono::Utc::now().to_rfc3339();

    let positive = WeightGroup {
        lora_a: lora_a.clone(),
        lora_b: lora_b.clone(),
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
        lora_a,
        lora_b,
        metadata: WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Combined,
            created_at,
        },
    };

    let config = WeightGroupConfig {
        use_separate_weights: false,
        combination_strategy: CombinationStrategy::Difference,
    };

    Ok((
        AdapterWeights {
            positive,
            negative,
            combined: Some(combined),
        },
        config,
    ))
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

        // Detect format first
        let format = detect_format(path)?;

        match format {
            FormatVersion::AosV2 => {
                // Load AOS 2.0 format
                Self::load_aos2_format(path, options).await
            }
            FormatVersion::ZipV1 => {
                // Load ZIP format (existing logic)
                if options.use_mmap {
                    // Use mmap loader for ZIP
                    let mmap_adapter =
                        crate::mmap_loader::MmapAdapterLoader::global().load(path, &options)?;
                    let mut adapter = mmap_adapter.to_standard_adapter()?;

                    // Apply verification semantics
                    if !options.skip_verification {
                        let sig_backup = if options.skip_signature_check {
                            adapter.signature.take()
                        } else {
                            None
                        };
                        if !adapter.verify()? {
                            return Err(AosError::Training(
                                "Adapter integrity verification failed".to_string(),
                            ));
                        }
                        if options.skip_signature_check {
                            adapter.signature = sig_backup;
                        }
                    }

                    tracing::info!(
                        "Loaded (mmap ZIP) .aos adapter from: {} (format v{}, signed: {})",
                        path.display(),
                        adapter.manifest.format_version,
                        adapter.is_signed()
                    );
                    return Ok(adapter);
                }

                // Standard ZIP loader
                Self::load_zip_format(path, options).await
            }
        }
    }

    /// Load AOS 2.0 format adapter
    async fn load_aos2_format(path: &Path, options: LoadOptions) -> Result<SingleFileAdapter> {
        let aos_adapter = AosAdapter::load(path)?;
        let adapter_arc = aos_adapter.to_single_file_adapter()?;
        let mut adapter = (*adapter_arc).clone();

        // Apply verification semantics
        if !options.skip_verification {
            let sig_backup = if options.skip_signature_check {
                adapter.signature.take()
            } else {
                None
            };
            if !adapter.verify()? {
                return Err(AosError::Training(
                    "Adapter integrity verification failed".to_string(),
                ));
            }
            if options.skip_signature_check {
                adapter.signature = sig_backup;
            }
        }

        tracing::info!(
            "Loaded AOS 2.0 adapter from: {} (format v{}, signed: {})",
            path.display(),
            adapter.manifest.format_version,
            adapter.is_signed()
        );

        Ok(adapter)
    }

    /// Load ZIP format adapter (internal helper)
    async fn load_zip_format(path: &Path, options: LoadOptions) -> Result<SingleFileAdapter> {
        // Use synchronous I/O for ZIP operations
        let file = std::fs::File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        let mut zip = ZipArchive::new(file)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;

        // Load manifest
        let manifest_bytes = read_zip_bytes(&mut zip, "manifest.json")?;
        let mut manifest: AdapterManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Load config first (needed for legacy weights)
        let config_bytes = read_zip_bytes(&mut zip, "config.toml")?;
        let config_str = String::from_utf8(config_bytes)
            .map_err(|e| AosError::Parse(format!("Invalid UTF-8 in config.toml: {}", e)))?;
        let config: TrainingConfig = toml::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

        // Load weight groups metadata if available
        let weight_groups_manifest = read_weight_manifest(&mut zip)?;

        // Load weights (new separated format or legacy fallback)
        let (weights, weight_config) = if let Some(info) = weight_groups_manifest {
            let positive_bytes =
                read_zip_bytes(&mut zip, "weights_positive.safetensors").map_err(|_| {
                    AosError::Training(
                        "Missing weights_positive.safetensors in .aos file".to_string(),
                    )
                })?;
            let negative_bytes =
                read_zip_bytes(&mut zip, "weights_negative.safetensors").map_err(|_| {
                    AosError::Training(
                        "Missing weights_negative.safetensors in .aos file".to_string(),
                    )
                })?;
            let combined_bytes = read_zip_bytes(&mut zip, "weights_combined.safetensors").ok();

            let positive = crate::weights::deserialize_weight_group(
                &positive_bytes,
                disk_metadata_to_runtime(&info.positive, WeightGroupType::Positive),
            )?;
            let negative = crate::weights::deserialize_weight_group(
                &negative_bytes,
                disk_metadata_to_runtime(&info.negative, WeightGroupType::Negative),
            )?;
            let combined = match (combined_bytes, info.combined.as_ref()) {
                (Some(bytes), Some(meta)) => Some(crate::weights::deserialize_weight_group(
                    &bytes,
                    disk_metadata_to_runtime(meta, WeightGroupType::Combined),
                )?),
                _ => None,
            };

            (
                AdapterWeights {
                    positive,
                    negative,
                    combined,
                },
                WeightGroupConfig {
                    use_separate_weights: info.use_separate_weights,
                    combination_strategy: info.combination_strategy.clone(),
                },
            )
        } else {
            let legacy_bytes = read_zip_bytes(&mut zip, "weights.safetensors").map_err(|_| {
                AosError::Training(
                    "Missing weights file (expected weights.safetensors or separated weights)"
                        .to_string(),
                )
            })?;
            deserialize_legacy_weights(&legacy_bytes, &config)?
        };

        manifest.weight_groups = weight_config;

        // Load training data
        let training_bytes = read_zip_bytes(&mut zip, "training_data.jsonl")?;
        let training_data_str = String::from_utf8(training_bytes)
            .map_err(|e| AosError::Parse(format!("Invalid UTF-8 in training data: {}", e)))?;
        let mut training_data = Vec::new();
        for (idx, line) in training_data_str.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let example: TrainingExample = serde_json::from_str(trimmed).map_err(|e| {
                AosError::Parse(format!("Failed to parse training data line {}: {}", idx, e))
            })?;
            training_data.push(example);
        }

        // Config already loaded above

        // Load lineage
        let lineage_bytes = read_zip_bytes(&mut zip, "lineage.json")?;
        let lineage: LineageInfo = serde_json::from_slice(&lineage_bytes)
            .map_err(|e| AosError::Parse(format!("Failed to parse lineage: {}", e)))?;

        // Load signature if present
        let signature =
            match zip.by_name("signature.sig") {
                Ok(mut sig_file) => {
                    let mut sig_data = Vec::new();
                    sig_file
                        .read_to_end(&mut sig_data)
                        .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;
                    Some(serde_json::from_slice(&sig_data).map_err(|e| {
                        AosError::Parse(format!("Failed to parse signature: {}", e))
                    })?)
                }
                Err(ZipError::FileNotFound) => None,
                Err(err) => return Err(AosError::Io(format!("Failed to read signature: {}", err))),
            };

        let mut adapter = SingleFileAdapter {
            manifest,
            weights,
            training_data,
            config,
            lineage,
            signature,
        };

        // Verify integrity unless skipped
        if !options.skip_verification {
            // Temporarily remove signature if we're skipping signature checks
            let sig_backup = if options.skip_signature_check {
                adapter.signature.take()
            } else {
                None
            };

            if !adapter.verify()? {
                return Err(AosError::Training(
                    "Adapter integrity verification failed".to_string(),
                ));
            }

            // Restore signature if we removed it
            if options.skip_signature_check {
                adapter.signature = sig_backup;
            }
        }

        tracing::info!(
            "Loaded .aos adapter from: {} (format v{}, signed: {})",
            path.display(),
            adapter.manifest.format_version,
            adapter.is_signed()
        );
        Ok(adapter)
    }

    /// Load only the manifest without extracting full weights (fast)
    pub async fn load_manifest_only<P: AsRef<Path>>(path: P) -> Result<AdapterManifest> {
        let path = path.as_ref();

        let file = std::fs::File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        let mut zip = ZipArchive::new(file)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;

        // Load only manifest
        let mut manifest_file = zip
            .by_name("manifest.json")
            .map_err(|_| AosError::Training("Missing manifest.json in .aos file".to_string()))?;
        let mut manifest_data = Vec::new();
        manifest_file
            .read_to_end(&mut manifest_data)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
        let manifest: AdapterManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Verify format version
        verify_format_version(manifest.format_version)?;

        tracing::debug!("Loaded manifest for adapter: {}", manifest.adapter_id);
        Ok(manifest)
    }

    /// Extract a specific component from .aos file without loading everything
    pub async fn extract_component<P: AsRef<Path>>(path: P, component: &str) -> Result<Vec<u8>> {
        let path = path.as_ref();

        let file = std::fs::File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        let mut zip = ZipArchive::new(file)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;

        // Map component names to candidate file names
        let candidates: &[&str] = match component {
            "manifest" => &["manifest.json"],
            "weights" => &[
                "weights.safetensors",
                "weights_combined.safetensors",
                "weights_positive.safetensors",
            ],
            "weights_positive" => &["weights_positive.safetensors"],
            "weights_negative" => &["weights_negative.safetensors"],
            "weights_combined" => &["weights_combined.safetensors"],
            "weight_groups" => &["weight_groups.json"],
            "training_data" => &["training_data.jsonl"],
            "config" => &["config.toml"],
            "lineage" => &["lineage.json"],
            "signature" => &["signature.sig"],
            _ => {
                return Err(AosError::Training(format!(
                    "Unknown component: {}",
                    component
                )))
            }
        };

        for candidate in candidates.iter() {
            if let Ok(mut file) = zip.by_name(candidate) {
                let mut data = Vec::new();
                file.read_to_end(&mut data)
                    .map_err(|e| AosError::Io(format!("Failed to read {}: {}", candidate, e)))?;
                tracing::debug!("Extracted component '{}' ({} bytes)", component, data.len());
                return Ok(data);
            }
        }

        Err(AosError::Training(format!(
            "Missing {} in .aos file",
            candidates.first().unwrap()
        )))
    }
}
