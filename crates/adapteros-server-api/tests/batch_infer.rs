use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use adapteros_orchestrator::TrainingService;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers::batch;
use adapteros_server_api::state::{
    ApiConfig, AppState, MetricsConfig, OperationRetryConfig, RepositoryPathsConfig, SecurityConfig,
};
use adapteros_server_api::types::{
    BatchInferItemRequest, BatchInferRequest, InferRequest, WorkerInferRequest,
};
use axum::extract::State;
use axum::{http::StatusCode, Extension, Json};
use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

async fn setup_state(uds_path: Option<&PathBuf>) -> anyhow::Result<AppState> {
    let db = adapteros_db::Db::connect(":memory:").await?;

    adapteros_db::sqlx::query(
        "CREATE TABLE workers (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            node_id TEXT NOT NULL,
            plan_id TEXT NOT NULL,
            uds_path TEXT NOT NULL,
            pid INTEGER,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            last_seen_at TEXT
        )",
    )
    .execute(db.pool())
    .await?;

    if let Some(path) = uds_path {
        adapteros_db::sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at)
             VALUES (?, ?, ?, ?, ?, NULL, 'ready', '2024-01-01T00:00:00Z', NULL)",
        )
        .bind("worker-1")
        .bind("tenant-1")
        .bind("node-1")
        .bind("plan-1")
        .bind(path.to_string_lossy().to_string())
        .execute(db.pool())
        .await?;
    }

    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: String::new(),
            system_metrics_interval_secs: 0,
            telemetry_buffer_capacity: 1000,
            telemetry_channel_capacity: 100,
            trace_buffer_capacity: 100,
            server_port: 9090,
            server_enabled: false,
        },
        golden_gate: None,
        bundles_root: "var/bundles".to_string(),
        repository_paths: RepositoryPathsConfig::default(),
        model_load_timeout_secs: 300,
        model_unload_timeout_secs: 30,
        operation_retry: OperationRetryConfig::default(),
        security: SecurityConfig::default(),
        mlx: None,
        production_mode: false,
        rate_limits: None,
        path_policy: adapteros_server_api::state::PathPolicyConfig::default(),
    };

    let metrics = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
        0.1, 0.5, 1.0,
    ])?);
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new()?);
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }

    let training_service = Arc::new(TrainingService::new());

    Ok(AppState::with_sqlite(
        db,
        b"test-secret".to_vec(),
        Arc::new(RwLock::new(config)),
        metrics,
        metrics_collector,
        metrics_registry,
        training_service,
    ))
}

fn test_claims() -> Claims {
    Claims {
        sub: "tenant-1-user".to_string(),
        email: "user@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: "tenant-1".to_string(),
        exp: 0,
        iat: 0,
        jti: "test-token".to_string(),
        nbf: 0,
    }
}

async fn handle_connection(stream: &mut UnixStream) -> anyhow::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let header_end = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|idx| idx + 4)
        .unwrap_or(buffer.len());
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, ':');
            let name = parts.next()?.trim().to_ascii_lowercase();
            if name == "content-length" {
                parts.next()?.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let mut additional = vec![0u8; content_length - body.len()];
        let read = stream.read(&mut additional).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&additional[..read]);
    }

    let request: WorkerInferRequest = serde_json::from_slice(&body)?;

    if request.prompt.contains("slow") {
        tokio::time::sleep(Duration::from_secs(35)).await;
    } else {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let response_body = json!({
        "text": format!("echo: {}", request.prompt),
        "status": "completed",
        "trace": {
            "router_summary": {
                "adapters_used": ["adapter-a"]
            }
        }
    })
    .to_string();

    let http_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );

    stream.write_all(http_response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn spawn_mock_worker() -> anyhow::Result<(TempDir, PathBuf, tokio::task::JoinHandle<()>)> {
    let dir = tempfile::tempdir()?;
    let socket_path = dir.path().join("worker.sock");
    let listener = UnixListener::bind(&socket_path)?;

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, _)) => {
                    if handle_connection(&mut stream).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok((dir, socket_path, handle))
}

#[tokio::test]
async fn batch_infer_processes_multiple_requests() -> anyhow::Result<()> {
    let (temp_dir, socket_path, handle) = spawn_mock_worker().await?;
    let state = setup_state(Some(&socket_path)).await?;
    let claims = test_claims();

    let request = BatchInferRequest {
        requests: vec![
            BatchInferItemRequest {
                id: "req-1".to_string(),
                request: InferRequest {
                    prompt: "first".to_string(),
                    max_tokens: Some(128),
                    temperature: None,
                    top_k: None,
                    top_p: None,
                    seed: None,
                    require_evidence: Some(false),
                },
            },
            BatchInferItemRequest {
                id: "req-2".to_string(),
                request: InferRequest {
                    prompt: "second".to_string(),
                    max_tokens: Some(64),
                    temperature: None,
                    top_k: None,
                    top_p: None,
                    seed: None,
                    require_evidence: Some(false),
                },
            },
        ],
    };

    let response =
        match batch::batch_infer(State(state.clone()), Extension(claims), Json(request)).await {
            Ok(r) => r,
            Err((status, err_json)) => {
                return Err(anyhow::anyhow!(format!(
                    "handler error {}: {}",
                    status,
                    serde_json::to_string(&err_json.0).unwrap_or_default()
                )));
            }
        };
    let Json(batch_response) = response;

    assert_eq!(batch_response.responses.len(), 2);
    assert_eq!(batch_response.responses[0].id, "req-1");
    assert_eq!(
        batch_response.responses[0].response.as_ref().unwrap().text,
        "echo: first"
    );
    assert!(batch_response.responses[0].error.is_none());
    assert_eq!(batch_response.responses[1].id, "req-2");
    assert!(batch_response.responses[1].error.is_none());

    handle.abort();
    drop(temp_dir);
    Ok(())
}

#[tokio::test]
async fn batch_infer_enforces_max_size() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims = test_claims();

    let requests = (0..33)
        .map(|idx| BatchInferItemRequest {
            id: format!("req-{idx}"),
            request: InferRequest {
                prompt: "prompt".to_string(),
                max_tokens: None,
                temperature: None,
                top_k: None,
                top_p: None,
                seed: None,
                require_evidence: None,
            },
        })
        .collect();

    let result = batch::batch_infer(
        State(state),
        Extension(claims),
        Json(BatchInferRequest { requests }),
    )
    .await;

    assert!(matches!(result, Err((StatusCode::BAD_REQUEST, _))));
    Ok(())
}

#[tokio::test]
async fn batch_infer_marks_timeouts() -> anyhow::Result<()> {
    let (temp_dir, socket_path, handle) = spawn_mock_worker().await?;
    let state = setup_state(Some(&socket_path)).await?;
    let claims = test_claims();

    let request = BatchInferRequest {
        requests: vec![
            BatchInferItemRequest {
                id: "fast".to_string(),
                request: InferRequest {
                    prompt: "fast".to_string(),
                    max_tokens: None,
                    temperature: None,
                    top_k: None,
                    top_p: None,
                    seed: None,
                    require_evidence: None,
                },
            },
            BatchInferItemRequest {
                id: "slow".to_string(),
                request: InferRequest {
                    prompt: "slow request".to_string(),
                    max_tokens: None,
                    temperature: None,
                    top_k: None,
                    top_p: None,
                    seed: None,
                    require_evidence: None,
                },
            },
        ],
    };

    let response =
        match batch::batch_infer(State(state.clone()), Extension(claims), Json(request)).await {
            Ok(r) => r,
            Err((status, err_json)) => {
                return Err(anyhow::anyhow!(format!(
                    "handler error {}: {}",
                    status,
                    serde_json::to_string(&err_json.0).unwrap_or_default()
                )));
            }
        };
    let Json(batch_response) = response;

    assert_eq!(batch_response.responses.len(), 2);
    assert!(batch_response.responses[0].error.is_none());
    assert_eq!(batch_response.responses[1].id, "slow");
    let timeout_error = batch_response.responses[1].error.as_ref().unwrap();
    assert_eq!(timeout_error.code, "REQUEST_TIMEOUT");

    handle.abort();
    drop(temp_dir);
    Ok(())
}
