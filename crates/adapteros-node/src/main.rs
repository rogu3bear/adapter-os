use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent;
use agent::{NodeAgent, NodeHealth, WorkerInfo};

#[derive(Parser)]
#[command(name = "aos-node")]
#[command(about = "AdapterOS Node Agent", long_about = None)]
struct Cli {
    /// Agent listen port
    #[arg(short, long, default_value = "9443")]
    port: u16,
}

#[derive(Clone)]
struct AppState {
    agent: Arc<NodeAgent>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SpawnWorkerRequest {
    tenant_id: String,
    plan_id: String,
    uid: u32,
    gid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SpawnWorkerResponse {
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aos_node=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    info!("AdapterOS Node Agent starting on port {}", cli.port);

    // Initialize node agent
    let agent = Arc::new(NodeAgent::new());
    let state = AppState { agent };

    // Build application router
    let app = Router::new()
        .route("/spawn_worker", post(spawn_worker))
        .route("/workers", get(list_workers))
        .route("/workers/:pid", delete(stop_worker))
        .route("/health", get(health))
        // Tier 6: Cluster management endpoints
        .route("/status", get(node_status))
        .route("/adapters", get(node_adapters))
        .route("/hashes", get(node_hashes))
        .route("/sync/manifest", post(sync_receive_manifest))
        .route("/sync/create-manifest", post(sync_create_manifest))
        .with_state(state);

    // Start server
    let addr = format!("0.0.0.0:{}", cli.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Node agent listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// POST /spawn_worker - Spawn a new worker process
async fn spawn_worker(
    State(state): State<AppState>,
    Json(req): Json<SpawnWorkerRequest>,
) -> impl IntoResponse {
    info!(
        "Received spawn_worker request for tenant {} with plan {}",
        req.tenant_id, req.plan_id
    );

    match state
        .agent
        .spawn_worker(&req.tenant_id, &req.plan_id, req.uid, req.gid)
        .await
    {
        Ok(pid) => (StatusCode::OK, Json(SpawnWorkerResponse { pid })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to spawn worker: {}", e),
            }),
        )
            .into_response(),
    }
}

/// GET /workers - List all active workers
async fn list_workers(State(state): State<AppState>) -> impl IntoResponse {
    match state.agent.list_workers().await {
        Ok(workers) => {
            // Convert WorkerInfo to a serializable format
            let workers_json: Vec<_> = workers
                .into_iter()
                .map(|w| {
                    serde_json::json!({
                        "pid": w.pid,
                        "tenant_id": w.tenant_id,
                        "plan_id": w.plan_id,
                        "uds_path": w.uds_path,
                        "uptime_secs": w.started_at.elapsed().as_secs(),
                    })
                })
                .collect();

            (StatusCode::OK, Json(workers_json)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to list workers: {}", e),
            }),
        )
            .into_response(),
    }
}

/// DELETE /workers/:pid - Stop a worker by PID
async fn stop_worker(State(state): State<AppState>, Path(pid): Path<u32>) -> impl IntoResponse {
    info!("Received stop_worker request for PID {}", pid);

    match state.agent.stop_worker(pid).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "message": format!("Worker {} stopped", pid) })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Failed to stop worker: {}", e),
            }),
        )
            .into_response(),
    }
}

/// GET /health - Get node health status
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match state.agent.get_health().await {
        Ok(health) => (StatusCode::OK, Json(health)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to get health: {}", e),
            }),
        )
            .into_response(),
    }
}

// ============================================================
// Tier 6: Cluster Management Endpoints
// ============================================================

#[derive(Debug, Serialize)]
struct NodeStatusResponse {
    worker_count: usize,
    vram_bytes: u64,
    hostname: String,
    uptime_secs: u64,
}

/// GET /status - Get node status for cluster management
async fn node_status(State(state): State<AppState>) -> impl IntoResponse {
    let workers = state.agent.list_workers().await.unwrap_or_default();

    // Mock VRAM calculation - in production would query actual GPU
    let vram_bytes: u64 = (workers.len() as u64) * 4 * 1024 * 1024 * 1024; // Mock 4GB per worker

    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let response = NodeStatusResponse {
        worker_count: workers.len(),
        vram_bytes,
        hostname,
        uptime_secs: 0, // Mock uptime
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Serialize)]
struct AdapterHashResponse {
    id: String,
    hash: String,
}

/// GET /adapters - List loaded adapters with hashes
async fn node_adapters(State(_state): State<AppState>) -> impl IntoResponse {
    // Mock adapter list - in production would query from workers
    let adapters = vec![
        AdapterHashResponse {
            id: "adapter1".to_string(),
            hash: "b3:1234567890abcdef".to_string(),
        },
        AdapterHashResponse {
            id: "adapter2".to_string(),
            hash: "b3:fedcba0987654321".to_string(),
        },
    ];

    (StatusCode::OK, Json(adapters)).into_response()
}

#[derive(Debug, Serialize)]
struct ComponentHashResponse {
    component: String,
    hash: String,
}

/// GET /hashes - Get component hashes for determinism verification
async fn node_hashes(State(_state): State<AppState>) -> impl IntoResponse {
    // Mock component hashes - in production would compute from actual components
    use adapteros_core::B3Hash;

    let hashes = vec![
        ComponentHashResponse {
            component: "plan".to_string(),
            hash: B3Hash::hash(b"mock_plan").to_hex(),
        },
        ComponentHashResponse {
            component: "kernel".to_string(),
            hash: B3Hash::hash(b"mock_kernel").to_hex(),
        },
        ComponentHashResponse {
            component: "adapter1".to_string(),
            hash: B3Hash::hash(b"mock_adapter1").to_hex(),
        },
    ];

    (StatusCode::OK, Json(hashes)).into_response()
}

#[derive(Debug, Deserialize, Serialize)]
struct ReplicationManifest {
    session_id: String,
    artifacts: Vec<ArtifactInfo>,
    signature: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ArtifactInfo {
    adapter_id: String,
    hash: String,
    size_bytes: u64,
}

/// POST /sync/manifest - Receive replication manifest
async fn sync_receive_manifest(
    State(_state): State<AppState>,
    Json(manifest): Json<ReplicationManifest>,
) -> impl IntoResponse {
    info!(
        "Received replication manifest: session_id={}",
        manifest.session_id
    );

    // In production, would:
    // 1. Verify signature
    // 2. Check available space
    // 3. Prepare to receive artifacts

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ready",
            "session_id": manifest.session_id,
            "artifacts_count": manifest.artifacts.len()
        })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct CreateManifestRequest {
    adapters: Vec<String>,
}

/// POST /sync/create-manifest - Create replication manifest for requested adapters
async fn sync_create_manifest(
    State(_state): State<AppState>,
    Json(req): Json<CreateManifestRequest>,
) -> impl IntoResponse {
    info!("Creating manifest for {} adapters", req.adapters.len());

    // Mock manifest creation - in production would query CAS store
    let artifacts: Vec<ArtifactInfo> = req
        .adapters
        .iter()
        .map(|id| ArtifactInfo {
            adapter_id: id.clone(),
            hash: format!(
                "b3:{}",
                adapteros_core::B3Hash::hash(id.as_bytes()).to_hex()
            ),
            size_bytes: 1024 * 1024, // Mock 1MB
        })
        .collect();

    let manifest = ReplicationManifest {
        session_id: uuid::Uuid::new_v4().to_string(),
        artifacts,
        signature: "mock_signature".to_string(),
    };

    Json(manifest)
}
