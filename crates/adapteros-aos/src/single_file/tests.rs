#[cfg(test)]
mod tests {
    use super::format::{
        get_compatibility_report, AdapterWeights, LineageInfo, WeightGroup, WeightGroupConfig,
        WeightGroupType, WeightMetadata, AOS_FORMAT_VERSION,
    };
    use super::loader::{LoadOptions, SingleFileAdapterLoader};
    use super::packager::{PackageOptions, SingleFileAdapterPackager};
    use super::training::{TrainingConfig, TrainingExample};
    use super::validator::SingleFileAdapterValidator;
    use super::SingleFileAdapter;
    use adapteros_crypto::Keypair;
    use adapteros_types::training::ExampleMetadataV1;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("create temp dir")
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

        let metadata = ExampleMetadataV1::new("test", 1, "row-hash", "{}", 0);
        let attention_mask = TrainingExample::attention_mask_from_tokens(&[1, 2, 3], 0);
        let training_data = vec![TrainingExample::new(
            vec![1, 2, 3],
            vec![4, 5, 6],
            attention_mask,
            metadata,
        )];
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

        SingleFileAdapter::create("test_adapter".to_string(), weights, training_data, config, lineage)
            .unwrap()
    }

    #[tokio::test]
    async fn test_aos_create_and_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test.aos");

        // Create test adapter
        let adapter = create_test_adapter();

        // Save to .aos file
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        // Load from .aos file
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();

        // Verify contents
        assert_eq!(adapter.manifest.adapter_id, loaded.manifest.adapter_id);
        assert_eq!(adapter.manifest.rank, loaded.manifest.rank);
    }

    #[tokio::test]
    async fn test_aos_validation() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("test.aos");

        // Create and save test adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

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
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();
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
    async fn test_aos_manifest_only_load() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("manifest_test.aos");

        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

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
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        // Extract manifest component
        let manifest_bytes = SingleFileAdapterLoader::extract_component(&aos_path, "manifest")
            .await
            .unwrap();
        assert!(!manifest_bytes.is_empty());

        // Extract weights component
        let weights_bytes = SingleFileAdapterLoader::extract_component(&aos_path, "weights")
            .await
            .unwrap();
        assert!(!weights_bytes.is_empty());
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
        SingleFileAdapterPackager::save(&adapter, &aos_path)
            .await
            .unwrap();

        // Load with verification skipped (faster)
        let options = LoadOptions {
            skip_verification: true,
            skip_signature_check: false,
        };
        let loaded = SingleFileAdapterLoader::load_with_options(&aos_path, options)
            .await
            .unwrap();

        assert_eq!(loaded.manifest.adapter_id, adapter.manifest.adapter_id);
    }

    #[tokio::test]
    async fn test_aos_package_options() {
        let temp_dir = new_test_tempdir();
        let aos_path = temp_dir.path().join("options_test.aos");

        let adapter = create_test_adapter();

        // Test with custom options
        let options = PackageOptions::with_combined_weights();
        SingleFileAdapterPackager::save_with_options(&adapter, &aos_path, options)
            .await
            .unwrap();

        // Should be able to load
        let loaded = SingleFileAdapterLoader::load(&aos_path).await.unwrap();
        assert_eq!(loaded.manifest.adapter_id, adapter.manifest.adapter_id);
    }
}
