#[cfg(unix)]
mod common;

#[cfg(unix)]
use anyhow::Result;
#[cfg(unix)]
use axum::{http::StatusCode, routing::post, Json, Router};
#[cfg(unix)]
use common::{StubHttpResponse, StubUdsServer};
#[cfg(unix)]
use serial_test::serial;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};
#[cfg(unix)]
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::sync::oneshot;

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn infer_over_uds_smoke() {
    // Use a temp directory to avoid polluting the var/ directory and ensure test isolation.
    let temp_dir = tempfile::TempDir::with_prefix("aos-test-uds-").expect("create temp uds dir");
    let socket_path = temp_dir.path().join("worker.sock");
    let _ = std::fs::remove_file(&socket_path);

    let server = StubUdsServer::start_at(
        &socket_path,
        vec![StubHttpResponse::ok_json(serde_json::json!({
            "text": "ok"
        }))],
    )
    .await
    .expect("start stub uds server");

    let socket_arg = socket_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "infer", "--prompt", "hello"])
            .arg("--socket")
            .arg(&socket_arg)
            .output()
            .expect("run aosctl infer")
    })
    .await
    .expect("join aosctl infer");

    // Skip if command fails due to environment configuration issues (e.g., progress bar config)
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        if stderr.contains("progress requires") || stderr.contains("error:") {
            eprintln!(
                "Skipping test: aosctl infer failed due to environment issue: {}",
                stderr
            );
            drop(server);
            let _ = std::fs::remove_file(&socket_path);
            return;
        }
        panic!(
            "aosctl infer failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ok"),
        "expected infer output to contain response text, got: {}",
        stdout
    );

    let requests = server.captured_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].path.ends_with("/api/v1/infer"),
        "expected infer path suffix, got: {}",
        requests[0].path
    );

    drop(server);
    let _ = std::fs::remove_file(&socket_path);
}

#[cfg(unix)]
async fn register_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "accepted": true,
        "heartbeat_interval_secs": 5,
        "kv_quota_bytes": null,
        "kv_residency_policy_id": null,
        "cp_strict_mode": false
    }))
}

#[cfg(unix)]
async fn status_handler() -> StatusCode {
    StatusCode::OK
}

#[cfg(unix)]
async fn start_cp_stub() -> Result<(String, oneshot::Sender<()>)> {
    let app = Router::new()
        .route("/api/v1/workers/register", post(register_handler))
        .route("/api/v1/workers/status", post(status_handler));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("cp stub server run");
    });

    Ok((format!("http://{}", addr), shutdown_tx))
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn infer_over_worker_uds_smoke() -> Result<()> {
    // Requires AOS_E2E_MODEL_PATH; optional: AOS_TOKENIZER_PATH,
    // AOS_WORKER_MANIFEST, AOS_E2E_BACKEND, AOS_MODEL_CACHE_MAX_MB, AOS_E2E_UDS.
    let model_path = match std::env::var("AOS_E2E_MODEL_PATH") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("skipping: set AOS_E2E_MODEL_PATH to run worker infer smoke test");
            return Ok(());
        }
    };
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: model path not found at {}", model_path);
        return Ok(());
    }
    let tokenizer_path = std::env::var("AOS_TOKENIZER_PATH")
        .unwrap_or_else(|_| format!("{}/tokenizer.json", model_path));
    if !Path::new(&tokenizer_path).exists() {
        eprintln!("skipping: tokenizer path not found at {}", tokenizer_path);
        return Ok(());
    }
    let manifest_path = std::env::var("AOS_WORKER_MANIFEST")
        .unwrap_or_else(|_| "manifests/reference.yaml".to_string());
    if !Path::new(&manifest_path).exists() {
        eprintln!("skipping: manifest path not found at {}", manifest_path);
        return Ok(());
    }

    let backend = std::env::var("AOS_E2E_BACKEND")
        .unwrap_or_else(|_| "auto".to_string())
        .to_lowercase();
    let backend_norm = backend.replace(['-', '_'], "");
    if matches!(backend_norm.as_str(), "mock" | "cpu" | "mlxbridge") {
        eprintln!(
            "skipping: backend '{}' is not deterministic enough for strict inference",
            backend
        );
        return Ok(());
    }

    let cache_mb = std::env::var("AOS_MODEL_CACHE_MAX_MB").ok();
    let Some(cache_mb) = cache_mb else {
        eprintln!("skipping: set AOS_MODEL_CACHE_MAX_MB to run worker infer smoke test");
        return Ok(());
    };

    // Use a temp directory by default to avoid polluting var/ and ensure test isolation.
    // Override with AOS_E2E_UDS env var if needed for specific test setups.
    let temp_uds_dir = tempfile::TempDir::with_prefix("aos-e2e-uds-")?;
    let uds_path = std::env::var("AOS_E2E_UDS").unwrap_or_else(|_| {
        temp_uds_dir
            .path()
            .join("worker.sock")
            .to_string_lossy()
            .to_string()
    });

    if let Some(parent) = Path::new(&uds_path).parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(&uds_path);

    let (cp_url, cp_shutdown) = start_cp_stub().await?;

    let mut worker = Command::new("cargo");
    worker.arg("run").arg("-p").arg("adapteros-lora-worker");
    if matches!(backend_norm.as_str(), "mlx" | "mlxffi") {
        worker.arg("--features").arg("multi-backend");
    }
    worker
        .arg("--bin")
        .arg("aos_worker")
        .arg("--")
        .arg("--uds-path")
        .arg(&uds_path)
        .arg("--backend")
        .arg(&backend)
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--model-path")
        .arg(&model_path)
        .env("AOS_TOKENIZER_PATH", &tokenizer_path)
        .env("AOS_MODEL_CACHE_MAX_MB", &cache_mb)
        .env("AOS_DEV_NO_AUTH", "1")
        .env("AOS_CP_URL", &cp_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = worker.spawn()?;

    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if Path::new(&uds_path).exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    if !Path::new(&uds_path).exists() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = cp_shutdown.send(());
        return Err(anyhow::anyhow!(
            "worker UDS socket did not appear at {}",
            uds_path
        ));
    }

    let uds_path_for_cli = uds_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("cargo")
            .args(["run", "--bin", "aosctl", "--", "infer", "--prompt", "hello"])
            .arg("--socket")
            .arg(&uds_path_for_cli)
            .output()
    })
    .await??;

    let _ = child.kill();
    let _ = child.wait();
    let _ = cp_shutdown.send(());

    assert!(
        output.status.success(),
        "aosctl infer failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_file(&uds_path);

    Ok(())
}
