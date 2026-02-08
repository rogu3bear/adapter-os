//! Training report integration tests.

use adapteros_lora_worker::training::{LoRAWeights, TrainingResult};
use adapteros_types::training::OptimizerConfigSummary;
use tempfile::TempDir;

use crate::training::report::write_training_report;

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
fn report_hash_is_stable_for_identical_inputs() {
    let tmp = TempDir::new().expect("temp dir");
    let report_root = tmp.path();
    let training_result = sample_training_result();

    let optimizer = OptimizerConfigSummary {
        optimizer_type: "adam".to_string(),
        beta1: 0.5,
        beta2: 0.25,
        epsilon: 0.0,
        weight_decay: 0.0,
        momentum: 0.0,
    };

    let report_path = write_training_report(
        report_root,
        "pipeline-1",
        "dataset-1",
        "b3:dataset",
        "b3:split",
        "base-1",
        "b3:base",
        optimizer.clone(),
        "b3:config",
        2,
        1_700_000_000_000,
        &training_result,
    )
    .expect("write report");

    let first = std::fs::read_to_string(&report_path).expect("read report");
    let first_hash = blake3::hash(first.as_bytes()).to_hex().to_string();

    let report_path_second = write_training_report(
        report_root,
        "pipeline-1",
        "dataset-1",
        "b3:dataset",
        "b3:split",
        "base-1",
        "b3:base",
        optimizer,
        "b3:config",
        2,
        1_700_000_000_000,
        &training_result,
    )
    .expect("write report");

    let second = std::fs::read_to_string(&report_path_second).expect("read report");
    let second_hash = blake3::hash(second.as_bytes()).to_hex().to_string();

    assert_eq!(first_hash, second_hash);
}
