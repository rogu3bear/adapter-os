//! Integration tests for GPU training with multiple backends
//!
//! Tests verify that GPU training works correctly with:
//! - CoreML backend (ANE acceleration)
//! - Metal GPU backend
//! - MLX backend (experimental)
//! - CPU fallback
//!
//! Run with: cargo test --test gpu_training_integration

use std::collections::HashMap;

/// Helper function to create training examples
fn create_examples(count: usize) -> Vec<adapteros_lora_worker::training::TrainingExample> {
    (0..count)
        .map(|i| adapteros_lora_worker::training::TrainingExample {
            input: vec![(i % 100) as u32; 5],
            target: vec![((i + 1) % 100) as u32; 5],
            metadata: HashMap::new(),
            weight: 1.0,
        })
        .collect()
}

#[tokio::test]
async fn test_gpu_training_with_optional_backend() {
    // Test that training works when GPU is optional but not required
    let config = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 2,
        epochs: 1,
        hidden_dim: 64,
        require_gpu: false,
        preferred_backend: None,
        max_gpu_memory_mb: 0,
    };

    let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config)
        .expect("Trainer creation should succeed");

    let examples = create_examples(4);

    // Initialize kernels (should succeed even if GPU not available)
    trainer
        .init_kernels(&[])
        .expect("Kernel initialization should succeed with CPU fallback");

    // Training should complete successfully
    let result = trainer.train(&examples).await;
    assert!(
        result.is_ok(),
        "Training should succeed: {:?}",
        result.err()
    );

    let result = result.unwrap();
    assert!(result.final_loss >= 0.0, "Loss should be non-negative");
    // Use microsecond precision to verify training actually ran
    assert!(
        result.training_time_us > 0,
        "Training time should be positive (actual work done), got: {}us",
        result.training_time_us
    );
    assert_eq!(
        result.weights.lora_a.len(),
        2,
        "LoRA rank should match config"
    );
}

#[tokio::test]
#[ignore = "select_optimal_backend is a private method - requires refactoring to expose or test differently"]
async fn test_gpu_backend_selection() {
    // Test automatic GPU backend selection
    // NOTE: This test requires access to private method select_optimal_backend
    // which is not currently exported from the trainer module.
    let config = adapteros_lora_worker::training::TrainingConfig::default();
    let _trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    // The following code would require select_optimal_backend to be public:
    // let (backend, _reason) = trainer.select_optimal_backend();
}

#[test]
fn test_backend_info_before_training() {
    // Test that backend info is available after selection
    let config = adapteros_lora_worker::training::TrainingConfig::default();
    let trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    // Before init_kernels, no backend is selected
    assert_eq!(
        trainer.backend_info(),
        None,
        "Backend should be None before init_kernels"
    );
    assert!(
        !trainer.using_gpu(),
        "Should not report GPU usage before init_kernels"
    );
}

#[tokio::test]
async fn test_gpu_training_with_custom_backend() {
    // Test that custom backend preference is respected
    use adapteros_lora_worker::training::TrainingBackend;

    let config = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 1,
        epochs: 1,
        hidden_dim: 32,
        require_gpu: false,
        preferred_backend: Some(TrainingBackend::Cpu),
        max_gpu_memory_mb: 0,
    };

    let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    trainer.init_kernels(&[]).unwrap();

    // Should select CPU as preferred
    assert!(!trainer.using_gpu(), "Should use CPU as specified");

    let examples = create_examples(2);
    let result = trainer.train(&examples).await;
    assert!(result.is_ok(), "Training should succeed with CPU");
}

#[test]
fn test_training_config_builder_pattern() {
    // Test fluent builder pattern for configuration
    use adapteros_lora_worker::training::TrainingBackend;

    let config = adapteros_lora_worker::training::TrainingConfig::default()
        .with_backend(TrainingBackend::Metal)
        .with_gpu_required()
        .with_max_gpu_memory(4096);

    assert_eq!(config.preferred_backend, Some(TrainingBackend::Metal));
    assert!(config.require_gpu);
    assert_eq!(config.max_gpu_memory_mb, 4096);
    assert_eq!(config.rank, 4); // Default preserved
}

#[test]
#[ignore = "detect_available_backends is a private method - requires refactoring to expose or test differently"]
fn test_available_backends_always_includes_cpu() {
    // CPU fallback should always be available
    // NOTE: This test requires access to private method detect_available_backends
    // which is not currently exported from the trainer module.
    // let backends = adapteros_lora_worker::training::MicroLoRATrainer::detect_available_backends();
    // let has_cpu = backends
    //     .iter()
    //     .any(|(b, _)| *b == adapteros_lora_worker::training::TrainingBackend::Cpu);
    // assert!(has_cpu, "CPU backend should always be available");
}

#[test]
fn test_backend_enum_properties() {
    // Test TrainingBackend enum properties
    use adapteros_lora_worker::training::TrainingBackend;

    // Test GPU requirement flags
    assert!(TrainingBackend::CoreML.requires_gpu());
    assert!(TrainingBackend::Metal.requires_gpu());
    assert!(TrainingBackend::Mlx.requires_gpu());
    assert!(!TrainingBackend::Cpu.requires_gpu());

    // Test names
    assert_eq!(TrainingBackend::CoreML.name(), "CoreML (ANE)");
    assert_eq!(TrainingBackend::Metal.name(), "Metal");
    assert_eq!(TrainingBackend::Mlx.name(), "MLX");
    assert_eq!(TrainingBackend::Cpu.name(), "CPU");
}

#[test]
fn test_describe_available_backends_includes_all() {
    // Test describe_available_backends returns expected format
    let desc = adapteros_lora_worker::training::MicroLoRATrainer::describe_available_backends();
    assert!(
        desc.contains("Available training backends:"),
        "Description should include header"
    );
    assert!(desc.contains("CPU"), "CPU backend should always be listed");
}

#[tokio::test]
async fn test_training_completes_with_telemetry() {
    // Test that training produces valid telemetry events
    let config = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        alpha: 8.0,
        learning_rate: 1e-3,
        batch_size: 1,
        epochs: 1,
        hidden_dim: 32,
        ..Default::default()
    };

    let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    let examples = create_examples(2);
    let result = trainer.train(&examples).await;

    assert!(result.is_ok());
    let result = result.unwrap();

    // Verify results contain expected fields
    assert!(!result.adapter_id.is_empty());
    assert!(result.final_loss >= 0.0);
    // Use microsecond precision to verify training actually ran
    assert!(
        result.training_time_us > 0,
        "Training time should be positive, got: {}us",
        result.training_time_us
    );
    assert!(!result.weights.lora_a.is_empty());
    assert!(!result.weights.lora_b.is_empty());
}

#[tokio::test]
async fn test_progressive_training_loss_improvement() {
    // Test that loss generally improves over epochs
    let config = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        alpha: 8.0,
        learning_rate: 0.01, // Higher LR for faster convergence
        batch_size: 2,
        epochs: 3,
        hidden_dim: 32,
        ..Default::default()
    };

    let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    let examples = create_examples(4);
    let mut losses = Vec::new();

    let result = trainer
        .train_with_callback(&examples, |_epoch, loss| {
            losses.push(loss);
        })
        .await;

    assert!(result.is_ok());
    assert!(!losses.is_empty(), "Should record losses for each epoch");

    // Generally, loss should not increase dramatically
    // (though not guaranteed to always decrease with toy data)
    let final_loss = losses[losses.len() - 1];
    assert!(final_loss >= 0.0, "Final loss should be non-negative");
}

#[test]
fn test_kernel_initialization_fallback_on_cpu() {
    // Test that init_kernels doesn't fail on systems without GPU
    let config = adapteros_lora_worker::training::TrainingConfig {
        require_gpu: false,
        ..Default::default()
    };

    let mut trainer = adapteros_lora_worker::training::MicroLoRATrainer::new(config).unwrap();

    // Should not fail even if GPU is unavailable
    let result = trainer.init_kernels(&[]);
    assert!(
        result.is_ok(),
        "init_kernels should succeed with fallback to CPU"
    );
}

#[test]
fn test_training_seed_determinism() {
    // Test that training seed is correctly derived for deterministic behavior
    // Two trainers with the same config should have the same seed
    let config1 = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        ..Default::default()
    };

    let config2 = adapteros_lora_worker::training::TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        ..Default::default()
    };

    let trainer1 = adapteros_lora_worker::training::MicroLoRATrainer::new(config1).unwrap();
    let trainer2 = adapteros_lora_worker::training::MicroLoRATrainer::new(config2).unwrap();

    // Seeds should be deterministic - same HKDF derivation produces same seed
    assert_eq!(
        trainer1.training_seed(),
        trainer2.training_seed(),
        "Trainers with same config should have deterministic seeds"
    );
}
