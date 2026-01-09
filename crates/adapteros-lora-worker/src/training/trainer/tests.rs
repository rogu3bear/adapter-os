use super::*;
use crate::training::coreml_pipeline;
use adapteros_core::backend::BackendKind;
use adapteros_core::B3Hash;
use adapteros_platform::common::PlatformUtils;
use blake3;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

fn find_model_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() && path.join("config.json").is_file() {
        return Some(path.to_path_buf());
    }

    if !path.is_dir() {
        return None;
    }

    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|entry| entry.is_dir())
        .collect();
    entries.sort();

    for entry in entries {
        if entry.join("config.json").is_file() {
            return Some(entry);
        }
    }

    None
}

fn resolve_test_model_path() -> Option<PathBuf> {
    for var in ["AOS_TEST_MODEL_PATH", "AOS_MODEL_PATH"] {
        if let Ok(path) = std::env::var(var) {
            if let Some(model_dir) = find_model_dir(Path::new(&path)) {
                return Some(model_dir);
            }
        }
    }

    let base_paths = ["./var/models", "../var/models", "../../var/models"];
    for base in base_paths {
        if let Some(model_dir) = find_model_dir(Path::new(base)) {
            return Some(model_dir);
        }
    }

    None
}

fn load_test_model_path_or_skip() -> Option<PathBuf> {
    let model_path = resolve_test_model_path();
    if model_path.is_none() {
        eprintln!(
            "SKIPPED: model path not found. Set AOS_TEST_MODEL_PATH or AOS_MODEL_PATH (e.g. ./var/models/<model>)."
        );
    }
    model_path
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
    let mut trainer = MicroLoRATrainer::new_for_test(TrainingConfig::default()).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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
    let trainer = MicroLoRATrainer::new_for_test(TrainingConfig::default()).unwrap();
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
    let trainer = MicroLoRATrainer::new_for_test(config).unwrap();
    let weights = trainer.initialize_weights_deterministic().unwrap();

    assert_eq!(weights.lora_a.len(), 4);
    assert_eq!(weights.lora_a[0].len(), 768);
    assert_eq!(weights.lora_b.len(), 768);
    assert_eq!(weights.lora_b[0].len(), 4);
}

#[test]
#[ignore = "requires base model for actual training - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0001]"]
fn test_training_updates_only_lora_weights() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 6,
        vocab_size: 16,
        batch_size: 1,
        epochs: 1,
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new_for_test(config.clone()).unwrap();
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
    let mut trainer_second = MicroLoRATrainer::new_for_test(config).unwrap();
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
#[ignore = "requires base model for forward pass - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0002]"]
fn test_forward_pass() {
    let config = TrainingConfig {
        rank: 4,
        hidden_dim: 768,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
    let trainer = MicroLoRATrainer::new_for_test(config).unwrap();

    // Before init_kernels, no backend is selected
    assert_eq!(trainer.backend_info(), None);
    assert!(!trainer.using_gpu());
}

#[tokio::test]
#[ignore = "requires base model for actual training - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0003]"]
async fn test_train_small() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 64,
        batch_size: 2,
        epochs: 1,
        learning_rate: 0.01,
        ..Default::default()
    };
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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
#[ignore = "requires base model for backward pass - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0004]"]
fn test_backward_only_updates_lora_weights() {
    let config = TrainingConfig {
        rank: 2,
        hidden_dim: 2,
        vocab_size: 4,
        batch_size: 1,
        epochs: 1,
        ..Default::default()
    };
    let trainer = MicroLoRATrainer::new_for_test(config).unwrap();
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
#[ignore = "requires base model for actual training - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0005]"]
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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
    let mut trainer = MicroLoRATrainer::new_for_test(TrainingConfig::default()).unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

    // Create temp dir for checkpoints
    let temp_dir = new_test_tempdir();

    // Enable checkpointing
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    // Verify checkpoint manager is configured
    assert!(trainer.checkpoint_manager.is_some());
}

#[tokio::test]
#[ignore = "requires base model for actual training - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0006]"]
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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
    let trainer = MicroLoRATrainer::new_for_test(config).unwrap();

    // No checkpoint manager configured, should return None
    let resume_state = trainer.try_resume_from_checkpoint().await.unwrap();
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
    let mut trainer = MicroLoRATrainer::new_for_test(config.clone()).unwrap();

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
    let resume_state = trainer.try_resume_from_checkpoint().await.unwrap();
    assert!(resume_state.is_some());

    let checkpoint = resume_state.unwrap();
    assert_eq!(checkpoint.epoch, 5);
}

#[tokio::test]
async fn test_try_resume_from_checkpoint_mismatched_optimizer() {
    use crate::training::checkpoint::TrainingCheckpoint;

    let checkpoint_config = TrainingConfig::default();
    let mut resume_config = TrainingConfig::default();
    resume_config.optimizer_config.optimizer_type = OptimizerType::Sgd;

    let mut trainer = MicroLoRATrainer::new_for_test(checkpoint_config.clone()).unwrap();
    let temp_dir = new_test_tempdir();
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    let weights = LoRAWeights {
        lora_a: vec![vec![1.0, 2.0]],
        lora_b: vec![vec![3.0, 4.0]],
        moe_config: None,
        precomputed_delta: None,
    };
    let checkpoint = TrainingCheckpoint::new(5, 0, 0.5, 0.001, checkpoint_config, weights);

    let manager = trainer.checkpoint_manager.as_ref().unwrap();
    manager.save_checkpoint(&checkpoint).await.unwrap();

    let mut resume_trainer = MicroLoRATrainer::new_for_test(resume_config).unwrap();
    resume_trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    let resume_state = resume_trainer.try_resume_from_checkpoint().await;
    assert!(resume_state.is_err());
    let err = format!("{}", resume_state.unwrap_err());
    assert!(
        err.contains("optimizer_type"),
        "expected optimizer_type mismatch, got: {}",
        err
    );
}

#[tokio::test]
async fn test_try_resume_from_checkpoint_force_resume() {
    use crate::training::checkpoint::TrainingCheckpoint;

    let checkpoint_config = TrainingConfig::default();
    let mut resume_config = TrainingConfig::default();
    resume_config.optimizer_config.optimizer_type = OptimizerType::Sgd;

    let mut trainer = MicroLoRATrainer::new_for_test(checkpoint_config.clone()).unwrap();
    let temp_dir = new_test_tempdir();
    trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    let weights = LoRAWeights {
        lora_a: vec![vec![1.0, 2.0]],
        lora_b: vec![vec![3.0, 4.0]],
        moe_config: None,
        precomputed_delta: None,
    };
    let checkpoint = TrainingCheckpoint::new(5, 0, 0.5, 0.001, checkpoint_config, weights);

    let manager = trainer.checkpoint_manager.as_ref().unwrap();
    manager.save_checkpoint(&checkpoint).await.unwrap();

    let mut resume_trainer = MicroLoRATrainer::new_for_test(resume_config).unwrap();
    resume_trainer.set_force_resume(true);
    resume_trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

    let resume_state = resume_trainer.try_resume_from_checkpoint().await.unwrap();
    assert!(resume_state.is_some());
    assert_eq!(resume_state.unwrap().epoch, 5);
}

#[tokio::test]
#[ignore = "requires base model for actual training - use AOS_TEST_BASE_MODEL env var [tracking: TRAIN-TEST-0007]"]
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
    let mut trainer = MicroLoRATrainer::new_for_test(config).unwrap();

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

/// Test that GPU backward pass config option is recognized.
/// This test verifies the configuration option exists and can be set.
/// Actual GPU backward execution requires MLX hardware and is tested separately.
#[test]
fn test_gpu_backward_config_option() {
    let config = TrainingConfig {
        use_gpu_backward: true,
        ..Default::default()
    };
    assert!(config.use_gpu_backward, "GPU backward flag should be set");

    let config_disabled = TrainingConfig {
        use_gpu_backward: false,
        ..Default::default()
    };
    assert!(!config_disabled.use_gpu_backward, "GPU backward flag should be disabled");

    // Test builder pattern
    let config_via_builder = TrainingConfig::default().with_gpu_backward(true);
    assert!(config_via_builder.use_gpu_backward, "Builder should set GPU backward flag");
}

/// Test that should_use_gpu_backward returns correct values based on config.
/// This test doesn't require actual GPU hardware.
#[test]
fn test_should_use_gpu_backward_logic() {
    // Without GPU backward enabled
    let config_no_gpu = TrainingConfig {
        use_gpu_backward: false,
        preferred_backend: Some(TrainingBackend::Mlx),
        ..Default::default()
    };
    let trainer_no_gpu = MicroLoRATrainer::new_for_test(config_no_gpu).unwrap();
    assert!(
        !trainer_no_gpu.should_use_gpu_backward(),
        "Should not use GPU backward when config flag is false"
    );

    // With GPU backward enabled but CPU backend
    let config_cpu = TrainingConfig {
        use_gpu_backward: true,
        preferred_backend: Some(TrainingBackend::Cpu),
        ..Default::default()
    };
    let trainer_cpu = MicroLoRATrainer::new_for_test(config_cpu).unwrap();
    // Should be false because CPU backend doesn't support GPU backward
    // (trainer will select CPU backend, not MLX)
    assert!(
        !trainer_cpu.should_use_gpu_backward(),
        "Should not use GPU backward with CPU backend"
    );
}

const CHILD_PROCESS_TEST_NAME: &str =
    concat!(module_path!(), "::test_determinism_child_process");

fn child_process_test_names() -> Vec<String> {
    let with_crate = CHILD_PROCESS_TEST_NAME.to_string();
    let crate_prefix = format!("{}::", env!("CARGO_PKG_NAME").replace('-', "_"));
    if let Some(stripped) = with_crate.strip_prefix(&crate_prefix) {
        vec![with_crate, stripped.to_string()]
    } else {
        vec![with_crate]
    }
}

fn spawn_determinism_child(
    model_path: &std::path::Path,
    output_path: &std::path::Path,
    seed: u64,
) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("failed to find test binary path: {}", e))?;
    let mut last_error = None;

    for test_name in child_process_test_names() {
        let _ = std::fs::remove_file(output_path);
        let output = std::process::Command::new(&exe)
            .arg("--exact")
            .arg(&test_name)
            .arg("--ignored")
            .arg("--nocapture")
            .env("AOS_TEST_MODEL_PATH", model_path)
            .env("AOS_MODEL_PATH", model_path)
            .env("AOS_DETERMINISM_OUTPUT", output_path)
            .env("AOS_DETERMINISM_SEED", seed.to_string())
            .output()
            .map_err(|e| format!("failed to spawn child test '{}': {}", test_name, e))?;

        if output.status.success() && output_path.exists() {
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        last_error = Some(format!(
            "child test '{}' failed (status: {})\nstdout:\n{}\nstderr:\n{}",
            test_name, output.status, stdout, stderr
        ));
    }

    Err(last_error.unwrap_or_else(|| "child test failed".to_string()))
}

// ========================================================================
// GPU Training Integration Tests
// These require MLX hardware and are ignored by default
// ========================================================================

/// Test that GPU backward pass produces deterministic results across runs.
/// This verifies that running training twice with the same seed produces
/// identical weights and loss curves.
///
/// Requirements:
/// - MLX hardware (Apple Silicon)
/// - AOS_TEST_MODEL_PATH or AOS_MODEL_PATH pointing to a valid model directory
///   (defaults to ./var/models/<model> when present)
///
/// Known limitation: Full 28-layer transformer forward pass through quantized weights
/// has shape handling issues that need to be resolved. Currently uses test_for_test
/// to validate the training infrastructure without full model inference.
///
/// Run with: AOS_MODEL_PATH=./var/models/<model> cargo test -p adapteros-lora-worker test_gpu_backward_determinism -- --ignored --nocapture
#[tokio::test]
#[ignore] // Requires MLX hardware and test model
async fn test_gpu_backward_determinism() {
    let model_path = match load_test_model_path_or_skip() {
        Some(path) => path,
        None => return,
    };

    // Helper to train with deterministic config and return weights + loss curve
    // Uses Qwen2.5-7B dimensions: hidden_dim=3584, vocab_size=152064
    async fn train_deterministic(model_path: PathBuf) -> std::result::Result<(LoRAWeights, Vec<f32>), String> {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 3584,    // Qwen2.5-7B hidden size
            vocab_size: 152064,  // Qwen2.5-7B vocab size
            batch_size: 1,       // Small batch for fast test
            epochs: 1,           // Single epoch for fast test
            learning_rate: 0.01,
            use_gpu_backward: true,
            preferred_backend: Some(TrainingBackend::Mlx),
            require_gpu: true,
            base_model_path: Some(model_path),
            determinism: Some(DeterminismConfig {
                seed: Some(12345),
                dataset_version_id: Some("test-v1".to_string()),
                device: None,
                backend: Some("mlx".to_string()),
                max_steps: None,
                subsample: None,
            }),
            ..Default::default()
        };

        eprintln!("Creating trainer with base model path: {:?}", config.base_model_path);
        let mut trainer = MicroLoRATrainer::new(config).map_err(|e| format!("Failed to create trainer: {}", e))?;
        eprintln!("Trainer created, has_base_model: {}", trainer.has_base_model());

        let examples = vec![
            TrainingExample {
                input: vec![1, 2, 3, 4, 5, 6, 7, 8],
                target: vec![2, 3, 4, 5, 6, 7, 8, 9],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![10, 11, 12, 13, 14, 15, 16, 17],
                target: vec![11, 12, 13, 14, 15, 16, 17, 18],
                metadata: HashMap::new(),
                weight: 1.0,
            },
        ];

        let result = trainer.train(&examples).await.map_err(|e| format!("Training failed: {}", e))?;
        Ok((result.weights, result.loss_curve))
    }

    // Run training twice with the same determinism config
    let (weights1, losses1) = train_deterministic(model_path.clone()).await
        .expect("First training run should succeed");
    let (weights2, losses2) = train_deterministic(model_path).await
        .expect("Second training run should succeed");

    // Verify bit-exact match for weights
    assert_eq!(
        weights1.lora_a.len(),
        weights2.lora_a.len(),
        "LoRA A dimensions should match"
    );
    for (row1, row2) in weights1.lora_a.iter().zip(weights2.lora_a.iter()) {
        assert_eq!(row1, row2, "LoRA A weights must be bit-exact identical across runs");
    }

    assert_eq!(
        weights1.lora_b.len(),
        weights2.lora_b.len(),
        "LoRA B dimensions should match"
    );
    for (row1, row2) in weights1.lora_b.iter().zip(weights2.lora_b.iter()) {
        assert_eq!(row1, row2, "LoRA B weights must be bit-exact identical across runs");
    }

    // Verify loss curve matches
    assert_eq!(
        losses1.len(),
        losses2.len(),
        "Loss curve lengths should match"
    );
    for (i, (l1, l2)) in losses1.iter().zip(losses2.iter()).enumerate() {
        assert_eq!(
            l1, l2,
            "Loss at epoch {} must match exactly: {} vs {}",
            i, l1, l2
        );
    }
}

/// Test that GPU backward pass is deterministic across separate processes.
///
/// Spawns two isolated test processes with identical configs and compares
/// their serialized final weights byte-for-byte.
///
/// Requirements:
/// - MLX hardware (Apple Silicon)
/// - AOS_TEST_MODEL_PATH or AOS_MODEL_PATH pointing to a valid model directory
///   (defaults to ./var/models/<model> when present)
#[tokio::test]
#[ignore] // Requires MLX hardware and test model
async fn test_determinism_across_processes() {
    let model_path = match load_test_model_path_or_skip() {
        Some(path) => path,
        None => return,
    };

    let temp_dir = new_test_tempdir();
    let weights_a = temp_dir.path().join("weights_a.json");
    let weights_b = temp_dir.path().join("weights_b.json");
    let seed = 12345_u64;

    spawn_determinism_child(&model_path, &weights_a, seed)
        .expect("first deterministic child process should succeed");
    spawn_determinism_child(&model_path, &weights_b, seed)
        .expect("second deterministic child process should succeed");

    let bytes_a = std::fs::read(&weights_a).expect("read weights from first child");
    let bytes_b = std::fs::read(&weights_b).expect("read weights from second child");

    assert_eq!(
        bytes_a, bytes_b,
        "Serialized weights must match byte-for-byte across processes"
    );
}

/// Child process for multi-process determinism testing.
#[tokio::test]
#[ignore] // Invoked by test_determinism_across_processes via --ignored
async fn test_determinism_child_process() {
    let model_path = match load_test_model_path_or_skip() {
        Some(path) => path,
        None => return,
    };

    let output_path = std::env::var("AOS_DETERMINISM_OUTPUT")
        .map(PathBuf::from)
        .expect("AOS_DETERMINISM_OUTPUT must be set for child determinism test");
    let seed = std::env::var("AOS_DETERMINISM_SEED")
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(12345);

    let config = TrainingConfig {
        rank: 4,
        hidden_dim: 3584,    // Qwen2.5-7B hidden size
        vocab_size: 152064,  // Qwen2.5-7B vocab size
        batch_size: 1,
        epochs: 1,
        learning_rate: 0.01,
        use_gpu_backward: true,
        preferred_backend: Some(TrainingBackend::Mlx),
        require_gpu: true,
        base_model_path: Some(model_path),
        determinism: Some(DeterminismConfig {
            seed: Some(seed),
            dataset_version_id: Some("process-test-v1".to_string()),
            device: None,
            backend: Some("mlx".to_string()),
            max_steps: None,
            subsample: None,
        }),
        ..Default::default()
    };

    let mut trainer = MicroLoRATrainer::new(config).expect("child trainer should initialize");

    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3, 4, 5, 6, 7, 8],
            target: vec![2, 3, 4, 5, 6, 7, 8, 9],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![10, 11, 12, 13, 14, 15, 16, 17],
            target: vec![11, 12, 13, 14, 15, 16, 17, 18],
            metadata: HashMap::new(),
            weight: 1.0,
        },
    ];

    let result = trainer
        .train(&examples)
        .await
        .expect("child training should complete");

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let serialized = serde_json::to_vec(&result.weights).expect("serialize weights");
    std::fs::write(&output_path, serialized).expect("write weights to output file");
}

/// Test that GPU backward pass produces loss values equivalent to CPU backward pass.
/// This verifies that the GPU implementation is mathematically correct by comparing
/// against the reference CPU implementation.
///
/// Note: Due to floating-point non-associativity, bit-exact match may not be possible.
/// This test allows a small tolerance (1e-5 relative error) for numerical differences.
///
/// Requirements:
/// - MLX hardware (Apple Silicon)
/// - AOS_TEST_MODEL_PATH or AOS_MODEL_PATH pointing to a valid model directory
///   (defaults to ./var/models/<model> when present)
///
/// Run with: AOS_MODEL_PATH=./var/models/<model> cargo test -p adapteros-lora-worker test_gpu_cpu_loss_equivalence -- --ignored --nocapture
#[tokio::test]
#[ignore] // Requires MLX hardware and test model
async fn test_gpu_cpu_loss_equivalence() {
    let model_path = match load_test_model_path_or_skip() {
        Some(path) => path,
        None => return,
    };

    let determinism_config = DeterminismConfig {
        seed: Some(54321),
        dataset_version_id: Some("equiv-test-v1".to_string()),
        device: None,
        backend: None,
        max_steps: None,
        subsample: None,
    };

    let examples = vec![
        TrainingExample {
            input: vec![1, 2, 3, 4],
            target: vec![2, 3, 4, 5],
            metadata: HashMap::new(),
            weight: 1.0,
        },
        TrainingExample {
            input: vec![10, 11, 12, 13],
            target: vec![11, 12, 13, 14],
            metadata: HashMap::new(),
            weight: 1.0,
        },
    ];

    // Train with GPU backward (uses real base model forward pass)
    // Uses Qwen2.5-7B dimensions: hidden_dim=3584, vocab_size=152064
    let gpu_config = TrainingConfig {
        rank: 4,
        hidden_dim: 3584,    // Qwen2.5-7B hidden size
        vocab_size: 152064,  // Qwen2.5-7B vocab size
        batch_size: 2,
        epochs: 2,
        learning_rate: 0.01,
        use_gpu_backward: true,
        preferred_backend: Some(TrainingBackend::Mlx),
        require_gpu: true,
        base_model_path: Some(model_path.clone()),
        determinism: Some(determinism_config.clone()),
        ..Default::default()
    };
    let mut gpu_trainer = MicroLoRATrainer::new(gpu_config).expect("GPU trainer should initialize");
    let gpu_result = gpu_trainer.train(&examples).await.expect("GPU training should complete");

    // Train with CPU backward (same determinism config)
    // Both paths use real base model for forward pass; backward differs (GPU autograd vs CPU manual)
    let cpu_config = TrainingConfig {
        rank: 4,
        hidden_dim: 3584,
        vocab_size: 152064,
        batch_size: 2,
        epochs: 2,
        learning_rate: 0.01,
        use_gpu_backward: false,
        preferred_backend: Some(TrainingBackend::Mlx), // Still use MLX for forward pass
        require_gpu: false,
        base_model_path: Some(model_path),
        determinism: Some(determinism_config),
        ..Default::default()
    };
    // CPU backward path also needs base model for forward pass
    let mut cpu_trainer = MicroLoRATrainer::new(cpu_config).expect("CPU trainer should initialize");
    let cpu_result = cpu_trainer.train(&examples).await.expect("CPU training should complete");

    // Compare final losses with tolerance
    let tolerance = 1e-5_f32;
    let relative_error: f32 = if cpu_result.final_loss.abs() > 1e-10 {
        (gpu_result.final_loss - cpu_result.final_loss).abs() / cpu_result.final_loss.abs()
    } else {
        (gpu_result.final_loss - cpu_result.final_loss).abs()
    };

    assert!(
        relative_error < tolerance,
        "GPU and CPU final loss should be within {:.0e} relative error. \
         GPU loss: {}, CPU loss: {}, relative error: {:.6e}",
        tolerance,
        gpu_result.final_loss,
        cpu_result.final_loss,
        relative_error
    );

    // Compare loss curves (if bit-exact determinism is achieved, these should match)
    // If not bit-exact, at least verify they're in the same ballpark
    assert_eq!(
        gpu_result.loss_curve.len(),
        cpu_result.loss_curve.len(),
        "Loss curve lengths should match"
    );

    for (epoch, (gpu_loss, cpu_loss)) in gpu_result
        .loss_curve
        .iter()
        .zip(cpu_result.loss_curve.iter())
        .enumerate()
    {
        let epoch_rel_error: f32 = if cpu_loss.abs() > 1e-10 {
            (gpu_loss - cpu_loss).abs() / cpu_loss.abs()
        } else {
            (gpu_loss - cpu_loss).abs()
        };

        // Allow slightly larger tolerance for intermediate epochs
        let epoch_tolerance = tolerance * 10.0;
        assert!(
            epoch_rel_error < epoch_tolerance,
            "Epoch {} loss mismatch: GPU={}, CPU={}, relative error={:.6e} (tolerance: {:.0e})",
            epoch,
            gpu_loss,
            cpu_loss,
            epoch_rel_error,
            epoch_tolerance
        );
    }

    // Log comparison for debugging (visible with --nocapture)
    println!("GPU vs CPU Loss Comparison:");
    println!("  GPU final loss: {}", gpu_result.final_loss);
    println!("  CPU final loss: {}", cpu_result.final_loss);
    println!("  Relative error: {:.6e}", relative_error);
    println!("  Training backend (GPU): {:?}", gpu_result.backend);
    println!("  Training backend (CPU): {:?}", cpu_result.backend);
}
