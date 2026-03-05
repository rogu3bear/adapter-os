use adapteros_artifacts::CasStore;
use adapteros_core::B3Hash;
use adapteros_crypto::SigningKey;
use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UnixListener;
use tokio::sync::RwLock;
#[allow(unused_imports)]
use tracing::error;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod agent;
use agent::NodeAgent;

#[derive(Parser)]
#[command(name = "aos-node")]
#[command(about = "adapterOS Node Agent", long_about = None)]
struct Cli {
    /// Agent listen port (ignored in production mode)
    #[arg(short, long, default_value = "18083")]
    port: u16,

    /// Enable production mode (requires UDS binding, no TCP)
    #[arg(long, env = "AOS_PRODUCTION_MODE")]
    production_mode: bool,

    /// Unix Domain Socket path for production mode
    #[arg(
        long,
        env = "AOS_NODE_UDS_PATH",
        default_value = "/var/run/aos/node.sock"
    )]
    uds_path: String,

    /// CAS store directory for artifacts
    #[arg(long, env = "AOS_CAS_PATH", default_value = "/var/lib/aos/cas")]
    cas_path: String,

    /// Kernel library path for hash computation
    #[arg(long, env = "AOS_KERNEL_PATH", default_value = "/usr/lib/aos/kernels")]
    kernel_path: String,

    /// Plan configuration path
    #[arg(long, env = "AOS_PLAN_PATH", default_value = "/etc/aos/plans")]
    plan_path: String,

    /// Directory containing peer public keys ({node_id}.pub files, 32 bytes raw Ed25519)
    #[arg(long, env = "AOS_PEER_KEYS_DIR", default_value = "/var/lib/aos/peers")]
    peer_keys_dir: String,
}

/// Component hashes tracked by the node
struct ComponentHashes {
    /// Hash of the current execution plan
    plan_hash: B3Hash,
    /// Hash of the Metal/CoreML kernel library
    kernel_hash: B3Hash,
    /// Timestamp when hashes were computed
    computed_at: Instant,
}

#[derive(Clone)]
struct AppState {
    agent: Arc<NodeAgent>,
    cas_store: Arc<CasStore>,
    component_hashes: Arc<RwLock<ComponentHashes>>,
    /// Directory containing known peer public keys ({node_id}.pub, 32 bytes raw Ed25519)
    peer_keys_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct SpawnWorkerRequest {
    tenant_id: String,
    plan_id: String,
    uid: u32,
    gid: u32,
    /// Model cache budget in megabytes (propagated to worker as AOS_MODEL_CACHE_MAX_MB)
    #[serde(default)]
    model_cache_max_mb: Option<u64>,
    /// Path to config TOML file (propagated to worker as AOS_CONFIG_TOML)
    #[serde(default)]
    config_toml_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SpawnWorkerResponse {
    pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %e, "Failed to install Ctrl+C handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
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

    info!(
        build_id = adapteros_core::version::BUILD_ID,
        git_commit = adapteros_core::version::GIT_COMMIT_HASH,
        version = adapteros_core::version::VERSION,
        profile = adapteros_core::version::BUILD_PROFILE,
        "aos-node starting"
    );

    // Initialize CAS store
    let cas_path = PathBuf::from(&cli.cas_path);
    if !cas_path.exists() {
        std::fs::create_dir_all(&cas_path)?;
    }
    let cas_store = Arc::new(CasStore::new(&cas_path)?);

    // Compute initial component hashes
    let kernel_path = PathBuf::from(&cli.kernel_path);
    let plan_path = PathBuf::from(&cli.plan_path);

    let plan_hash = compute_plan_hash(&plan_path);
    let kernel_hash = compute_kernel_hash(&kernel_path);

    info!(
        plan_hash = %plan_hash,
        kernel_hash = %kernel_hash,
        "Computed component hashes"
    );

    let component_hashes = Arc::new(RwLock::new(ComponentHashes {
        plan_hash,
        kernel_hash,
        computed_at: Instant::now(),
    }));

    // Initialize node agent
    let agent = Arc::new(NodeAgent::new());
    let peer_keys_dir = PathBuf::from(&cli.peer_keys_dir);
    if !peer_keys_dir.exists() {
        info!(
            path = %peer_keys_dir.display(),
            "Peer keys directory does not exist, creating it"
        );
        std::fs::create_dir_all(&peer_keys_dir)?;
    }
    let state = AppState {
        agent,
        cas_store,
        component_hashes,
        peer_keys_dir,
    };

    // Build application router
    let app = Router::new()
        .route("/spawn_worker", post(spawn_worker))
        .route("/workers", get(list_workers))
        .route("/workers/{pid}", delete(stop_worker))
        .route("/health", get(health))
        // Tier 6: Cluster management endpoints
        .route("/status", get(node_status))
        .route("/adapters", get(node_adapters))
        .route("/hashes", get(node_hashes))
        .route("/sync/manifest", post(sync_receive_manifest))
        .route("/sync/create-manifest", post(sync_create_manifest))
        .layer(middleware::from_fn(
            adapteros_telemetry::middleware::api_logger_middleware,
        ))
        .with_state(state.clone());

    // Start server based on mode
    if cli.production_mode {
        // Production mode: Use Unix Domain Socket only (egress policy compliance)
        info!("adapterOS Node Agent starting in PRODUCTION mode");

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&cli.uds_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket file if present
        if std::path::Path::new(&cli.uds_path).exists() {
            std::fs::remove_file(&cli.uds_path)?;
        }

        let listener = UnixListener::bind(&cli.uds_path)?;
        info!("Node agent listening on UDS: {}", cli.uds_path);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        info!("Server stopped, cleaning up workers");
        state.agent.shutdown_all_workers().await;

        // Clean up UDS socket
        if std::path::Path::new(&cli.uds_path).exists() {
            if let Err(e) = std::fs::remove_file(&cli.uds_path) {
                warn!(error = %e, path = %cli.uds_path, "failed to remove UDS socket on shutdown");
            } else {
                info!(path = %cli.uds_path, "removed UDS socket");
            }
        }
    } else {
        // Development mode: TCP binding allowed
        warn!(
            "Node agent running in DEVELOPMENT mode with TCP binding - not suitable for production"
        );

        let bind_all = std::env::var("AOS_NODE_DEV_BIND_ALL")
            .map(|v| v == "1")
            .unwrap_or(false);

        let bind_host = if bind_all {
            warn!("Node agent bound to 0.0.0.0 in dev mode \u{2014} all endpoints are unauthenticated");
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };

        let addr = format!("{}:{}", bind_host, cli.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        info!("Node agent listening on TCP: {}", addr);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        info!("Server stopped, cleaning up workers");
        state.agent.shutdown_all_workers().await;
    }

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
        .spawn_worker(
            &req.tenant_id,
            &req.plan_id,
            req.uid,
            req.gid,
            req.model_cache_max_mb,
            req.config_toml_path.as_deref(),
        )
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

/// DELETE /workers/{pid} - Stop a worker by PID
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

    // Calculate actual VRAM usage from workers
    // Each loaded adapter uses approximately 1-4GB depending on model size
    let vram_bytes: u64 = workers
        .iter()
        .map(|w| {
            // Estimate based on plan - could be refined with actual worker queries
            if w.plan_id.contains("large") {
                4 * 1024 * 1024 * 1024 // 4GB for large models
            } else if w.plan_id.contains("medium") {
                2 * 1024 * 1024 * 1024 // 2GB for medium
            } else {
                1024 * 1024 * 1024 // 1GB default
            }
        })
        .sum();

    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Calculate actual uptime from component hash computation time
    let uptime_secs = state
        .component_hashes
        .read()
        .await
        .computed_at
        .elapsed()
        .as_secs();

    let response = NodeStatusResponse {
        worker_count: workers.len(),
        vram_bytes,
        hostname,
        uptime_secs,
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Serialize)]
struct AdapterHashResponse {
    id: String,
    hash: String,
}

/// GET /adapters - List loaded adapters with hashes
async fn node_adapters(State(state): State<AppState>) -> impl IntoResponse {
    // Get loaded adapters from workers
    let workers = state.agent.list_workers().await.unwrap_or_default();

    let adapters: Vec<AdapterHashResponse> = workers
        .iter()
        .map(|worker| {
            // Compute adapter hash from plan_id
            // In production, would query registry for actual adapter file hash
            let hash = B3Hash::hash(worker.plan_id.as_bytes());
            AdapterHashResponse {
                id: format!("{}:{}", worker.tenant_id, worker.plan_id),
                hash: format!("b3:{}", hash.to_short_hex()),
            }
        })
        .collect();

    (StatusCode::OK, Json(adapters)).into_response()
}

#[derive(Debug, Serialize)]
struct ComponentHashResponse {
    component: String,
    hash: String,
}

/// GET /hashes - Get component hashes for determinism verification
async fn node_hashes(State(state): State<AppState>) -> impl IntoResponse {
    let mut hashes = Vec::new();

    // Get cached component hashes
    let component_hashes = state.component_hashes.read().await;

    // Plan hash (from execution plan configuration)
    hashes.push(ComponentHashResponse {
        component: "plan".to_string(),
        hash: component_hashes.plan_hash.to_hex(),
    });

    // Kernel hash (from Metal/CoreML kernel library)
    hashes.push(ComponentHashResponse {
        component: "kernel".to_string(),
        hash: component_hashes.kernel_hash.to_hex(),
    });

    drop(component_hashes);

    // Add hashes for loaded adapters
    let workers = state.agent.list_workers().await.unwrap_or_default();
    for worker in workers {
        // Compute adapter hash from plan_id and tenant
        let adapter_hash =
            B3Hash::hash(format!("{}:{}", worker.tenant_id, worker.plan_id).as_bytes());
        hashes.push(ComponentHashResponse {
            component: format!("adapter:{}", worker.plan_id),
            hash: adapter_hash.to_hex(),
        });
    }

    (StatusCode::OK, Json(hashes)).into_response()
}

#[derive(Debug, Deserialize, Serialize)]
struct ReplicationManifest {
    session_id: String,
    /// Node ID of the sender (hex-encoded BLAKE3 hash of the sender's Ed25519 public key)
    sender_node_id: String,
    artifacts: Vec<ArtifactInfo>,
    /// Hex-encoded Ed25519 signature over `serde_json::to_vec(&artifacts)`
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
    State(state): State<AppState>,
    Json(manifest): Json<ReplicationManifest>,
) -> impl IntoResponse {
    info!(
        session_id = %manifest.session_id,
        sender_node_id = %manifest.sender_node_id,
        artifacts_count = manifest.artifacts.len(),
        "Received replication manifest"
    );

    // 1. Verify Ed25519 signature against sender's known public key
    if let Err(e) = verify_manifest_signature(&manifest, &state.peer_keys_dir) {
        warn!(
            session_id = %manifest.session_id,
            sender_node_id = %manifest.sender_node_id,
            error = %e,
            "Manifest signature verification failed"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": format!("Signature verification failed: {}", e),
                "session_id": manifest.session_id,
            })),
        )
            .into_response();
    }

    info!(
        session_id = %manifest.session_id,
        sender_node_id = %manifest.sender_node_id,
        "Manifest signature verified successfully"
    );

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
    State(state): State<AppState>,
    Json(req): Json<CreateManifestRequest>,
) -> impl IntoResponse {
    info!("Creating manifest for {} adapters", req.adapters.len());

    // Build artifacts list from requested adapters
    let mut artifacts: Vec<ArtifactInfo> = Vec::new();

    for adapter_id in &req.adapters {
        // Check if artifact exists in CAS store
        let hash = B3Hash::hash(adapter_id.as_bytes());
        let exists = state.cas_store.exists("adapters", &hash);

        if exists {
            // Load artifact to get actual size
            match state.cas_store.load("adapters", &hash) {
                Ok(data) => {
                    artifacts.push(ArtifactInfo {
                        adapter_id: adapter_id.clone(),
                        hash: format!("b3:{}", hash.to_hex()),
                        size_bytes: data.len() as u64,
                    });
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        "Failed to load adapter from CAS"
                    );
                    artifacts.push(ArtifactInfo {
                        adapter_id: adapter_id.clone(),
                        hash: format!("b3:{}", hash.to_hex()),
                        size_bytes: 0,
                    });
                }
            }
        } else {
            // Adapter not in CAS store, include with computed hash
            warn!(adapter_id = %adapter_id, "Adapter not found in CAS store");
            artifacts.push(ArtifactInfo {
                adapter_id: adapter_id.clone(),
                hash: format!("b3:{}", hash.to_hex()),
                size_bytes: 0,
            });
        }
    }

    // Generate session ID with UUID v7 for time-ordering
    let session_id = adapteros_id::TypedId::new(adapteros_id::IdPrefix::Ses).to_string();

    // Sign manifest with node's signing key and derive node ID from public key
    let manifest_data = serde_json::to_vec(&artifacts).unwrap_or_default();
    let (signature, sender_node_id) = match sign_manifest_with_identity(&manifest_data) {
        Ok((sig, node_id)) => (sig, node_id),
        Err(e) => {
            warn!(error = %e, "Failed to sign manifest, using placeholder");
            ("unsigned".to_string(), "unknown".to_string())
        }
    };

    let manifest = ReplicationManifest {
        session_id,
        sender_node_id,
        artifacts,
        signature,
    };

    Json(manifest)
}

/// Compute hash of the execution plan directory
fn compute_plan_hash(plan_path: &std::path::Path) -> B3Hash {
    if !plan_path.exists() {
        warn!(path = %plan_path.display(), "Plan path does not exist, using zero hash");
        return B3Hash::zero();
    }

    // Hash all plan files in the directory
    let mut hasher = blake3::Hasher::new();

    if plan_path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(plan_path)
            .map(|rd| rd.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();

        // Sort for deterministic ordering
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            if path.is_file()
                && (path
                    .extension()
                    .map(|e| e == "json" || e == "yaml")
                    .unwrap_or(false))
            {
                if let Ok(contents) = std::fs::read(&path) {
                    hasher.update(path.file_name().unwrap_or_default().as_encoded_bytes());
                    hasher.update(&contents);
                }
            }
        }
    } else if plan_path.is_file() {
        if let Ok(contents) = std::fs::read(plan_path) {
            hasher.update(&contents);
        }
    }

    B3Hash::new(*hasher.finalize().as_bytes())
}

/// Compute hash of the kernel library
fn compute_kernel_hash(kernel_path: &std::path::Path) -> B3Hash {
    if !kernel_path.exists() {
        warn!(path = %kernel_path.display(), "Kernel path does not exist, using zero hash");
        return B3Hash::zero();
    }

    // Hash kernel library files (.metallib, .mlmodelc, etc.)
    let mut hasher = blake3::Hasher::new();

    if kernel_path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(kernel_path)
            .map(|rd| rd.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();

        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "metallib" | "mlmodelc" | "dylib" | "so" | "a") {
                    if let Ok(contents) = std::fs::read(&path) {
                        hasher.update(path.file_name().unwrap_or_default().as_encoded_bytes());
                        hasher.update(&contents);
                    }
                }
            }
        }
    } else if kernel_path.is_file() {
        if let Ok(contents) = std::fs::read(kernel_path) {
            hasher.update(&contents);
        }
    }

    B3Hash::new(*hasher.finalize().as_bytes())
}

/// Derive a node ID from an Ed25519 public key (hex-encoded BLAKE3 hash, truncated to 16 bytes)
fn node_id_from_public_key(public_key: &ed25519_dalek::VerifyingKey) -> String {
    let hash = blake3::hash(&public_key.to_bytes());
    // Use first 16 bytes (32 hex chars) for a compact but collision-resistant ID
    hex::encode(&hash.as_bytes()[..16])
}

/// Load the node's Ed25519 signing key, generating one if it doesn't exist.
/// Returns the signing key.
fn resolve_node_key_path() -> PathBuf {
    if let Ok(explicit) = std::env::var("AOS_NODE_KEY_PATH") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    // Prefer system path when writable.
    let system_path = PathBuf::from("/var/lib/aos/node.key");
    if system_path.exists() {
        return system_path;
    }

    // Dev fallback keeps keys under the workspace runtime dir.
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join("var/node/node.key");
    }

    system_path
}

fn load_or_generate_node_key() -> Result<SigningKey> {
    let key_path = resolve_node_key_path();
    let key_path_ref = key_path.as_path();

    if key_path_ref.exists() {
        // Warn if permissions are loose (do not auto-fix -- operator should know)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(key_path_ref) {
                let mode = metadata.permissions().mode();
                if mode & 0o077 != 0 {
                    warn!(
                        path = %key_path_ref.display(),
                        mode = format!("{:o}", mode & 0o777),
                        "Node signing key has loose permissions (should be 0600). \
                         This is a security risk -- the key may be readable by other users."
                    );
                }
            }
        }
        let key_bytes = std::fs::read(key_path_ref)?;
        let key_array: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
        Ok(SigningKey::from_bytes(&key_array))
    } else {
        // Generate new key for this node
        let mut csprng = rand::rngs::OsRng;
        let key = SigningKey::generate(&mut csprng);
        if let Some(parent) = key_path_ref.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(key_path_ref, key.to_bytes())?;
        // Restrict to owner-only permissions immediately after creation
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(key_path_ref)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(key_path_ref, perms)?;
        }
        #[cfg(not(unix))]
        {
            warn!(
                path = %key_path_ref.display(),
                "Cannot restrict key file permissions on non-Unix platform"
            );
        }

        let node_id = node_id_from_public_key(&key.verifying_key());
        info!(
            node_id = %node_id,
            key_path = %key_path_ref.display(),
            "Generated new node signing key"
        );
        Ok(key)
    }
}

/// Sign manifest data with node's Ed25519 key and return (hex_signature, node_id)
fn sign_manifest_with_identity(data: &[u8]) -> Result<(String, String)> {
    let signing_key = load_or_generate_node_key()?;
    let node_id = node_id_from_public_key(&signing_key.verifying_key());
    let signature = signing_key.sign(data);
    Ok((hex::encode(signature.to_bytes()), node_id))
}

/// Verify a received manifest's Ed25519 signature against the sender's known public key.
///
/// Peer keys are looked up as `{peer_keys_dir}/{sender_node_id}.pub` files containing
/// 32 bytes of raw Ed25519 public key. The signed payload is the JSON-serialized
/// artifacts array, matching what `sign_manifest_with_identity` signs.
fn verify_manifest_signature(
    manifest: &ReplicationManifest,
    peer_keys_dir: &std::path::Path,
) -> std::result::Result<(), String> {
    // Reconstruct the signed payload (must match what sign_manifest_with_identity signs)
    let signed_payload = serde_json::to_vec(&manifest.artifacts)
        .map_err(|e| format!("Failed to serialize artifacts for verification: {}", e))?;

    // Decode the hex signature
    let sig_bytes =
        hex::decode(&manifest.signature).map_err(|e| format!("Invalid hex signature: {}", e))?;
    if sig_bytes.len() != 64 {
        return Err(format!(
            "Invalid signature length: expected 64 bytes, got {}",
            sig_bytes.len()
        ));
    }
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "Signature byte conversion failed".to_string())?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

    // Load the sender's public key from the peer keys directory
    let peer_key_path = peer_keys_dir.join(format!("{}.pub", manifest.sender_node_id));
    if !peer_key_path.exists() {
        return Err(format!(
            "Unknown peer: no public key file at {}",
            peer_key_path.display()
        ));
    }

    let key_bytes = std::fs::read(&peer_key_path)
        .map_err(|e| format!("Failed to read peer key {}: {}", peer_key_path.display(), e))?;
    if key_bytes.len() != 32 {
        return Err(format!(
            "Invalid peer key length in {}: expected 32 bytes, got {}",
            peer_key_path.display(),
            key_bytes.len()
        ));
    }
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Peer key byte conversion failed".to_string())?;

    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&key_array).map_err(|e| {
        format!(
            "Invalid Ed25519 public key in {}: {}",
            peer_key_path.display(),
            e
        )
    })?;

    // Verify that the node_id matches the public key (prevents key substitution)
    let expected_node_id = node_id_from_public_key(&verifying_key);
    if expected_node_id != manifest.sender_node_id {
        return Err(format!(
            "Node ID mismatch: file {} contains key with ID {}, but manifest claims {}",
            peer_key_path.display(),
            expected_node_id,
            manifest.sender_node_id
        ));
    }

    // Verify the Ed25519 signature (constant-time via ed25519-dalek)
    use ed25519_dalek::Verifier;
    verifying_key
        .verify(&signed_payload, &signature)
        .map_err(|e| format!("Ed25519 signature invalid: {}", e))
}
