use super::*;
use crate::training::coreml_pipeline;
use adapteros_core::B3Hash;
use adapteros_platform::common::PlatformUtils;
use blake3;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::HashMap;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

fn make_prepared(example: &TrainingExample, hidden_dim: usize) -> coreml_pipeline::PreparedExample {
    let mut scaled_input: Vec<f32> = example.input.iter().map(|t| *t as f32).collect();
    if scaled_input.len() < hidden_dim {
        scaled_input.resize(hidden_dim, 0.0);
    } else {
        scaled_input.truncate(hidden_dim);
    }

    coreml_pipeline::PreparedExample {
        input_tokens: example.input.clone(),
        target_tokens: example.target.clone(),
        padded_input: example.input.clone(),
        padded_target: example.target.clone(),
        scaled_input,
        input_mask: vec![1; example.input.len()],
        target_mask: vec![1; example.target.len()],
        input_len: example.input.len(),
        target_len: example.target.len(),
        metadata: example.metadata.clone(),
        weight: example.weight,
    }
}

#[test]
fn test_training_backend_enum() {
    assert!(TrainingBackend::CoreML.requires_gpu());
    assert!(TrainingBackend::Metal.requires_gpu());
    assert!(TrainingBackend::Mlx.requires_gpu());
    assert!(!TrainingBackend::Cpu.requires_gpu());

    assert_eq!(TrainingBackend::CoreML.name(), "CoreML (ANE)");
    assert_eq!(TrainingBackend::Cpu.name(), "CPU");
}

#[test]
fn test_training_config_with_gpu_required() {
    let config = TrainingConfig::default().with_gpu_required();
    assert!(config.require_gpu);
    assert_eq!(config.rank, 4); // Default values preserved
}

#[test]
fn test_training_config_with_backend() {
    let config = TrainingConfig::default().with_backend(TrainingBackend::Metal);
    assert_eq!(config.preferred_backend, Some(TrainingBackend::Metal));
}

#[test]
fn test_backend_kind_conversion() {
    assert_eq!(
        TrainingBackend::try_from(BackendKind::Metal).unwrap(),
        TrainingBackend::Metal
    );
    assert_eq!(
        TrainingBackend::try_from(BackendKind::Mlx).unwrap(),
        TrainingBackend::Mlx
    );
    assert_eq!(
        TrainingBackend::try_from(BackendKind::CPU).unwrap(),
        TrainingBackend::Cpu
    );
    assert!(TrainingBackend::try_from(BackendKind::Auto).is_err());
    assert_eq!(
        BackendKind::from(TrainingBackend::CoreML),
        BackendKind::CoreML
    );
}

#[test]
fn test_training_config_with_max_gpu_memory() {
    let config = TrainingConfig::default().with_max_gpu_memory(2048);
    assert_eq!(config.max_gpu_memory_mb, 2048);
}

#[test]
fn test_backend_candidates_priority_order_includes_coreml_first() {
    let mut trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
    let availability = BackendAvailability {
        coreml: true,
        mlx: true,
        metal: true,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::CoreML);
    assert_eq!(candidates[1], TrainingBackend::Mlx);
    assert_eq!(candidates[2], TrainingBackend::Metal);
    assert_eq!(candidates.last(), Some(&TrainingBackend::Cpu));
    assert!(candidates.contains(&TrainingBackend::CoreML));
}

#[test]
fn test_backend_candidates_preferred_fallback() {
    let config = TrainingConfig {
        preferred_backend: Some(TrainingBackend::Metal),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: false,
        mlx: true,
        metal: false,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::Mlx);
    assert!(candidates.contains(&TrainingBackend::Cpu));
}

#[test]
fn test_backend_policy_coreml_only_fails_without_coreml() {
    let config = TrainingConfig {
        backend_policy: Some(TrainingBackendPolicy::CoremlOnly),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: false,
        mlx: true,
        metal: true,
    };

    let result = trainer.build_backend_candidates(&availability);
    assert!(result.is_err());
}

#[test]
fn test_backend_policy_coreml_else_fallback_uses_fallback() {
    let config = TrainingConfig {
        backend_policy: Some(TrainingBackendPolicy::CoremlElseFallback),
        coreml_fallback_backend: Some(TrainingBackend::Mlx),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: false,
        mlx: true,
        metal: false,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::Mlx);
}

#[test]
fn test_coreml_preference_uses_coreml_when_available() {
    let config = TrainingConfig {
        preferred_backend: Some(TrainingBackend::CoreML),
        coreml_fallback_backend: Some(TrainingBackend::Mlx),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: true,
        mlx: true,
        metal: true,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::CoreML);
    assert!(
        !trainer
            .backend_reason()
            .unwrap_or_default()
            .contains("coreml_unavailable"),
        "coreml available should not emit unavailable reason"
    );
}

#[test]
fn test_coreml_preference_falls_back_when_unavailable_and_fallback_provided() {
    let config = TrainingConfig {
        preferred_backend: Some(TrainingBackend::CoreML),
        coreml_fallback_backend: Some(TrainingBackend::Mlx),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: false,
        mlx: true,
        metal: true,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::Mlx);
    assert!(!candidates.contains(&TrainingBackend::CoreML));
    let reason = trainer.backend_reason().unwrap_or_default();
    assert!(
        reason.contains("coreml_unavailable"),
        "expected reason to mention CoreML unavailable, got: {reason}"
    );
    assert!(
        reason.contains("mlx"),
        "expected reason to include fallback backend tag, got: {reason}"
    );
}

#[test]
fn test_backend_candidates_require_gpu_error_when_none() {
    let config = TrainingConfig {
        require_gpu: true,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let availability = BackendAvailability {
        coreml: false,
        mlx: false,
        metal: false,
    };

    assert!(trainer.build_backend_candidates(&availability).is_err());
}

#[test]
fn test_coreml_preference_without_gpu_uses_cpu_and_reason() {
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
    let config = TrainingConfig {
        preferred_backend: Some(TrainingBackend::CoreML),
        coreml_fallback_backend: Some(TrainingBackend::Mlx),
        require_gpu: false,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    trainer
        .init_kernels(&[])
        .expect("CPU fallback should succeed when GPU is optional");

    assert_eq!(trainer.backend_info(), Some("CPU"));
    let reason = trainer.backend_reason().unwrap_or_default();
    assert!(
        reason.contains("coreml_unavailable"),
        "expected backend reason to mention CoreML fallback, got: {reason}"
    );
    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
}

#[test]
fn test_coreml_latency_metrics_tracking() {
    let trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
    trainer.record_coreml_forward_latency(10);
    trainer.record_coreml_forward_latency(30);
    let metrics = trainer.get_performance_metrics();
    assert_eq!(metrics.coreml_forward_samples, 2);
    assert_eq!(metrics.coreml_forward_total_us, 40);
    assert_eq!(metrics.coreml_forward_mean_us, Some(20.0));
    assert_eq!(metrics.coreml_forward_p95_us, Some(30));
}

#[test]
fn test_available_backends_detection() {
    let backends = MicroLoRATrainer::detect_available_backends();
    // At minimum, CPU should always be available
    assert!(!backends.is_empty());
    let has_cpu = backends.iter().any(|(b, _)| *b == TrainingBackend::Cpu);
    assert!(has_cpu, "CPU backend should always be available");
}

#[test]
fn test_describe_available_backends() {
    let desc = MicroLoRATrainer::describe_available_backends();
    assert!(desc.contains("Available training backends:"));
    assert!(desc.contains("CPU")); // At minimum, CPU should be listed
}

#[test]
fn test_initialize_weights() {
    let config = TrainingConfig {
        rank: 4,
        hidden_dim: 768,
        ..Default::default()
    };
    let trainer = MicroLoRATrainer::new(config).unwrap();
    let weights = trainer.initialize_weights_deterministic().unwrap();

    assert_eq!(weights.lora_a.len(), 4);
    assert_eq!(weights.lora_a[0].len(), 768);
    assert_eq!(weights.lora_b.len(), 768);
    assert_eq!(weights.lora_b[0].len(), 4);
}

#[test]
fn test_training_updates_only_lora_weights() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 6,
        vocab_size: 16,
        batch_size: 1,
        epochs: 1,
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new(config.clone()).unwrap();
    let mut weights = trainer.initialize_weights_deterministic().unwrap();
    let initial_weights = weights.clone();

    let base_snapshot = vec![1.0f32, 2.0, 3.0, 4.0];

    let examples = vec![TrainingExample {
        input: vec![1, 2, 3, 4],
        target: vec![4, 3, 2, 1],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    let dataset = trainer
        .prepare_dataset_for_training(&examples)
        .expect("dataset prep");

    // Run a single epoch; only LoRA weights should change.
    let mut base_hash_bytes = Vec::new();
    for f in &base_snapshot {
        base_hash_bytes.extend_from_slice(&f.to_le_bytes());
    }
    let base_hash_before = B3Hash::hash(&base_hash_bytes);

    let loss = trainer
        .train_epoch_deterministic(&mut weights, &dataset, 0)
        .unwrap();

    assert!(loss.is_finite());
    assert_ne!(
        weights.lora_a, initial_weights.lora_a,
        "LoRA A should change during training"
    );
    assert_ne!(
        weights.lora_b, initial_weights.lora_b,
        "LoRA B should change during training"
    );

    // Base model buffers are not part of the optimizer set and must remain untouched.
    assert_eq!(base_snapshot, vec![1.0, 2.0, 3.0, 4.0]);
    let mut base_hash_bytes_after = Vec::new();
    for f in &base_snapshot {
        base_hash_bytes_after.extend_from_slice(&f.to_le_bytes());
    }
    let base_hash_after = B3Hash::hash(&base_hash_bytes_after);
    assert_eq!(
        base_hash_before, base_hash_after,
        "Base checksum must remain stable during training"
    );

    // Ensure deterministic RNG usage remains stable between runs
    let mut trainer_second = MicroLoRATrainer::new(config).unwrap();
    let mut weights_second = trainer_second.initialize_weights_deterministic().unwrap();
    let dataset_second = trainer_second
        .prepare_dataset_for_training(&examples)
        .expect("dataset prep second");
    trainer_second
        .train_epoch_deterministic(&mut weights_second, &dataset_second, 0)
        .unwrap();
    assert_eq!(
        weights.lora_a, weights_second.lora_a,
        "Deterministic training should yield identical LoRA A updates"
    );
    assert_eq!(
        weights.lora_b, weights_second.lora_b,
        "Deterministic training should yield identical LoRA B updates"
    );
}

#[test]
fn test_forward_pass() {
    let config = TrainingConfig {
        rank: 4,
        hidden_dim: 768,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();
    let weights = trainer.initialize_weights_deterministic().unwrap();

    let examples = vec![TrainingExample {
        input: vec![1, 2, 3, 4, 5],
        target: vec![1, 2, 3, 4, 5],
        metadata: HashMap::new(),
        weight: 1.0,
    }];
    let dataset = trainer
        .prepare_dataset_for_training(&examples)
        .expect("prepare dataset");
    let (output, hidden) = trainer.forward(&weights, &dataset.examples[0]).unwrap();

    assert_eq!(output.len(), 768);
    assert_eq!(hidden.len(), 768);
}

#[test]
fn test_trainer_gpu_status_initially_cpu() {
    let config = TrainingConfig::default();
    let trainer = MicroLoRATrainer::new(config).unwrap();

    // Before init_kernels, no backend is selected
    assert_eq!(trainer.backend_info(), None);
    assert!(!trainer.using_gpu());
}

#[tokio::test]
async fn test_train_small() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 64,
        batch_size: 2,
        epochs: 1,
        learning_rate: 0.01,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3],
            target: vec![4, 5, 6],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![7, 8, 9],
            target: vec![10, 11, 12],
            metadata: HashMap::new(),
            weight: 1.0,
        },
    ];

    let result = trainer.train(&examples).await.unwrap();
    assert!(result.final_loss >= 0.0);
    assert!(
        result.training_time_us > 0,
        "Training time should be positive (actual work done), got: {}us",
        result.training_time_us
    );
    assert_eq!(result.weights.lora_a.len(), 2);
    assert!(
        result.effective_batch_size.unwrap_or_default() > 0,
        "effective batch size should be captured"
    );
}

#[test]
fn test_backward_only_updates_lora_weights() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 2,
        vocab_size: 4,
        batch_size: 1,
        epochs: 1,
        ..Default::default()
    };
    let trainer = MicroLoRATrainer::new(config).unwrap();
    let mut weights = trainer.initialize_weights_deterministic().unwrap();
    let original_weights = weights.clone();

    let example = TrainingExample {
        input: vec![1, 2],
        target: vec![1, 2, 3, 4],
        metadata: HashMap::new(),
        weight: 1.0,
    };
    let prepared = make_prepared(&example, trainer.config.hidden_dim);
    let (output, hidden) = trainer.forward(&weights, &prepared).unwrap();
    let target = example.target.clone();

    let mut rng = ChaCha20Rng::from_seed(derive_seed(
        &B3Hash::hash(b"test_backward_only_updates_lora_weights"),
        "deterministic-noise",
    ));
    let base_stub = vec![42.0f32, 43.0];
    let base_before = base_stub.clone();

    trainer
        .backward_and_update_deterministic(&mut weights, &hidden, &output, &target, 0.1, &mut rng)
        .unwrap();

    // LoRA weights should change
    assert_ne!(weights.lora_a, original_weights.lora_a);
    // Base model buffer (not part of trainer) remains unchanged
    assert_eq!(base_stub, base_before);
}

#[tokio::test]
async fn test_train_with_cpu_backend_optional() {
    // Training should work without GPU when GPU is optional
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        batch_size: 1,
        epochs: 1,
        learning_rate: 0.01,
        require_gpu: false,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    let examples = vec![TrainingExample {
        input: vec![1, 2],
        target: vec![3, 4],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    // init_kernels should complete successfully (CPU path)
    trainer
        .init_kernels(&[])
        .expect("CPU kernel init should succeed");

    // Training should complete without errors
    let result = trainer.train(&examples).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().weights.lora_a.len(), 2);
}

#[test]
fn test_backend_selection_priority() {
    let config = TrainingConfig {
        preferred_backend: Some(TrainingBackend::Metal),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    let availability = BackendAvailability {
        coreml: false,
        mlx: false,
        metal: true,
    };
    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(candidates[0], TrainingBackend::Metal);
}

#[test]
fn test_device_policy_prefers_coreml_first() {
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "all");
    let mut trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
    let availability = BackendAvailability {
        coreml: true,
        mlx: true,
        metal: true,
    };

    let candidates = trainer.build_backend_candidates(&availability).unwrap();
    assert_eq!(
        candidates[0],
        TrainingBackend::CoreML,
        "CoreML should be first when available"
    );
    assert!(candidates.contains(&TrainingBackend::Cpu));
    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
}

// ========================================================================
// Checkpoint Integration Tests
// ========================================================================

#[test]
fn test_checkpoint_interval_config() {
    let config = TrainingConfig::default().with_checkpoint_interval(5);
    assert_eq!(config.checkpoint_interval, Some(5));
}

#[test]
fn test_checkpoint_interval_default_none() {
    let config = TrainingConfig::default();
    assert_eq!(config.checkpoint_interval, None);
}

#[tokio::test]
async fn test_enable_checkpointing() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        epochs: 10,
        checkpoint_interval: Some(2),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    // Create temp dir for checkpoints
    let temp_dir = new_test_tempdir();

    // Enable checkpointing
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    // Verify checkpoint manager is configured
    assert!(trainer.checkpoint_manager.is_some());
}

#[tokio::test]
async fn test_train_with_checkpointing() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        batch_size: 1,
        epochs: 4,
        learning_rate: 0.01,
        checkpoint_interval: Some(1), // Save every epoch to ensure checkpoints exist in tests
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    // Create temp dir for checkpoints
    let temp_dir = new_test_tempdir();
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    let examples = vec![TrainingExample {
        input: vec![1, 2],
        target: vec![3, 4],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    // Train - checkpoints should be saved each epoch
    let result = trainer.train(&examples).await;
    assert!(result.is_ok());

    // Verify checkpoint files were created
    let checkpoint_files: Vec<_> = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "ckpt"))
        .collect();

    // Should have at least the latest checkpoint
    assert!(
        !checkpoint_files.is_empty(),
        "Expected checkpoint files to be created"
    );
}

#[tokio::test]
async fn test_try_resume_from_checkpoint_no_checkpoint() {
    let config = TrainingConfig {
        checkpoint_interval: Some(5),
        ..Default::default()
    };
    let trainer = MicroLoRATrainer::new(config).unwrap();

    // No checkpoint manager configured, should return None
    let resume_state = trainer.try_resume_from_checkpoint().await;
    assert!(resume_state.is_none());
}

#[tokio::test]
async fn test_try_resume_from_checkpoint_with_checkpoint() {
    use crate::training::checkpoint::TrainingCheckpoint;

    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 32,
        checkpoint_interval: Some(2),
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config.clone()).unwrap();

    // Create temp dir and save a checkpoint
    let temp_dir = new_test_tempdir();
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    // Manually create a checkpoint
    let weights = LoRAWeights {
        lora_a: vec![vec![1.0, 2.0]],
        lora_b: vec![vec![3.0, 4.0]],
        moe_config: None,
        precomputed_delta: None,
    };
    let checkpoint = TrainingCheckpoint::new(
        5, // epoch 5
        0, 0.5, // loss
        0.001, config, weights,
    );

    // Save checkpoint using the manager
    let manager = trainer.checkpoint_manager.as_ref().unwrap();
    manager.save_checkpoint(&checkpoint).await.unwrap();

    // Now try to resume
    let resume_state = trainer.try_resume_from_checkpoint().await;
    assert!(resume_state.is_some());

    let (epoch, _weights, _best_loss) = resume_state.unwrap();
    assert_eq!(epoch, 5);
}

#[tokio::test]
async fn test_adapter_only_training_updates_lora_only() {
    fn hash_weights(weights: &LoRAWeights) -> blake3::Hash {
        let mut bytes = Vec::new();
        for row in &weights.lora_a {
            for v in row {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        for row in &weights.lora_b {
            for v in row {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        blake3::hash(&bytes)
    }

    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 16,
        batch_size: 1,
        epochs: 1,
        learning_rate: 0.05,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new(config).unwrap();

    let examples = vec![TrainingExample {
        input: vec![1, 2, 3, 4],
        target: vec![5, 6, 7, 8],
        metadata: HashMap::new(),
        weight: 1.0,
    }];

    // Snapshot initial LoRA weights and base (input-derived) hidden state.
    let initial_weights = trainer.initialize_weights_deterministic().unwrap();
    let initial_hash = hash_weights(&initial_weights);
    let prepared = make_prepared(&examples[0], trainer.config.hidden_dim);
    let (_out_before, base_hidden_before) = trainer.forward(&initial_weights, &prepared).unwrap();

    // Run a tiny training step.
    let result = trainer.train(&examples).await.unwrap();
    let updated_hash = hash_weights(&result.weights);

    // Adapter-only guarantee: LoRA weights must change, base path stays identical.
    assert_ne!(
        initial_hash, updated_hash,
        "LoRA weights should update during training"
    );

    let prepared_after = make_prepared(&examples[0], trainer.config.hidden_dim);
    let (_out_after, base_hidden_after) =
        trainer.forward(&result.weights, &prepared_after).unwrap();
    assert_eq!(
        base_hidden_before, base_hidden_after,
        "Base (input-derived) hidden path must remain unchanged; only LoRA deltas mutate"
    );
}
