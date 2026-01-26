//! CoreML export flow tests

#![allow(clippy::field_reassign_with_default)]
#![allow(unused_mut)]

use adapteros_storage::platform::common::PlatformUtils;
use tempfile::TempDir;

use crate::test_support::TestEnvGuard;
#[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
use crate::training::coreml::perform_coreml_export;
use crate::training::job::{
    DataLineageMode, TrainingBackendKind, TrainingConfig, TrainingJob, TrainingJobStatus,
};
use crate::training::service::TrainingService;
#[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
use adapteros_lora_worker::{ComputeUnits, CoreMLExportJob};

fn new_test_tempdir() -> TempDir {
    let root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

#[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
#[test]
fn stub_coreml_export_path_is_invokable_when_allowed() {
    let _env = TestEnvGuard::new();
    let tmp = new_test_tempdir();
    let base = tmp.path().join("base.json");
    let adapter = tmp.path().join("adapter.aos");
    std::fs::write(&base, b"base-bytes").unwrap();
    std::fs::write(&adapter, b"adapter-bytes").unwrap();

    std::env::set_var("AOS_ALLOW_COREML_EXPORT_STUB", "1");
    let record = perform_coreml_export(CoreMLExportJob {
        base_package: base.clone(),
        adapter_aos: adapter.clone(),
        output_package: tmp.path().join("fused"),
        compute_units: ComputeUnits::All,
        base_model_id: None,
        adapter_id: None,
    })
    .expect("stub export should be allowed when env enabled");
    std::env::remove_var("AOS_ALLOW_COREML_EXPORT_STUB");

    assert!(record.metadata_path.exists());
}

#[tokio::test]
async fn start_training_records_coreml_intent_and_fallback() {
    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.preferred_backend = Some(TrainingBackendKind::CoreML);
    config.coreml_training_fallback = Some(TrainingBackendKind::Mlx);

    let job = service
        .start_training(
            "coreml-intent".to_string(),
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

    assert_eq!(job.requested_backend.as_deref(), Some("coreml"));
    assert_eq!(job.coreml_training_fallback.as_deref(), Some("mlx"));
    assert!(job.backend.is_none(), "backend is recorded post-selection");
}

#[tokio::test]
async fn coreml_export_flow_updates_job_and_registry() {
    let _env = TestEnvGuard::new();
    std::env::set_var("AOS_ALLOW_COREML_EXPORT_STUB", "1");
    let temp = new_test_tempdir();
    let base_dir = temp.path().join("base");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(base_dir.join("Manifest.json"), "{}").unwrap();
    std::env::set_var("AOS_MODEL_PATH", base_dir.to_string_lossy().to_string());
    let aos_path = temp.path().join("adapter.aos");
    std::fs::write(&aos_path, b"adapter-bytes").unwrap();

    let mut db = adapteros_db::factory::DbFactory::create_in_memory()
        .await
        .expect("db");
    db.migrate().await.expect("migrate");

    let service = TrainingService::with_db(db.clone(), temp.path().to_path_buf());
    let mut job = TrainingJob::new(
        "job-export".into(),
        "adapter-export".into(),
        TrainingConfig::default(),
    );
    job.status = TrainingJobStatus::Completed;
    job.adapter_id = Some("adapter-export".into());
    job.aos_path = Some(aos_path.to_string_lossy().to_string());
    job.manifest_base_model = Some("base-model-x".into());
    job.package_hash_b3 = Some("hash123".into());
    job.tenant_id = Some("tenant-test".into());
    service.insert_job_for_test(job).await;

    let updated = service
        .export_coreml_for_job("job-export")
        .await
        .expect("export");

    assert_eq!(
        updated.coreml_export_status.as_deref(),
        Some("metadata_only")
    );
    assert!(updated.coreml_fused_package_hash.is_some());

    let pair = db
        .get_coreml_fusion_pair("tenant-test", "base-model-x", "adapter-export")
        .await
        .expect("pair lookup");
    assert!(pair.is_some(), "fusion pair should be recorded");

    std::env::remove_var("AOS_MODEL_PATH");
    std::env::remove_var("AOS_ALLOW_COREML_EXPORT_STUB");
}
