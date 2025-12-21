//! Format migration utilities for .aos files
//!
//! Handles upgrading legacy .aos formats to newer versions while preserving data integrity.

use super::format::*;
use adapteros_core::{AosError, Result};
use std::path::Path;

/// Migration result containing original and migrated adapters
#[derive(Debug)]
pub struct MigrationResult {
    pub original_version: u8,
    pub new_version: u8,
    pub changes_applied: Vec<String>,
    pub adapter: SingleFileAdapter,
}

/// Migrate an adapter from an older format version
pub fn migrate_adapter(mut adapter: SingleFileAdapter) -> Result<MigrationResult> {
    let original_version = adapter.manifest.format_version;
    let mut changes_applied = Vec::new();

    // If already at current version, no migration needed
    if original_version == AOS_FORMAT_VERSION {
        return Ok(MigrationResult {
            original_version,
            new_version: AOS_FORMAT_VERSION,
            changes_applied: vec!["No migration needed".to_string()],
            adapter,
        });
    }

    // Handle legacy format (v0 - no format_version field)
    if original_version == 0 {
        changes_applied.push("Added format_version field".to_string());

        // Add default compression_method if missing
        if adapter.manifest.compression_method.is_empty() {
            adapter.manifest.compression_method = "deflate-fast".to_string();
            changes_applied.push("Added compression_method field".to_string());
        }

        adapter.manifest.format_version = 1;
    }

    // Handle v1 -> v2 migration (separate positive/negative weight groups)
    if original_version < 2 {
        // In v2, we ensure positive and negative weight groups are properly structured
        // The weights should already be separated, but we ensure the format version is updated
        changes_applied.push("Updated to v2 format with separate weight groups".to_string());
        adapter.manifest.format_version = 2;
    }

    tracing::info!(
        "Migrated adapter {} from v{} to v{} ({} changes)",
        adapter.manifest.adapter_id,
        original_version,
        adapter.manifest.format_version,
        changes_applied.len()
    );

    Ok(MigrationResult {
        original_version,
        new_version: adapter.manifest.format_version,
        changes_applied,
        adapter,
    })
}

/// Migrate an adapter file in-place
pub async fn migrate_file<P: AsRef<Path>>(path: P) -> Result<MigrationResult> {
    use super::loader::SingleFileAdapterLoader;
    use super::packager::SingleFileAdapterPackager;

    let path = path.as_ref();

    // Load the adapter (this will verify format compatibility)
    let adapter = SingleFileAdapterLoader::load(path).await?;

    // Migrate to current format
    let result = migrate_adapter(adapter)?;

    // Save back if changes were made
    if result.original_version != result.new_version {
        // Create backup
        let backup_path = path.with_extension("aos.bak");
        std::fs::copy(path, &backup_path)
            .map_err(|e| AosError::Io(format!("Failed to create backup: {}", e)))?;

        tracing::info!("Created backup: {}", backup_path.display());

        // Save migrated version
        SingleFileAdapterPackager::save(&result.adapter, path).await?;

        tracing::info!("Migrated {} in-place", path.display());
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{AdapterWeights, WeightGroup, WeightGroupType, WeightMetadata};
    use crate::training::{TrainingConfig, TrainingExample};
    use std::collections::HashMap;

    fn create_legacy_adapter() -> SingleFileAdapter {
        let pos_meta = WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Positive,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let neg_meta = WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Negative,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let weights = AdapterWeights {
            positive: WeightGroup {
                lora_a: vec![],
                lora_b: vec![],
                metadata: pos_meta,
            },
            negative: WeightGroup {
                lora_a: vec![],
                lora_b: vec![],
                metadata: neg_meta,
            },
            combined: None,
        };
        let training_data = vec![TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        }];
        let config = TrainingConfig {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0005,
            batch_size: 8,
            epochs: 4,
            hidden_dim: 3584,
            weight_group_config: WeightGroupConfig::default(),
        };
        let lineage = LineageInfo {
            adapter_id: "legacy_adapter".to_string(),
            version: "0.1.0".to_string(),
            parent_version: None,
            parent_hash: None,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Create adapter with v0 format
        let mut adapter = SingleFileAdapter::create(
            "legacy_adapter".to_string(),
            weights,
            training_data,
            config,
            lineage,
        )
        .unwrap();

        // Simulate legacy format
        adapter.manifest.format_version = 0;
        adapter.manifest.compression_method = String::new();

        adapter
    }

    #[test]
    fn test_migrate_legacy_adapter() {
        let legacy = create_legacy_adapter();
        assert_eq!(legacy.manifest.format_version, 0);

        let result = migrate_adapter(legacy).unwrap();
        assert_eq!(result.original_version, 0);
        assert_eq!(result.new_version, AOS_FORMAT_VERSION);
        assert!(!result.changes_applied.is_empty());
        assert_eq!(result.adapter.manifest.format_version, AOS_FORMAT_VERSION);
        assert!(!result.adapter.manifest.compression_method.is_empty());
    }

    #[test]
    fn test_migrate_current_version() {
        let pos_meta = WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Positive,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let neg_meta = WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type: WeightGroupType::Negative,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let weights = AdapterWeights {
            positive: WeightGroup {
                lora_a: vec![],
                lora_b: vec![],
                metadata: pos_meta,
            },
            negative: WeightGroup {
                lora_a: vec![],
                lora_b: vec![],
                metadata: neg_meta,
            },
            combined: None,
        };
        let adapter = SingleFileAdapter::create(
            "test".to_string(),
            weights,
            vec![],
            TrainingConfig {
                rank: 16,
                alpha: 32.0,
                learning_rate: 0.0005,
                batch_size: 8,
                epochs: 4,
                hidden_dim: 3584,
                weight_group_config: WeightGroupConfig::default(),
            },
            LineageInfo {
                adapter_id: "test".to_string(),
                version: "1.0.0".to_string(),
                parent_version: None,
                parent_hash: None,
                mutations: vec![],
                quality_delta: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();

        let result = migrate_adapter(adapter).unwrap();
        assert_eq!(result.original_version, AOS_FORMAT_VERSION);
        assert_eq!(result.new_version, AOS_FORMAT_VERSION);
    }
}
