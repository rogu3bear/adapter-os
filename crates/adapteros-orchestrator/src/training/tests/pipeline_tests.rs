//! Pipeline state machine tests.

use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

use adapteros_core::B3Hash;
use adapteros_lora_worker::training::{LoRAWeights, TrainingResult};

use crate::training::job::{DataLineageMode, TrainingConfig};
use crate::training::pipeline::{
    PhaseStatus, PipelineConfigSnapshot, PipelinePhase, TrainingPipeline,
};

fn snapshot_with_dataset(dataset_id: Option<&str>) -> PipelineConfigSnapshot {
    PipelineConfigSnapshot {
        training_config: TrainingConfig::default(),
        dataset_id: dataset_id.map(|id| id.to_string()),
        dataset_version_ids: None,
        data_spec_hash: None,
        synthetic_mode: true,
        data_lineage_mode: DataLineageMode::Synthetic,
        base_model_id: None,
    }
}

fn sample_training_result() -> TrainingResult {
    TrainingResult {
        adapter_id: "adapter-1".to_string(),
        final_loss: 0.1,
        training_time_us: 1234,
        weights: LoRAWeights {
            modules: std::collections::HashMap::new(),
            lora_a: vec![vec![0.1, 0.2]],
            lora_b: vec![vec![0.3, 0.4]],
            moe_config: None,
            precomputed_delta: None,
        },
        cancelled: false,
        stopped_at_epoch: Some(1),
        examples_processed: Some(2),
        tokens_processed: Some(10),
        tokens_per_sec: 1.0,
        examples_per_sec: 0.5,
        backend: Some("CPU".to_string()),
        backend_device: None,
        using_gpu: false,
        effective_batch_size: Some(1),
        loss_curve: vec![0.1],
        determinism_seed: Some(42),
        determinism_backend: Some("cpu".to_string()),
        determinism_device: Some("cpu".to_string()),
        dataset_version_id: None,
        validation_loss_curve: vec![0.2],
        train_perplexity_curve: vec![1.0],
        validation_perplexity_curve: vec![1.1],
        split_hash_b3: Some("split".to_string()),
        train_example_count: 1,
        validation_example_count: 1,
        train_token_count: 10,
        validation_token_count: 5,
        best_validation: Some((0.2, 1)),
        final_validation_loss: Some(0.2),
    }
}

fn hash_training_result(training_result: &TrainingResult) -> String {
    let bytes = serde_json::to_vec(training_result).expect("serialize training result");
    B3Hash::hash(&bytes).to_hex().to_string()
}

#[tokio::test]
async fn pipeline_persists_and_resumes() {
    let temp = TempDir::new().expect("temp dir");
    let snapshot = snapshot_with_dataset(Some("dataset-1"));

    let mut pipeline = TrainingPipeline::load_or_init("job-1", snapshot.clone(), Some(temp.path()))
        .await
        .expect("pipeline init");
    pipeline
        .seed_receipt(
            "cfg-hash",
            "base-hash",
            Some("dataset-1"),
            snapshot.training_config.training_contract_version.as_str(),
        )
        .await
        .unwrap();
    assert_eq!(pipeline.current_phase(), PipelinePhase::DatasetBuild);

    pipeline
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    let mut dataset_inputs = HashMap::new();
    dataset_inputs.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut dataset_outputs = HashMap::new();
    dataset_outputs.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            dataset_inputs,
            dataset_outputs,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();

    pipeline
        .enter_phase(PipelinePhase::Preprocess)
        .await
        .unwrap();
    let mut preprocess_inputs = HashMap::new();
    preprocess_inputs.insert("dataset_content_hash".to_string(), "abc".to_string());
    let preprocess_outputs = HashMap::new();
    pipeline
        .complete_phase(
            PipelinePhase::Preprocess,
            PhaseStatus::Skipped,
            preprocess_inputs,
            preprocess_outputs,
            json!({"strategy": "noop"}),
        )
        .await
        .unwrap();

    pipeline.enter_phase(PipelinePhase::Split).await.unwrap();
    let mut split_inputs = HashMap::new();
    split_inputs.insert("dataset_content_hash".to_string(), "abc".to_string());
    let mut split_outputs = HashMap::new();
    split_outputs.insert("split_hash".to_string(), "split".to_string());
    pipeline
        .complete_phase(
            PipelinePhase::Split,
            PhaseStatus::Completed,
            split_inputs,
            split_outputs,
            json!({"split_hash_b3": "split"}),
        )
        .await
        .unwrap();

    drop(pipeline);

    let pipeline = TrainingPipeline::load_or_init("job-1", snapshot.clone(), Some(temp.path()))
        .await
        .expect("pipeline reload");
    assert_eq!(pipeline.current_phase(), PipelinePhase::TrainingLoop);
    assert!(pipeline.receipt(PipelinePhase::DatasetBuild).is_some());
}

#[tokio::test]
async fn pipeline_resumes_in_progress_phase() {
    let temp = TempDir::new().expect("temp dir");
    let snapshot = snapshot_with_dataset(Some("dataset-1"));

    let mut pipeline = TrainingPipeline::load_or_init("job-1", snapshot.clone(), Some(temp.path()))
        .await
        .expect("pipeline init");
    pipeline
        .seed_receipt("cfg-hash", "base-hash", Some("dataset-1"), "1.0")
        .await
        .unwrap();
    pipeline
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    drop(pipeline);

    let mut pipeline = TrainingPipeline::load_or_init("job-1", snapshot, Some(temp.path()))
        .await
        .expect("pipeline reload");
    pipeline
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();

    let mut inputs = HashMap::new();
    inputs.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut outputs = HashMap::new();
    outputs.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            inputs,
            outputs,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();
    assert_eq!(pipeline.current_phase(), PipelinePhase::Preprocess);
}

#[tokio::test]
async fn pipeline_resume_guard_rejects_mismatch() {
    let temp = TempDir::new().expect("temp dir");
    let snapshot = snapshot_with_dataset(Some("dataset-1"));

    let mut pipeline = TrainingPipeline::load_or_init("job-1", snapshot.clone(), Some(temp.path()))
        .await
        .expect("pipeline init");
    pipeline
        .seed_receipt(
            "cfg-hash",
            "base-hash",
            Some("dataset-1"),
            snapshot.training_config.training_contract_version.as_str(),
        )
        .await
        .unwrap();
    pipeline
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    let mut dataset_inputs = HashMap::new();
    dataset_inputs.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut dataset_outputs = HashMap::new();
    dataset_outputs.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            dataset_inputs,
            dataset_outputs,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();

    let err = pipeline
        .assert_resume_compatible(
            "abc",
            "split",
            "mismatch",
            "cfg-hash",
            snapshot.training_config.training_contract_version.as_str(),
            false,
        )
        .expect_err("resume guard mismatch should fail");
    assert!(
        format!("{err}").contains("base_model_hash"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn pipeline_id_is_deterministic() {
    let temp = TempDir::new().expect("temp dir");
    let snapshot = snapshot_with_dataset(Some("dataset-1"));

    let mut pipeline_a =
        TrainingPipeline::load_or_init("job-a", snapshot.clone(), Some(temp.path()))
            .await
            .expect("pipeline init");
    pipeline_a
        .seed_receipt(
            "cfg-hash",
            "base-hash",
            Some("dataset-1"),
            snapshot.training_config.training_contract_version.as_str(),
        )
        .await
        .unwrap();
    pipeline_a
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    let mut inputs_a = HashMap::new();
    inputs_a.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut outputs_a = HashMap::new();
    outputs_a.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline_a
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            inputs_a,
            outputs_a,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();
    let id_a = pipeline_a.receipt_v1().pipeline_id.clone();

    let mut pipeline_b =
        TrainingPipeline::load_or_init("job-b", snapshot.clone(), Some(temp.path()))
            .await
            .expect("pipeline init");
    pipeline_b
        .seed_receipt(
            "cfg-hash",
            "base-hash",
            Some("dataset-1"),
            snapshot.training_config.training_contract_version.as_str(),
        )
        .await
        .unwrap();
    pipeline_b
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    let mut inputs_b = HashMap::new();
    inputs_b.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut outputs_b = HashMap::new();
    outputs_b.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline_b
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            inputs_b,
            outputs_b,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();
    let id_b = pipeline_b.receipt_v1().pipeline_id.clone();

    assert_eq!(id_a, id_b);

    let mut pipeline_c =
        TrainingPipeline::load_or_init("job-c", snapshot.clone(), Some(temp.path()))
            .await
            .expect("pipeline init");
    pipeline_c
        .seed_receipt(
            "cfg-hash",
            "base-hash-alt",
            Some("dataset-1"),
            snapshot.training_config.training_contract_version.as_str(),
        )
        .await
        .unwrap();
    pipeline_c
        .enter_phase(PipelinePhase::DatasetBuild)
        .await
        .unwrap();
    let mut inputs_c = HashMap::new();
    inputs_c.insert("dataset_id".to_string(), "dataset-1".to_string());
    let mut outputs_c = HashMap::new();
    outputs_c.insert("dataset_content_hash".to_string(), "abc".to_string());
    pipeline_c
        .complete_phase(
            PipelinePhase::DatasetBuild,
            PhaseStatus::Completed,
            inputs_c,
            outputs_c,
            json!({"dataset_hash_b3": "abc"}),
        )
        .await
        .unwrap();
    let id_c = pipeline_c.receipt_v1().pipeline_id.clone();

    assert_ne!(id_a, id_c);
}

#[tokio::test]
async fn pipeline_persists_training_result() {
    let temp = TempDir::new().expect("temp dir");
    let snapshot = snapshot_with_dataset(Some("dataset-1"));

    let mut pipeline = TrainingPipeline::load_or_init("job-1", snapshot, Some(temp.path()))
        .await
        .expect("pipeline init");
    pipeline
        .seed_receipt("cfg-hash", "base-hash", Some("dataset-1"), "1.0")
        .await
        .unwrap();

    let training_result = sample_training_result();
    let expected_hash = hash_training_result(&training_result);
    let stored_hash = pipeline
        .persist_training_result(&training_result)
        .await
        .expect("persist training result");
    assert_eq!(expected_hash, stored_hash);

    let loaded = pipeline
        .load_training_result()
        .await
        .expect("load training result")
        .expect("training result missing");
    let loaded_hash = hash_training_result(&loaded);
    assert_eq!(expected_hash, loaded_hash);
}
