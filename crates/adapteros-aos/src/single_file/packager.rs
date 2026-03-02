//! Single-file adapter packager
//!
//! Creates .aos archives in AOS2 binary format using the segment-indexed structure.

use super::format::*;
use crate::{AosWriter, BackendTag};
use adapteros_core::{AosError, Result};
use safetensors::tensor::TensorView;
use std::path::Path;

/// Options for packaging .aos files
#[derive(Debug, Clone, Default)]
pub struct PackageOptions {
    /// Use combined weights if available (default: true)
    /// When true, prefers adapter.weights.combined over positive weights
    pub use_combined_weights: bool,
}

impl PackageOptions {
    /// Create options that prefer combined weights
    pub fn with_combined_weights() -> Self {
        Self {
            use_combined_weights: true,
        }
    }
}

/// Serialize a WeightGroup to safetensors binary format
fn serialize_weights_to_safetensors(weights: &WeightGroup) -> Result<Vec<u8>> {
    // Flatten lora_a: Vec<Vec<f32>> -> Vec<f32> -> bytes
    let lora_a_flat: Vec<f32> = weights
        .lora_a
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect();
    let lora_a_bytes: Vec<u8> = lora_a_flat.iter().flat_map(|f| f.to_le_bytes()).collect();

    // Flatten lora_b: Vec<Vec<f32>> -> Vec<f32> -> bytes
    let lora_b_flat: Vec<f32> = weights
        .lora_b
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect();
    let lora_b_bytes: Vec<u8> = lora_b_flat.iter().flat_map(|f| f.to_le_bytes()).collect();

    // Get shapes from weight matrices
    let rank = weights.lora_a.len();
    let hidden_dim = weights.lora_a.first().map(|r| r.len()).unwrap_or(0);

    if rank == 0 || hidden_dim == 0 {
        return Err(AosError::Training(
            "Cannot serialize empty weight matrices".to_string(),
        ));
    }

    // Create tensor views
    let lora_a_view = TensorView::new(
        safetensors::Dtype::F32,
        vec![rank, hidden_dim],
        &lora_a_bytes,
    )
    .map_err(|e| AosError::Training(format!("Failed to create lora_a tensor: {}", e)))?;

    let lora_b_view = TensorView::new(
        safetensors::Dtype::F32,
        vec![hidden_dim, rank],
        &lora_b_bytes,
    )
    .map_err(|e| AosError::Training(format!("Failed to create lora_b tensor: {}", e)))?;

    // Serialize to safetensors format
    let tensors = vec![("lora_a", lora_a_view), ("lora_b", lora_b_view)];

    safetensors::tensor::serialize(tensors, None)
        .map_err(|e| AosError::Training(format!("Failed to serialize weights: {}", e)))
}

impl SingleFileAdapter {
    /// Save adapter to .aos file using AOS2 binary format
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

        // Get scope_path from manifest metadata
        let scope_path = adapter
            .manifest
            .metadata
            .get("scope_path")
            .cloned()
            .filter(|s| !s.is_empty());

        // Determine which weights to use
        let weights_to_serialize = if options.use_combined_weights {
            adapter
                .weights
                .combined
                .as_ref()
                .unwrap_or(&adapter.weights.positive)
        } else {
            &adapter.weights.positive
        };

        // Serialize weights to safetensors format
        let weights_bytes = serialize_weights_to_safetensors(weights_to_serialize)?;

        // Create AOS archive
        let mut writer = AosWriter::new();
        writer.add_segment(BackendTag::Canonical, scope_path, &weights_bytes)?;

        // Write archive (signature is stored in SingleFileAdapter.signature, not in manifest)
        writer.write_archive(path, &adapter.manifest)?;

        tracing::info!(
            "Saved AOS archive: {} (adapter_id={})",
            path.display(),
            adapter.manifest.adapter_id
        );

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
    use super::super::loader::SingleFileAdapterLoader;
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("create temp dir")
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

        // Verify file size (AOS header is at least 64 bytes)
        let metadata = std::fs::metadata(&aos_path).unwrap();
        assert!(metadata.len() >= 64);
    }

    #[tokio::test]
    async fn test_roundtrip_save_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("roundtrip_adapter.aos");

        // Create and save
        let original = create_test_adapter();
        SingleFileAdapterPackager::save(&original, &aos_path)
            .await
            .unwrap();

        // Load back
        let loaded: SingleFileAdapter = SingleFileAdapterLoader::load(&aos_path).await.unwrap();

        // Verify key fields match
        assert_eq!(loaded.manifest.adapter_id, original.manifest.adapter_id);
        assert_eq!(loaded.manifest.rank, original.manifest.rank);
    }

    fn create_test_adapter() -> SingleFileAdapter {
        use super::super::training::TrainingConfig;

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
