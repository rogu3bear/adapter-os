//! Golden path API test using the E2E harness:
//! upload JSONL → rows → train → package .aos → infer.

mod common;
mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use adapteros_aos::open_aos;
use adapteros_api_types::training::{
    DatasetResponse, TrainingConfigRequest, TrainingJobResponse, UploadDatasetResponse,
    TRAINING_DATA_CONTRACT_VERSION,
};
use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_api_types::{InferRequest, API_SCHEMA_VERSION};
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_model_hub::manifest::ManifestV3;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::handlers::chunked_upload::MIN_CHUNK_SIZE;
use adapteros_server_api::handlers::datasets::{
    CompleteChunkedUploadResponse, InitiateChunkedUploadResponse, UploadChunkResponse,
};
use adapteros_server_api::routes;
use adapteros_server_api::types::CreateTrainingJobRequest;
use adapteros_types::training::TrainingBackendKind;
use anyhow::{anyhow, bail, Context, Result};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use serde::de::DeserializeOwned;
use tower::ServiceExt;
use uuid::Uuid;

use support::e2e_harness::{E2eHarness, HarnessSetup};

#[tokio::test]
async fn test_golden_path_api_e2e_harness() {
    let _guard = common::env_lock().await;
    let setup = match E2eHarness::from_env().await {
        Ok(setup) => setup,
        Err(err) => {
            eprintln!("golden path harness init failed: {:#}", err);
            panic!("golden path harness init failed");
        }
    };

    let mut harness = match setup {
        HarnessSetup::Skip { reason } => {
            eprintln!("skipping: {}", reason);
            return;
        }
        HarnessSetup::Ready(h) => h,
    };

    if let Err(err) = run_golden_path(&mut harness).await {
        eprintln!("golden path E2E failed: {:#}", err);
        panic!("golden path E2E failed");
    }
}

#[tokio::test]
async fn test_chunked_upload_full_loop_e2e_harness() {
    let _guard = common::env_lock().await;
    let setup = match E2eHarness::from_env().await {
        Ok(setup) => setup,
        Err(err) => {
            eprintln!("chunked upload harness init failed: {:#}", err);
            panic!("chunked upload harness init failed");
        }
    };

    let mut harness = match setup {
        HarnessSetup::Skip { reason } => {
            eprintln!("skipping: {}", reason);
            return;
        }
        HarnessSetup::Ready(h) => h,
    };

    if let Err(err) = run_chunked_upload_full_loop(&mut harness).await {
        eprintln!("chunked upload E2E failed: {:#}", err);
        panic!("chunked upload E2E failed");
    }
}

#[tokio::test]
async fn test_create_training_job_optional_metadata_roundtrip_e2e_harness() {
    let _guard = common::env_lock().await;
    let setup = match E2eHarness::from_env().await {
        Ok(setup) => setup,
        Err(err) => {
            eprintln!("create training metadata harness init failed: {:#}", err);
            panic!("create training metadata harness init failed");
        }
    };

    let mut harness = match setup {
        HarnessSetup::Skip { reason } => {
            eprintln!("skipping: {}", reason);
            return;
        }
        HarnessSetup::Ready(h) => h,
    };

    if let Err(err) = run_create_training_job_optional_metadata_roundtrip(&mut harness).await {
        eprintln!("create training metadata roundtrip failed: {:#}", err);
        panic!("create training metadata roundtrip failed");
    }
}

fn prepare_harness_app(
    harness: &mut E2eHarness,
) -> Result<(adapteros_server_api::config::PathsConfig, axum::Router)> {
    let paths = {
        let config = harness.state.config.read().unwrap();
        config.paths.clone()
    };

    for root in [
        &paths.datasets_root,
        &paths.artifacts_root,
        &paths.adapters_root,
        &paths.bundles_root,
        &paths.documents_root,
        &paths.plan_dir,
    ] {
        std::fs::create_dir_all(root)?;
    }

    std::env::set_var("AOS_MODEL_PATH", &harness.model.model_dir);

    let mut training_service = TrainingService::with_db(
        harness.state.db.raw().clone(),
        PathBuf::from(&paths.datasets_root),
    );
    training_service.set_artifacts_root(PathBuf::from(&paths.artifacts_root));
    harness.state = harness
        .state
        .clone()
        .with_training_service(std::sync::Arc::new(training_service));

    Ok((paths, routes::build(harness.state.clone())))
}

async fn run_golden_path(harness: &mut E2eHarness) -> Result<()> {
    let token = String::new();
    let tenant_id = "default";

    let (paths, app) = prepare_harness_app(harness)?;

    let model_id = harness.model.registered_id.clone();
    let worker_id = register_worker(
        &harness.state.db,
        tenant_id,
        &harness.uds_path,
        true,
        backend_from_env(),
    )
    .await?;

    let upload = upload_dataset(&app, &token).await?;
    let dataset_version_id = upload
        .dataset_version_id
        .clone()
        .ok_or_else(|| anyhow!("dataset_version_id missing from upload response"))?;

    let dataset = get_dataset(&app, &token, &upload.dataset_id).await?;
    if dataset.status != "ready" {
        bail!("dataset status is {}, expected ready", dataset.status);
    }

    let rows = get_dataset_rows(&app, &token, &dataset_version_id).await?;
    if rows.is_empty() {
        bail!("dataset rows are empty after upload");
    }

    harness
        .trust_dataset_version(&dataset_version_id)
        .await
        .context("mark dataset version safety")?;

    let training_job = start_training_job(
        &app,
        &token,
        tenant_id,
        &model_id,
        &upload.dataset_id,
        &dataset_version_id,
    )
    .await?;

    let job_id = training_job.id.clone();
    let completed = poll_training_job(&app, &token, &job_id, Duration::from_secs(120))
        .await
        .context("wait for training completion")?;

    let mut search_paths: Vec<PathBuf> = Vec::new();
    let app_for_training = app.clone();
    let token_for_training = token.clone();
    let post_training = {
        let search_paths = &mut search_paths;
        async move {
            let adapter_id = completed
                .adapter_id
                .clone()
                .ok_or_else(|| anyhow!("adapter_id missing after training"))?;
            let aos_path = completed
                .aos_path
                .clone()
                .ok_or_else(|| anyhow!("aos_path missing after training"))?;

            *search_paths = build_artifact_search_paths(&paths, &adapter_id, &aos_path);
            verify_aos_manifest(&aos_path, search_paths)?;

            harness
                .state
                .db
                .transition_worker_status(&worker_id, "serving", "ready for inference", None)
                .await
                .context("mark worker serving")?;

            let infer_text = run_infer(
                &app_for_training,
                &token_for_training,
                &model_id,
                &adapter_id,
            )
            .await?;
            if infer_text.trim().is_empty() {
                bail!("inference returned empty completion");
            }

            run_streaming_infer(
                &app_for_training,
                &token_for_training,
                &model_id,
                &adapter_id,
            )
            .await?;

            Ok(())
        }
    }
    .await;

    match post_training {
        Ok(_) => {}
        Err(err) => {
            dump_training_debug(&app, &token, &job_id, &search_paths).await;
            return Err(err);
        }
    }

    Ok(())
}

async fn run_chunked_upload_full_loop(harness: &mut E2eHarness) -> Result<()> {
    let token = String::new();
    let tenant_id = "default";

    let (paths, app) = prepare_harness_app(harness)?;
    let model_id = harness.model.registered_id.clone();

    let worker_id = register_worker(
        &harness.state.db,
        tenant_id,
        &harness.uds_path,
        true,
        backend_from_env(),
    )
    .await?;

    let upload = upload_dataset_chunked(&app, &token).await?;
    let dataset_version_id = upload
        .dataset_version_id
        .clone()
        .ok_or_else(|| anyhow!("dataset_version_id missing from chunked upload response"))?;

    let dataset = get_dataset(&app, &token, &upload.dataset_id).await?;
    if dataset.status != "ready" {
        bail!("dataset status is {}, expected ready", dataset.status);
    }

    let rows = get_dataset_rows(&app, &token, &dataset_version_id).await?;
    if rows.is_empty() {
        bail!("dataset rows are empty after chunked upload");
    }

    harness
        .trust_dataset_version(&dataset_version_id)
        .await
        .context("mark dataset version safety")?;

    let training_job = start_training_job(
        &app,
        &token,
        tenant_id,
        &model_id,
        &upload.dataset_id,
        &dataset_version_id,
    )
    .await?;

    let job_id = training_job.id.clone();
    let completed = poll_training_job(&app, &token, &job_id, Duration::from_secs(120))
        .await
        .context("wait for training completion")?;

    let mut search_paths: Vec<PathBuf> = Vec::new();
    let app_for_training = app.clone();
    let token_for_training = token.clone();
    let post_training = {
        let search_paths = &mut search_paths;
        async move {
            let adapter_id = completed
                .adapter_id
                .clone()
                .ok_or_else(|| anyhow!("adapter_id missing after training"))?;
            let aos_path = completed
                .aos_path
                .clone()
                .ok_or_else(|| anyhow!("aos_path missing after training"))?;

            *search_paths = build_artifact_search_paths(&paths, &adapter_id, &aos_path);
            verify_aos_manifest(&aos_path, search_paths)?;

            harness
                .state
                .db
                .transition_worker_status(&worker_id, "serving", "ready for inference", None)
                .await
                .context("mark worker serving")?;

            let infer_text = run_infer(
                &app_for_training,
                &token_for_training,
                &model_id,
                &adapter_id,
            )
            .await?;
            if infer_text.trim().is_empty() {
                bail!("inference returned empty completion");
            }

            Ok(())
        }
    }
    .await;

    match post_training {
        Ok(_) => {}
        Err(err) => {
            dump_training_debug(&app, &token, &job_id, &search_paths).await;
            return Err(err);
        }
    }

    Ok(())
}

async fn run_create_training_job_optional_metadata_roundtrip(
    harness: &mut E2eHarness,
) -> Result<()> {
    let token = String::new();
    let tenant_id = "default";
    let (_paths, app) = prepare_harness_app(harness)?;
    let model_id = harness.model.registered_id.clone();

    register_worker(
        &harness.state.db,
        tenant_id,
        &harness.uds_path,
        true,
        backend_from_env(),
    )
    .await?;

    let upload = upload_dataset(&app, &token).await?;
    let dataset_version_id = upload
        .dataset_version_id
        .clone()
        .ok_or_else(|| anyhow!("dataset_version_id missing from upload response"))?;

    harness
        .trust_dataset_version(&dataset_version_id)
        .await
        .context("mark dataset version safety")?;

    let repo_id = harness
        .state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id,
            name: "golden-metadata-repo",
            base_model_id: Some(&model_id),
            default_branch: Some("main"),
            created_by: Some("dev-no-auth"),
            description: Some("golden metadata repo"),
        })
        .await?;

    let mut config = default_training_config();
    config.early_stopping = Some(true);
    config.patience = Some(3);
    config.min_delta = Some(0.005);

    let request_body = CreateTrainingJobRequest {
        workspace_id: tenant_id.to_string(),
        base_model_id: model_id,
        dataset_id: upload.dataset_id.clone(),
        dataset_version_id: Some(dataset_version_id.clone()),
        adapter_name: Some(format!("golden-meta-{}", Uuid::new_v4().simple())),
        params: config,
        lora_tier: None,
        template_id: Some("tpl-golden-roundtrip".to_string()),
        repo_id: Some(repo_id.clone()),
        description: Some("golden metadata roundtrip".to_string()),
        adapter_type: Some("identify".to_string()),
        category: Some("code".to_string()),
    };

    let job = start_training_job_with_request(&app, &token, request_body).await?;
    assert_eq!(job.template_id.as_deref(), Some("tpl-golden-roundtrip"));
    assert_eq!(job.repo_id.as_deref(), Some(repo_id.as_str()));
    assert_eq!(job.category.as_deref(), Some("code"));
    assert_eq!(
        job.description.as_deref(),
        Some("golden metadata roundtrip")
    );
    assert_eq!(job.dataset_id.as_deref(), Some(upload.dataset_id.as_str()));

    let versions = job
        .dataset_version_ids
        .as_ref()
        .ok_or_else(|| anyhow!("dataset_version_ids missing from job response"))?;
    if !versions
        .iter()
        .any(|selection| selection.dataset_version_id == dataset_version_id)
    {
        bail!(
            "dataset_version_ids do not include expected version {}",
            dataset_version_id
        );
    }

    let completed = poll_training_job(&app, &token, &job.id, Duration::from_secs(120))
        .await
        .context("wait for metadata roundtrip training completion")?;
    assert_eq!(
        completed.template_id.as_deref(),
        Some("tpl-golden-roundtrip")
    );
    assert_eq!(completed.repo_id.as_deref(), Some(repo_id.as_str()));
    assert_eq!(completed.category.as_deref(), Some("code"));
    assert_eq!(
        completed.description.as_deref(),
        Some("golden metadata roundtrip")
    );
    assert_eq!(
        completed.dataset_id.as_deref(),
        Some(upload.dataset_id.as_str())
    );

    let stored = harness
        .state
        .db
        .get_training_job(&job.id)
        .await?
        .ok_or_else(|| anyhow!("training job {} not found in database", job.id))?;
    let stored_cfg: adapteros_types::training::TrainingConfig =
        serde_json::from_str(&stored.training_config_json).context("parse training_config_json")?;
    assert_eq!(stored.repo_id, repo_id);
    assert_eq!(stored_cfg.early_stopping, Some(true));
    assert_eq!(stored_cfg.patience, Some(3));
    assert!(
        (stored_cfg.min_delta.unwrap_or_default() - 0.005).abs() < 1e-6,
        "expected min_delta 0.005, got {:?}",
        stored_cfg.min_delta
    );

    Ok(())
}

async fn upload_dataset(app: &axum::Router, token: &str) -> Result<UploadDatasetResponse> {
    let boundary = format!("----adapteros-boundary-{}", Uuid::new_v4().simple());
    let jsonl = r#"{"prompt":"Hello","response":"Hi"}
{"prompt":"Goodbye","response":"See you"}"#;
    let mut body = Vec::new();

    push_form_field(&mut body, &boundary, "name", "golden-path-dataset");
    push_form_field(&mut body, &boundary, "format", "jsonl");
    push_form_field(
        &mut body,
        &boundary,
        "description",
        "golden path harness dataset",
    );
    push_file_field(
        &mut body,
        &boundary,
        "file",
        "golden.jsonl",
        "application/jsonl",
        jsonl.as_bytes(),
    );
    body.extend(format!("--{}--\r\n", boundary).as_bytes());

    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/datasets")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={}", boundary),
            ),
        token,
    )
    .body(Body::from(body))
    .unwrap();

    let (status, upload) = send_json::<UploadDatasetResponse>(app, request).await?;
    if status != StatusCode::OK {
        bail!("dataset upload failed with status {}", status);
    }
    if upload.status.as_deref() != Some("ready") {
        bail!(
            "dataset upload status is {:?}, expected ready",
            upload.status
        );
    }
    Ok(upload)
}

fn build_chunked_payload(min_size: usize) -> Vec<u8> {
    let line = b"{\"prompt\":\"hello\",\"response\":\"world\"}\n";
    let lines = min_size.div_ceil(line.len());
    let total_size = lines * line.len();
    line.repeat(lines).into_iter().take(total_size).collect()
}

async fn upload_dataset_chunked(
    app: &axum::Router,
    token: &str,
) -> Result<CompleteChunkedUploadResponse> {
    let payload = build_chunked_payload(MIN_CHUNK_SIZE + 128);
    let total_size = payload.len() as u64;

    let initiate_body = serde_json::json!({
        "file_name": "chunked.jsonl",
        "total_size": total_size,
        "chunk_size": MIN_CHUNK_SIZE,
        "content_type": "application/jsonl"
    });

    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/datasets/chunked-upload/initiate")
            .header("content-type", "application/json"),
        token,
    )
    .body(Body::from(serde_json::to_vec(&initiate_body)?))
    .unwrap();

    let (status, initiate) = send_json::<InitiateChunkedUploadResponse>(app, request).await?;
    if status != StatusCode::OK {
        bail!("chunked upload initiate failed with status {}", status);
    }

    for chunk_index in 0..initiate.expected_chunks {
        let start = chunk_index * initiate.chunk_size;
        let end = std::cmp::min(start + initiate.chunk_size, payload.len());
        let chunk = payload[start..end].to_vec();

        let request = request_with_optional_auth(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/v1/datasets/chunked-upload/{}/chunk?chunk_index={}",
                    initiate.session_id, chunk_index
                ))
                .header("content-type", "application/octet-stream"),
            token,
        )
        .body(Body::from(chunk))
        .unwrap();

        let (status, response) = send_json::<UploadChunkResponse>(app, request).await?;
        if status != StatusCode::OK {
            bail!("chunk upload {} failed with status {}", chunk_index, status);
        }
        if response.chunks_received == 0 {
            bail!(
                "chunk upload {} did not increment chunks_received",
                chunk_index
            );
        }
    }

    let complete_body = serde_json::json!({
        "name": "chunked-golden-path-dataset",
        "format": "jsonl"
    });

    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri(format!(
                "/v1/datasets/chunked-upload/{}/complete",
                initiate.session_id
            ))
            .header("content-type", "application/json"),
        token,
    )
    .body(Body::from(serde_json::to_vec(&complete_body)?))
    .unwrap();

    let (status, complete) = send_json::<CompleteChunkedUploadResponse>(app, request).await?;
    if status != StatusCode::OK {
        bail!("chunked upload complete failed with status {}", status);
    }

    Ok(complete)
}

async fn get_dataset(app: &axum::Router, token: &str, dataset_id: &str) -> Result<DatasetResponse> {
    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/datasets/{}", dataset_id)),
        token,
    )
    .body(Body::empty())
    .unwrap();
    let (status, dataset) = send_json::<DatasetResponse>(app, request).await?;
    if status != StatusCode::OK {
        bail!("dataset lookup failed with status {}", status);
    }
    Ok(dataset)
}

async fn get_dataset_rows(
    app: &axum::Router,
    token: &str,
    dataset_version_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let request = request_with_optional_auth(
        Request::builder().method(Method::GET).uri(format!(
            "/v1/training/dataset_versions/{}/rows",
            dataset_version_id
        )),
        token,
    )
    .body(Body::empty())
    .unwrap();
    let (status, rows) = send_json::<Vec<serde_json::Value>>(app, request).await?;
    if status != StatusCode::OK {
        bail!("dataset rows lookup failed with status {}", status);
    }
    Ok(rows)
}

async fn start_training_job(
    app: &axum::Router,
    token: &str,
    workspace_id: &str,
    base_model_id: &str,
    dataset_id: &str,
    dataset_version_id: &str,
) -> Result<TrainingJobResponse> {
    let request_body = CreateTrainingJobRequest {
        workspace_id: workspace_id.to_string(),
        base_model_id: base_model_id.to_string(),
        dataset_id: dataset_id.to_string(),
        dataset_version_id: Some(dataset_version_id.to_string()),
        adapter_name: Some(format!("golden-adapter-{}", Uuid::new_v4().simple())),
        params: default_training_config(),
        lora_tier: None,
        template_id: None,
        repo_id: None,
        description: None,
        adapter_type: None,
        category: None,
    };

    start_training_job_with_request(app, token, request_body).await
}

async fn start_training_job_with_request(
    app: &axum::Router,
    token: &str,
    request_body: CreateTrainingJobRequest,
) -> Result<TrainingJobResponse> {
    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/training/jobs")
            .header("content-type", "application/json"),
        token,
    )
    .body(Body::from(serde_json::to_vec(&request_body)?))
    .unwrap();

    let (status, job) = send_json::<TrainingJobResponse>(app, request).await?;
    if status != StatusCode::CREATED {
        bail!("training start failed with status {}", status);
    }
    Ok(job)
}

fn default_training_config() -> TrainingConfigRequest {
    TrainingConfigRequest {
        rank: 2,
        alpha: 4,
        targets: vec!["q_proj".to_string()],
        training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
        pad_token_id: 0,
        ignore_index: -100,
        epochs: 1,
        learning_rate: 0.0001,
        batch_size: 1,
        warmup_steps: None,
        max_seq_length: Some(64),
        gradient_accumulation_steps: None,
        validation_split: Some(0.0),
        preferred_backend: backend_from_env(),
        backend_policy: None,
        coreml_training_fallback: None,
        coreml_placement: None,
        enable_coreml_export: None,
        require_gpu: None,
        max_gpu_memory_mb: None,
        base_model_path: None,
        preprocessing: None,
        force_resume: None,
        multi_module_training: None,
        lora_layer_indices: None,
        early_stopping: None,
        patience: None,
        min_delta: None,
    }
}

async fn poll_training_job(
    app: &axum::Router,
    token: &str,
    job_id: &str,
    timeout: Duration,
) -> Result<TrainingJobResponse> {
    let start = Instant::now();
    loop {
        let request = request_with_optional_auth(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/v1/training/jobs/{}", job_id)),
            token,
        )
        .body(Body::empty())
        .unwrap();
        let (status, job) = send_json::<TrainingJobResponse>(app, request).await?;
        if status != StatusCode::OK {
            bail!("training job lookup failed with status {}", status);
        }

        match job.status.as_str() {
            "completed" => return Ok(job),
            "failed" | "cancelled" => {
                dump_training_debug(app, token, job_id, &[]).await;
                bail!("training job ended with status {}", job.status);
            }
            _ => {}
        }

        if start.elapsed() > timeout {
            dump_training_debug(app, token, job_id, &[]).await;
            bail!("training job timed out after {:?}", timeout);
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn verify_aos_manifest(aos_path: &str, search_paths: &[PathBuf]) -> Result<()> {
    let path = PathBuf::from(aos_path);
    if !path.exists() {
        dump_artifact_search_paths(search_paths);
        bail!("missing .aos file at {}", path.display());
    }

    let data = std::fs::read(&path).context("read .aos")?;
    let view = open_aos(&data).context("open .aos")?;
    let manifest = std::str::from_utf8(view.manifest_bytes).context("manifest utf8")?;
    let manifest = ManifestV3::from_json(manifest).context("parse manifest v3")?;
    if manifest.schema != "adapteros.manifest.v3" {
        dump_artifact_search_paths(search_paths);
        bail!(
            "manifest schema is {}, expected adapteros.manifest.v3",
            manifest.schema
        );
    }

    Ok(())
}

async fn run_infer(
    app: &axum::Router,
    token: &str,
    model_id: &str,
    adapter_id: &str,
) -> Result<String> {
    let request_body = InferRequest {
        prompt: "Say hello.".to_string(),
        model: Some(model_id.to_string()),
        adapters: Some(vec![adapter_id.to_string()]),
        ..Default::default()
    };

    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/infer")
            .header("content-type", "application/json"),
        token,
    )
    .body(Body::from(serde_json::to_vec(&request_body)?))
    .unwrap();

    let (status, payload) = send_json::<serde_json::Value>(app, request).await?;
    if status != StatusCode::OK {
        bail!("infer failed with status {}", status);
    }
    Ok(payload
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

async fn run_streaming_infer(
    app: &axum::Router,
    token: &str,
    model_id: &str,
    adapter_id: &str,
) -> Result<()> {
    let request_body = serde_json::json!({
        "prompt": "Stream a short response.",
        "model": model_id,
        "adapters": [adapter_id],
        "max_tokens": 32
    });

    let request = request_with_optional_auth(
        Request::builder()
            .method(Method::POST)
            .uri("/v1/infer/stream")
            .header("content-type", "application/json"),
        token,
    )
    .body(Body::from(serde_json::to_vec(&request_body)?))
    .unwrap();

    let response = app
        .clone()
        .oneshot(request)
        .await
        .context("router response")?;
    let status = response.status();
    if status != StatusCode::OK {
        bail!("streaming infer failed with status {}", status);
    }

    let body = tokio::time::timeout(
        Duration::from_secs(15),
        to_bytes(response.into_body(), usize::MAX),
    )
    .await
    .map_err(|_| anyhow!("streaming infer timed out"))?
    .context("read streaming body")?;

    let text = String::from_utf8_lossy(&body);
    let mut saw_token = false;
    let mut saw_done = false;
    let mut saw_error = false;
    let mut awaiting_error_payload = false;

    for line in text.lines() {
        if line.starts_with("event:") {
            awaiting_error_payload = line.trim_end() == "event: error";
            continue;
        }
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                saw_done = true;
                continue;
            }
            if awaiting_error_payload {
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(data) {
                    if payload.get("code").is_some() && payload.get("message").is_some() {
                        saw_error = true;
                    }
                }
                awaiting_error_payload = false;
                continue;
            }
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(data) {
                if payload
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                    .is_some()
                {
                    saw_token = true;
                }
                if payload
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|f| f.as_str())
                    .is_some()
                {
                    saw_done = true;
                }
            }
        }
    }

    if !saw_done && !saw_error {
        bail!("stream ended without completion or error event");
    }
    if !saw_token && !saw_done {
        bail!("stream produced no tokens or completion marker");
    }

    Ok(())
}

async fn send_json<T: DeserializeOwned>(
    app: &axum::Router,
    request: Request<Body>,
) -> Result<(StatusCode, T)> {
    let response = app
        .clone()
        .oneshot(request)
        .await
        .context("router response")?;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .context("read body")?;
    let parsed = serde_json::from_slice(&body)
        .with_context(|| format!("parse json: {}", String::from_utf8_lossy(&body)))?;
    Ok((status, parsed))
}

fn push_form_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend(format!("--{}\r\n", boundary).as_bytes());
    body.extend(format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes());
    body.extend(value.as_bytes());
    body.extend(b"\r\n");
}

fn push_file_field(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: &str,
    content_type: &str,
    content: &[u8],
) {
    body.extend(format!("--{}\r\n", boundary).as_bytes());
    body.extend(
        format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
            name, filename
        )
        .as_bytes(),
    );
    body.extend(format!("Content-Type: {}\r\n\r\n", content_type).as_bytes());
    body.extend(content);
    body.extend(b"\r\n");
}

async fn register_worker(
    db: &adapteros_db::Db,
    tenant_id: &str,
    uds_path: &Path,
    gpu_backward: bool,
    backend: Option<TrainingBackendKind>,
) -> Result<String> {
    let worker_id = format!("worker-{}", Uuid::new_v4().simple());
    let node_id = format!("node-{}", Uuid::new_v4().simple());
    let plan_id = format!("plan-{}", Uuid::new_v4().simple());
    let manifest_hash = format!("manifest-{}", Uuid::new_v4().simple());

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind(&node_id)
    .bind("test-node")
    .bind("http://localhost:0")
    .execute(db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind(&manifest_hash)
    .bind(tenant_id)
    .bind(&manifest_hash)
    .bind("{}")
    .execute(db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&plan_id)
    .bind(tenant_id)
    .bind(format!("plan-b3:{}", worker_id))
    .bind(&manifest_hash)
    .bind("[]")
    .bind("layout-b3:test")
    .execute(db.pool_result()?)
    .await?;

    let backend_kind = backend
        .unwrap_or(TrainingBackendKind::Auto)
        .as_str()
        .to_string();
    let caps = WorkerCapabilities {
        backend_kind: backend_kind.clone(),
        implementation: None,
        supports_step: true,
        supports_bulk: true,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward,
        multi_backend: false,
    };

    let params = WorkerRegistrationParams {
        worker_id: worker_id.clone(),
        tenant_id: tenant_id.to_string(),
        node_id: node_id.clone(),
        plan_id: plan_id.clone(),
        uds_path: uds_path.to_string_lossy().to_string(),
        pid: 12345,
        manifest_hash,
        backend: Some(backend_kind),
        model_hash_b3: None,
        tokenizer_hash_b3: None,
        tokenizer_vocab_size: None,
        capabilities_json: Some(serde_json::to_string(&caps)?),
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    db.register_worker(params).await?;
    db.transition_worker_status(&worker_id, "healthy", "test", None)
        .await?;

    Ok(worker_id)
}

fn backend_from_env() -> Option<TrainingBackendKind> {
    let value = read_env_with_fallback(
        "AOS_TEST_WORKER_BACKEND",
        &["AOS_E2E_BACKEND", "AOS_E2E_TRAINING_BACKEND"],
    )?;
    match value.to_ascii_lowercase().as_str() {
        "coreml" => Some(TrainingBackendKind::CoreML),
        "mlx" => Some(TrainingBackendKind::Mlx),
        "metal" => Some(TrainingBackendKind::Metal),
        "cpu" => Some(TrainingBackendKind::Cpu),
        "auto" => Some(TrainingBackendKind::Auto),
        _ => None,
    }
}

fn build_artifact_search_paths(
    paths: &adapteros_server_api::config::PathsConfig,
    adapter_id: &str,
    aos_path: &str,
) -> Vec<PathBuf> {
    let mut search = Vec::new();
    search.push(PathBuf::from(aos_path));
    search.push(PathBuf::from(&paths.adapters_root).join(format!("{}.aos", adapter_id)));
    search.push(PathBuf::from(&paths.artifacts_root).join(format!("{}.aos", adapter_id)));
    search
}

fn dump_artifact_search_paths(paths: &[PathBuf]) {
    eprintln!("artifact paths searched:");
    if paths.is_empty() {
        eprintln!("  - (none)");
    } else {
        for path in paths {
            eprintln!("  - {}", path.display());
        }
    }
}

async fn dump_training_debug(
    app: &axum::Router,
    token: &str,
    job_id: &str,
    artifact_paths: &[PathBuf],
) {
    let report_request = request_with_optional_auth(
        Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/training/jobs/{}/report", job_id)),
        token,
    )
    .body(Body::empty())
    .unwrap();
    if let Ok((status, body)) = send_json::<serde_json::Value>(app, report_request).await {
        eprintln!("training job report (status {}): {}", status, body);
    }

    let metrics_request = request_with_optional_auth(
        Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/training/jobs/{}/metrics?limit=5", job_id)),
        token,
    )
    .body(Body::empty())
    .unwrap();
    if let Ok((status, body)) = send_json::<serde_json::Value>(app, metrics_request).await {
        eprintln!("last progress events (status {}): {}", status, body);
    }

    dump_artifact_search_paths(artifact_paths);
}

fn request_with_optional_auth(
    builder: axum::http::request::Builder,
    token: &str,
) -> axum::http::request::Builder {
    if token.is_empty() {
        builder
    } else {
        builder.header("authorization", format!("Bearer {}", token))
    }
}

fn read_env_with_fallback(primary: &str, fallbacks: &[&str]) -> Option<String> {
    std::env::var(primary)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            fallbacks
                .iter()
                .find_map(|name| std::env::var(name).ok())
                .filter(|value| !value.trim().is_empty())
        })
}
