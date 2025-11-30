//! aos-worker binary - Standalone inference worker
//!
//! This binary provides a UDS-based inference server that can be spawned
//! by the node agent or run standalone for development/testing.
//!
//! Usage:
//!   aos-worker --uds-path /tmp/worker.sock --manifest manifests/qwen7b.yaml \
//!              --model-path models/qwen2.5-7b-mlx --tokenizer models/qwen2.5-7b-mlx/tokenizer.json

use adapteros_core::Result;
use adapteros_lora_worker::{
    backend_factory::{create_backend_with_model, BackendChoice},
    uds_server::UdsServer,
    Worker,
};
use adapteros_manifest::ManifestV3;
use adapteros_telemetry::TelemetryWriter;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// AdapterOS Inference Worker
#[derive(Parser, Debug)]
#[command(name = "aos-worker")]
#[command(about = "AdapterOS inference worker with UDS communication")]
struct Args {
    /// Tenant ID for this worker
    #[arg(long, env = "TENANT_ID", default_value = "default")]
    tenant_id: String,

    /// Plan ID for this worker
    #[arg(long, env = "PLAN_ID", default_value = "dev")]
    plan_id: String,

    /// UDS socket path for communication
    /// Standard production path: /var/run/aos/{tenant_id}/worker.sock
    /// Development fallback: /tmp/aos-worker.sock
    #[arg(long, env = "AOS_WORKER_SOCKET")]
    uds_path: Option<PathBuf>,

    /// Path to manifest YAML file
    #[arg(long, default_value = "manifests/qwen7b.yaml")]
    manifest: PathBuf,

    /// Path to model directory (auto-discovered from AOS_MODEL_PATH)
    #[arg(long, env = "AOS_MODEL_PATH")]
    model_path: Option<PathBuf>,

    /// Path to tokenizer JSON file (auto-discovered from AOS_TOKENIZER_PATH or model directory)
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    tokenizer: Option<PathBuf>,

    /// Backend choice (auto, metal, coreml, mlx)
    #[arg(long, default_value = "auto")]
    backend: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("aos_worker=info".parse().unwrap())
                .add_directive("adapteros_lora_worker=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    // Resolve UDS path with fallback logic
    let uds_path = args.uds_path.unwrap_or_else(|| {
        // Try production path first, fallback to /tmp for development
        let prod_path = PathBuf::from(format!("/var/run/aos/{}/worker.sock", args.tenant_id));
        if let Some(parent) = prod_path.parent() {
            if parent.exists() || std::fs::create_dir_all(parent).is_ok() {
                return prod_path;
            }
        }
        // Fallback to temp directory for development
        std::env::temp_dir().join(format!("aos-worker-{}.sock", args.tenant_id))
    });

    info!(
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        uds_path = %uds_path.display(),
        "Starting aos-worker"
    );

    // Validate paths
    if !args.manifest.exists() {
        error!(path = %args.manifest.display(), "Manifest file not found");
        return Err(adapteros_core::AosError::Validation(format!(
            "Manifest file not found: {}",
            args.manifest.display()
        )));
    }

    // Resolve model and tokenizer paths
    let model_path = match &args.model_path {
        Some(path) => path.clone(),
        None => adapteros_config::get_model_path_with_fallback()?,
    };

    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())?;

    // Load manifest
    info!(path = %args.manifest.display(), "Loading manifest");
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to read manifest: {}", e)))?;

    let manifest: ManifestV3 = serde_yaml::from_str(&manifest_content).map_err(|e| {
        adapteros_core::AosError::Validation(format!("Failed to parse manifest: {}", e))
    })?;

    info!(
        model_id = %manifest.base.model_id,
        k_sparse = manifest.router.k_sparse,
        "Manifest loaded"
    );

    // Select backend
    let backend_choice = match args.backend.to_lowercase().as_str() {
        "auto" => BackendChoice::Auto,
        "metal" => BackendChoice::Metal,
        "coreml" => BackendChoice::CoreML,
        "mlx" => BackendChoice::Mlx,
        _ => {
            warn!(backend = %args.backend, "Unknown backend, using auto");
            BackendChoice::Auto
        }
    };

    // Create kernel backend
    info!(backend = %args.backend, "Creating kernel backend");
    let kernels = create_backend_with_model(backend_choice, &model_path)?;

    // Create telemetry writer - use env var or temp directory
    let telemetry_dir = std::env::var("AOS_TELEMETRY_DIR")
        .unwrap_or_else(|_| std::env::temp_dir().join("aos-worker-telemetry").to_string_lossy().to_string());
    std::fs::create_dir_all(&telemetry_dir).ok();
    let telemetry = TelemetryWriter::new(&telemetry_dir, 10000, 100_000_000).map_err(|e| {
        adapteros_core::AosError::Worker(format!("Failed to create telemetry writer: {}", e))
    })?;

    // Create worker
    info!("Creating worker instance");
    let worker = Worker::new(
        manifest,
        kernels,
        None, // No RAG system for now
        tokenizer_path.to_str().unwrap_or(""),
        model_path.to_str().unwrap_or(""),
        telemetry,
    )
    .await?;

    let worker = Arc::new(Mutex::new(worker));

    // Start UDS server
    info!(uds_path = %uds_path.display(), "Starting UDS server");
    let server = UdsServer::new(uds_path, worker);

    // Run server (blocking)
    server.serve().await?;

    Ok(())
}
