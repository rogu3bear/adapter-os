#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for .aos single-file adapter format

// ============================================================================
// AOS COORDINATION HEADER
// ============================================================================
// File: tests/aos_integration_test.rs
// Phase: 3 - Advanced Features (Testing)
// Assigned: Intern G (Testing Team)
// Status: Complete - Integration tests implemented
// Dependencies: SingleFileAdapter, CLI commands, Database
// Last Updated: 2024-01-15
//
// COORDINATION NOTES:
// - This file affects: Test coverage, CI/CD pipelines, quality assurance
// - Changes require: Updates when SingleFileAdapter format changes
// - Testing needed: These tests validate the .aos format implementation
// - CLI Impact: Tests validate CLI command functionality
// - UI Impact: Tests validate UI component integration
// - Database Impact: Tests validate database schema changes
// ============================================================================

use adapteros_lora_worker::training::{TrainingConfig, TrainingExample};
use adapteros_single_file_adapter::{
    LineageInfo, SingleFileAdapter, SingleFileAdapterLoader, SingleFileAdapterPackager,
    SingleFileAdapterValidator,
};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).unwrap()
}

fn create_test_adapter() -> SingleFileAdapter {
    let weights = vec![1, 2, 3, 4, 5]; // Dummy weights
    let training_data = vec![
        TrainingExample::new(vec![1, 2, 3], vec![4, 5, 6]),
        TrainingExample::new(vec![7, 8, 9], vec![10, 11, 12]),
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
    )
    .unwrap()
}

#[tokio::test]
async fn test_aos_full_lifecycle() {
    let temp_dir = new_test_tempdir();
    let aos_path = temp_dir.path().join("test_adapter.aos");

    // Create adapter
    let adapter = create_test_adapter();
    let original_weights_len = adapter.weights.len();
    let original_training_data_len = adapter.training_data.len();

    // Save to .aos file
    SingleFileAdapterPackager::save(&adapter, &aos_path)
        .await
        .unwrap();

    // Verify file exists
    assert!(aos_path.exists());

    // Load from .aos file
    let loaded_adapter = SingleFileAdapterLoader::load(&aos_path).await.unwrap();

    // Verify integrity
    assert!(loaded_adapter.verify().unwrap());

    // Verify contents
    assert_eq!(loaded_adapter.manifest.adapter_id, "test_adapter");
    assert_eq!(loaded_adapter.weights.len(), original_weights_len);
    assert_eq!(
        loaded_adapter.training_data.len(),
        original_training_data_len
    );
}

#[tokio::test]
async fn test_aos_validation() {
    let temp_dir = new_test_tempdir();
    let aos_path = temp_dir.path().join("test_adapter.aos");

    // Create and save adapter
    let adapter = create_test_adapter();
    SingleFileAdapterPackager::save(&adapter, &aos_path)
        .await
        .unwrap();

    // Validate
    let result = SingleFileAdapterValidator::validate(&aos_path)
        .await
        .unwrap();

    assert!(result.is_valid);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_aos_missing_file_validation() {
    let temp_dir = new_test_tempdir();
    let aos_path = temp_dir.path().join("nonexistent.aos");

    let result = SingleFileAdapterValidator::validate(&aos_path)
        .await
        .unwrap();

    assert!(!result.is_valid);
    assert!(result.errors.contains(&"File does not exist".to_string()));
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
async fn test_aos_extract_components() {
    let adapter = create_test_adapter();

    // Test weight extraction
    let weights = adapter.extract_weights();
    assert_eq!(weights.len(), 5);

    // Test training data extraction
    let training_data = adapter.extract_training_data();
    assert_eq!(training_data.len(), 2);

    // Test metadata extraction
    let metadata = adapter.get_metadata();
    assert_eq!(metadata.adapter_id, "test_adapter");
    assert_eq!(metadata.version, "1.0.0");
}
