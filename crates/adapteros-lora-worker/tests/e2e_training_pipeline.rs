//! End-to-end training pipeline tests (CPU-only)
//!
//! Verifies the full training pipeline works: config → examples → train → quantize → package → verify
//! These tests run on CPU backend only (no GPU required) and prove the training code actually executes.

use adapteros_lora_worker::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingBackend, TrainingConfig,
    TrainingExample,
};
use std::collections::HashMap;
use tempfile::TempDir;

fn create_examples(n: usize) -> Vec<TrainingExample> {
    (0..n)
        .map(|i| TrainingExample {
            input: vec![(i % 100) as u32; 5],
            target: vec![((i + 1) % 100) as u32; 5],
            metadata: HashMap::new(),
            weight: 1.0,
        })
        .collect()
}

/// Test that minimal training executes and produces valid results
#[tokio::test]
async fn test_e2e_minimal_training() {
    let config = TrainingConfig {
        rank: 2,
        alpha: 4.0,
        learning_rate: 0.01,
        batch_size: 2,
        epochs: 2,
        hidden_dim: 32,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        max_gpu_memory_mb: 0,
    };

    let mut trainer = MicroLoRATrainer::new(config).expect("trainer creation should succeed");
    let examples = create_examples(4);
    let result = trainer
        .train(&examples)
        .await
        .expect("training should succeed");

    assert!(result.final_loss >= 0.0, "Loss should be non-negative");
    assert!(
        result.training_time_us > 0,
        "Training time should be positive (actual work done), got: {}us",
        result.training_time_us
    );
    assert_eq!(
        result.weights.lora_a.len(),
        2,
        "LoRA A weights should match rank"
    );
}

/// Test full pipeline: train → quantize → package → verify output exists
#[tokio::test]
async fn test_e2e_full_pipeline() {
    let temp = TempDir::new().expect("temp dir creation");

    let config = TrainingConfig {
        rank: 4,
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        max_gpu_memory_mb: 0,
    };

    // Step 1: Train
    let mut trainer = MicroLoRATrainer::new(config.clone()).expect("trainer creation");
    let examples = create_examples(8);
    let result = trainer.train(&examples).await.expect("training");

    // Step 2: Quantize
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    assert!(
        !quantized.lora_a_q15.is_empty(),
        "Quantized LoRA A weights should exist"
    );

    // Step 3: Package
    let packager = AdapterPackager::new(temp.path());
    let packaged = packager
        .package_aos("e2e_test", &quantized, &config, "base-model")
        .await
        .expect("packaging should succeed");

    // Step 4: Verify output
    assert!(
        packaged.weights_path.exists(),
        "Packaged .aos file should exist at {:?}",
        packaged.weights_path
    );
    let size = tokio::fs::metadata(&packaged.weights_path)
        .await
        .expect("should read file metadata")
        .len();
    assert!(
        size > 0,
        "Packaged adapter should have content (got {} bytes)",
        size
    );
}

/// Test that training is deterministic: same config + data = same result
#[tokio::test]
async fn test_e2e_deterministic_training() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        learning_rate: 0.01,
        batch_size: 2,
        epochs: 1,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        ..Default::default()
    };

    let examples = create_examples(4);

    // Train twice with identical config
    let mut trainer1 = MicroLoRATrainer::new(config.clone()).expect("trainer 1 creation");
    let result1 = trainer1.train(&examples).await.expect("training 1");

    let mut trainer2 = MicroLoRATrainer::new(config.clone()).expect("trainer 2 creation");
    let result2 = trainer2.train(&examples).await.expect("training 2");

    // Results should be identical (deterministic)
    assert!(
        (result1.final_loss - result2.final_loss).abs() < 1e-6,
        "Training should be deterministic: loss1={} vs loss2={}",
        result1.final_loss,
        result2.final_loss
    );
}
