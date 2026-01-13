//! Test for auto-train workflow after dataset upload.

mod common;
mod support;

use adapteros_api_types::training::UploadDatasetResponse;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::routes;
use anyhow::{bail, Result};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use support::e2e_harness::{E2eHarness, HarnessSetup};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn test_dataset_upload_auto_train_workflow() {
    let _guard = common::env_lock().await;
    let setup = match E2eHarness::from_env().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("harness init failed: {:#}", e);
            panic!("harness init failed");
        }
    };

    let mut harness = match setup {
        HarnessSetup::Skip { reason } => {
            eprintln!("skipping: {}", reason);
            return;
        }
        HarnessSetup::Ready(h) => h,
    };

    if let Err(e) = run_auto_train_test(&mut harness).await {
        panic!("auto train test failed: {:#}", e);
    }
}

async fn run_auto_train_test(harness: &mut E2eHarness) -> Result<()> {
    let (_paths, app) = prepare_harness_app(harness)?;

    // 1. Upload with auto_train=true
    let boundary = format!("----adapteros-boundary-{}", Uuid::new_v4().simple());
    let jsonl = r#"{"prompt":"Hello","response":"Hi"}
{"prompt":"Goodbye","response":"Bye"}"#;
    let mut body = Vec::new();

    push_form_field(&mut body, &boundary, "name", "auto-train-dataset");
    push_form_field(&mut body, &boundary, "format", "jsonl");
    push_form_field(&mut body, &boundary, "auto_train", "true");
    push_form_field(&mut body, &boundary, "adapter_name", "auto-adapter");

    // We need a valid base model ID. Harness usually has one.
    let model_id = harness.model.registered_id.clone();
    push_form_field(&mut body, &boundary, "base_model_id", &model_id);

    push_file_field(
        &mut body,
        &boundary,
        "file",
        "train.jsonl",
        "application/jsonl",
        jsonl.as_bytes(),
    );
    body.extend(format!("--{}--\r\n", boundary).as_bytes());

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/datasets")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let (status, upload) = send_json::<UploadDatasetResponse>(&app, request).await?;

    if status != StatusCode::OK {
        bail!("upload failed: status {}", status);
    }

    if upload.training_job_id.is_none() {
        bail!("training_job_id missing from response, auto-train failed");
    }

    let job_id = upload.training_job_id.unwrap();
    println!("Auto-train started job: {}", job_id);

    // 2. Verify job exists in DB
    let job = harness.state.db.get_training_job(&job_id).await?;
    if job.is_none() {
        bail!("training job not found in DB");
    }
    let job = job.unwrap();
    if job.adapter_name.as_deref() != Some("auto-adapter") {
        bail!(
            "job adapter name mismatch (expected auto-adapter, got {:?})",
            job.adapter_name
        );
    }

    // 3. Verify stack creation triggered (via post actions default)
    // Note: Stack creation happens AFTER training completes.
    // The upload just starts the job.
    // We don't need to wait for training completion here to verify auto-train started.

    println!("Auto-train trigger verified successfully");

    Ok(())
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

// Helpers copied from golden_path_api_e2e.rs

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
    mime: &str,
    data: &[u8],
) {
    body.extend(format!("--{}\r\n", boundary).as_bytes());
    body.extend(
        format!(
            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
            name, filename
        )
        .as_bytes(),
    );
    body.extend(format!("Content-Type: {}\r\n\r\n", mime).as_bytes());
    body.extend(data);
    body.extend(b"\r\n");
}

async fn send_json<T: DeserializeOwned>(
    app: &axum::Router,
    request: Request<Body>,
) -> Result<(StatusCode, T)> {
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;

    if !status.is_success() {
        // Try to parse error response
        if let Ok(err) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            println!("Error response: {}", serde_json::to_string_pretty(&err)?);
        } else {
            println!("Error body: {:?}", String::from_utf8_lossy(&bytes));
        }
    }

    let body = serde_json::from_slice(&bytes)?;
    Ok((status, body))
}
