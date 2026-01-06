//! Edge case tests for boundary conditions in the training pipeline.
//!
//! These tests cover zero/empty inputs, extreme values, and boundary
//! conditions that could cause panics or incorrect behavior.

#![allow(clippy::field_reassign_with_default)]
#![allow(unused_imports)]

use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;

use crate::training::dataset::weighted_round_robin_merge;
use crate::training::job::{DataLineageMode, DatasetVersionSelection, TrainingConfig};
use crate::training::service::TrainingService;

// ============================================================================
// Zero/Empty Dataset Tests
// ============================================================================

/// Test that weighted_round_robin_merge returns empty for empty input.
#[test]
fn test_weighted_round_robin_empty_input() {
    let datasets: Vec<(Vec<WorkerTrainingExample>, f32)> = vec![];
    let merged = weighted_round_robin_merge(datasets);
    assert!(merged.is_empty(), "Empty input should produce empty output");
}

/// Test that weighted_round_robin_merge handles all empty queues.
#[test]
fn test_weighted_round_robin_all_empty_queues() {
    let datasets = vec![
        (vec![], 1.0), // Empty queue with weight 1
        (vec![], 2.0), // Empty queue with weight 2
    ];
    let merged = weighted_round_robin_merge(datasets);
    assert!(
        merged.is_empty(),
        "All empty queues should produce empty output"
    );
}

/// Test that weighted_round_robin_merge handles single dataset.
#[test]
fn test_weighted_round_robin_single_dataset() {
    let examples = vec![
        WorkerTrainingExample {
            input: vec![1],
            target: vec![2],
            metadata: Default::default(),
            weight: 1.0,
        },
        WorkerTrainingExample {
            input: vec![3],
            target: vec![4],
            metadata: Default::default(),
            weight: 1.0,
        },
    ];
    let datasets = vec![(examples.clone(), 1.0)];
    let merged = weighted_round_robin_merge(datasets);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].input, vec![1]);
    assert_eq!(merged[1].input, vec![3]);
}

/// Test that zero weight is treated as 1 slot (minimum).
#[test]
fn test_weighted_round_robin_zero_weight() {
    let ds1 = vec![WorkerTrainingExample {
        input: vec![1],
        target: vec![2],
        metadata: Default::default(),
        weight: 1.0,
    }];

    // Zero weight should still get 1 slot
    let datasets = vec![(ds1, 0.0)];
    let merged = weighted_round_robin_merge(datasets);

    assert_eq!(merged.len(), 1, "Zero weight should still produce output");
}

/// Test that negative weight is clamped to 0, then to 1 slot.
#[test]
fn test_weighted_round_robin_negative_weight() {
    let ds1 = vec![WorkerTrainingExample {
        input: vec![1],
        target: vec![2],
        metadata: Default::default(),
        weight: 1.0,
    }];

    // Negative weight should be clamped to 0, then treated as 1 slot
    let datasets = vec![(ds1, -5.0)];
    let merged = weighted_round_robin_merge(datasets);

    assert_eq!(
        merged.len(),
        1,
        "Negative weight should still produce output"
    );
}

/// Test weighted round-robin with extreme weight ratio.
#[test]
fn test_weighted_round_robin_extreme_ratio() {
    let ds1 = vec![
        WorkerTrainingExample {
            input: vec![1],
            target: vec![2],
            metadata: Default::default(),
            weight: 1.0,
        },
        WorkerTrainingExample {
            input: vec![3],
            target: vec![4],
            metadata: Default::default(),
            weight: 1.0,
        },
    ];
    let ds2 = vec![WorkerTrainingExample {
        input: vec![100],
        target: vec![200],
        metadata: Default::default(),
        weight: 1.0,
    }];

    // Weight 100:1 - ds1 should get 100 slots per cycle
    let datasets = vec![(ds1, 100.0), (ds2, 1.0)];
    let merged = weighted_round_robin_merge(datasets);

    // Total should be 3 examples (2 from ds1, 1 from ds2)
    assert_eq!(merged.len(), 3);
    // ds1 examples should appear before ds2 due to scheduling
}

// ============================================================================
// Empty Dataset Version IDs Tests
// ============================================================================

/// Test that empty dataset_version_ids with synthetic=false fails.
#[tokio::test]
async fn test_start_training_rejects_empty_dataset_versions() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let result = service
        .start_training(
            "empty-versions".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            Some(vec![]), // Empty vec, not None
            false,        // synthetic_mode = false
            DataLineageMode::Versioned,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

    assert!(
        result.is_err(),
        "Empty dataset_version_ids should fail for non-synthetic training"
    );
}

// ============================================================================
// Progress Calculation Edge Cases
// ============================================================================

/// Test progress update with zero total_epochs doesn't cause division by zero.
/// The job defaults to 3 epochs, but we test the calculation path.
#[tokio::test]
async fn test_progress_update_calculation() {
    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.epochs = 5; // Set specific epoch count

    let job = service
        .start_training(
            "progress-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Update progress to epoch 2 of 5
    service
        .update_progress(&job.id, 2, 0.5, 1000.0)
        .await
        .unwrap();

    let updated = service.get_job(&job.id).await.unwrap();
    // progress_pct = (2 / 5) * 100 = 40%
    assert!(
        (updated.progress_pct - 40.0).abs() < 0.1,
        "Progress should be 40%, got {}",
        updated.progress_pct
    );
}

/// Test that progress update at final epoch sets 100%.
#[tokio::test]
async fn test_progress_update_at_completion() {
    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.epochs = 3;

    let job = service
        .start_training(
            "progress-complete".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Update progress to final epoch
    service
        .update_progress(&job.id, 3, 0.1, 2000.0)
        .await
        .unwrap();

    let updated = service.get_job(&job.id).await.unwrap();
    assert!(
        (updated.progress_pct - 100.0).abs() < 0.1,
        "Progress should be 100% at final epoch"
    );
}

// ============================================================================
// Backend Policy Edge Cases
// ============================================================================

/// Test CoremlOnly policy sets require_gpu=true.
#[tokio::test]
async fn test_backend_policy_coreml_only_requires_gpu() {
    use crate::training::job::TrainingBackendPolicy;

    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.backend_policy = Some(TrainingBackendPolicy::CoremlOnly);
    config.require_gpu = false; // Will be overridden

    let job = service
        .start_training(
            "coreml-only-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // CoremlOnly should force require_gpu=true
    assert!(
        job.config.require_gpu,
        "CoremlOnly policy should set require_gpu=true"
    );
}

/// Test CoremlElseFallback policy sets default fallback.
#[tokio::test]
async fn test_backend_policy_coreml_else_fallback() {
    use crate::training::job::{TrainingBackendKind, TrainingBackendPolicy};

    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.backend_policy = Some(TrainingBackendPolicy::CoremlElseFallback);
    config.coreml_training_fallback = None; // Should be filled in

    let job = service
        .start_training(
            "coreml-fallback-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        job.config.coreml_training_fallback,
        Some(TrainingBackendKind::Mlx),
        "CoremlElseFallback should set MLX as fallback"
    );
}

// ============================================================================
// Data Spec Hash Edge Cases
// ============================================================================

/// Test that data_spec_hash is computed from data_spec_json when not provided.
#[tokio::test]
async fn test_data_spec_hash_computed_from_json() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let data_spec_json = r#"{"format": "jsonl", "columns": ["input", "target"]}"#.to_string();

    let job = service
        .start_training(
            "data-spec-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(data_spec_json.clone()),
            None, // data_spec_hash not provided
        )
        .await
        .unwrap();

    // Hash should be computed from the JSON
    assert!(job.data_spec_hash.is_some(), "data_spec_hash should be set");
    let hash = job.data_spec_hash.unwrap();
    // Verify it's a valid blake3 hex string (64 chars)
    assert_eq!(hash.len(), 64, "Hash should be 64 hex chars");
}

// ============================================================================
// Lora Tier and Scope Tests
// ============================================================================

/// Test default scope is "tenant".
#[tokio::test]
async fn test_default_scope_is_tenant() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "scope-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None, // scope = None
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        job.scope,
        Some("tenant".to_string()),
        "Default scope should be 'tenant'"
    );
}

/// Test custom scope is preserved.
#[tokio::test]
async fn test_custom_scope_preserved() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "custom-scope-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            Some("global".to_string()), // custom scope
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(job.scope, Some("global".to_string()));
}

// ============================================================================
// Job Logs Edge Cases
// ============================================================================

/// Test get_logs for a job that hasn't started.
#[tokio::test]
async fn test_get_logs_pending_job() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "logs-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let logs = service.get_logs(&job.id).await.unwrap();
    assert!(!logs.is_empty(), "Should have at least creation log");
    assert!(
        logs[0].contains("created"),
        "First log should mention creation"
    );
}

/// Test get_logs for non-existent job.
#[tokio::test]
async fn test_get_logs_nonexistent_job() {
    let service = TrainingService::new();

    let result = service.get_logs("nonexistent-id").await;
    assert!(result.is_err(), "Should error for non-existent job");
}
