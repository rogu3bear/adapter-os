//! aos-worker binary - Standalone inference worker
//!
//! This binary provides a UDS-based inference server that can be spawned
//! by the node agent or run standalone for development/testing.
//!
//! Usage:
//!   aos-worker --uds-path ./var/run/worker.sock --manifest manifests/qwen7b.yaml \
//!              --model-path ./var/model-cache/models/qwen2.5-7b-instruct-bf16

use adapteros_core::{B3Hash, Result};
use adapteros_lora_worker::{
    backend_coordinator::BackendCoordinator,
    backend_factory::{
        create_backend_with_model_and_hash, detect_capabilities as detect_backend_capabilities,
        BackendChoice,
    },
    uds_server::UdsServer,
    CoordinatedKernels, DirectKernels, KernelWrapper, Worker,
};
use adapteros_manifest::ManifestV3;
use adapteros_telemetry::TelemetryWriter;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

// Schema and API versions for worker registration
const SCHEMA_VERSION: &str = "1.0";
const API_VERSION: &str = "1.0";

// Worker panic hook support for fatal error reporting
// Global state for panic hook (must be static for panic handler access)
static WORKER_IDENTITY: OnceLock<WorkerIdentity> = OnceLock::new();

/// Register worker with control plane
///
/// Returns (accepted, heartbeat_interval_secs) on success.
/// Returns error message on rejection or communication failure.
fn register_with_cp(
    cp_url: &str,
    worker_id: &str,
    tenant_id: &str,
    plan_id: &str,
    manifest_hash: &str,
    uds_path: &str,
    capabilities: &[String],
) -> std::result::Result<(bool, u32), String> {
    let registration = serde_json::json!({
        "worker_id": worker_id,
        "tenant_id": tenant_id,
        "plan_id": plan_id,
        "manifest_hash": manifest_hash,
        "schema_version": SCHEMA_VERSION,
        "api_version": API_VERSION,
        "pid": std::process::id() as i32,
        "uds_path": uds_path,
        "capabilities": capabilities
    });

    let url = format!("{}/api/v1/workers/register", cp_url);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();

    match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(registration.to_string().as_bytes())
    {
        Ok(response) => {
            let body = response.into_body().read_to_string().unwrap_or_default();
            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json) => {
                    let accepted = json["accepted"].as_bool().unwrap_or(false);
                    let heartbeat = json["heartbeat_interval_secs"].as_u64().unwrap_or(30) as u32;

                    if !accepted {
                        let reason = json["rejection_reason"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        Err(reason)
                    } else {
                        Ok((true, heartbeat))
                    }
                }
                Err(e) => Err(format!("Invalid response: {}", e)),
            }
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Notify control plane of worker status change
fn notify_cp_status(cp_url: &str, worker_id: &str, status: &str, reason: &str) {
    let notification = serde_json::json!({
        "worker_id": worker_id,
        "status": status,
        "reason": reason
    });

    let url = format!("{}/api/v1/workers/status", cp_url);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .new_agent();

    match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(notification.to_string().as_bytes())
    {
        Ok(_) => {
            info!(status = %status, reason = %reason, "Status notification sent to CP");
        }
        Err(e) => {
            warn!(status = %status, error = %e, "Failed to notify CP of status change");
        }
    }
}

/// Compute BLAKE3 hash of manifest content for cache key and determinism
fn compute_manifest_hash(manifest_bytes: &[u8]) -> B3Hash {
    B3Hash::hash(manifest_bytes)
}

/// Detect backend capabilities
fn detect_capabilities(backend_choice: &str) -> Vec<String> {
    let mut caps = vec![];

    // Add backend capability
    match backend_choice.to_lowercase().as_str() {
        "coreml" => caps.push("coreml".to_string()),
        "mlx" => caps.push("mlx".to_string()),
        "metal" => caps.push("metal".to_string()),
        "auto" => {
            // Auto tries in order: CoreML -> MLX -> Metal
            #[cfg(target_os = "macos")]
            {
                caps.push("coreml".to_string());
                caps.push("mlx".to_string());
                caps.push("metal".to_string());
            }
        }
        _ => {}
    }

    caps
}

#[derive(Debug, Clone)]
struct WorkerIdentity {
    worker_id: String,
    cp_url: String,
}

/// Set up panic hook to report fatal errors to the control plane
fn setup_panic_hook() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        // Extract panic message
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        // Extract location
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        // Capture backtrace (first 2000 chars to avoid oversized messages)
        let backtrace = std::backtrace::Backtrace::capture();
        let backtrace_str = format!("{}", backtrace);
        let backtrace_snippet = if backtrace_str.len() > 2000 {
            format!("{}...(truncated)", &backtrace_str[..2000])
        } else {
            backtrace_str
        };

        // Attempt to notify CP of fatal error
        if let Some(identity) = WORKER_IDENTITY.get() {
            // Build fatal error payload
            let fatal_payload = serde_json::json!({
                "worker_id": identity.worker_id,
                "reason": format!("PANIC at {}: {}", location, message),
                "backtrace_snippet": backtrace_snippet,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            // Use blocking HTTP client (ureq) since we're in panic context
            // and can't use async. Best-effort delivery with short timeout.
            let url = format!("{}/api/v1/workers/fatal", identity.cp_url);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();
            let result = agent
                .post(&url)
                .header("Content-Type", "application/json")
                .send(fatal_payload.to_string().as_bytes());

            match result {
                Ok(_) => {
                    eprintln!("[PANIC HOOK] Fatal error reported to CP");
                }
                Err(e) => {
                    eprintln!("[PANIC HOOK] Failed to report fatal to CP: {}", e);
                }
            }
        } else {
            eprintln!("[PANIC HOOK] Worker identity not set, cannot report to CP");
        }

        // Call default hook for normal panic handling
        default_hook(panic_info);
    }));
}

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
    /// Development path: ./var/run/worker.sock (relative to cwd)
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

    /// Worker ID (auto-generated if not provided)
    #[arg(long, env = "WORKER_ID")]
    worker_id: Option<String>,

    /// Control plane URL for fatal error reporting
    #[arg(long, env = "AOS_CP_URL", default_value = "http://127.0.0.1:8080")]
    cp_url: String,
    /// Enable backend coordinator (primary + fallback) for runtime failover
    #[arg(long, env = "AOS_COORDINATOR_ENABLED", default_value_t = false)]
    coordinator_enabled: bool,
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

    // Set up panic hook for fatal error reporting
    let worker_id = args
        .worker_id
        .clone()
        .unwrap_or_else(|| format!("worker-{}", uuid::Uuid::now_v7()));

    // Store worker identity for panic hook access
    let _ = WORKER_IDENTITY.set(WorkerIdentity {
        worker_id: worker_id.clone(),
        cp_url: args.cp_url.clone(),
    });

    // Install panic hook for fatal error reporting
    setup_panic_hook();
    info!(worker_id = %worker_id, cp_url = %args.cp_url, "Panic hook installed for fatal error reporting");

    // Resolve UDS path with fallback logic
    let uds_path = args.uds_path.unwrap_or_else(|| {
        // Try production path first: /var/run/aos/{tenant_id}/worker.sock
        let prod_path = PathBuf::from(format!("/var/run/aos/{}/worker.sock", args.tenant_id));
        if let Some(parent) = prod_path.parent() {
            if parent.exists() || std::fs::create_dir_all(parent).is_ok() {
                return prod_path;
            }
        }
        // Fallback to development path: ./var/run/worker.sock (relative to cwd)
        let dev_path = PathBuf::from("./var/run/worker.sock");
        if let Some(parent) = dev_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        dev_path
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

    // Resolve tokenizer via canonical discovery (CLI arg > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())?;

    // Load manifest
    info!(path = %args.manifest.display(), "Loading manifest");
    let manifest_bytes = std::fs::read(&args.manifest)
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to read manifest: {}", e)))?;

    // Compute manifest hash for cache key, determinism, and registration
    let manifest_hash = compute_manifest_hash(&manifest_bytes);
    info!(manifest_hash = %manifest_hash.to_hex(), "Computed manifest hash for deduplication and determinism");

    let manifest_content = String::from_utf8(manifest_bytes.clone())
        .map_err(|e| adapteros_core::AosError::Io(format!("Invalid UTF-8 in manifest: {}", e)))?;

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

    // Create kernel backend with manifest hash for deterministic caching
    info!(backend = %args.backend, "Creating kernel backend with manifest hash");
    let primary_kernels =
        create_backend_with_model_and_hash(backend_choice, &model_path, Some(&manifest_hash))?;

    // Optional fallback backend via coordinator
    let fallback_kernels = if args.coordinator_enabled {
        let capabilities = detect_backend_capabilities();
        match BackendCoordinator::select_fallback_backend(&backend_choice, &capabilities) {
            Ok(choice) => {
                match create_backend_with_model_and_hash(choice, &model_path, Some(&manifest_hash))
                {
                    Ok(k) => {
                        info!(fallback_backend = ?choice, "Created fallback backend");
                        Some(k)
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to create fallback backend, continuing without fallback");
                        None
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "No suitable fallback backend available, continuing without fallback");
                None
            }
        }
    } else {
        None
    };

    let kernels = if args.coordinator_enabled {
        KernelWrapper::Coordinated(CoordinatedKernels::new(primary_kernels, fallback_kernels))
    } else {
        KernelWrapper::Direct(DirectKernels::new(primary_kernels))
    };

    // Create telemetry writer - use env var or ./var/telemetry
    let telemetry_dir =
        std::env::var("AOS_TELEMETRY_DIR").unwrap_or_else(|_| "./var/telemetry".to_string());
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

    // Register with control plane
    let capabilities = detect_capabilities(&args.backend);
    let uds_path_str = uds_path.to_string_lossy().to_string();

    let manifest_hash_hex = manifest_hash.to_hex();
    info!(
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        manifest_hash = %manifest_hash_hex,
        "Registering with control plane"
    );

    match register_with_cp(
        &args.cp_url,
        &worker_id,
        &args.tenant_id,
        &args.plan_id,
        &manifest_hash_hex,
        &uds_path_str,
        &capabilities,
    ) {
        Ok((accepted, heartbeat)) => {
            if accepted {
                info!(
                    heartbeat_interval = heartbeat,
                    "Worker registration accepted by control plane"
                );
            }
        }
        Err(reason) => {
            // Log but don't fail - allows running in dev mode without CP
            warn!(
                reason = %reason,
                "Worker registration rejected or failed - continuing anyway (dev mode)"
            );
        }
    }

    // Transition to serving status
    notify_cp_status(&args.cp_url, &worker_id, "serving", "worker ready");

    // Start UDS server
    info!(uds_path = %uds_path.display(), "Starting UDS server");
    let server = UdsServer::new(uds_path, worker);

    // Run server (blocking)
    server.serve().await?;

    // Notify stopped on clean exit
    notify_cp_status(&args.cp_url, &worker_id, "stopped", "clean shutdown");

    Ok(())
}
