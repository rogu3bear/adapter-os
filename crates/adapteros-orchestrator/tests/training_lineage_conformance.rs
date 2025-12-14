use adapteros_db::factory::DbFactory;
use adapteros_db::sqlx;
use adapteros_orchestrator::training::TrainingService;
use adapteros_types::training::{
    DataLineageMode, DatasetVersionSelection, TrainingBackendKind, TrainingConfig,
};

async fn seed_dataset_version(
    db: &adapteros_db::Db,
    dataset_id: &str,
    version_id: &str,
    tenant_id: &str,
    trust_state: &str,
    hash_b3: &str,
) {
    let _ = db
        .create_training_dataset_with_id(
            dataset_id,
            "orch-dataset",
            Some("desc"),
            "jsonl",
            hash_b3,
            "/tmp/orch-dataset",
            Some("tester"),
        )
        .await
        .unwrap();

    let _ = db
        .create_training_dataset_version_with_id(
            version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "/tmp/orch-dataset/version",
            hash_b3,
            None,
            None,
            Some("tester"),
        )
        .await
        .unwrap();

    sqlx::query(
        "UPDATE training_dataset_versions SET trust_state = ?, overall_trust_status = ? WHERE id = ?",
    )
    .bind(trust_state)
    .bind(trust_state)
    .bind(version_id)
    .execute(db.pool())
    .await
    .unwrap();
}

fn minimal_config() -> TrainingConfig {
    let mut cfg = TrainingConfig::default();
    cfg.rank = 4;
    cfg.alpha = 8;
    cfg.targets = vec!["q_proj".into()];
    cfg.epochs = 1;
    cfg.learning_rate = 0.01;
    cfg.batch_size = 1;
    cfg.preferred_backend = Some(TrainingBackendKind::Auto);
    cfg
}

#[tokio::test]
async fn orch_rejects_synthetic_with_dataset_versions() {
    let mut db = DbFactory::create_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    let service = TrainingService::with_db(db.clone(), std::env::temp_dir());

    let cfg = minimal_config();
    let result = service
        .start_training(
            "adapter".into(),
            cfg,
            None,
            None,
            None,
            None,
            None,
            Some(vec![DatasetVersionSelection {
                dataset_version_id: "dsv-synth".into(),
                weight: 1.0,
            }]),
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            Some("tenant-1".into()),
            Some("tester".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await;

    let err = format!("{:?}", result.err().expect("expected error"));
    assert!(
        err.contains("synthetic_mode=true requires dataset_version_ids to be empty"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn orch_rejects_non_synthetic_without_datasets() {
    let mut db = DbFactory::create_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    let service = TrainingService::with_db(db.clone(), std::env::temp_dir());

    let cfg = minimal_config();
    let result = service
        .start_training(
            "adapter".into(),
            cfg,
            None,
            None,
            None,
            None,
            None,
            None,
            false, // synthetic_mode
            DataLineageMode::Versioned,
            Some("tenant-1".into()),
            Some("tester".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await;

    let err = format!("{:?}", result.err().expect("expected error"));
    assert!(
        err.contains("dataset_version_ids are required for non-synthetic training jobs"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn orch_rejects_blocked_trust_state() {
    let mut db = DbFactory::create_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    seed_dataset_version(
        &db,
        "ds-block",
        "dsv-block",
        "tenant-1",
        "blocked_regressed",
        "hash-block",
    )
    .await;
    let service = TrainingService::with_db(db.clone(), std::env::temp_dir());

    let cfg = minimal_config();
    let result = service
        .start_training(
            "adapter".into(),
            cfg,
            None,
            None,
            None,
            None,
            None,
            Some(vec![DatasetVersionSelection {
                dataset_version_id: "dsv-block".into(),
                weight: 1.0,
            }]),
            false,
            DataLineageMode::Versioned,
            Some("tenant-1".into()),
            Some("tester".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await;

    let err = format!("{:?}", result.err().expect("expected error"));
    assert!(
        err.contains("trust_state=blocked"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn orch_rejects_unknown_trust_as_needs_approval() {
    let mut db = DbFactory::create_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    seed_dataset_version(
        &db,
        "ds-unknown",
        "dsv-unknown",
        "tenant-1",
        "unknown",
        "hash-unknown",
    )
    .await;
    let service = TrainingService::with_db(db.clone(), std::env::temp_dir());

    let cfg = minimal_config();
    let result = service
        .start_training(
            "adapter".into(),
            cfg,
            None,
            None,
            None,
            None,
            None,
            Some(vec![DatasetVersionSelection {
                dataset_version_id: "dsv-unknown".into(),
                weight: 1.0,
            }]),
            false,
            DataLineageMode::Versioned,
            Some("tenant-1".into()),
            Some("tester".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await;

    let err = format!("{:?}", result.err().expect("expected error"));
    assert!(
        err.contains("trust_state=unknown"),
        "unexpected error: {}",
        err
    );
}

#[tokio::test]
async fn orch_rejects_data_spec_hash_mismatch() {
    let mut db = DbFactory::create_in_memory().await.expect("db");
    db.migrate().await.expect("migrate");
    seed_dataset_version(&db, "ds-hash", "dsv-hash", "tenant-1", "allowed", "hash-ok").await;
    let service = TrainingService::with_db(db.clone(), std::env::temp_dir());

    let cfg = minimal_config();
    let result = service
        .start_training(
            "adapter".into(),
            cfg,
            None,
            None,
            None,
            None,
            None,
            Some(vec![DatasetVersionSelection {
                dataset_version_id: "dsv-hash".into(),
                weight: 1.0,
            }]),
            false,
            DataLineageMode::Versioned,
            Some("tenant-1".into()),
            Some("tester".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,                      // retry_of_job_id
            None,                      // versioning
            None,                      // code_commit_sha
            None,                      // data_spec_json
            Some("wrong-hash".into()), // data_spec_hash
        )
        .await;

    let err = format!("{:?}", result.err().expect("expected error"));
    assert!(
        err.contains("DATA_SPEC_HASH_MISMATCH"),
        "unexpected error: {}",
        err
    );
}
