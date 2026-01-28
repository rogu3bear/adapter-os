//! Training control integration tests

#![allow(deprecated)]

use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "coreml-backend")]
use adapteros_aos::open_aos;
use adapteros_api_types::{
    workers::WorkerCapabilities, DatasetVersionSelection, StartTrainingRequest,
    TrainingConfigRequest, TrainingListParams,
};
use adapteros_core::B3Hash;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
#[cfg(feature = "coreml-backend")]
use adapteros_lora_worker::training::AdapterManifest;
use adapteros_orchestrator::training::compute_combined_data_spec_hash;
use adapteros_orchestrator::TrainingJobStatus;
use adapteros_server_api::handlers::get_training_logs;
use adapteros_server_api::handlers::training::{
    cancel_training, list_training_jobs, start_training,
};
use adapteros_server_api::state::AppState;
use adapteros_types::training::{BranchClassification, TrainingConfig};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{extract::State, Extension, Json, Router};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::sleep;
use tower::ServiceExt;
#[cfg(feature = "coreml-backend")]
use walkdir::WalkDir;

mod common;
use common::{
    create_test_dataset, register_test_model, register_test_worker, test_admin_claims,
    TestkitEnvGuard,
};

async fn create_test_repo(
    state: &AppState,
    tenant_id: &str,
    created_by: &str,
    base_model_id: &str,
) -> String {
    state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id,
            name: "test-repo",
            base_model_id: Some(base_model_id),
            default_branch: Some("main"),
            created_by: Some(created_by),
            description: None,
        })
        .await
        .expect("create adapter repository")
}

#[cfg(all(feature = "coreml-backend", target_os = "macos"))]
fn coreml_runtime_available() -> bool {
    adapteros_lora_kernel_coreml::is_coreml_available()
        && adapteros_lora_kernel_coreml::is_neural_engine_available()
}

#[cfg(all(feature = "coreml-backend", not(target_os = "macos")))]
fn coreml_runtime_available() -> bool {
    false
}

#[cfg(feature = "coreml-backend")]
fn has_coreml_artifacts(path: &PathBuf) -> bool {
    let has_coreml_extension = |p: &std::path::Path| {
        p.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "mlmodel" | "mlmodelc" | "mlpackage"))
            .unwrap_or(false)
    };

    if path.is_file() {
        return has_coreml_extension(path);
    }

    WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .any(|entry| has_coreml_extension(entry.path()))
}

fn make_request(name: &str, repo_id: String, base_model_id: &str) -> StartTrainingRequest {
    let cfg = TrainingConfig::quick_training();
    StartTrainingRequest {
        adapter_name: name.to_string(),
        config: TrainingConfigRequest {
            rank: cfg.rank,
            alpha: cfg.alpha,
            targets: cfg.targets,
            training_contract_version: cfg.training_contract_version,
            pad_token_id: cfg.pad_token_id,
            ignore_index: cfg.ignore_index,
            coreml_training_fallback: None,
            coreml_placement: None,
            epochs: cfg.epochs,
            learning_rate: cfg.learning_rate,
            batch_size: cfg.batch_size,
            warmup_steps: cfg.warmup_steps,
            max_seq_length: cfg.max_seq_length,
            gradient_accumulation_steps: cfg.gradient_accumulation_steps,
            validation_split: cfg.validation_split,
            preferred_backend: None,
            backend_policy: None,
            enable_coreml_export: None,
            require_gpu: None,
            max_gpu_memory_mb: None,
            base_model_path: None,
            preprocessing: None,
            force_resume: None,
            multi_module_training: None,
            lora_layer_indices: None,
        },
        template_id: None,
        repo_id: Some(repo_id),
        target_branch: None,
        branch_classification: Some(BranchClassification::Protected),
        base_version_id: None,
        code_commit_sha: None,
        data_spec: None,
        data_spec_hash: None,
        hyperparameters: None,
        dataset_id: None,
        dataset_version_ids: None,
        synthetic_mode: true,
        data_lineage_mode: None,
        base_model_id: base_model_id.to_string(),
        collection_id: None,
        lora_tier: None,
        scope: None,
        category: None,
        description: None,
        language: None,
        symbol_targets: None,
        framework_id: None,
        framework_version: None,
        api_patterns: None,
        repo_scope: None,
        file_patterns: None,
        exclude_patterns: None,
        post_actions: None,
    }
}

async fn setup_training_state() -> (AppState, TempDir, String, bool) {
    std::env::set_var("AOS_ALLOW_NONDET_TRAINING", "1");
    let mut state = common::setup_state(None).await.expect("state");
    let temp_dir = tempfile::TempDir::with_prefix("aos-test-").expect("tempdir");

    if let Some(service) = Arc::get_mut(&mut state.training_service) {
        service.set_db(state.db.raw().clone());
        service.set_storage_root(temp_dir.path().to_path_buf());
    } else {
        state.training_service = Arc::new(adapteros_orchestrator::TrainingService::with_db(
            state.db.raw().clone(),
            temp_dir.path().to_path_buf(),
        ));
    }

    let (model_path, has_real_model) = match std::env::var("AOS_TEST_MODEL_PATH") {
        Ok(raw) => {
            let path = PathBuf::from(raw);
            if path.exists() {
                (path, true)
            } else {
                (temp_dir.path().join("model.safetensors"), false)
            }
        }
        Err(_) => (temp_dir.path().join("model.safetensors"), false),
    };
    if !model_path.exists() {
        std::fs::write(&model_path, b"stub").expect("write model stub");
    }
    let base_model_id = register_test_model(&state, &model_path)
        .await
        .expect("register model");

    let caps = WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("register worker");

    (state, temp_dir, base_model_id, has_real_model)
}

async fn wait_for_terminal(state: &AppState, job_id: &str) -> TrainingJobStatus {
    for _ in 0..120 {
        let job = state.training_service.get_job(job_id).await.unwrap();
        match job.status {
            TrainingJobStatus::Completed
            | TrainingJobStatus::Cancelled
            | TrainingJobStatus::Failed => return job.status,
            _ => sleep(Duration::from_millis(50)).await,
        }
    }
    state.training_service.get_job(job_id).await.unwrap().status
}

#[tokio::test]
async fn test_training_start() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims),
        Json(make_request("adapter-start", repo_id, &base_model_id)),
    )
    .await
    .expect("start training");

    assert!(!job.id.is_empty(), "job id should be returned");
    assert_eq!(job.adapter_name, "adapter-start");
}

#[cfg(feature = "coreml-backend")]
#[tokio::test]
async fn training_coreml_export_writes_artifacts_and_metadata() {
    let _guard = common::env_lock().await;
    if !coreml_runtime_available() {
        eprintln!(
            "SKIP: CoreML export test requires macOS + coreml-backend feature + ANE/CoreML availability"
        );
        return;
    }

    let base_package = match std::env::var("AOS_COREML_EXPORT_BASE_PACKAGE") {
        Ok(raw) => PathBuf::from(raw),
        Err(_) => {
            eprintln!(
                "SKIP: set AOS_COREML_EXPORT_BASE_PACKAGE to a CoreML .mlpackage/.mlmodelc path"
            );
            return;
        }
    };
    if !base_package.exists() {
        eprintln!(
            "SKIP: AOS_COREML_EXPORT_BASE_PACKAGE does not exist: {}",
            base_package.display()
        );
        return;
    }

    let prior_model_path = std::env::var("AOS_MODEL_PATH").ok();
    std::env::set_var("AOS_MODEL_PATH", base_package.to_string_lossy().to_string());

    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let mut request = make_request("adapter-coreml-export", repo_id, &base_model_id);
    request.config.enable_coreml_export = Some(true);

    let Json(job) = start_training(State(state.clone()), Extension(claims), Json(request))
        .await
        .expect("start training");

    let status = wait_for_terminal(&state, &job.id).await;
    assert_eq!(status, TrainingJobStatus::Completed);

    let job = state.training_service.get_job(&job.id).await.expect("job");
    assert_eq!(job.coreml_export_status.as_deref(), Some("succeeded"));
    let coreml_package_path = PathBuf::from(
        job.coreml_package_path
            .clone()
            .expect("coreml_package_path"),
    );
    let coreml_metadata_path = PathBuf::from(
        job.coreml_metadata_path
            .clone()
            .expect("coreml_metadata_path"),
    );
    assert!(coreml_package_path.exists(), "coreml package missing");
    assert!(coreml_metadata_path.exists(), "coreml metadata missing");
    assert!(
        has_coreml_artifacts(&coreml_package_path),
        "coreml package missing .mlmodel/.mlmodelc/.mlpackage"
    );

    let fusion = adapteros_lora_worker::verify_coreml_export(&coreml_metadata_path)
        .expect("coreml fusion metadata");
    assert_ne!(
        fusion.base_manifest_hash, fusion.fused_manifest_hash,
        "fused manifest hash must differ from base"
    );

    let aos_path = job.aos_path.expect("aos_path");
    let aos_bytes = std::fs::read(&aos_path).expect("read aos");
    let aos = open_aos(&aos_bytes).expect("open aos");
    let manifest: AdapterManifest =
        serde_json::from_slice(aos.manifest_bytes).expect("adapter manifest");
    let placement = manifest.coreml_placement.expect("coreml placement");
    assert!(
        placement.version > 0 && !placement.bindings.is_empty(),
        "coreml placement metadata missing bindings"
    );
    assert!(
        manifest
            .coreml
            .as_ref()
            .map(|meta| meta.coreml_used)
            .unwrap_or(false),
        "manifest should preserve coreml export intent"
    );

    match prior_model_path {
        Some(value) => std::env::set_var("AOS_MODEL_PATH", value),
        None => std::env::remove_var("AOS_MODEL_PATH"),
    }
}

#[tokio::test]
async fn training_rejects_missing_base_model_id() {
    let _env = TestkitEnvGuard::disabled().await;
    let state = common::setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let app = Router::new()
        .route("/v1/training/start", post(start_training))
        .layer(Extension(claims))
        .with_state(state);

    let body = serde_json::json!({
        "adapter_name": "adapter-missing-base",
        "config": {
            "rank": 4,
            "alpha": 8,
            "targets": ["q_proj"],
            "epochs": 1,
            "learning_rate": 0.01,
            "batch_size": 1
        },
        "repo_id": "repo-missing-base",
        "synthetic_mode": true
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/training/start")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request build"),
        )
        .await
        .expect("router response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn training_rejects_unknown_base_model_id() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let req = make_request("adapter-missing-model", repo_id, "missing-model");
    let result = start_training(State(state.clone()), Extension(claims), Json(req)).await;
    let (status, _body) = result.expect_err("missing base model should be rejected");

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_training_rejects_missing_dataset_versions_when_non_synthetic() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let mut req = make_request("adapter-no-dataset", repo_id, &base_model_id);
    req.synthetic_mode = false;

    let result = start_training(State(state.clone()), Extension(claims), Json(req)).await;
    let (status, _body) = result.expect_err("missing dataset_version_ids should be rejected");
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_training_status_completes() {
    let (state, _temp_dir, base_model_id, has_real_model) = setup_training_state().await;
    if !has_real_model {
        eprintln!(
            "SKIPPED: AOS_TEST_MODEL_PATH not set; training completion requires a real model"
        );
        return;
    }
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims),
        Json(make_request("adapter-status", repo_id, &base_model_id)),
    )
    .await
    .expect("start training");

    let status = wait_for_terminal(&state, &job.id).await;
    assert_eq!(status, TrainingJobStatus::Completed);
}

#[tokio::test]
async fn test_training_list_includes_started_job() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(make_request("adapter-list", repo_id, &base_model_id)),
    )
    .await
    .expect("start training");

    let Json(list) = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        axum::extract::Query(TrainingListParams::default()),
    )
    .await
    .expect("list jobs");

    assert!(
        list.jobs.iter().any(|j| j.id == job.id),
        "started job should appear in list"
    );
}

#[tokio::test]
async fn test_training_list_exposes_required_metadata() {
    let (state, _temp_dir, base_model_id, has_real_model) = setup_training_state().await;
    if !has_real_model {
        eprintln!(
            "SKIPPED: AOS_TEST_MODEL_PATH not set; training completion requires a real model"
        );
        return;
    }
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let mut req = make_request("adapter-meta", repo_id.clone(), &base_model_id);
    req.data_spec = Some(r#"{"mode":"synthetic","purpose":"metadata-test"}"#.to_string());

    let Json(job) = start_training(State(state.clone()), Extension(claims.clone()), Json(req))
        .await
        .expect("start training");

    let status = wait_for_terminal(&state, &job.id).await;
    assert_eq!(status, TrainingJobStatus::Completed);

    let Json(list) = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        axum::extract::Query(TrainingListParams::default()),
    )
    .await
    .expect("list jobs");

    let listed = list
        .jobs
        .iter()
        .find(|j| j.id == job.id)
        .expect("job should appear in list");

    assert_eq!(listed.adapter_repo_id.as_deref(), Some(repo_id.as_str()));
    assert!(listed
        .adapter_version_id
        .as_deref()
        .is_some_and(|v| !v.is_empty()));
    assert!(listed
        .config_hash_b3
        .as_deref()
        .is_some_and(|h| !h.is_empty()));
    assert!(listed
        .data_spec_hash
        .as_deref()
        .is_some_and(|h| !h.is_empty()));
    assert!(listed
        .artifact_path
        .as_deref()
        .is_some_and(|p| p.ends_with(".aos")));
    assert!(listed
        .artifact_hash_b3
        .as_deref()
        .is_some_and(|h| !h.is_empty()));
    assert_eq!(listed.artifact_path, listed.aos_path);
    assert_eq!(listed.artifact_hash_b3, listed.package_hash_b3);
}

#[tokio::test]
async fn test_training_logs_return_entries() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(make_request("adapter-logs", repo_id, &base_model_id)),
    )
    .await
    .expect("start training");

    let _ = wait_for_terminal(&state, &job.id).await;

    let Json(logs) = get_training_logs(
        State(state.clone()),
        Extension(claims),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .expect("logs");

    assert!(
        !logs.is_empty(),
        "training logs should include at least one entry"
    );
    assert!(
        logs.iter().any(|l| l.contains("Training job")),
        "logs should contain creation message"
    );
}

#[tokio::test]
async fn test_training_cancel_transitions_job() {
    let (state, _temp_dir, base_model_id, has_real_model) = setup_training_state().await;
    if !has_real_model {
        eprintln!("SKIPPED: AOS_TEST_MODEL_PATH not set; training cancel requires a real model");
        return;
    }
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;

    let mut req = make_request("adapter-cancel", repo_id, &base_model_id);
    req.config.epochs = 25;
    req.config.gradient_accumulation_steps = Some(16);

    let Json(job) = start_training(State(state.clone()), Extension(claims.clone()), Json(req))
        .await
        .expect("start training");

    let status = cancel_training(
        State(state.clone()),
        Extension(claims),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .expect("cancel training");
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

    let terminal = wait_for_terminal(&state, &job.id).await;
    assert!(
        matches!(
            terminal,
            TrainingJobStatus::Cancelled | TrainingJobStatus::Completed
        ),
        "job should end after cancellation request"
    );
}

async fn seed_dataset_version(
    state: &AppState,
    dataset_id: &str,
    version_id: &str,
    tenant_id: &str,
    hash: &str,
) -> anyhow::Result<()> {
    create_test_dataset(state, dataset_id).await?;
    state
        .db
        .create_training_dataset_version_with_id(
            version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "var/test-dataset",
            hash,
            None,
            None,
            Some("tester"),
        )
        .await?;
    // Mark the seeded version as fully validated and trusted so training requests
    // are not blocked by default "unknown" trust state.
    state
        .db
        .update_dataset_version_structural_validation(version_id, "valid", None)
        .await?;
    state
        .db
        .update_dataset_version_safety_status(
            version_id,
            Some("clean"),
            Some("clean"),
            Some("clean"),
            Some("clean"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn ui_path_computes_data_spec_hash_when_missing() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;
    let dataset_id = "ds-ui";
    let version_id = "ds-ui-ver-1";
    let manifest_hash = B3Hash::hash(b"dataset-ui-manifest").to_hex();

    seed_dataset_version(
        &state,
        dataset_id,
        version_id,
        &claims.tenant_id,
        &manifest_hash,
    )
    .await
    .expect("seed dataset version");

    let mut req = make_request("adapter-versioned", repo_id, &base_model_id);
    req.synthetic_mode = false;
    req.dataset_id = Some(dataset_id.to_string());
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: version_id.to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = None;

    let Json(job) = start_training(State(state.clone()), Extension(claims), Json(req))
        .await
        .expect("start training with dataset_version_ids");

    let versions = job.dataset_version_ids.expect("dataset_version_ids");
    assert_eq!(versions[0].dataset_version_id, version_id);
    let combined_hash =
        compute_combined_data_spec_hash(&[(version_id.to_string(), manifest_hash.clone(), 1.0)]);
    assert_eq!(job.data_spec_hash, Some(combined_hash));
}

#[tokio::test]
async fn cli_path_rejects_data_spec_hash_mismatch() {
    let (state, _temp_dir, base_model_id, _has_real_model) = setup_training_state().await;
    let claims = test_admin_claims();
    let repo_id = create_test_repo(&state, &claims.tenant_id, &claims.sub, &base_model_id).await;
    let dataset_id = "ds-cli";
    let version_id = "ds-cli-ver-1";
    let manifest_hash = B3Hash::hash(b"dataset-cli-manifest").to_hex();

    seed_dataset_version(
        &state,
        dataset_id,
        version_id,
        &claims.tenant_id,
        &manifest_hash,
    )
    .await
    .expect("seed dataset version");

    let mut req = make_request("adapter-cli", repo_id, &base_model_id);
    req.synthetic_mode = false;
    req.dataset_id = Some(dataset_id.to_string());
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: version_id.to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = Some("mismatch-hash".to_string());

    let Err((status, Json(err))) =
        start_training(State(state.clone()), Extension(claims), Json(req)).await
    else {
        panic!("expected start_training to reject hash mismatch");
    };

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err.code.as_str(), "DATA_SPEC_HASH_MISMATCH");
}
