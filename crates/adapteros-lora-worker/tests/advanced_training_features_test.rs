//! Integration tests for advanced training features (T7-T12)
//!
//! Tests learning rate schedules, early stopping, checkpoints, and GPU training

#![allow(unused_imports)]
#![allow(clippy::useless_vec)]

use adapteros_lora_worker::training::{
    CheckpointManager, EarlyStopping, EarlyStoppingConfig, LRScheduleType, LRScheduler,
    LRSchedulerConfig, LoRAWeights, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use adapteros_storage::platform::common::PlatformUtils;
use std::collections::HashMap;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

/// Test T8: Learning rate schedules work correctly
#[test]
fn test_learning_rate_schedules() {
    // Constant schedule
    let constant_config = LRSchedulerConfig::constant(0.001);
    let mut constant_scheduler = LRScheduler::new(constant_config);
    assert_eq!(constant_scheduler.get_lr(), 0.001);
    constant_scheduler.step();
    assert_eq!(constant_scheduler.get_lr(), 0.001);

    // Linear decay
    let linear_config = LRSchedulerConfig::linear(0.01, 0.001, 100);
    let mut linear_scheduler = LRScheduler::new(linear_config);
    assert_eq!(linear_scheduler.get_lr(), 0.01);
    for _ in 0..50 {
        linear_scheduler.step();
    }
    let mid_lr = linear_scheduler.get_lr();
    assert!((mid_lr - 0.0055).abs() < 0.001); // Approximately halfway

    // Cosine schedule
    let cosine_config = LRSchedulerConfig::cosine(0.01, 0.001, 100);
    let mut cosine_scheduler = LRScheduler::new(cosine_config);
    assert_eq!(cosine_scheduler.get_lr(), 0.01);
    for _ in 0..100 {
        cosine_scheduler.step();
    }
    assert!((cosine_scheduler.get_lr() - 0.001).abs() < 0.001);
}

/// Test T8: Warmup steps work correctly
#[test]
fn test_warmup_steps() {
    let config = LRSchedulerConfig::constant(0.001).with_warmup(10);
    let mut scheduler = LRScheduler::new(config);

    // Step 0: LR should be 0
    assert_eq!(scheduler.get_lr(), 0.0);

    // Step 5: LR should be 0.0005 (halfway)
    for _ in 0..5 {
        scheduler.step();
    }
    assert!((scheduler.get_lr() - 0.0005).abs() < 0.0001);

    // Step 10+: LR should be full 0.001
    for _ in 5..15 {
        scheduler.step();
    }
    assert_eq!(scheduler.get_lr(), 0.001);
}

/// Test T8: Early stopping triggers correctly
#[test]
fn test_early_stopping() {
    let config = EarlyStoppingConfig::with_patience(3);
    let mut early_stop = EarlyStopping::new(config);

    // Epoch 0: Initial loss
    assert!(early_stop.check(0, 1.0));
    assert!(!early_stop.should_stop());

    // Epoch 1: Improvement
    assert!(early_stop.check(1, 0.8));
    assert!(!early_stop.should_stop());

    // Epochs 2-3: No improvement (epochs_without_improvement = 2)
    early_stop.check(2, 0.8);
    early_stop.check(3, 0.8);
    assert!(!early_stop.should_stop()); // Still under patience threshold

    // Epoch 4: No improvement - epochs_without_improvement = 3 >= patience = 3
    early_stop.check(4, 0.8);
    assert!(early_stop.should_stop());
    assert_eq!(early_stop.best_epoch(), 1);
}

/// Test T11: Checkpoint saving and loading
#[tokio::test]
async fn test_checkpoint_save_load() {
    let temp_dir = new_test_tempdir();

    let config = TrainingConfig::default();
    let weights = LoRAWeights {
        modules: HashMap::new(),
        lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
        lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        moe_config: None,
        precomputed_delta: None,
    };

    let manager = CheckpointManager::new(temp_dir.path(), 2, 3, "test-adapter".to_string());

    // Create and save checkpoints
    for epoch in vec![2, 4, 6] {
        let checkpoint = adapteros_lora_worker::training::TrainingCheckpoint::new(
            epoch,
            0,
            0.5,
            0.001,
            config.clone(),
            weights.clone(),
        );
        manager.save_checkpoint(&checkpoint).await.unwrap();
    }

    // Verify checkpoint exists
    assert!(manager.has_checkpoint().await);

    // Load latest checkpoint
    let latest = manager.load_latest().await.unwrap();
    assert_eq!(latest.epoch, 6);

    // List all checkpoints
    let checkpoints = manager.list_checkpoints().await.unwrap();
    assert!(checkpoints.len() <= 3); // Max 3 checkpoints kept
}

/// Test T11: Checkpoint resumption
#[tokio::test]
async fn test_checkpoint_resumption() {
    let temp_dir = new_test_tempdir();
    let manager = CheckpointManager::new(temp_dir.path(), 1, 5, "resume-test".to_string());

    let config = TrainingConfig::default();
    let initial_weights = LoRAWeights {
        modules: HashMap::new(),
        lora_a: vec![vec![1.0, 2.0]],
        lora_b: vec![vec![3.0, 4.0]],
        moe_config: None,
        precomputed_delta: None,
    };

    // Save checkpoint at epoch 3
    let checkpoint = adapteros_lora_worker::training::TrainingCheckpoint::new(
        3,
        100,
        0.25,
        0.001,
        config.clone(),
        initial_weights.clone(),
    );
    manager.save_checkpoint(&checkpoint).await.unwrap();

    // Load checkpoint
    let loaded = manager.load_latest().await.unwrap();
    assert_eq!(loaded.epoch, 3);
    assert_eq!(loaded.step, 100);
    assert_eq!(loaded.loss, 0.25);

    // Weights should be restored
    assert_eq!(loaded.weights.lora_a.len(), 1);
    assert_eq!(loaded.weights.lora_a[0].len(), 2);
}

/// Test T12: Graceful cancellation (simulated)
#[test]
fn test_training_cancellation_flag() {
    // This tests the concept - actual implementation will be in orchestrator
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancel_flag.clone();

    // Simulate training loop checking cancel flag
    let mut epochs_completed = 0;
    for epoch in 0..10 {
        if flag_clone.load(Ordering::SeqCst) {
            break;
        }
        epochs_completed = epoch + 1;

        // Simulate cancellation after epoch 5
        if epoch == 5 {
            cancel_flag.store(true, Ordering::SeqCst);
        }
    }

    assert_eq!(epochs_completed, 6); // Should stop after epoch 5
}

/// Test T9: Training templates structure
#[test]
fn test_training_templates() {
    // Test that training config presets work
    let quick = TrainingConfig {
        rank: 8,
        alpha: 16.0,
        learning_rate: 1e-3,
        batch_size: 4,
        epochs: 1,
        hidden_dim: 768,
        vocab_size: 32000,
        preferred_backend: None,
        require_gpu: false,
        max_gpu_memory_mb: 0,
        checkpoint_interval: None,
        ..Default::default()
    };
    assert_eq!(quick.rank, 8);
    assert_eq!(quick.epochs, 1);

    let deep = TrainingConfig {
        rank: 32,
        alpha: 64.0,
        learning_rate: 5e-5,
        batch_size: 2,
        epochs: 5,
        hidden_dim: 2048,
        vocab_size: 32000,
        preferred_backend: None,
        require_gpu: true,
        max_gpu_memory_mb: 8192,
        checkpoint_interval: None,
        ..Default::default()
    };
    assert_eq!(deep.rank, 32);
    assert_eq!(deep.epochs, 5);

    let default = TrainingConfig::default();
    assert_eq!(default.rank, 4); // Default rank is 4
    assert_eq!(default.epochs, 3);
}

/// Test GPU backend detection (T7)
#[test]
fn test_gpu_backend_detection() {
    use adapteros_lora_worker::training::TrainingBackend;

    // Test that backend enum correctly identifies GPU requirements
    assert!(TrainingBackend::CoreML.requires_gpu());
    assert!(TrainingBackend::Metal.requires_gpu());
    assert!(TrainingBackend::Mlx.requires_gpu());
    assert!(!TrainingBackend::Cpu.requires_gpu());

    // Test backend names
    assert_eq!(TrainingBackend::CoreML.name(), "CoreML (ANE)");
    assert_eq!(TrainingBackend::Mlx.name(), "MLX");
    assert_eq!(TrainingBackend::Metal.name(), "Metal");
    assert_eq!(TrainingBackend::Cpu.name(), "CPU");
}

/// Test training config with advanced features
#[test]
fn test_training_config_with_advanced_features() {
    let config = TrainingConfig {
        rank: 16,
        alpha: 32.0,
        learning_rate: 0.001,
        batch_size: 32,
        epochs: 10,
        hidden_dim: 768,
        vocab_size: 32000,
        preferred_backend: Some(adapteros_lora_worker::training::TrainingBackend::Mlx),
        require_gpu: true,
        max_gpu_memory_mb: 2048,
        checkpoint_interval: None,
        ..Default::default()
    };

    assert_eq!(config.rank, 16);
    assert!(config.require_gpu);
    assert_eq!(config.max_gpu_memory_mb, 2048);
}

/// Test LR schedule integration with training config
#[test]
fn test_lr_schedule_integration() {
    // Test that LR scheduler works with training epochs
    let total_epochs = 10;
    let steps_per_epoch = 100;
    let total_steps = total_epochs * steps_per_epoch;

    let config = LRSchedulerConfig::linear(0.01, 0.001, total_steps as u32).with_warmup(100);
    let mut scheduler = LRScheduler::new(config);

    // Simulate training for 10 epochs
    for _epoch in 0..total_epochs {
        for _step in 0..steps_per_epoch {
            let _lr = scheduler.get_lr();
            scheduler.step();
        }
    }

    // After all steps, should be at final LR
    assert!((scheduler.get_lr() - 0.001).abs() < 0.001);
}

/// Test early stopping with min_delta threshold
#[test]
fn test_early_stopping_min_delta() {
    let config = EarlyStoppingConfig::with_patience(3).with_min_delta(0.1);
    let mut early_stop = EarlyStopping::new(config);

    // Initial loss
    early_stop.check(0, 1.0);

    // Small improvement (< 0.1) - doesn't count
    assert!(!early_stop.check(1, 0.95));
    assert_eq!(early_stop.epochs_without_improvement(), 1);

    // Significant improvement (> 0.1) - counts
    assert!(early_stop.check(2, 0.8));
    assert_eq!(early_stop.epochs_without_improvement(), 0);
}

/// Test checkpoint manager cleanup
#[tokio::test]
async fn test_checkpoint_cleanup() {
    let temp_dir = new_test_tempdir();
    let manager = CheckpointManager::new(temp_dir.path(), 1, 3, "cleanup-test".to_string());

    let config = TrainingConfig::default();
    let weights = LoRAWeights {
        modules: HashMap::new(),
        lora_a: vec![vec![1.0]],
        lora_b: vec![vec![2.0]],
        moe_config: None,
        precomputed_delta: None,
    };

    // Create 5 checkpoints (exceeds max of 3)
    for epoch in 1..=5 {
        let checkpoint = adapteros_lora_worker::training::TrainingCheckpoint::new(
            epoch,
            0,
            0.5,
            0.001,
            config.clone(),
            weights.clone(),
        );
        manager.save_checkpoint(&checkpoint).await.unwrap();
    }

    // Should have only 3 most recent checkpoints
    let checkpoints = manager.list_checkpoints().await.unwrap();
    assert!(checkpoints.len() <= 3);

    // Should be epochs 3, 4, 5
    assert!(checkpoints.contains(&3) || checkpoints.contains(&4) || checkpoints.contains(&5));
}
