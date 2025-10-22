#[cfg(test)]
mod tests {
    use super::*;
    use super::format::{AdapterWeights, WeightGroup, WeightMetadata, WeightGroupType, WeightGroupConfig};
    use super::training::{TrainingConfig, TrainingExample};
    use adapteros_crypto::Keypair;
    use tempfile::TempDir;
    use std::collections::HashMap;

    fn create_test_adapter() -> SingleFileAdapter {
        // Create v2 format weights with positive/negative groups
        let weights = AdapterWeights {
            positive: WeightGroup {
                lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
                lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
                metadata: WeightMetadata {
                    group_type: WeightGroupType::Positive,
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["q_proj".to_string()],
                    config: WeightGroupConfig { dropout: 0.1 },
                    checksum: "test".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            },
            negative: WeightGroup {
                lora_a: vec![vec![-1.0, -2.0], vec![-3.0, -4.0]],
                lora_b: vec![vec![-5.0, -6.0], vec![-7.0, -8.0]],
                metadata: WeightMetadata {
                    group_type: WeightGroupType::Negative,
                    rank: 16,
                    alpha: 32.0,
                    target_modules: vec!["q_proj".to_string()],
                    config: WeightGroupConfig { dropout: 0.1 },
                    checksum: "test_neg".to_string(),
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
        let temp_dir = TempDir::new().unwrap();
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
        let temp_dir = TempDir::new().unwrap();
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
        modified_adapter.weights.push(99);
        assert!(!modified_adapter.verify().unwrap());
    }

    #[tokio::test]
    async fn test_aos_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("nonexistent.aos");
        
        let result = SingleFileAdapterValidator::validate(&aos_path).await.unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.contains(&"File does not exist".to_string()));
    }

    #[tokio::test]
    async fn test_aos_signature_verification() {
        let temp_dir = TempDir::new().unwrap();
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
        let temp_dir = TempDir::new().unwrap();
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
        let temp_dir = TempDir::new().unwrap();
        
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
        let temp_dir = TempDir::new().unwrap();
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
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("component_test.aos");
        
        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
        // Extract specific components
        let manifest_data = SingleFileAdapterLoader::extract_component(&aos_path, "manifest")
            .await
            .unwrap();
        assert!(!manifest_data.is_empty());
        
        let weights_data = SingleFileAdapterLoader::extract_component(&aos_path, "weights")
            .await
            .unwrap();
        assert_eq!(weights_data, adapter.weights);
        
        // Try invalid component
        let result = SingleFileAdapterLoader::extract_component(&aos_path, "invalid").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_aos_format_version_check() {
        // Test compatibility check
        let report = get_compatibility_report(AOS_FORMAT_VERSION);
        assert!(report.compatible);
        assert!(!report.can_upgrade);
        
        // Test older version
        let report = get_compatibility_report(0);
        assert!(report.compatible);
        assert!(report.can_upgrade);
        
        // Test future version
        let report = get_compatibility_report(99);
        assert!(!report.compatible);
        assert!(!report.can_upgrade);
    }

    #[tokio::test]
    async fn test_aos_skip_verification() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = temp_dir.path().join("skip_verify_test.aos");
        
        // Create and save adapter
        let adapter = create_test_adapter();
        SingleFileAdapterPackager::save(&adapter, &aos_path).await.unwrap();
        
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
}
