//! Single-file adapter loader

use super::format::*;
use super::training::{TrainingConfig, TrainingExample};
use adapteros_core::{AosError, Result};
use serde::Deserialize;
use std::convert::TryInto;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Load options for .aos files
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Skip integrity verification (faster but unsafe)
    pub skip_verification: bool,
    /// Skip signature verification even if present
    pub skip_signature_check: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            skip_verification: false,
            skip_signature_check: false,
        }
    }
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

        // Use synchronous I/O for ZIP operations
        let file = std::fs::File::open(path)
            .map_err(|e| AosError::Io(format!("Failed to open .aos file: {}", e)))?;

        let mut zip = ZipArchive::new(file)
            .map_err(|e| AosError::Io(format!("Failed to open ZIP archive: {}", e)))?;

        // Load manifest
        let mut manifest_file = zip
            .by_name("manifest.json")
            .map_err(|_| AosError::Training("Missing manifest.json in .aos file"))?;
        let mut manifest_data = Vec::new();
        manifest_file
            .read_to_end(&mut manifest_data)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
        let manifest: AdapterManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Load weight groups metadata if available
        let weight_groups_info =
            match read_zip_json::<_, WeightGroupsInfo>(&mut zip, "weight_groups.json") {
                Ok(info) => Some(info),
                Err(AosError::Training(_)) => None,
                Err(e) => return Err(e),
            };

        // Load weights (new separated format or legacy fallback)
        let (mut weights, weight_config) = if let Some(info) = weight_groups_info {
            let positive_bytes = read_zip_bytes(&mut zip, "weights_positive.safetensors")
                .map_err(|_| {
                    AosError::Training(
                        "Missing weights_positive.safetensors in .aos file".to_string(),
                    )
                })?;
            let negative_bytes = read_zip_bytes(&mut zip, "weights_negative.safetensors")
                .map_err(|_| {
                    AosError::Training(
                        "Missing weights_negative.safetensors in .aos file".to_string(),
                    )
                })?;
            let combined_bytes = read_zip_bytes(&mut zip, "weights_combined.safetensors").ok();

            let positive = deserialize_weight_group(
                &positive_bytes,
                &info.positive,
                WeightGroupType::Positive,
            )?;
            let negative = deserialize_weight_group(
                &negative_bytes,
                &info.negative,
                WeightGroupType::Negative,
            )?;
            let combined = match (combined_bytes, info.combined.as_ref()) {
                (Some(bytes), Some(meta)) => Some(deserialize_weight_group(
                    &bytes,
                    meta,
                    WeightGroupType::Combined,
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
                    use_separate_weights: info.use_separate_weights.unwrap_or(true),
                    combination_strategy: info
                        .combination_strategy
                        .clone()
                        .unwrap_or(CombinationStrategy::WeightedDifference),
                    positive_scale: info.positive_scale.unwrap_or(1.0),
                    negative_scale: info.negative_scale.unwrap_or(1.0),
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
        let mut training_file = zip
            .by_name("training_data.jsonl")
            .map_err(|_| AosError::Training("Missing training_data.jsonl in .aos file"))?;
        let mut training_data_str = String::new();
        training_file
            .read_to_string(&mut training_data_str)
            .map_err(|e| AosError::Io(format!("Failed to read training data: {}", e)))?;
        let training_data: Vec<TrainingExample> = training_data_str
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AosError::Parse(format!("Failed to parse training data: {}", e)))?;

        // Load config
        let mut config_file = zip
            .by_name("config.toml")
            .map_err(|_| AosError::Training("Missing config.toml in .aos file"))?;
        let mut config_str = String::new();
        config_file
            .read_to_string(&mut config_str)
            .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;
        let config: TrainingConfig = toml::from_str(&config_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

        // Load lineage
        let mut lineage_file = zip
            .by_name("lineage.json")
            .map_err(|_| AosError::Training("Missing lineage.json in .aos file"))?;
        let mut lineage_data = Vec::new();
        lineage_file
            .read_to_end(&mut lineage_data)
            .map_err(|e| AosError::Io(format!("Failed to read lineage: {}", e)))?;
        let lineage: LineageInfo = serde_json::from_slice(&lineage_data)
            .map_err(|e| AosError::Parse(format!("Failed to parse lineage: {}", e)))?;

        // Load signature if present
        let signature = if zip.by_name("signature.sig").is_ok() {
            let mut sig_file = zip.by_name("signature.sig")?;
            let mut sig_data = Vec::new();
            sig_file
                .read_to_end(&mut sig_data)
                .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;
            Some(
                serde_json::from_slice(&sig_data)
                    .map_err(|e| AosError::Parse(format!("Failed to parse signature: {}", e)))?,
            )
        } else {
            None
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
                return Err(AosError::Training("Adapter integrity verification failed"));
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
            .map_err(|_| AosError::Training("Missing manifest.json in .aos file"))?;
        let mut manifest_data = Vec::new();
        manifest_file
            .read_to_end(&mut manifest_data)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
        let manifest: AdapterManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| AosError::Parse(format!("Failed to parse manifest: {}", e)))?;

        // Verify format version
        if !verify_format_version(manifest.format_version)? {
            return Err(AosError::Training(format!(
                "Unsupported format version: {}",
                manifest.format_version
            )));
        }

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

        // Map component names to file names
        let file_name = match component {
            "manifest" => "manifest.json",
            "weights" => "weights.safetensors",
            "weights_positive" => "weights_positive.safetensors",
            "weights_negative" => "weights_negative.safetensors",
            "weights_combined" => "weights_combined.safetensors",
            "weight_groups" => "weight_groups.json",
            "training_data" => "training_data.jsonl",
            "config" => "config.toml",
            "lineage" => "lineage.json",
            "signature" => "signature.sig",
            _ => {
                return Err(AosError::Training(format!(
                    "Unknown component: {}",
                    component
                )))
            }
        };

        let mut file = zip
            .by_name(file_name)
            .map_err(|_| AosError::Training(format!("Missing {} in .aos file", file_name)))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| AosError::Io(format!("Failed to read {}: {}", file_name, e)))?;

        tracing::debug!("Extracted component '{}' ({} bytes)", component, data.len());
        Ok(data)
    }
}
