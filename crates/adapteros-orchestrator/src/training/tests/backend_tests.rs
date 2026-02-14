//! Backend selection and GPU training tests

#![allow(clippy::field_reassign_with_default)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use adapteros_lora_worker::backend_factory::BackendCapabilities;
use adapteros_lora_worker::training::TrainingBackend as WorkerTrainingBackend;
use tempfile::TempDir;
use tokio::sync::RwLock;

use crate::test_support::TestEnvGuard;
use crate::training::config::map_preferred_backend;
use crate::training::execution::run_training_job;
use crate::training::job::{
    DataLineageMode, TrainingBackendKind, TrainingConfig, TrainingJob, TrainingJobStatus,
};

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

fn cpu_only_config() -> TrainingConfig {
    let mut cfg = TrainingConfig::default();
    cfg.epochs = 1;
    cfg.batch_size = 1;
    cfg.learning_rate = 0.0001;
    cfg.preferred_backend = None;
    cfg.require_gpu = false;
    cfg.max_gpu_memory_mb = None;
    cfg
}

fn gpu_required_config() -> TrainingConfig {
    let mut cfg = cpu_only_config();
    cfg.require_gpu = true;
    cfg
}

fn no_package_actions() -> Option<String> {
    Some(
        serde_json::json!({
            "package": false,
            "register": false,
            "create_stack": false,
            "activate_stack": false
        })
        .to_string(),
    )
}

#[test]
fn map_preferred_backend_coreml_does_not_inject_default_fallback() {
    let mapped = map_preferred_backend(Some(TrainingBackendKind::CoreML), None);
    assert_eq!(mapped.preferred, Some(WorkerTrainingBackend::CoreML));
    assert_eq!(mapped.coreml_fallback, None);
}

#[test]
fn map_preferred_backend_coreml_respects_explicit_fallback() {
    let mapped = map_preferred_backend(
        Some(TrainingBackendKind::CoreML),
        Some(TrainingBackendKind::Metal),
    );
    assert_eq!(mapped.preferred, Some(WorkerTrainingBackend::CoreML));
    assert_eq!(mapped.coreml_fallback, Some(WorkerTrainingBackend::Metal));
}

#[tokio::test]
async fn cpu_training_succeeds_without_gpu_init() {
    let _env = TestEnvGuard::new();
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
    let jobs = Arc::new(RwLock::new(HashMap::new()));
    let job_id = "cpu-job".to_string();
    let config = cpu_only_config();
    let job = TrainingJob::new(job_id.clone(), "adapter-cpu".to_string(), config.clone());
    jobs.write().await.insert(job_id.clone(), job);

    let result = run_training_job(
        jobs.clone(),
        job_id.clone(),
        "adapter-cpu".to_string(),
        config,
        None,
        false,
        DataLineageMode::Synthetic,
        None,
        None,
        None,
        None,
        None,
        no_package_actions(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .await;

    assert!(result.is_ok(), "CPU training should succeed");
    let jobs_guard = jobs.read().await;
    let finished = jobs_guard.get(&job_id).unwrap();
    assert_eq!(finished.status, TrainingJobStatus::Completed);
    assert_eq!(finished.require_gpu, Some(false));
    assert_eq!(
        finished
            .backend
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase(),
        "cpu"
    );
    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
}

#[tokio::test]
async fn coreml_preference_records_fallback_reason() {
    let _env = TestEnvGuard::new();
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
    let jobs = Arc::new(RwLock::new(HashMap::new()));
    let job_id = "coreml-pref-job".to_string();
    let mut config = cpu_only_config();
    config.preferred_backend = Some(TrainingBackendKind::CoreML);
    config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);
    let job = TrainingJob::new(
        job_id.clone(),
        "adapter-coreml-pref".to_string(),
        config.clone(),
    );
    jobs.write().await.insert(job_id.clone(), job);

    let result = run_training_job(
        jobs.clone(),
        job_id.clone(),
        "adapter-coreml-pref".to_string(),
        config,
        None,
        false,
        DataLineageMode::Synthetic,
        None,
        None,
        None,
        None,
        None,
        no_package_actions(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .await;

    assert!(
        result.is_ok(),
        "CoreML request should fall back deterministically"
    );
    let jobs_guard = jobs.read().await;
    let finished = jobs_guard.get(&job_id).unwrap();
    let reason = finished.backend_reason.clone().unwrap_or_default();
    assert!(
        reason.contains("coreml"),
        "expected backend_reason to mention CoreML fallback, got: {reason}"
    );
    assert_eq!(
        finished
            .backend
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase(),
        "cpu"
    );
    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
}

#[tokio::test]
async fn gpu_optional_falls_back_when_init_fails() {
    let _env = TestEnvGuard::new();
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "metal");
    let temp_model = new_test_tempdir();
    let model_path = temp_model.path().join("model.safetensors");
    std::fs::write(&model_path, b"not-a-real-model").unwrap();
    std::env::set_var("AOS_MODEL_PATH", temp_model.path());

    let jobs = Arc::new(RwLock::new(HashMap::new()));
    let job_id = "gpu-fallback-job".to_string();
    let mut config = cpu_only_config();
    config.preferred_backend = Some(TrainingBackendKind::Metal);
    let job = TrainingJob::new(
        job_id.clone(),
        "adapter-gpu-fallback".to_string(),
        config.clone(),
    );
    jobs.write().await.insert(job_id.clone(), job);

    let result = run_training_job(
        jobs.clone(),
        job_id.clone(),
        "adapter-gpu-fallback".to_string(),
        config,
        None,
        false,
        DataLineageMode::Synthetic,
        None,
        None,
        None,
        None,
        None,
        no_package_actions(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .await;

    assert!(
        result.is_ok(),
        "Optional GPU init should fall back to CPU even if GPU init fails"
    );
    let jobs_guard = jobs.read().await;
    let finished = jobs_guard.get(&job_id).unwrap();
    assert_eq!(finished.status, TrainingJobStatus::Completed);
    assert_eq!(
        finished
            .backend
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase(),
        "cpu"
    );

    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    std::env::remove_var("AOS_MODEL_PATH");
}

#[tokio::test]
async fn gpu_required_errors_when_unavailable() {
    let _env = TestEnvGuard::new();
    std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
    let jobs = Arc::new(RwLock::new(HashMap::new()));
    let job_id = "gpu-required-job".to_string();
    let mut config = gpu_required_config();
    config.epochs = 1;
    let job = TrainingJob::new(
        job_id.clone(),
        "adapter-gpu-required".to_string(),
        config.clone(),
    );
    jobs.write().await.insert(job_id.clone(), job);

    let result = run_training_job(
        jobs.clone(),
        job_id.clone(),
        "adapter-gpu-required".to_string(),
        config,
        None,
        false,
        DataLineageMode::Synthetic,
        None,
        None,
        None,
        None,
        None,
        no_package_actions(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
    )
    .await;

    assert!(result.is_err(), "GPU-required job should error without GPU");
    let jobs_guard = jobs.read().await;
    let failed = jobs_guard.get(&job_id).unwrap();
    assert_eq!(failed.status, TrainingJobStatus::Failed);
    assert_eq!(failed.require_gpu, Some(true));
    std::env::remove_var("AOS_FORCE_GPU_BACKEND");
}

// ---------------------------------------------------------------------------
// CPU-proxy detection: `use_gpu_backward` derivation logic
//
// The orchestrator determines `use_gpu_backward` via:
//   runtime_caps.has_mlx || matches!(preferred_backend.preferred, Some(Mlx))
//
// These tests verify that equation against all meaningful input combinations
// without invoking the full training pipeline (detect_capabilities is
// hardware-dependent and cannot be mocked).
// ---------------------------------------------------------------------------

/// Replicates the orchestrator's `use_gpu_backward` derivation so we can
/// test it in isolation from `detect_capabilities()` side-effects.
fn derive_use_gpu_backward(
    caps: &BackendCapabilities,
    preferred: &Option<WorkerTrainingBackend>,
) -> bool {
    caps.has_mlx || matches!(preferred, Some(WorkerTrainingBackend::Mlx))
}

#[test]
fn cpu_proxy_no_mlx_no_preference_yields_cpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: false,
        ..Default::default()
    };
    let mapped = map_preferred_backend(None, None);
    assert!(
        !derive_use_gpu_backward(&caps, &mapped.preferred),
        "no MLX + no preference should produce use_gpu_backward=false (CPU proxy)"
    );
}

#[test]
fn cpu_proxy_no_mlx_metal_preference_yields_cpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: false,
        has_metal: true,
        ..Default::default()
    };
    let mapped = map_preferred_backend(Some(TrainingBackendKind::Metal), None);
    assert!(
        !derive_use_gpu_backward(&caps, &mapped.preferred),
        "no MLX + Metal preference should produce use_gpu_backward=false"
    );
}

#[test]
fn cpu_proxy_no_mlx_coreml_preference_yields_cpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: false,
        has_coreml: true,
        ..Default::default()
    };
    let mapped = map_preferred_backend(Some(TrainingBackendKind::CoreML), None);
    assert!(
        !derive_use_gpu_backward(&caps, &mapped.preferred),
        "no MLX + CoreML preference should produce use_gpu_backward=false"
    );
}

#[test]
fn mlx_detected_yields_gpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: true,
        ..Default::default()
    };
    // Even without an explicit preference, has_mlx alone triggers GPU backward.
    let mapped = map_preferred_backend(None, None);
    assert!(
        derive_use_gpu_backward(&caps, &mapped.preferred),
        "has_mlx=true should produce use_gpu_backward=true regardless of preference"
    );
}

#[test]
fn mlx_preference_without_runtime_mlx_yields_gpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: false,
        ..Default::default()
    };
    let mapped = map_preferred_backend(Some(TrainingBackendKind::Mlx), None);
    assert!(
        derive_use_gpu_backward(&caps, &mapped.preferred),
        "MLX preference should produce use_gpu_backward=true even without runtime MLX"
    );
}

#[test]
fn mlx_detected_and_mlx_preference_yields_gpu_backward() {
    let caps = BackendCapabilities {
        has_mlx: true,
        ..Default::default()
    };
    let mapped = map_preferred_backend(Some(TrainingBackendKind::Mlx), None);
    assert!(
        derive_use_gpu_backward(&caps, &mapped.preferred),
        "both MLX detected and preferred should produce use_gpu_backward=true"
    );
}

#[test]
fn cpu_proxy_config_does_not_require_base_model() {
    use adapteros_lora_worker::training::TrainingConfig as WorkerTrainingConfig;

    let mut cfg = WorkerTrainingConfig::default();
    cfg.use_gpu_backward = false;
    cfg.validation_split = 0.0;
    cfg.multi_module_training = false;
    assert!(
        !cfg.requires_base_model(),
        "CPU-proxy config (use_gpu_backward=false) should not require a base model"
    );
}

#[test]
fn gpu_backward_config_requires_base_model() {
    use adapteros_lora_worker::training::TrainingConfig as WorkerTrainingConfig;

    let mut cfg = WorkerTrainingConfig::default();
    cfg.use_gpu_backward = true;
    assert!(
        cfg.requires_base_model(),
        "GPU backward config should require a base model"
    );
}
