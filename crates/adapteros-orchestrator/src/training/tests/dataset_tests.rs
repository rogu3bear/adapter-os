//! Dataset loading and weighted round-robin merge tests

use adapteros_lora_worker::training::TrainingExample as WorkerTrainingExample;
use adapteros_types::training::ExampleMetadataV1;

use crate::training::dataset::weighted_round_robin_merge;
use crate::training::job::{DataLineageMode, TrainingConfig};
use crate::training::service::TrainingService;

fn make_example(input_tokens: Vec<u32>, target_tokens: Vec<u32>, row_id: u64) -> WorkerTrainingExample {
    let metadata = ExampleMetadataV1::new("test", row_id, "{}", 0);
    let attention_mask =
        WorkerTrainingExample::attention_mask_from_tokens(&input_tokens, 0);
    WorkerTrainingExample::new(input_tokens, target_tokens, attention_mask, metadata)
}

#[test]
fn weighted_round_robin_is_deterministic() {
    let ds1 = vec![
        make_example(vec![1], vec![2], 1),
        make_example(vec![3], vec![4], 2),
    ];
    let ds2 = vec![make_example(vec![5], vec![6], 1)];

    let merged = weighted_round_robin_merge(vec![(ds1.clone(), 2.0), (ds2.clone(), 1.0)]);
    // Expect pattern: ds1, ds1, ds2 (since ds1 weight rounds to 2 slots)
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].input_tokens, vec![1]);
    assert_eq!(merged[1].input_tokens, vec![3]);
    assert_eq!(merged[2].input_tokens, vec![5]);

    let merged_again = weighted_round_robin_merge(vec![(ds1, 2.0), (ds2, 1.0)]);
    assert_eq!(
        merged.iter().map(|e| &e.input_tokens).collect::<Vec<_>>(),
        merged_again.iter().map(|e| &e.input_tokens).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn start_training_rejects_missing_dataset_versions_when_non_synthetic() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let result = service
        .start_training(
            "missing-datasets".to_string(),
            config,
            None,  // template_id
            None,  // repo_id
            None,  // target_branch
            None,  // base_version_id
            None,  // dataset_id
            None,  // dataset_version_ids
            false, // synthetic_mode
            DataLineageMode::Synthetic,
            None, // tenant_id
            None, // initiated_by
            None, // initiated_by_role
            None, // base_model_id
            None, // collection_id
            None, // scope
            None, // lora_tier
            None, // category
            None, // description
            None, // language
            None, // framework_id
            None, // framework_version
            None, // post_actions_json
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await;

    assert!(
        result.is_err(),
        "non-synthetic training without datasets must fail"
    );
}

#[tokio::test]
async fn test_create_and_list_jobs() {
    let service = TrainingService::new();

    let config = TrainingConfig::default();
    let job = service
        .start_training(
            "test-adapter".to_string(),
            config,
            None, // template_id
            None, // repo_id
            None, // target_branch
            None, // base_version_id
            None, // dataset_id
            None, // dataset_version_ids
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            None, // tenant_id
            None, // initiated_by
            None, // initiated_by_role
            None, // base_model_id
            None, // collection_id
            None, // scope
            None, // lora_tier
            None, // category
            None, // description
            None, // language
            None, // framework_id
            None, // framework_version
            None, // post_actions_json
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await
        .unwrap();

    assert_eq!(job.status, crate::training::job::TrainingJobStatus::Pending);
    assert_eq!(job.adapter_name, "test-adapter");

    let jobs = service.list_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
}

#[tokio::test]
async fn test_cancel_job() {
    let service = TrainingService::new();

    let config = TrainingConfig::default();
    let job = service
        .start_training(
            "test-adapter".to_string(),
            config,
            None, // template_id
            None, // repo_id
            None, // target_branch
            None, // base_version_id
            None, // dataset_id
            None, // dataset_version_ids
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            None, // tenant_id
            None, // initiated_by
            None, // initiated_by_role
            None, // base_model_id
            None, // collection_id
            None, // scope
            None, // lora_tier
            None, // category
            None, // description
            None, // language
            None, // framework_id
            None, // framework_version
            None, // post_actions_json
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await
        .unwrap();

    // Test without UDS client (will mark as Cancelled via token)
    service.cancel_job(&job.id, None, None).await.unwrap();

    let updated_job = service.get_job(&job.id).await.unwrap();
    assert_eq!(
        updated_job.status,
        crate::training::job::TrainingJobStatus::Cancelled
    );
}

#[tokio::test]
async fn test_update_progress() {
    let service = TrainingService::new();

    let config = TrainingConfig::default();
    let job = service
        .start_training(
            "test-adapter".to_string(),
            config,
            None, // template_id
            None, // repo_id
            None, // target_branch
            None, // base_version_id
            None, // dataset_id
            None, // dataset_version_ids
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            None, // tenant_id
            None, // initiated_by
            None, // initiated_by_role
            None, // base_model_id
            None, // collection_id
            None, // scope
            None, // lora_tier
            None, // category
            None, // description
            None, // language
            None, // framework_id
            None, // framework_version
            None, // post_actions_json
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await
        .unwrap();

    service
        .update_progress(&job.id, 1, 0.5, 1000.0)
        .await
        .unwrap();

    let updated_job = service.get_job(&job.id).await.unwrap();
    assert_eq!(
        updated_job.status,
        crate::training::job::TrainingJobStatus::Running
    );
    assert_eq!(updated_job.current_epoch, 1);
    assert!((updated_job.current_loss - 0.5).abs() < 0.01);
}

#[tokio::test]
async fn test_list_templates() {
    let service = TrainingService::new();
    let templates = service.list_templates().await.unwrap();

    assert!(templates.len() >= 4);
    assert!(templates.iter().any(|t| t.id == "general-code"));
    assert!(templates.iter().any(|t| t.id == "framework-specific"));
}
