//! Single-file adapter packager

use super::format::*;
use crate::weights::{serialize_weight_group, WeightGroupDiskInfo, WeightGroupsManifest};
use adapteros_core::{AosError, Result};
use std::io::Write;
use std::path::Path;
use zip::{write::FileOptions, ZipWriter};

/// Options for packaging .aos files
#[derive(Debug, Clone)]
pub struct PackageOptions {
    pub compression: CompressionLevel,
    pub include_signature: bool,
    pub include_combined_weights: bool,
}

impl Default for PackageOptions {
    fn default() -> Self {
        Self {
            compression: CompressionLevel::Fast,
            include_signature: true,
            include_combined_weights: true,
        }
    }
}

impl SingleFileAdapter {
    /// Save adapter to .aos file
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let options = PackageOptions::default();
        Self::save_with_options(self, path, options).await
    }

    /// Save adapter to .aos file with custom options
    pub async fn save_with_options<P: AsRef<Path>>(
        adapter: &SingleFileAdapter,
        path: P,
        options: PackageOptions,
    ) -> Result<()> {
        let path = path.as_ref();

        // Create file synchronously for ZipWriter
        let file = std::fs::File::create(path)
            .map_err(|e| AosError::Io(format!("Failed to create .aos file: {}", e)))?;

        let mut zip = ZipWriter::new(file);

        // Configure compression
        let (compression_method, compression_level) = options.compression.to_zip_options();
        let mut file_options = FileOptions::default()
            .compression_method(compression_method)
            .unix_permissions(0o644);

        if let Some(level) = compression_level {
            file_options = file_options.compression_level(Some(level as i32));
        }

        // Add manifest (always use best compression for small JSON)
        let manifest_options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(9))
            .unix_permissions(0o644);
        zip.start_file("manifest.json", manifest_options)
            .map_err(|e| AosError::Io(format!("Failed to start manifest file: {}", e)))?;
        zip.write_all(
            &serde_json::to_vec_pretty(&adapter.manifest)
                .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?,
        )
        .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;

        // Add positive weights
        zip.start_file("weights_positive.safetensors", file_options)
            .map_err(|e| AosError::Io(format!("Failed to start positive weights file: {}", e)))?;
        let positive_weights_bytes = serialize_weight_group(&adapter.weights.positive)?;
        zip.write_all(&positive_weights_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write positive weights: {}", e)))?;

        // Add negative weights
        zip.start_file("weights_negative.safetensors", file_options)
            .map_err(|e| AosError::Io(format!("Failed to start negative weights file: {}", e)))?;
        let negative_weights_bytes = serialize_weight_group(&adapter.weights.negative)?;
        zip.write_all(&negative_weights_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write negative weights: {}", e)))?;

        // Add combined weights if requested and available
        if options.include_combined_weights {
            if let Some(ref combined) = adapter.weights.combined {
                zip.start_file("weights_combined.safetensors", file_options)
                    .map_err(|e| {
                        AosError::Io(format!("Failed to start combined weights file: {}", e))
                    })?;
                let combined_weights_bytes = serialize_weight_group(combined)?;
                zip.write_all(&combined_weights_bytes).map_err(|e| {
                    AosError::Io(format!("Failed to write combined weights: {}", e))
                })?;
            }
        }

        // Add training data (use best compression for text)
        let data_options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(9))
            .unix_permissions(0o644);
        zip.start_file("training_data.jsonl", data_options)
            .map_err(|e| AosError::Io(format!("Failed to start training data file: {}", e)))?;
        for example in &adapter.training_data {
            zip.write_all(&serde_json::to_vec(example).map_err(|e| {
                AosError::Training(format!("Failed to serialize training example: {}", e))
            })?)
            .map_err(|e| AosError::Io(format!("Failed to write training example: {}", e)))?;
            zip.write_all(b"\n")
                .map_err(|e| AosError::Io(format!("Failed to write newline: {}", e)))?;
        }

        // Add config
        zip.start_file("config.toml", data_options)
            .map_err(|e| AosError::Io(format!("Failed to start config file: {}", e)))?;
        zip.write_all(
            toml::to_string(&adapter.config)
                .map_err(|e| AosError::Training(format!("Failed to serialize config: {}", e)))?
                .as_bytes(),
        )
        .map_err(|e| AosError::Io(format!("Failed to write config: {}", e)))?;

        // Add lineage
        zip.start_file("lineage.json", data_options)
            .map_err(|e| AosError::Io(format!("Failed to start lineage file: {}", e)))?;
        zip.write_all(
            &serde_json::to_vec_pretty(&adapter.lineage)
                .map_err(|e| AosError::Training(format!("Failed to serialize lineage: {}", e)))?,
        )
        .map_err(|e| AosError::Io(format!("Failed to write lineage: {}", e)))?;

        // Add signature if present and requested
        if options.include_signature {
            if let Some(ref signature) = adapter.signature {
                zip.start_file("signature.sig", data_options)
                    .map_err(|e| AosError::Io(format!("Failed to start signature file: {}", e)))?;
                zip.write_all(&serde_json::to_vec_pretty(signature).map_err(|e| {
                    AosError::Training(format!("Failed to serialize signature: {}", e))
                })?)
                .map_err(|e| AosError::Io(format!("Failed to write signature: {}", e)))?;
            }
        }

        // Add weight group metadata
        zip.start_file("weight_groups.json", data_options)
            .map_err(|e| AosError::Io(format!("Failed to start weight groups file: {}", e)))?;
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
            combined: adapter
                .weights
                .combined
                .as_ref()
                .map(|c| WeightGroupDiskInfo {
                    example_count: c.metadata.example_count,
                    avg_loss: c.metadata.avg_loss,
                    training_time_ms: c.metadata.training_time_ms,
                    created_at: c.metadata.created_at.clone(),
                }),
            combination_strategy: adapter.manifest.weight_groups.combination_strategy.clone(),
            use_separate_weights: adapter.manifest.weight_groups.use_separate_weights,
        };
        zip.write_all(&serde_json::to_vec_pretty(&weight_manifest).map_err(|e| {
            AosError::Training(format!("Failed to serialize weight groups info: {}", e))
        })?)
        .map_err(|e| AosError::Io(format!("Failed to write weight groups info: {}", e)))?;

        zip.finish()
            .map_err(|e| AosError::Io(format!("Failed to finalize .aos file: {}", e)))?;

        Ok(())
    }
}

/// Single-file adapter packager
pub struct SingleFileAdapterPackager;

impl SingleFileAdapterPackager {
    /// Save adapter to .aos file
    pub async fn save<P: AsRef<Path>>(adapter: &SingleFileAdapter, path: P) -> Result<()> {
        SingleFileAdapter::save_with_options(adapter, path, PackageOptions::default()).await
    }

    /// Save adapter to .aos file with custom options
    pub async fn save_with_options<P: AsRef<Path>>(
        adapter: &SingleFileAdapter,
        path: P,
        options: PackageOptions,
    ) -> Result<()> {
        SingleFileAdapter::save_with_options(adapter, path, options).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    #[tokio::test]
    async fn test_package_adapter() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test_adapter.aos");

        // Create test adapter
        let adapter = create_test_adapter();

        // Package adapter
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        // Verify file exists
        assert!(aos_path.exists());

        // Verify file size
        let metadata = std::fs::metadata(&aos_path).unwrap();
        assert!(metadata.len() > 0);
    }

    fn create_test_adapter() -> SingleFileAdapter {
        use crate::training::TrainingConfig;

        let positive_weights = WeightGroup {
            lora_a: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            lora_b: vec![vec![0.5, 0.6], vec![0.7, 0.8]],
            metadata: WeightMetadata {
                example_count: 10,
                avg_loss: 0.5,
                training_time_ms: 1000,
                group_type: WeightGroupType::Positive,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };

        let negative_weights = WeightGroup {
            lora_a: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            lora_b: vec![vec![0.5, 0.6], vec![0.7, 0.8]],
            metadata: WeightMetadata {
                example_count: 5,
                avg_loss: 0.3,
                training_time_ms: 500,
                group_type: WeightGroupType::Negative,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };

        let adapter_weights = AdapterWeights {
            positive: positive_weights,
            negative: negative_weights,
            combined: None,
        };

        let training_data = vec![];
        let config = TrainingConfig::default();
        let lineage = LineageInfo {
            adapter_id: "test_adapter".to_string(),
            version: "1.0.0".to_string(),
            parent_version: None,
            parent_hash: None,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        SingleFileAdapter::create(
            "test_adapter".to_string(),
            adapter_weights,
            training_data,
            config,
            lineage,
        )
        .unwrap()
    }
}
