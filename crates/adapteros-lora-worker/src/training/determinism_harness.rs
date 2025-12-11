//! Deterministic drift harness utilities shared by CLI and promotion gates.
//!
//! Provides deterministic slicing, backend runners, and drift metrics that
//! operate on existing `MicroLoRATrainer` APIs.

use super::{
    DatasetSubsample, DeterminismConfig, LoRAWeights, MicroLoRATrainer, TrainingBackend,
    TrainingConfig, TrainingExample, TrainingResult,
};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Hyperparameters used by the drift/determinism harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessHyperparams {
    pub rank: usize,
    pub alpha: f32,
    pub learning_rate: f32,
    pub batch_size: usize,
    pub hidden_dim: usize,
    pub vocab_size: usize,
}

impl Default for HarnessHyperparams {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 4,
            hidden_dim: 768,
            vocab_size: 32000,
        }
    }
}

/// Per-backend harness run result.
#[derive(Debug, Clone)]
pub struct BackendRun {
    pub backend: TrainingBackend,
    pub result: TrainingResult,
}

/// Drift metrics across backends.
#[derive(Debug, Clone)]
pub struct DriftMetrics {
    pub backend: String,
    pub weight_l_inf: f32,
    pub loss_l_inf: f32,
    pub cosine_similarity: Option<f32>,
}

/// Deterministically slice examples by hashed content + seed, with optional windowing.
pub fn deterministic_slice(
    mut examples: Vec<TrainingExample>,
    seed: u64,
    slice_size: Option<usize>,
    subsample: Option<DatasetSubsample>,
) -> Vec<TrainingExample> {
    examples.sort_by_key(|ex| {
        let mut buf = Vec::with_capacity(ex.input.len() * 4 + ex.target.len() * 4 + 8);
        buf.extend_from_slice(&seed.to_le_bytes());
        for t in &ex.input {
            buf.extend_from_slice(&t.to_le_bytes());
        }
        for t in &ex.target {
            buf.extend_from_slice(&t.to_le_bytes());
        }
        blake3::hash(&buf).as_bytes().to_owned()
    });

    if let Some(window) = subsample {
        let start = window.offset.min(examples.len());
        let end = start.saturating_add(window.length).min(examples.len());
        examples = examples[start..end].to_vec();
    }

    if let Some(limit) = slice_size {
        examples.truncate(limit.min(examples.len()));
    }
    examples
}

/// Build a harness-scoped training config with deterministic fields populated.
pub fn build_harness_training_config(
    hyperparams: HarnessHyperparams,
    backend: TrainingBackend,
    steps: usize,
    seed: u64,
    dataset_version_id: Option<String>,
    device: Option<String>,
    subsample: Option<DatasetSubsample>,
) -> TrainingConfig {
    let epochs = steps.max(1);
    let mut cfg = TrainingConfig {
        rank: hyperparams.rank,
        alpha: hyperparams.alpha,
        learning_rate: hyperparams.learning_rate,
        batch_size: hyperparams.batch_size,
        epochs,
        hidden_dim: hyperparams.hidden_dim,
        vocab_size: hyperparams.vocab_size,
        coreml_placement: None,
        preferred_backend: Some(backend),
        backend_policy: None,
        coreml_fallback_backend: None,
        require_gpu: matches!(
            backend,
            TrainingBackend::CoreML | TrainingBackend::Metal | TrainingBackend::Mlx
        ),
        max_gpu_memory_mb: 0,
        max_tokens_per_batch: None,
        device_policy: None,
        checkpoint_interval: None,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        determinism: Some(DeterminismConfig {
            seed: Some(seed),
            dataset_version_id,
            device,
            backend: Some(backend.tag().to_string()),
            max_steps: Some(epochs),
            subsample,
        }),
    };

    // Enforce deterministic GPU fallback policy explicitly.
    cfg.require_gpu = cfg.require_gpu && backend.requires_gpu();
    cfg
}

/// Run a deterministic harness pass on the given backend and dataset slice.
pub async fn run_backend_with_examples(
    hyperparams: HarnessHyperparams,
    backend: TrainingBackend,
    steps: usize,
    seed: u64,
    dataset_version_id: Option<String>,
    device: Option<String>,
    subsample: Option<DatasetSubsample>,
    examples: &[TrainingExample],
) -> Result<BackendRun> {
    let training_cfg = build_harness_training_config(
        hyperparams,
        backend,
        steps,
        seed,
        dataset_version_id,
        device,
        subsample,
    );

    info!(
        backend = backend.tag(),
        steps = steps,
        dataset_version_id = training_cfg
            .determinism
            .as_ref()
            .and_then(|d| d.dataset_version_id.clone())
            .unwrap_or_else(|| "unknown".into()),
        "Starting deterministic harness run"
    );

    let mut trainer = MicroLoRATrainer::new(training_cfg)?;
    let result = trainer.train(examples).await?;
    Ok(BackendRun { backend, result })
}

/// Compute drift metrics between two training results.
pub fn compute_drift(reference: &TrainingResult, candidate: &TrainingResult) -> DriftMetrics {
    let reference_weights = flatten_weights(&reference.weights);
    let candidate_weights = flatten_weights(&candidate.weights);
    let weight_l_inf = max_abs_diff(&reference_weights, &candidate_weights);
    let cosine_similarity = cosine_sim(&reference_weights, &candidate_weights);

    let loss_l_inf = max_abs_diff(&reference.loss_curve, &candidate.loss_curve);

    DriftMetrics {
        backend: candidate
            .backend
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        weight_l_inf,
        loss_l_inf,
        cosine_similarity,
    }
}

fn flatten_weights(weights: &LoRAWeights) -> Vec<f32> {
    let mut out = Vec::new();
    for row in &weights.lora_a {
        out.extend_from_slice(row);
    }
    for row in &weights.lora_b {
        out.extend_from_slice(row);
    }
    out
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    let mut max_diff = 0.0_f32;
    let len = a.len().min(b.len());
    for i in 0..len {
        max_diff = max_diff.max((a[i] - b[i]).abs());
    }
    if a.len() > len {
        for v in &a[len..] {
            max_diff = max_diff.max(v.abs());
        }
    }
    if b.len() > len {
        for v in &b[len..] {
            max_diff = max_diff.max(v.abs());
        }
    }
    max_diff
}

fn cosine_sim(a: &[f32], b: &[f32]) -> Option<f32> {
    let len = a.len().min(b.len());
    if len == 0 {
        return None;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return None;
    }
    Some(dot / (na.sqrt() * nb.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn simple_result(loss: f32, backend: &str) -> TrainingResult {
        TrainingResult {
            adapter_id: backend.into(),
            final_loss: loss,
            training_time_us: 1,
            weights: LoRAWeights {
                lora_a: vec![vec![loss]],
                lora_b: vec![vec![loss * 2.0]],
            },
            cancelled: false,
            stopped_at_epoch: Some(1),
            examples_processed: Some(1),
            tokens_processed: Some(1),
            tokens_per_sec: 0.0,
            examples_per_sec: 0.0,
            backend: Some(backend.into()),
            backend_device: None,
            using_gpu: false,
            effective_batch_size: Some(1),
            loss_curve: vec![loss],
            determinism_seed: Some(1),
            determinism_backend: Some(backend.into()),
            determinism_device: None,
            dataset_version_id: None,
        }
    }

    #[test]
    fn deterministic_slice_is_stable_with_subsample() {
        let examples = vec![
            TrainingExample {
                input: vec![1, 2],
                target: vec![3],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![2, 3],
                target: vec![4],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![3, 4],
                target: vec![5],
                metadata: HashMap::new(),
                weight: 1.0,
            },
        ];

        let window = DatasetSubsample {
            offset: 1,
            length: 2,
        };
        let first = deterministic_slice(examples.clone(), 42, Some(2), Some(window.clone()));
        let second = deterministic_slice(examples, 42, Some(2), Some(window));
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].input, second[0].input);
        assert_eq!(first[1].target, second[1].target);
    }

    #[test]
    fn drift_is_zero_for_identical_runs() {
        let reference = simple_result(0.5, "cpu");
        let candidate = simple_result(0.5, "cpu");

        let metrics = compute_drift(&reference, &candidate);
        assert_eq!(metrics.weight_l_inf, 0.0);
        assert_eq!(metrics.loss_l_inf, 0.0);
        assert_eq!(metrics.cosine_similarity, Some(1.0));
    }

    #[test]
    fn drift_is_positive_for_divergent_runs() {
        let reference = simple_result(0.5, "cpu");
        let candidate = simple_result(0.6, "coreml");

        let metrics = compute_drift(&reference, &candidate);
        assert!(metrics.weight_l_inf > 0.0);
        assert!(metrics.loss_l_inf > 0.0);
    }
}
