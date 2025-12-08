//! aos-worker binary - Standalone inference worker
//!
//! This binary provides a UDS-based inference server that can be spawned
//! by the node agent or run standalone for development/testing.
//!
//! Usage:
//!   aos-worker --uds-path ./var/run/worker.sock --manifest manifests/qwen32b-coder-mlx.yaml \
//!              --model-path ./var/models/Qwen2.5-7B-Instruct-4bit --manifest-hash <HASH>

use adapteros_config::{
    resolve_manifest_cache_dir, resolve_telemetry_dir, resolve_worker_socket_for_worker,
};
use adapteros_core::{AosError, B3Hash, Result};
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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::{fs, path::PathBuf};
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{error, info, info_span, warn};

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
    backend: &str,
    model_hash: &str,
    uds_path: &str,
    capabilities: &[String],
) -> std::result::Result<(bool, u32), String> {
    let registration = serde_json::json!({
        "worker_id": worker_id,
        "tenant_id": tenant_id,
        "plan_id": plan_id,
        "manifest_hash": manifest_hash,
        "backend": backend,
        "model_hash": model_hash,
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
fn notify_cp_status(
    cp_url: &str,
    worker_id: &str,
    status: &str,
    reason: &str,
    backend: &str,
    model_hash: &str,
    manifest_hash: &str,
) {
    let notification = serde_json::json!({
        "worker_id": worker_id,
        "status": status,
        "reason": reason,
        "backend": backend,
        "model_hash": model_hash,
        "manifest_hash": manifest_hash,
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

/// Parse manifest content from YAML or JSON
fn parse_manifest(content: &str) -> Result<ManifestV3> {
    serde_yaml::from_str(content).or_else(|yaml_err| {
        serde_json::from_str(content).map_err(|json_err| {
            AosError::Validation(format!(
                "Failed to parse manifest as YAML ({}) or JSON ({})",
                yaml_err, json_err
            ))
        })
    })
}

/// Fetch manifest from control plane by hash
fn fetch_manifest_from_cp(cp_url: &str, tenant_id: &str, manifest_hash: &B3Hash) -> Result<String> {
    let url = format!(
        "{}/api/v1/tenants/{}/manifests/{}",
        cp_url,
        tenant_id,
        manifest_hash.to_hex()
    );

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();

    let response = agent
        .get(&url)
        .call()
        .map_err(|e| AosError::Worker(format!("Failed to fetch manifest: {}", e)))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| AosError::Worker(format!("Failed to read manifest response: {}", e)))?;

    let parsed: adapteros_api_types::workers::WorkerManifestFetchResponse =
        serde_json::from_str(&body).map_err(|e| {
            AosError::Worker(format!("Failed to parse manifest response JSON: {}", e))
        })?;

    if parsed.manifest_hash != manifest_hash.to_hex() {
        return Err(AosError::Validation(format!(
            "Manifest hash mismatch from CP: expected {}, got {}",
            manifest_hash.to_hex(),
            parsed.manifest_hash
        )));
    }

    let computed = B3Hash::hash(parsed.manifest_json.as_bytes());
    if computed != *manifest_hash {
        return Err(AosError::Validation(format!(
            "Manifest content hash mismatch: expected {}, computed {}",
            manifest_hash.to_hex(),
            computed.to_hex()
        )));
    }

    Ok(parsed.manifest_json)
}

/// Cache manifest locally for reuse
fn cache_manifest(manifest_hash: &B3Hash, manifest_json: &str) {
    let resolved_cache = resolve_manifest_cache_dir();
    let cache_dir = resolved_cache.path;
    if fs::create_dir_all(&cache_dir).is_ok() {
        let cache_path = cache_dir.join(format!("{}.json", manifest_hash.to_hex()));
        info!(
            path = %cache_path.display(),
            source = %resolved_cache.source,
            "Writing manifest cache entry"
        );
        if let Err(e) = fs::write(&cache_path, manifest_json) {
            warn!(error = %e, path = %cache_path.display(), "Failed to write manifest cache");
        }
    } else {
        warn!(
            path = %cache_dir.display(),
            source = %resolved_cache.source,
            "Failed to create manifest cache directory"
        );
    }
}

struct LoadedManifest {
    manifest: ManifestV3,
    _canonical_json: String,
    hash: B3Hash,
}

fn validate_backend_feature(choice: &BackendChoice) -> Result<()> {
    if matches!(choice, BackendChoice::Mlx) && !cfg!(feature = "multi-backend") {
        return Err(AosError::Config(
            "MLX backend requested but this binary was built without 'multi-backend'. \
             Rebuild with: cargo build --features multi-backend"
                .to_string(),
        ));
    }
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerRuntimeMode {
    Dev,
    Staging,
    Prod,
}

impl WorkerRuntimeMode {
    fn from_env() -> Self {
        match std::env::var("AOS_RUNTIME_MODE")
            .unwrap_or_else(|_| "dev".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "prod" | "production" => WorkerRuntimeMode::Prod,
            "staging" | "stage" => WorkerRuntimeMode::Staging,
            _ => WorkerRuntimeMode::Dev,
        }
    }
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

    /// Manifest hash (preferred) to fetch/verify
    #[arg(long, env = "AOS_MANIFEST_HASH")]
    manifest_hash: Option<String>,

    /// Path to manifest YAML/JSON file (fallback when hash fetch is unavailable)
    #[arg(long, env = "AOS_WORKER_MANIFEST")]
    manifest: Option<PathBuf>,

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

    // Load canonical .env before any environment-based resolution
    adapteros_config::model::load_dotenv();

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
    let resolved_uds = resolve_worker_socket_for_worker(&args.tenant_id, args.uds_path.as_deref());
    let uds_path = resolved_uds.path.clone();

    info!(
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        uds_path = %uds_path.display(),
        uds_source = %resolved_uds.source,
        "Starting aos-worker"
    );

    // Resolve model and tokenizer paths
    let model_path = match &args.model_path {
        Some(path) => path.clone(),
        None => adapteros_config::get_model_path_with_fallback()?,
    };
    if !model_path.exists() {
        return Err(AosError::Validation(format!(
            "Model path does not exist: {}",
            model_path.display()
        )));
    }

    // Resolve tokenizer via canonical discovery (CLI arg > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())?;

    // Resolve manifest content (hash-first)
    let expected_manifest_hash = args
        .manifest_hash
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|h| B3Hash::from_hex(h).map_err(|e| AosError::Validation(e.to_string())))
        .transpose()?;

    let loaded_manifest = if let Some(expected_hash) = expected_manifest_hash {
        if let Some(path) = args.manifest.as_ref() {
            if !path.exists() {
                return Err(AosError::Validation(format!(
                    "Manifest file not found at {}",
                    path.display()
                )));
            }
            let manifest_raw = fs::read_to_string(path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read manifest at {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let manifest = parse_manifest(&manifest_raw)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        } else {
            info!(
                manifest_hash = %expected_hash.to_hex(),
                cp_url = %args.cp_url,
                tenant_id = %args.tenant_id,
                "Fetching manifest from control plane"
            );
            let manifest_json =
                fetch_manifest_from_cp(&args.cp_url, &args.tenant_id, &expected_hash)?;
            let manifest = parse_manifest(&manifest_json)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch after fetch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        }
    } else {
        let path = args.manifest.as_ref().ok_or_else(|| {
            AosError::Validation(
                "Manifest hash not provided. Supply --manifest-hash/AOS_MANIFEST_HASH or --manifest/AOS_WORKER_MANIFEST"
                    .to_string(),
            )
        })?;
        if !path.exists() {
            return Err(AosError::Validation(format!(
                "Manifest file not found at {}",
                path.display()
            )));
        }
        let manifest_raw = fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read manifest at {}: {}",
                path.display(),
                e
            ))
        })?;
        let manifest = parse_manifest(&manifest_raw)?;
        let computed_hash = manifest.compute_hash()?;
        let canonical_json = manifest
            .to_json()
            .map_err(|e| AosError::Validation(format!("Failed to canonicalize manifest: {}", e)))?;
        cache_manifest(&computed_hash, &canonical_json);
        LoadedManifest {
            manifest,
            _canonical_json: canonical_json,
            hash: computed_hash,
        }
    };

    let manifest = loaded_manifest.manifest;
    let manifest_hash = loaded_manifest.hash;

    info!(
        model_id = %manifest.base.model_id,
        manifest_hash = %manifest_hash.to_hex(),
        k_sparse = manifest.router.k_sparse,
        "Manifest loaded and verified"
    );
    let model_hash_hex = manifest.base.model_hash.to_hex();

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
    validate_backend_feature(&backend_choice)?;

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
    let resolved_telemetry = resolve_telemetry_dir();
    if let Err(e) = std::fs::create_dir_all(&resolved_telemetry.path) {
        warn!(
            error = %e,
            path = %resolved_telemetry.path.display(),
            source = %resolved_telemetry.source,
            "Failed to create telemetry directory; continuing"
        );
    }
    let telemetry =
        TelemetryWriter::new(&resolved_telemetry.path, 10000, 100_000_000).map_err(|e| {
            adapteros_core::AosError::Worker(format!("Failed to create telemetry writer: {}", e))
        })?;
    info!(
        path = %resolved_telemetry.path.display(),
        source = %resolved_telemetry.source,
        "Telemetry writer initialized"
    );

    // Create worker
    info!("Creating worker instance");
    let worker = Worker::new(
        manifest,
        &args.tenant_id,
        kernels,
        None, // No RAG system for now
        tokenizer_path.to_str().unwrap_or(""),
        model_path.to_str().unwrap_or(""),
        telemetry,
    )
    .await?;

    let worker = Arc::new(Mutex::new(worker));
    let drain_flag = Arc::new(AtomicBool::new(false));

    // Register with control plane
    let capabilities = detect_capabilities(&args.backend);
    let uds_path_str = uds_path.to_string_lossy().to_string();

    let manifest_hash_hex = manifest_hash.to_hex();
    info!(
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        manifest_hash = %manifest_hash_hex,
        backend = %args.backend,
        model_hash = %model_hash_hex,
        "Registering with control plane"
    );

    match register_with_cp(
        &args.cp_url,
        &worker_id,
        &args.tenant_id,
        &args.plan_id,
        &manifest_hash_hex,
        &args.backend,
        &model_hash_hex,
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
            let runtime_mode = WorkerRuntimeMode::from_env();
            match runtime_mode {
                WorkerRuntimeMode::Prod => {
                    error!(
                        reason = %reason,
                        "Worker registration failed in prod - exiting"
                    );
                    return Err(AosError::Worker(format!(
                        "Registration failed in prod: {}",
                        reason
                    )));
                }
                _ => {
                    warn!(
                        reason = %reason,
                        "Worker registration rejected or failed - continuing (non-prod)"
                    );
                }
            }
        }
    }

    // Transition to serving status
    notify_cp_status(
        &args.cp_url,
        &worker_id,
        "serving",
        "worker ready",
        &args.backend,
        &model_hash_hex,
        &manifest_hash_hex,
    );

    // Start UDS server
    info!(uds_path = %uds_path.display(), "Starting UDS server");
    let server = UdsServer::new(uds_path.clone(), worker, None, drain_flag.clone());

    let serve_span = info_span!(
        "worker_serve",
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        backend = %args.backend,
        manifest_hash = %manifest_hash_hex,
        uds_path = %uds_path_str,
        coordinator_enabled = args.coordinator_enabled,
    );
    let _serve_span_guard = serve_span.enter();

    // Run server with drain handling
    let shutdown_signal = signal::ctrl_c();
    tokio::pin!(shutdown_signal);
    let serve_fut = server.serve();
    tokio::pin!(serve_fut);
    tokio::select! {
        res = &mut serve_fut => res,
        _ = &mut shutdown_signal => {
            info!(worker_id = %worker_id, "Drain signal received, initiating worker drain");
            drain_flag.store(true, Ordering::Relaxed);
            notify_cp_status(
                &args.cp_url,
                &worker_id,
                "draining",
                "drain-signal",
                &args.backend,
                &model_hash_hex,
                &manifest_hash_hex,
            );
            serve_fut.await
        }
    }?;

    // Notify stopped on clean exit
    notify_cp_status(
        &args.cp_url,
        &worker_id,
        "stopped",
        "clean shutdown",
        &args.backend,
        &model_hash_hex,
        &manifest_hash_hex,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mlx_guard_triggers_without_feature() {
        if cfg!(feature = "multi-backend") {
            return;
        }
        let result = validate_backend_feature(&BackendChoice::Mlx);
        assert!(result.is_err());
    }

    #[test]
    fn mlx_guard_allows_with_feature() {
        if !cfg!(feature = "multi-backend") {
            return;
        }
        let result = validate_backend_feature(&BackendChoice::Mlx);
        assert!(result.is_ok());
    }

    #[test]
    fn worker_runtime_mode_parsing() {
        std::env::set_var("AOS_RUNTIME_MODE", "prod");
        assert_eq!(WorkerRuntimeMode::from_env(), WorkerRuntimeMode::Prod);

        std::env::set_var("AOS_RUNTIME_MODE", "staging");
        assert_eq!(WorkerRuntimeMode::from_env(), WorkerRuntimeMode::Staging);

        std::env::remove_var("AOS_RUNTIME_MODE");
        assert_eq!(WorkerRuntimeMode::from_env(), WorkerRuntimeMode::Dev);
    }
}
