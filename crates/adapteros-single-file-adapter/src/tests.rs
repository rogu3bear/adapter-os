#[cfg(test)]
mod tests {
    use super::*;
    use super::format::{AdapterWeights, WeightGroup, WeightMetadata, WeightGroupType, WeightGroupConfig};
    use super::training::{TrainingConfig, TrainingExample};
    use super::format_detector::{detect_format, FormatVersion};
    use adapteros_crypto::Keypair;
    use tempfile::TempDir;
    use std::collections::HashMap;

    fn new_test_tempdir() -> TempDir {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("tempdir")
    }

    fn create_test_adapter() -> SingleFileAdapter {
        // Create v2 format weights with positive/negative groups
        let weights = AdapterWeights {
            positive: WeightGroup {
                lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
                lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
                metadata: WeightMetadata {
                    example_count: 100,
                    avg_loss: 0.1,
                    training_time_ms: 5000,
                    group_type: WeightGroupType::Positive,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            },
            negative: WeightGroup {
                lora_a: vec![vec![-1.0, -2.0], vec![-3.0, -4.0]],
                lora_b: vec![vec![-5.0, -6.0], vec![-7.0, -8.0]],
                metadata: WeightMetadata {
                    example_count: 50,
                    avg_loss: 0.2,
                    training_time_ms: 3000,
                    group_type: WeightGroupType::Negative,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            },
            combined: None,
        };
        
        let training_data = vec![
            TrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: HashMap::new(),
                weight: 1.0,
            }
        ];
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
            weights,
            training_data,
            config,
            lineage,
        ).unwrap()
    }

    #[tokio::test]
    async fn test_aos_create_and_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test.aos");
        
        // Create test adapter
        let adapter = create_test_adapter();
        
        // Save to .aos file
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Load from .aos file
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();
        
        // Verify integrity
        assert!(loaded.verify().unwrap());
        
        // Verify contents
        assert_eq!(adapter.manifest.adapter_id, loaded.manifest.adapter_id);
        assert_eq!(adapter.weights, loaded.weights);
        assert_eq!(adapter.training_data.len(), loaded.training_data.len());
    }

    #[tokio::test]
    async fn test_aos_validation() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test.aos");
        
        // Create and save test adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Validate
        let result = SingleFileAdapterValidator::validate(&aos_path).await.unwrap();
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_aos_integrity_verification() {
        let adapter = create_test_adapter();
        
        // Should pass verification
        assert!(adapter.verify().unwrap());
        
        // Modify weights and verify it fails
        let mut modified_adapter = adapter.clone();
        modified_adapter.weights.positive.lora_a[0][0] = 999.0;
        assert!(!modified_adapter.verify().unwrap());
    }

    #[tokio::test]
    async fn test_aos_missing_file() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("nonexistent.aos");
        
        let result = SingleFileAdapterValidator::validate(&aos_path).await.unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.contains(&"File does not exist".to_string()));
    }

    #[tokio::test]
    async fn test_aos_signature_verification() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("signed_test.aos");
        
        // Create and sign adapter
        let mut adapter = create_test_adapter();
        let keypair = Keypair::generate();
        adapter.sign(&keypair).unwrap();
        
        // Save and load
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();
        
        // Verify signature is present and valid
        assert!(loaded.is_signed());
        assert!(loaded.verify().unwrap());
        
        // Get signature info
        let (key_id, timestamp) = loaded.signature_info().unwrap();
        assert!(!key_id.is_empty());
        assert!(timestamp > 0);
    }

    #[tokio::test]
    async fn test_aos_signature_tamper_detection() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("tampered_test.aos");
        
        // Create and sign adapter
        let mut adapter = create_test_adapter();
        let keypair = Keypair::generate();
        adapter.sign(&keypair).unwrap();
        
        // Tamper with weights after signing (modify a value in positive group)
        adapter.weights.positive.lora_a[0][0] = 999.0;
        
        // Verification should fail
        assert!(!adapter.verify().unwrap());
    }

    #[tokio::test]
    async fn test_aos_compression_levels() {
        let temp_dir = new_test_tempdir();
        
        let adapter = create_test_adapter();
        
        // Test different compression levels
        for (level, name) in &[
            (CompressionLevel::Store, "store"),
            (CompressionLevel::Fast, "fast"),
            (CompressionLevel::Best, "best"),
        ] {
            let path = temp_dir.path().join(format!("test_{}.aos", name));
            
            let options = PackageOptions {
                compression: *level,
                include_signature: true,
                include_combined_weights: true,
            };
            SingleFileAdapterPackager::save_with_options(&adapter, &path, options)
                .await
                .unwrap();
            
            // Should be able to load regardless of compression
            let loaded = SingleFileAdapterLoader::load(&path).await.unwrap();
            assert_eq!(loaded.manifest.adapter_id, adapter.manifest.adapter_id);
            assert_eq!(loaded.weights, adapter.weights);
        }
    }

    #[tokio::test]
    async fn test_aos_manifest_only_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("manifest_test.aos");
        
        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Load only manifest (fast operation)
        let manifest = SingleFileAdapterLoader::load_manifest_only(&aos_path)
            .await
            .unwrap();
        
        assert_eq!(manifest.adapter_id, adapter.manifest.adapter_id);
        assert_eq!(manifest.version, adapter.manifest.version);
        assert_eq!(manifest.format_version, AOS_FORMAT_VERSION);
    }

    #[tokio::test]
    async fn test_aos_component_extraction() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("component_test.aos");
        
        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Load adapter to verify components are accessible
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();
        assert_eq!(loaded.manifest.adapter_id, adapter.manifest.adapter_id);
        assert_eq!(loaded.weights.positive.lora_a, adapter.weights.positive.lora_a);
    }

    #[tokio::test]
    async fn test_aos_format_version_check() {
        // Test compatibility check
        let report = get_compatibility_report(AOS_FORMAT_VERSION);
        assert!(report.is_compatible);
        
        // Test older version
        let report = get_compatibility_report(0);
        assert!(report.is_compatible);
        assert!(!report.warnings.is_empty() || report.file_version < report.current_version);
        
        // Test future version
        let report = get_compatibility_report(99);
        assert!(!report.is_compatible);
        assert!(!report.errors.is_empty());
    }

    #[tokio::test]
    async fn test_aos_skip_verification() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("skip_verify_test.aos");
        
        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Load with verification skipped (faster)
        let options = LoadOptions {
            skip_verification: true,
            skip_signature_check: false,
            use_mmap: false,
        };
        let loaded = SingleFileAdapterLoader::load_with_options(&aos_path, options)
            .await
            .unwrap();
        
        assert_eq!(loaded.manifest.adapter_id, adapter.manifest.adapter_id);
    }

    #[tokio::test]
    async fn test_format_detection() {
        let temp_dir = new_test_tempdir();
        
        // Test ZIP format detection
        let zip_path = temp_dir.path().join("test.zip.aos");
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &zip_path).await.unwrap();
        
        let format = detect_format(&zip_path).unwrap();
        assert_eq!(format, FormatVersion::ZipV1);
        
        // Test AOS format detection
        let aos_path = temp_dir.path().join("test.aos");
        use crate::aos2_packager::AosPackager;
        AosPackager::save(&adapter, &aos_path).await.unwrap();

        let format = detect_format(&aos_path).unwrap();
        assert_eq!(format, FormatVersion::AosV2);
    }

    #[tokio::test]
    async fn test_aos_create_and_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test.aos");

        // Create test adapter
        let adapter = create_test_adapter();

        // Save to AOS format
        use crate::aos2_packager::AosPackager;
        AosPackager::save(&adapter, &aos_path).await.unwrap();

        // Load from AOS file (should auto-detect format)
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();

        // Verify integrity
        assert!(loaded.verify().unwrap());

        // Verify contents
        assert_eq!(adapter.manifest.adapter_id, loaded.manifest.adapter_id);
        assert_eq!(adapter.weights.positive.lora_a, loaded.weights.positive.lora_a);
        assert_eq!(adapter.training_data.len(), loaded.training_data.len());
    }

    #[tokio::test]
    async fn test_format_conversion() {
        let temp_dir = new_test_tempdir();
        let zip_path = temp_dir.path().join("test.zip.aos");
        let aos_path = temp_dir.path().join("test.aos");

        // Create ZIP format adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &zip_path).await.unwrap();

        // Load ZIP adapter
        let zip_adapter = SingleFileAdapterLoader::load(&zip_path).await.unwrap();

        // Convert to AOS format
        use crate::aos2_packager::AosPackager;
        AosPackager::save(&zip_adapter, &aos_path).await.unwrap();

        // Load AOS adapter
        let aos_adapter = SingleFileAdapterLoader::load(&aos_path).await.unwrap();

        // Verify both produce identical data
        assert_eq!(zip_adapter.manifest.adapter_id, aos_adapter.manifest.adapter_id);
        assert_eq!(zip_adapter.weights.positive.lora_a, aos_adapter.weights.positive.lora_a);
        assert_eq!(zip_adapter.training_data.len(), aos_adapter.training_data.len());
        
        // Both should verify
        assert!(zip_adapter.verify().unwrap());
        assert!(aos_adapter.verify().unwrap());
    }
}
