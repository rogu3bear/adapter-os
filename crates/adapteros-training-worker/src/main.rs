use std::path::PathBuf;
use std::sync::Arc;

use adapteros_client::uds::{
    CancelTrainingResponse, UdsTrainingStartRequest, UdsTrainingStartResponse,
};
use adapteros_config::{
    prepare_socket_path, reject_tmp_persistent_path, resolve_training_worker_socket_for_worker,
    ResolvedPath,
};
use adapteros_core::rebase_var_path;
use adapteros_db::Db;
use adapteros_orchestrator::training::TrainingService;
use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

fn resolve_persistent_dir(
    env_key: &str,
    default_rel: &str,
    label: &str,
) -> anyhow::Result<PathBuf> {
    let value = std::env::var(env_key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default_rel.to_string());
    let path = rebase_var_path(PathBuf::from(value));
    reject_tmp_persistent_path(&path, label)
        .map_err(|e| anyhow::anyhow!("Invalid {} path {}: {}", label, path.display(), e))?;
    Ok(path)
}

#[derive(Clone)]
struct AppState {
    service: Arc<TrainingService>,
}

struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

#[derive(Debug, serde::Deserialize)]
struct CancelTrainingRequest {
    job_id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Prevent recursive worker dispatch when this process delegates to TrainingService.
    std::env::set_var("AOS_TRAINING_EXECUTION_MODE", "in_process");

    let socket = resolve_training_worker_socket_for_worker(None)?;
    prepare_socket_path(&socket.path, "training-worker")?;

    let listener = UnixListener::bind(&socket.path)?;
    info!(
        socket_path = %socket.path.display(),
        socket_source = %socket.source,
        "Training worker listening on UDS"
    );

    let db = Db::connect_env()
        .await
        .context("Failed to connect training worker database")?;
    let datasets_root = resolve_persistent_dir(
        "AOS_DATASETS_DIR",
        "var/datasets",
        "training-worker-datasets-root",
    )?;
    std::fs::create_dir_all(&datasets_root).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create datasets root {}: {}",
            datasets_root.display(),
            e
        )
    })?;
    let artifacts_root = resolve_persistent_dir(
        "AOS_ARTIFACTS_DIR",
        "var/artifacts",
        "training-worker-artifacts-root",
    )?;
    std::fs::create_dir_all(&artifacts_root).map_err(|e| {
        anyhow::anyhow!(
            "Failed to create artifacts root {}: {}",
            artifacts_root.display(),
            e
        )
    })?;

    let mut training_service = TrainingService::with_db(db, datasets_root.clone());
    training_service.set_artifacts_root(artifacts_root.clone());
    let state = AppState {
        service: Arc::new(training_service),
    };
    info!(
        datasets_root = %datasets_root.display(),
        artifacts_root = %artifacts_root.display(),
        "Training worker initialized with DB-backed storage"
    );

    let shutdown = async {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    };
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            biased;

            _ = &mut shutdown => {
                info!("Shutdown signal received, stopping accept loop");
                break;
            }

            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let state = state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, state).await {
                                warn!(error = %e, "Training worker request failed");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "Accept error");
                    }
                }
            }
        }
    }

    // Allow in-flight handlers to complete
    info!("Waiting for in-flight requests to complete");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Clean up UDS socket
    if socket.path.exists() {
        if let Err(e) = std::fs::remove_file(&socket.path) {
            warn!(error = %e, path = %socket.path.display(), "failed to remove UDS socket on shutdown");
        } else {
            info!(path = %socket.path.display(), "removed UDS socket");
        }
    }

    info!("Training worker shut down cleanly");
    Ok(())
}

async fn handle_connection(mut stream: UnixStream, state: AppState) -> anyhow::Result<()> {
    let request = parse_request(&mut stream).await?;

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => {
            let payload = serde_json::json!({
                "status": "healthy",
                "service": "training-worker",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            send_json_response(&mut stream, 200, &payload).await?;
        }
        ("POST", "/training/start") => {
            let start_req: UdsTrainingStartRequest = serde_json::from_str(&request.body)?;
            let cp_job_id = start_req.job_id.clone();

            match state
                .service
                .start_training_with_job_id(
                    cp_job_id.clone(),
                    start_req.adapter_name,
                    start_req.config,
                    start_req.template_id,
                    start_req.repo_id,
                    start_req.target_branch,
                    start_req.base_version_id,
                    start_req.dataset_id,
                    start_req.dataset_version_ids,
                    start_req.synthetic_mode,
                    start_req.data_lineage_mode,
                    start_req.tenant_id,
                    start_req.initiated_by,
                    start_req.initiated_by_role,
                    start_req.base_model_id,
                    start_req.collection_id,
                    start_req.scope,
                    start_req.lora_tier,
                    start_req.category,
                    start_req.description,
                    start_req.language,
                    start_req.framework_id,
                    start_req.framework_version,
                    start_req.post_actions_json,
                    start_req.retry_of_job_id,
                    None,
                    start_req.code_commit_sha,
                    start_req.data_spec_json,
                    start_req.data_spec_hash,
                )
                .await
            {
                Ok(_worker_job) => {
                    let response = UdsTrainingStartResponse {
                        job_id: cp_job_id.clone(),
                        worker_job_id: Some(cp_job_id),
                        status: "accepted".to_string(),
                    };
                    let payload = serde_json::to_value(response)?;
                    send_json_response(&mut stream, 200, &payload).await?;
                }
                Err(e) => {
                    error!(error = %e, "Training start failed in training worker");
                    let payload = serde_json::json!({
                        "status": "error",
                        "error": e.to_string(),
                    });
                    send_json_response(&mut stream, 500, &payload).await?;
                }
            }
        }
        ("POST", "/training/cancel") => {
            let cancel_req: CancelTrainingRequest = serde_json::from_str(&request.body)?;
            let requested_job_id = cancel_req.job_id.clone();

            info!(
                requested_job_id = %requested_job_id,
                reason = ?cancel_req.reason,
                "Received training cancellation request"
            );

            let status = match state
                .service
                .cancel_job(&requested_job_id, None, None)
                .await
            {
                Ok(_) => "cancelled",
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("not found") {
                        "not_found"
                    } else if msg.contains("Cannot cancel job in state") {
                        "not_running"
                    } else {
                        warn!(error = %e, "Training cancel failed in training worker");
                        "error"
                    }
                }
            };

            let response = CancelTrainingResponse {
                job_id: requested_job_id,
                status: status.to_string(),
                tokens_processed: None,
                final_loss: None,
                stopped_at_epoch: None,
            };
            let payload = serde_json::to_value(response)?;
            send_json_response(&mut stream, 200, &payload).await?;
        }
        _ => {
            let payload = serde_json::json!({
                "status": "error",
                "error": "Not found",
            });
            send_json_response(&mut stream, 404, &payload).await?;
        }
    }

    Ok(())
}

async fn parse_request(stream: &mut UnixStream) -> anyhow::Result<HttpRequest> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let mut header_end = None;

    while header_end.is_none() {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("connection closed before headers were fully read");
        }

        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            anyhow::bail!("request headers exceed size limit");
        }

        header_end = find_bytes(&buffer, b"\r\n\r\n").map(|idx| idx + 4);
    }

    let header_end = header_end.expect("header_end is checked above");
    let header_str = std::str::from_utf8(&buffer[..header_end])?;

    let mut lines = header_str.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing request line"))?;

    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP method"))?
        .to_string();
    let path = request_line_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP path"))?
        .to_string();

    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = value.trim().parse::<usize>().unwrap_or(0);
            }
        }
    }

    if content_length > MAX_BODY_BYTES {
        anyhow::bail!("request body exceeds size limit");
    }

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > MAX_BODY_BYTES {
            anyhow::bail!("request body exceeds size limit");
        }
    }

    body.truncate(content_length);
    let body = String::from_utf8(body)?;

    Ok(HttpRequest { method, path, body })
}

async fn send_json_response(
    stream: &mut UnixStream,
    status: u16,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    let body = serde_json::to_string(payload)?;
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        reason_phrase(status),
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }

    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[allow(dead_code)]
fn socket_display(socket: &ResolvedPath) -> String {
    format!("{} ({})", socket.path.display(), socket.source)
}
