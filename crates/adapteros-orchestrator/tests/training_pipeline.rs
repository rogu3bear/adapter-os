use std::path::Path;
use std::time::Duration;

use adapteros_db::sqlx;
use adapteros_orchestrator::TrainingService;
use adapteros_storage::platform::common::PlatformUtils;
use adapteros_types::training::{DataLineageMode, TrainingConfig, TrainingJobStatus};
use tempfile::TempDir;

/// End-to-end pipeline: start training → package .aos → register adapter → materialize artifact.
#[tokio::test(flavor = "current_thread")]
async fn training_pipeline_produces_registered_aos() {
    let tmp_root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
    let temp_dir = TempDir::new_in(&tmp_root).expect("tempdir");
    let db_path = temp_dir.path().join("cp.sqlite3");
    // SAFETY: This test runs on a single-threaded Tokio runtime, so no other
    // threads read or write environment variables concurrently.
    unsafe {
        std::env::set_var("AOS_ALLOW_NONDET_TRAINING", "1");
    }

    let db = adapteros_db::Db::connect(db_path.to_str().unwrap())
        .await
        .unwrap();
    db.migrate()
        .await
        .or_else(|e| {
            // Some in-memory/temp runs may attempt to apply migrations twice; treat duplicate version as already migrated.
            if e.to_string().contains("_sqlx_migrations.version") {
                Ok(())
            } else {
                Err(e)
            }
        })
        .unwrap();
    // Seed a system tenant for registration FKs
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind("system")
        .bind("system")
        .execute(db.pool())
        .await
        .unwrap();
    // Seed a base model in the models table to satisfy FK (minimal placeholder hashes)
    sqlx::query("INSERT INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, license_hash_b3, license_text, model_card_hash_b3, tenant_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("qwen2.5-7b") // id must match base_model_id supplied to training_service
        .bind("qwen2.5-7b")
        .bind("00")
        .bind("00")
        .bind("00")
        .bind("00")
        .bind("00")
        .bind("placeholder")
        .bind("placeholder-card")
        .bind("system")
        .execute(db.pool())
        .await
        .unwrap();
    let storage_root = temp_dir.path().to_path_buf();
    let adapters_root = storage_root.join("adapters");
    std::fs::create_dir_all(&adapters_root).unwrap();
    let post_actions_json = serde_json::json!({
        "adapters_root": adapters_root.to_string_lossy(),
        "package": true,
        "register": true,
        "create_stack": false,
        "activate_stack": false
    })
    .to_string();

    let service = TrainingService::with_db(db.clone(), storage_root.clone());

    let mut config = TrainingConfig::default_for_adapter();
    config.rank = 2;
    config.alpha = 8;
    config.epochs = 1;
    config.batch_size = 1;
    config.learning_rate = 0.01;
    config.targets = vec!["q_proj".to_string()];

    // Launch training with tenant/base model metadata so registration can succeed.
    let job = service
        .start_training(
            "pipeline-adapter".to_string(),
            config,
            None, // template_id
            None, // repo_id
            None, // target_branch
            None, // base_version_id
            None, // dataset_id (synthetic fallback)
            None, // dataset_version_ids
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            Some("system".into()),           // tenant_id
            Some("user-test".into()),        // initiated_by
            Some("admin".into()),            // initiated_by_role
            Some("qwen2.5-7b".into()),       // base_model_id
            None,                            // collection_id
            None,                            // scope
            None,                            // lora_tier
            None,                            // category
            None,                            // description
            None,                            // language
            None,                            // framework_id
            None,                            // framework_version
            Some(post_actions_json.clone()), // post_actions_json (explicit paths)
            None,                            // retry_of_job_id
            None,                            // versioning
            None,                            // code_commit_sha
            None,                            // data_spec_json
            None,                            // data_spec_hash
        )
        .await
        .expect("training job should start");

    // Wait for completion (should be fast for tiny config)
    let mut completed = false;
    let mut last_status = String::new();
    let mut last_error: Option<String> = None;
    for _ in 0..120 {
        let current = service.get_job(&job.id).await.expect("job lookup");
        last_status = format!("{:?}", current.status);
        last_error = current.error_message.clone();
        if current.status == TrainingJobStatus::Completed {
            completed = true;
            // Verify artifact path is present and ends with .aos
            let artifact_path = current
                .artifact_path
                .as_ref()
                .expect("artifact path should be set");
            assert!(
                artifact_path.ends_with(".aos"),
                "artifact should be .aos, got {}",
                artifact_path
            );
            assert!(
                Path::new(artifact_path).exists(),
                "artifact file should exist at {}",
                artifact_path
            );

            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(
        completed,
        "training job should complete (last status: {}, last error: {:?})",
        last_status, last_error
    );

    // Allow executor to drain; ignore errors if already finished.
}
