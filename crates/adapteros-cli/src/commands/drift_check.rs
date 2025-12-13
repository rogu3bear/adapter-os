//! Deterministic drift harness for cross-backend training comparison.

use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::DatasetSubsample;
use adapteros_lora_worker::training::{
    compute_drift, deterministic_slice, run_backend_with_examples, DriftMetrics,
    HarnessHyperparams, TrainingBackend, TrainingExample, TrainingResult,
};
use adapteros_manifest::{AssuranceTier, ManifestV3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// CLI-facing arguments passed from `main.rs`
#[derive(Debug, Clone)]
pub struct DriftCheckArgs {
    pub config: PathBuf,
    pub dataset_override: Option<PathBuf>,
    pub manifest_override: Option<PathBuf>,
    pub backends_override: Vec<String>,
    pub reference_backend: Option<String>,
}

/// Harness configuration (file-backed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismDriftConfig {
    pub seed: u64,
    pub dataset: PathBuf,
    #[serde(default)]
    pub dataset_version_id: Option<String>,
    #[serde(default)]
    pub manifest_path: Option<PathBuf>,
    #[serde(default)]
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub backends: Vec<String>,
    #[serde(default)]
    pub reference_backend: Option<String>,
    #[serde(default = "default_steps")]
    pub steps: usize,
    #[serde(default)]
    pub slice_size: Option<usize>,
    #[serde(default)]
    pub slice_offset: Option<usize>,
    #[serde(default)]
    pub hyperparams: HarnessHyperparams,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub assurance_tier: Option<String>,
}

fn default_steps() -> usize {
    1
}

#[derive(Deserialize)]
struct TrainingData {
    examples: Vec<TrainingExampleData>,
}

#[derive(Deserialize)]
struct TrainingExampleData {
    input: Vec<u32>,
    target: Vec<u32>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftDecision {
    RecordOnly,
    ReviewRequired,
    Block,
}

pub async fn drift_check(args: DriftCheckArgs) -> Result<i32> {
    let mut cfg = load_config(&args.config)?;
    if let Some(ds) = args.dataset_override {
        cfg.dataset = ds;
    }
    if let Some(manifest) = args.manifest_override {
        cfg.manifest_path = Some(manifest);
    }
    if !args.backends_override.is_empty() {
        cfg.backends = args.backends_override;
    }
    if let Some(rb) = args.reference_backend {
        cfg.reference_backend = Some(rb);
    }
    let assurance_tier =
        parse_assurance_tier(cfg.assurance_tier.as_deref()).unwrap_or(AssuranceTier::Standard);

    let backends = resolve_backends(&cfg.backends)?;
    let reference_tag = choose_reference_backend(&backends, cfg.reference_backend.clone())
        .unwrap_or_else(|| {
            backends
                .first()
                .map(|b| b.tag().to_string())
                .unwrap_or_default()
        });

    info!(
        seed = cfg.seed,
        steps = cfg.steps,
        dataset_version_id = cfg
            .dataset_version_id
            .as_deref()
            .unwrap_or("unset"),
        slice_size = cfg.slice_size.unwrap_or(0),
        slice_offset = cfg.slice_offset.unwrap_or(0),
        backends = ?backends
            .iter()
            .map(|b| b.tag())
            .collect::<Vec<_>>(),
        "Determinism harness configuration resolved"
    );

    let dataset = load_dataset(&cfg.dataset)?;
    let ds_len = dataset.len();
    let subsample = cfg.slice_offset.map(|offset| DatasetSubsample {
        offset,
        length: cfg.slice_size.unwrap_or(ds_len),
    });
    let sliced = deterministic_slice(dataset, cfg.seed, cfg.slice_size, subsample.clone());

    info!(
        "Running deterministic drift harness over {} examples across {} backend(s)",
        sliced.len(),
        backends.len()
    );

    let mut runs = Vec::with_capacity(backends.len());
    for backend in &backends {
        let run = run_backend_with_examples(
            cfg.hyperparams.clone(),
            *backend,
            cfg.steps,
            cfg.seed,
            cfg.dataset_version_id.clone(),
            cfg.device.clone(),
            subsample.clone(),
            &sliced,
        )
        .await?;
        info!(
            backend = run.backend.tag(),
            loss = run.result.final_loss,
            "Backend run completed"
        );
        runs.push(run);
    }

    if runs.is_empty() {
        return Err(AosError::Validation("No backends requested".into()));
    }

    let reference = runs
        .iter()
        .find(|r| r.backend.tag() == reference_tag)
        .unwrap_or(&runs[0])
        .result
        .clone();

    let mut drift_summaries = Vec::new();
    let mut overall_decision = DriftDecision::RecordOnly;
    for run in &runs {
        if run.result.adapter_id == reference.adapter_id && run.backend.tag() == reference_tag {
            continue;
        }
        let metrics = compute_drift(&reference, &run.result);
        info!(
            backend = run.backend.tag(),
            weight_l_inf = metrics.weight_l_inf,
            loss_l_inf = metrics.loss_l_inf,
            "Computed drift metrics"
        );
        let decision = evaluate_drift(&metrics, assurance_tier);
        match decision {
            DriftDecision::RecordOnly => {
                info!(
                    backend = run.backend.tag(),
                    "Drift recorded (record-only tier)"
                );
            }
            DriftDecision::ReviewRequired => {
                warn!(
                    backend = run.backend.tag(),
                    weight_l_inf = metrics.weight_l_inf,
                    loss_l_inf = metrics.loss_l_inf,
                    "Drift exceeds standard thresholds; review required"
                );
            }
            DriftDecision::Block => {
                warn!(
                    backend = run.backend.tag(),
                    weight_l_inf = metrics.weight_l_inf,
                    loss_l_inf = metrics.loss_l_inf,
                    "Drift exceeds high tier thresholds; blocking"
                );
            }
        }
        overall_decision = merge_decision(overall_decision, decision);
        drift_summaries.push(metrics);
    }

    if let Err(e) = persist_manifest(
        &cfg,
        &reference_tag,
        drift_summaries.first(),
        cfg.assurance_tier.as_deref(),
        cfg.slice_size,
        cfg.slice_offset,
    ) {
        warn!(error = %e, "Failed to persist drift metadata to manifest (non-fatal)");
    }

    let drift_exceeded = drift_summaries
        .iter()
        .any(|m| m.weight_l_inf.is_nan() || m.loss_l_inf.is_nan());
    if drift_exceeded {
        warn!("Drift metrics contained NaN; treating as failure");
        return Ok(2);
    }

    let exit_code = match overall_decision {
        DriftDecision::Block => 1,
        DriftDecision::ReviewRequired => 3,
        DriftDecision::RecordOnly => 0,
    };

    Ok(exit_code)
}

fn load_config(path: &Path) -> Result<DeterminismDriftConfig> {
    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read config: {}", e)))?;
    serde_json::from_str(&content)
        .map_err(|e| AosError::Parse(format!("Failed to parse drift config: {}", e)))
}

fn resolve_backends(raw: &[String]) -> Result<Vec<TrainingBackend>> {
    if raw.is_empty() {
        return Ok(vec![TrainingBackend::Cpu]);
    }
    let mut out = Vec::new();
    for backend in raw {
        match backend.to_lowercase().as_str() {
            "cpu" => out.push(TrainingBackend::Cpu),
            "coreml" => out.push(TrainingBackend::CoreML),
            "mlx" => out.push(TrainingBackend::Mlx),
            "metal" => out.push(TrainingBackend::Metal),
            other => return Err(AosError::Validation(format!("Unknown backend '{}'", other))),
        }
    }
    Ok(out)
}

fn load_dataset(path: &Path) -> Result<Vec<TrainingExample>> {
    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read dataset: {}", e)))?;
    let data: TrainingData = serde_json::from_str(&content)
        .map_err(|e| AosError::Parse(format!("Dataset parse error: {}", e)))?;
    let examples = data
        .examples
        .into_iter()
        .map(|ex| TrainingExample {
            input: ex.input,
            target: ex.target,
            metadata: ex
                .metadata
                .unwrap_or_default()
                .into_iter()
                // Preserve metadata deterministically:
                // - if JSON string => use raw string (no quotes)
                // - else => stringify JSON value (numbers/bools/null/objects/arrays)
                .map(|(k, v)| (k, stringify_metadata_value(v)))
                .collect(),
            weight: 1.0,
        })
        .collect();
    Ok(examples)
}

fn stringify_metadata_value(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

fn persist_manifest(
    cfg: &DeterminismDriftConfig,
    reference_backend: &str,
    drift_metrics: Option<&DriftMetrics>,
    assurance_tier: Option<&str>,
    slice_size: Option<usize>,
    slice_offset: Option<usize>,
) -> Result<()> {
    let Some(manifest_path) = &cfg.manifest_path else {
        return Ok(());
    };

    let content = fs::read_to_string(manifest_path)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
    let mut manifest = ManifestV3::from_json(&content)?;

    let adapter = match cfg.adapter_id.as_deref() {
        Some(id) => manifest
            .adapters
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| AosError::Validation(format!("Adapter '{id}' not found in manifest")))?,
        None => {
            if manifest.adapters.is_empty() {
                return Err(AosError::Validation("No adapter entries in manifest".into()));
            }
            if manifest.adapters.len() == 1 {
                manifest
                    .adapters
                    .first_mut()
                    .ok_or_else(|| AosError::Validation("No adapter entries in manifest".into()))?
            } else {
                return Err(AosError::Validation(
                    "Manifest contains multiple adapters; pass --adapter-id to select one".into(),
                ));
            }
        }
    };

    adapter.determinism_seed = Some(cfg.seed);
    adapter.determinism_backend = Some(reference_backend.to_string());
    adapter.determinism_device = cfg.device.clone();
    adapter.drift_reference_backend = Some(reference_backend.to_string());
    adapter.drift_baseline_backend = Some(reference_backend.to_string());
    adapter.drift_test_backend = drift_metrics.map(|m| m.backend.clone());
    adapter.drift_metric = drift_metrics.map(|m| m.weight_l_inf).or(Some(0.0));
    adapter.drift_loss_metric = drift_metrics.map(|m| m.loss_l_inf);
    adapter.drift_tier = parse_assurance_tier(assurance_tier)
        .or(adapter.drift_tier)
        .or(Some(AssuranceTier::Standard));
    adapter.assurance_tier = adapter.drift_tier.unwrap_or(AssuranceTier::Standard);
    adapter.drift_slice_size = slice_size;
    adapter.drift_slice_offset = slice_offset;

    let serialized = manifest.to_json()?;
    fs::write(manifest_path, serialized)
        .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;

    info!(
        path = %manifest_path.display(),
        "Persisted drift metadata to manifest"
    );
    Ok(())
}

fn choose_reference_backend(
    backends: &[TrainingBackend],
    override_ref: Option<String>,
) -> Option<String> {
    if backends.iter().any(|b| matches!(b, TrainingBackend::Cpu)) {
        return Some("cpu".to_string());
    }

    if let Some(pref) = override_ref {
        let pref_lower = pref.to_lowercase();
        if backends
            .iter()
            .any(|b| b.tag().eq_ignore_ascii_case(&pref_lower))
        {
            return Some(pref_lower);
        }
    }

    if backends.iter().any(|b| matches!(b, TrainingBackend::Metal)) {
        return Some("metal".to_string());
    }
    if backends.iter().any(|b| matches!(b, TrainingBackend::Mlx)) {
        return Some("mlx".to_string());
    }

    backends.first().map(|b| b.tag().to_string())
}

fn parse_assurance_tier(value: Option<&str>) -> Option<AssuranceTier> {
    value.map(|v| match v.to_lowercase().as_str() {
        "low" => AssuranceTier::Low,
        "high" => AssuranceTier::High,
        _ => AssuranceTier::Standard,
    })
}

fn evaluate_drift(metrics: &DriftMetrics, tier: AssuranceTier) -> DriftDecision {
    const HIGH_WEIGHT_EPS: f32 = 1e-6;
    const HIGH_LOSS_EPS: f32 = 1e-4;
    const STANDARD_WEIGHT_EPS: f32 = 5e-5;
    const STANDARD_LOSS_EPS: f32 = 5e-4;

    match tier {
        AssuranceTier::Low => DriftDecision::RecordOnly,
        AssuranceTier::Standard => {
            if metrics.weight_l_inf > STANDARD_WEIGHT_EPS || metrics.loss_l_inf > STANDARD_LOSS_EPS
            {
                DriftDecision::ReviewRequired
            } else {
                DriftDecision::RecordOnly
            }
        }
        AssuranceTier::High => {
            if metrics.weight_l_inf > HIGH_WEIGHT_EPS || metrics.loss_l_inf > HIGH_LOSS_EPS {
                DriftDecision::Block
            } else {
                DriftDecision::RecordOnly
            }
        }
    }
}

fn merge_decision(left: DriftDecision, right: DriftDecision) -> DriftDecision {
    use DriftDecision::*;
    match (left, right) {
        (Block, _) | (_, Block) => Block,
        (ReviewRequired, _) | (_, ReviewRequired) => ReviewRequired,
        _ => RecordOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_lora_worker::training::LoRAWeights;

    #[test]
    fn test_deterministic_slice_stable() {
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
        ];

        let first = deterministic_slice(examples.clone(), 42, Some(2), None);
        let second = deterministic_slice(examples, 42, Some(2), None);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].input, second[0].input);
        assert_eq!(first[1].target, second[1].target);
    }

    #[test]
    fn test_compute_drift_metrics() {
        let weights_ref = LoRAWeights {
            lora_a: vec![vec![0.1, 0.2]],
            lora_b: vec![vec![0.3, 0.4]],
        };
        let weights_candidate = LoRAWeights {
            lora_a: vec![vec![0.2, 0.4]],
            lora_b: vec![vec![0.5, 0.6]],
        };

        let reference = TrainingResult {
            adapter_id: "ref".into(),
            final_loss: 0.5,
            training_time_us: 1,
            weights: weights_ref,
            cancelled: false,
            stopped_at_epoch: Some(1),
            examples_processed: Some(1),
            tokens_processed: Some(1),
            tokens_per_sec: 0.0,
            examples_per_sec: 0.0,
            backend: Some("cpu".into()),
            backend_device: None,
            using_gpu: false,
            effective_batch_size: Some(1),
            loss_curve: vec![0.5],
            determinism_seed: Some(1),
            determinism_backend: Some("cpu".into()),
            determinism_device: None,
            dataset_version_id: None,
        };
        let candidate = TrainingResult {
            adapter_id: "cand".into(),
            final_loss: 0.6,
            training_time_us: 1,
            weights: weights_candidate,
            cancelled: false,
            stopped_at_epoch: Some(1),
            examples_processed: Some(1),
            tokens_processed: Some(1),
            tokens_per_sec: 0.0,
            examples_per_sec: 0.0,
            backend: Some("coreml".into()),
            backend_device: None,
            using_gpu: false,
            effective_batch_size: Some(1),
            loss_curve: vec![0.6],
            determinism_seed: Some(1),
            determinism_backend: Some("coreml".into()),
            determinism_device: None,
            dataset_version_id: None,
        };

        let metrics = compute_drift(&reference, &candidate);
        assert!(metrics.weight_l_inf > 0.0);
        assert!(metrics.loss_l_inf > 0.0);
        assert_eq!(metrics.backend, "coreml");
    }
}
