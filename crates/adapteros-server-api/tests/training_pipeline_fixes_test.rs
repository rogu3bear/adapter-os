//! Integration tests for Training Pipeline fixes
//!
//! Tests for:
//! 1. HIGH: OOM during chunked upload assembly - streaming reads
//! 2. HIGH: Concurrent training job limit enforcement
//! 3. HIGH: Dataset upload session expiration race condition
//! 4. MEDIUM: Dataset validation status cleanup on error
//! 5. MEDIUM: Checkpoint corruption prevention with atomic writes

#![allow(clippy::useless_vec)]

use adapteros_lora_worker::training::checkpoint::{CheckpointManager, TrainingCheckpoint};
use adapteros_lora_worker::training::trainer::{LoRAWeights, TrainingConfig};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

#[tokio::test]
async fn test_checkpoint_atomic_write_prevents_corruption() {
    let temp_dir = new_test_tempdir();
    let checkpoint_path = temp_dir.path().join("test.ckpt");

    let config = TrainingConfig {
        rank: 8,
        alpha: 16.0,
        learning_rate: 0.001,
        batch_size: 32,
        epochs: 10,
        hidden_dim: 768,
        vocab_size: 32000,
        checkpoint_interval: None,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        determinism: None,
        ..Default::default()
    };

    let weights = LoRAWeights {
        lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
        lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        moe_config: None,
        precomputed_delta: None,
        modules: Default::default(),
    };

    let checkpoint = TrainingCheckpoint::new(5, 100, 0.5, 0.001, config.clone(), weights.clone());

    // Save checkpoint - should use atomic write pattern
    checkpoint.save(&checkpoint_path, None).await.unwrap();

    // Verify temp file was cleaned up
    let temp_path = checkpoint_path.with_extension("ckpt.tmp");
    assert!(!temp_path.exists(), "Temp file should be cleaned up");

    // Verify checkpoint can be loaded
    let loaded = TrainingCheckpoint::load(&checkpoint_path).await.unwrap();
    assert_eq!(loaded.epoch, 5);
    assert_eq!(loaded.step, 100);
}

#[tokio::test]
async fn test_checkpoint_load_validates_corruption() {
    let temp_dir = new_test_tempdir();
    let checkpoint_path = temp_dir.path().join("corrupted.ckpt");

    // Create a corrupted checkpoint file (invalid JSON)
    tokio::fs::write(&checkpoint_path, "{ invalid json")
        .await
        .unwrap();

    // Loading should fail with detailed error
    let result = TrainingCheckpoint::load(&checkpoint_path).await;
    assert!(result.is_err(), "Should fail to load corrupted checkpoint");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("corruption") || err_msg.contains("deserialize"),
        "Error should mention corruption or deserialization failure: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_checkpoint_load_detects_empty_file() {
    let temp_dir = new_test_tempdir();
    let checkpoint_path = temp_dir.path().join("empty.ckpt");

    // Create empty file
    tokio::fs::write(&checkpoint_path, "").await.unwrap();

    // Loading should fail
    let result = TrainingCheckpoint::load(&checkpoint_path).await;
    assert!(result.is_err(), "Should fail to load empty checkpoint");

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("empty"),
        "Error should mention empty file: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_checkpoint_load_validates_sanity() {
    let temp_dir = new_test_tempdir();
    let checkpoint_path = temp_dir.path().join("invalid.ckpt");

    // Create checkpoint with invalid data (epoch too high)
    let json = serde_json::json!({
        "epoch": 99999,
        "step": 100,
        "loss": 0.5,
        "learning_rate": 0.001,
        "config": {
            "rank": 8,
            "alpha": 16.0,
            "learning_rate": 0.001,
            "batch_size": 32,
            "epochs": 10,
            "hidden_dim": 768,
            "preferred_backend": null,
            "require_gpu": false,
            "max_gpu_memory_mb": 0
        },
        "weights": {
            "lora_a": [[1.0]],
            "lora_b": [[2.0]]
        },
        "best_loss": 0.5,
        "epochs_without_improvement": 0,
        "timestamp": "2024-01-01T00:00:00Z",
        "metadata": {}
    });

    tokio::fs::write(&checkpoint_path, json.to_string())
        .await
        .unwrap();

    // Loading should fail validation
    let result = TrainingCheckpoint::load(&checkpoint_path).await;
    assert!(
        result.is_err(),
        "Should fail sanity check for invalid epoch"
    );

    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("bounds") || err_msg.contains("corruption"),
        "Error should mention bounds or corruption: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_checkpoint_manager_cleanup() {
    let temp_dir = new_test_tempdir();
    let manager = CheckpointManager::new(temp_dir.path(), 2, 3, "test-adapter".to_string());

    let config = TrainingConfig::default();
    let weights = LoRAWeights {
        lora_a: vec![vec![1.0]],
        lora_b: vec![vec![2.0]],
        moe_config: None,
        precomputed_delta: None,
        modules: Default::default(),
    };

    // Create 5 checkpoints
    for epoch in vec![2, 4, 6, 8, 10] {
        let checkpoint =
            TrainingCheckpoint::new(epoch, 0, 0.5, 0.001, config.clone(), weights.clone());
        manager.save_checkpoint(&checkpoint).await.unwrap();
    }

    // Should have max 3 checkpoints kept
    let checkpoints = manager.list_checkpoints().await.unwrap();
    assert!(
        checkpoints.len() <= 3,
        "Should keep max 3 checkpoints, got {}",
        checkpoints.len()
    );

    // Most recent checkpoints should be kept (6, 8, 10)
    assert!(checkpoints.contains(&10), "Should keep epoch 10");
    assert!(checkpoints.contains(&8), "Should keep epoch 8");
    assert!(checkpoints.contains(&6), "Should keep epoch 6");
}

// Note: Full integration tests for chunked upload OOM fix, concurrent training limit,
// and session expiration race would require full AppState setup and are better suited
// for end-to-end tests. The fixes have been implemented in:
// - chunked_upload.rs: Streaming reads with bounded buffer
// - training.rs: Concurrent job limit check before starting training
// - chunked_upload.rs: Background cleanup task with proper locking
// - datasets.rs: Error path cleanup for validation status
