//! Training report generation for dataset-aware evaluation.

use std::path::{Path, PathBuf};

use adapteros_core::defaults::DEFAULT_TRAINING_REPORTS_SUBDIR;
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{loss_to_perplexity_curve, TrainingResult};
use adapteros_types::training::{
    OptimizerConfigSummary, TrainingQuantizationReportV1, TrainingReportCurves,
    TrainingReportMetricDefinitions, TrainingReportSummary, TrainingReportV1,
    TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS,
    TRAINING_QUANTIZATION_PROBE_STATUS_UNAVAILABLE,
};

pub(crate) fn training_report_path(artifacts_root: &Path, job_id: &str) -> PathBuf {
    artifacts_root
        .join(DEFAULT_TRAINING_REPORTS_SUBDIR)
        .join(job_id)
        .join("report.json")
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_training_report(
    artifacts_root: &Path,
    pipeline_id: &str,
    dataset_id: &str,
    dataset_content_hash: &str,
    split_hash: &str,
    base_model_id: &str,
    base_model_hash: &str,
    optimizer: OptimizerConfigSummary,
    training_config_hash: &str,
    target_epochs: u32,
    generated_at_unix_ms: u64,
    training_result: &TrainingResult,
) -> Result<PathBuf> {
    let report = build_training_report(
        pipeline_id,
        dataset_id,
        dataset_content_hash,
        split_hash,
        base_model_id,
        base_model_hash,
        optimizer,
        training_config_hash,
        target_epochs,
        generated_at_unix_ms,
        training_result,
    );

    let report_path = training_report_path(artifacts_root, pipeline_id);
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AosError::Io(format!(
                "Failed to create training report directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(&report).map_err(AosError::Serialization)?;
    std::fs::write(&report_path, json).map_err(|e| {
        AosError::Io(format!(
            "Failed to write training report to {}: {}",
            report_path.display(),
            e
        ))
    })?;

    Ok(report_path)
}

#[allow(clippy::too_many_arguments)]
fn build_training_report(
    pipeline_id: &str,
    dataset_id: &str,
    dataset_content_hash: &str,
    split_hash: &str,
    base_model_id: &str,
    base_model_hash: &str,
    optimizer: OptimizerConfigSummary,
    training_config_hash: &str,
    target_epochs: u32,
    generated_at_unix_ms: u64,
    training_result: &TrainingResult,
) -> TrainingReportV1 {
    let train_perplexity_curve = if training_result.train_perplexity_curve.is_empty()
        && !training_result.loss_curve.is_empty()
    {
        loss_to_perplexity_curve(&training_result.loss_curve)
    } else {
        training_result.train_perplexity_curve.clone()
    };
    let validation_perplexity_curve = if training_result.validation_perplexity_curve.is_empty()
        && !training_result.validation_loss_curve.is_empty()
    {
        loss_to_perplexity_curve(&training_result.validation_loss_curve)
    } else {
        training_result.validation_perplexity_curve.clone()
    };

    let final_epoch = training_result.stopped_at_epoch.unwrap_or(target_epochs);
    let early_stopped = !training_result.cancelled && final_epoch < target_epochs;
    let best_epoch = training_result
        .best_validation
        .map(|(_, epoch)| epoch)
        .unwrap_or(final_epoch);
    let total_steps = training_result
        .examples_processed
        .unwrap_or(training_result.train_example_count * final_epoch as u64);
    let total_tokens = training_result
        .tokens_processed
        .unwrap_or(training_result.train_token_count * final_epoch as u64);

    TrainingReportV1 {
        report_version: adapteros_types::training::TRAINING_REPORT_VERSION,
        pipeline_id: pipeline_id.to_string(),
        dataset_id: dataset_id.to_string(),
        dataset_content_hash: dataset_content_hash.to_string(),
        split_hash: split_hash.to_string(),
        base_model_id: base_model_id.to_string(),
        base_model_hash: base_model_hash.to_string(),
        optimizer,
        training_config_hash: training_config_hash.to_string(),
        curves: TrainingReportCurves {
            train_loss: training_result.loss_curve.clone(),
            train_ppl: train_perplexity_curve,
            val_loss: training_result.validation_loss_curve.clone(),
            val_ppl: validation_perplexity_curve,
        },
        summary: TrainingReportSummary {
            best_epoch,
            final_epoch,
            early_stopped,
            total_steps,
            total_tokens,
        },
        metric_definitions: TrainingReportMetricDefinitions {
            train_loss: "Mean cross-entropy loss per epoch on the training split.".to_string(),
            train_ppl: "Perplexity per epoch computed as exp(train_loss).".to_string(),
            val_loss: "Mean cross-entropy loss per epoch on the validation split.".to_string(),
            val_ppl: "Perplexity per epoch computed as exp(val_loss).".to_string(),
            best_epoch: "Epoch (1-based) with the lowest validation loss; defaults to final_epoch when validation is disabled.".to_string(),
            final_epoch: "Last completed epoch (1-based).".to_string(),
            early_stopped: "True when training stopped before target_epochs without cancellation.".to_string(),
            total_steps: "Total training steps, defined as examples processed across all epochs.".to_string(),
            total_tokens: "Total tokens processed across the training split.".to_string(),
        },
        quantization_report: Some(TrainingQuantizationReportV1 {
            gate_source: TRAINING_QUANTIZATION_GATE_SOURCE_POLICY_METRICS.to_string(),
            probe_status: TRAINING_QUANTIZATION_PROBE_STATUS_UNAVAILABLE.to_string(),
            probe_error: None,
            policy_metrics: None,
            probe_metrics: None,
        }),
        generated_at_unix_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_lora_worker::training::LoRAWeights;

    fn sample_training_result() -> TrainingResult {
        TrainingResult {
            adapter_id: "adapter-1".to_string(),
            final_loss: 0.25,
            training_time_us: 1000,
            weights: LoRAWeights {
                modules: std::collections::BTreeMap::new(),
                lora_a: vec![vec![0.1]],
                lora_b: vec![vec![0.2]],
                moe_config: None,
                precomputed_delta: None,
            },
            cancelled: false,
            stopped_at_epoch: Some(2),
            examples_processed: Some(20),
            tokens_processed: Some(200),
            tokens_per_sec: 0.0,
            examples_per_sec: 0.0,
            backend: Some("cpu".to_string()),
            backend_device: None,
            using_gpu: false,
            effective_batch_size: Some(1),
            loss_curve: vec![0.5, 0.25],
            determinism_seed: Some(1),
            determinism_backend: Some("cpu".to_string()),
            determinism_device: None,
            dataset_version_id: None,
            validation_loss_curve: vec![0.5, 0.25],
            train_perplexity_curve: vec![1.0, 1.0],
            validation_perplexity_curve: vec![1.5, 1.25],
            split_hash_b3: Some("split".to_string()),
            train_example_count: 10,
            validation_example_count: 2,
            train_token_count: 100,
            validation_token_count: 20,
            best_validation: Some((0.25, 2)),
            final_validation_loss: Some(0.25),
            mlx_version: None,
        }
    }

    #[test]
    fn report_serialization_is_stable() {
        let report = build_training_report(
            "pipeline-1",
            "dataset-1",
            "b3:dataset",
            "b3:split",
            "base-1",
            "b3:base",
            OptimizerConfigSummary {
                optimizer_type: "adam".to_string(),
                beta1: 0.5,
                beta2: 0.25,
                epsilon: 0.0,
                weight_decay: 0.0,
                momentum: 0.0,
            },
            "b3:config",
            2,
            1_700_000_000_000,
            &sample_training_result(),
        );

        let json = serde_json::to_string_pretty(&report).expect("serialize report");
        let expected = r#"{
  "report_version": 1,
  "pipeline_id": "pipeline-1",
  "dataset_id": "dataset-1",
  "dataset_content_hash": "b3:dataset",
  "split_hash": "b3:split",
  "base_model_id": "base-1",
  "base_model_hash": "b3:base",
  "optimizer": {
    "optimizer_type": "adam",
    "beta1": 0.5,
    "beta2": 0.25,
    "epsilon": 0.0,
    "weight_decay": 0.0,
    "momentum": 0.0
  },
  "training_config_hash": "b3:config",
  "curves": {
    "train_loss": [
      0.5,
      0.25
    ],
    "train_ppl": [
      1.0,
      1.0
    ],
    "val_loss": [
      0.5,
      0.25
    ],
    "val_ppl": [
      1.5,
      1.25
    ]
  },
  "summary": {
    "best_epoch": 2,
    "final_epoch": 2,
    "early_stopped": false,
    "total_steps": 20,
    "total_tokens": 200
  },
  "metric_definitions": {
    "train_loss": "Mean cross-entropy loss per epoch on the training split.",
    "train_ppl": "Perplexity per epoch computed as exp(train_loss).",
    "val_loss": "Mean cross-entropy loss per epoch on the validation split.",
    "val_ppl": "Perplexity per epoch computed as exp(val_loss).",
    "best_epoch": "Epoch (1-based) with the lowest validation loss; defaults to final_epoch when validation is disabled.",
    "final_epoch": "Last completed epoch (1-based).",
    "early_stopped": "True when training stopped before target_epochs without cancellation.",
    "total_steps": "Total training steps, defined as examples processed across all epochs.",
    "total_tokens": "Total tokens processed across the training split."
  },
  "quantization_report": {
    "gate_source": "policy_metrics",
    "probe_status": "unavailable"
  },
  "generated_at_unix_ms": 1700000000000
}"#;

        assert_eq!(json, expected);
    }
}
