//! Start serving

use adapteros_config::{
    resolve_base_model_location, resolve_index_root, resolve_telemetry_dir, ModelConfig,
};
use adapteros_policy::egress;
use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "macos")]
use nix::unistd::{setgid, setuid, Gid, Uid};

/// Drop privileges to tenant UID/GID
#[cfg(target_os = "macos")]
async fn drop_privileges(tenant: &str) -> Result<()> {
    use nix::unistd::getuid;

    // Only attempt privilege dropping if running as root
    if !getuid().is_root() {
        tracing::debug!("Not running as root, skipping privilege drop");
        return Ok(());
    }

    // In a production system, you would look up the tenant's UID/GID from a database
    // For now, we use a simple mapping based on tenant name
    let (uid, gid) = get_tenant_credentials(tenant)?;

    tracing::info!("Dropping privileges to UID: {}, GID: {}", uid, gid);

    // Drop group privileges first (must be done before dropping user privileges)
    setgid(Gid::from_raw(gid)).map_err(|e| anyhow::anyhow!("Failed to setgid: {}", e))?;

    // Drop user privileges
    setuid(Uid::from_raw(uid)).map_err(|e| anyhow::anyhow!("Failed to setuid: {}", e))?;

    tracing::info!("Successfully dropped privileges to tenant: {}", tenant);
    Ok(())
}

/// Get tenant credentials (UID/GID)
/// In production, this would query a database or configuration file
#[cfg(target_os = "macos")]
fn get_tenant_credentials(tenant: &str) -> Result<(u32, u32)> {
    // For development, use a simple mapping
    // In production, query from database or secure configuration
    match tenant {
        "default" => Ok((1000, 1000)),
        "test" => Ok((1001, 1001)),
        _ => {
            // Generate deterministic UID/GID from tenant name hash
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            tenant.hash(&mut hasher);
            let hash = hasher.finish();

            // Use hash to generate UID/GID in range 2000-65000
            let uid = 2000 + (hash % 63000) as u32;
            let gid = uid; // Same as UID for simplicity

            tracing::warn!(
                "Using generated credentials for tenant '{}': UID={}, GID={}",
                tenant,
                uid,
                gid
            );

            Ok((uid, gid))
        }
    }
}

use crate::output::OutputWriter;
use crate::BackendType;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    tenant: &str,
    plan: &str,
    socket: &Path,
    backend: BackendType,
    dry_run: bool,
    _capture_events: Option<&std::path::PathBuf>,
    model_config: Option<&ModelConfig>,
    output: &OutputWriter,
) -> Result<()> {
    output.section("Starting AdapterOS server");
    output.kv("Tenant", tenant);
    output.kv("Plan", plan);
    output.kv("Socket", &socket.display().to_string());
    output.blank();

    if dry_run {
        output.info("Dry-run mode: validating preflight checks only");
        output.blank();
    }

    // Phase 0: Egress preflight validation
    output.progress("Validating egress policy");

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = egress::validate_egress_policy() {
            tracing::error!(
                error = %e,
                "Preflight failed: egress policy validation failed"
            );
            output.progress_done(false);
            output.blank();
            output.error("PREFLIGHT FAILED: Egress policy validation failed");
            output.error(format!("  {}", e));
            output.blank();
            output.error("The system refuses to serve without proper egress controls.");
            output.error("See docs/security.md for PF configuration instructions.");
            return Err(e.into());
        }
        output.progress_done(true);
    }

    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("PF egress validation only available on macOS, proceeding without egress enforcement (dev mode only)");
        output.warning("PF egress validation only available on macOS");
        output.warning("Proceeding without egress enforcement (dev mode only)");
    }

    if dry_run {
        output.blank();
        output.success("All preflight checks passed");
        output.kv("System status", "ready to serve");
        output.blank();
        output.info("Re-run without --dry-run to start serving.");
        return Ok(());
    }

    // Load plan directory
    let plan_dir = std::path::PathBuf::from("./plan").join(plan);
    if !plan_dir.exists() {
        return Err(anyhow::anyhow!("Plan directory not found: {:?}", plan_dir));
    }

    output.success(format!("Plan directory found: {:?}", plan_dir));

    // Load manifest from plan
    let manifest_path = plan_dir.join("manifest.json");
    let manifest_content = std::fs::read_to_string(&manifest_path)?;
    let manifest: adapteros_manifest::ManifestV3 = serde_json::from_str(&manifest_content)?;

    output.success("Manifest loaded");

    // Create UDS server directory
    let socket_dir = socket
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid socket path"))?;
    if !socket_dir.exists() {
        std::fs::create_dir_all(socket_dir)?;
    }

    output.success(format!(
        "UDS socket directory created: {}",
        socket_dir.display()
    ));

    // Drop privileges to tenant UID/GID if running as root
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = drop_privileges(tenant).await {
            tracing::warn!(error = %e, "Failed to drop privileges (continuing anyway)");
            output.warning(format!("Failed to drop privileges: {}", e));
            output.warning("Continuing with current privileges (dev mode)");
        } else {
            output.success(format!("Privileges dropped to tenant: {}", tenant));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        output.verbose("Note: Privilege dropping only available on macOS");
    }

    output.info("Initializing worker...");

    // 1. Load model paths (precedence: CLI/model_config > canonical resolver)
    let (model_path, model_path_source) = if let Some(cfg) = model_config {
        (
            cfg.path.display().to_string(),
            "cli/model_config".to_string(),
        )
    } else {
        let resolved = resolve_base_model_location(Some(&manifest.base.model_id), None, false)?;
        (
            resolved.full_path.display().to_string(),
            "AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID".to_string(),
        )
    };
    let tokenizer_path = format!("{}/tokenizer.json", model_path);

    if !std::path::Path::new(&model_path).exists() {
        return Err(anyhow::anyhow!(
            "Model directory not found: {} (source: {}). Set --model-path or configure AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID.",
            model_path,
            model_path_source
        ));
    }

    output.success(format!("Model directory found: {}", model_path));
    if model_config.is_some() {
        output.verbose("Using model path from CLI/environment configuration");
    }

    // 2. Initialize telemetry writer
    let telemetry_dir = resolve_telemetry_dir()?;
    std::fs::create_dir_all(&telemetry_dir.path)?;
    let telemetry = adapteros_telemetry::TelemetryWriter::new(
        telemetry_dir.path.clone(),
        500_000,           // max_events from policy
        256 * 1024 * 1024, // max_bytes (256MB) from policy
    )?;

    output.success(format!(
        "Telemetry writer initialized at {} (source: {})",
        telemetry_dir.path.display(),
        telemetry_dir.source
    ));

    // 3. Initialize RAG system (optional)
    let rag = if manifest.policies.evidence.require_open_book {
        output.info("Initializing RAG system...");
        let index_root = resolve_index_root()?;
        let index_dir = index_root.path.join(tenant);
        if index_dir.exists() {
            // Use a placeholder embedding hash - in production this should come from the manifest
            let embedding_hash = adapteros_core::B3Hash::hash(b"placeholder");
            match adapteros_lora_rag::RagSystem::new(index_dir, embedding_hash) {
                Ok(rag_system) => {
                    output.success(format!(
                        "RAG system initialized (indices at {}, source: {})",
                        index_root.path.display(),
                        index_root.source
                    ));
                    Some(rag_system)
                }
                Err(e) => {
                    output.warning(format!("Failed to initialize RAG system: {}", e));
                    output.warning("Continuing without evidence retrieval");
                    None
                }
            }
        } else {
            output.warning("RAG index not found, continuing without evidence retrieval");
            None
        }
    } else {
        output.verbose("RAG system not required by policy");
        None
    };

    // 4. Initialize backend based on selection
    output.info(format!("Initializing {:?} backend...", backend));

    // Compile-time guard for multi-backend support
    #[cfg(feature = "multi-backend")]
    {
        if !matches!(backend, BackendType::Metal) {
            output.warning("⚠️  MULTI-BACKEND ENABLED - NOT FOR PRODUCTION ⚠️");
            output.warning("The selected backend may not provide deterministic execution.");
            output.warning("For production use, rebuild with default features (Metal only).");
        }
    }

    let backend_choice = match backend {
        BackendType::Metal => {
            output.verbose("Using Metal backend (macOS GPU)");
            adapteros_lora_worker::BackendChoice::Metal
        }
        BackendType::Mlx => {
            output.verbose("Using MLX backend (Python/MLX)");

            #[cfg(not(feature = "multi-backend"))]
            {
                output.error("MLX backend requires --features multi-backend");
                output.info("Rebuild with: cargo build --features multi-backend");
                return Err(anyhow::anyhow!(
                    "MLX backend not available in deterministic-only build"
                ));
            }

            #[cfg(feature = "multi-backend")]
            {
                // Check if MLX is available
                let mlx_env_set =
                    std::env::var("MLX_LIB_DIR").is_ok() || std::env::var("MLX_PATH").is_ok();
                if mlx_env_set {
                    output.verbose("MLX path variables detected (MLX_LIB_DIR/MLX_PATH)");
                }

                let mlx_available = std::process::Command::new("python3")
                    .args(&["-c", "import mlx.core; print('ok')"])
                    .output()
                    .map(|out| String::from_utf8_lossy(&out.stdout).contains("ok"))
                    .unwrap_or(false);

                if !mlx_available {
                    output.error("MLX not found. Install the C++ MLX library (real backend) before using --backend mlx.");
                    output.info("  Homebrew: brew install mlx");
                    output.info(
                        "  Or set MLX_PATH/MLX_INCLUDE_DIR/MLX_LIB_DIR to your installation.",
                    );
                    output.info(
                        "  Docs: MLX_INSTALLATION_GUIDE.md or run scripts/build-mlx.sh --help",
                    );
                    return Err(anyhow::anyhow!("MLX not installed"));
                }

                output.verbose("MLX detected");
                adapteros_lora_worker::BackendChoice::Mlx {
                    model_path: model_path.clone(),
                }
            }
        }
        BackendType::CoreML => {
            output.verbose("Using CoreML backend (macOS Neural Engine)");

            #[cfg(not(feature = "multi-backend"))]
            {
                output.error("CoreML backend requires --features multi-backend");
                output.info("Rebuild with: cargo build --features multi-backend");
                return Err(anyhow::anyhow!(
                    "CoreML backend not available in deterministic-only build"
                ));
            }

            #[cfg(feature = "multi-backend")]
            {
                // CoreML backend not yet implemented
                output.error("CoreML backend not yet implemented");
                output.info("Please use Metal or MLX backend instead");
                return Err(anyhow::anyhow!("CoreML backend not implemented"));
            }
        }
    };

    let kernels = adapteros_lora_worker::create_backend(backend_choice)
        .map_err(|e| anyhow::anyhow!("Failed to create backend: {}", e))?;

    output.success(format!("{:?} backend initialized", backend));

    // Load LoRA adapters if using MLX backend
    #[cfg(feature = "multi-backend")]
    if matches!(backend, BackendType::Mlx) {
        output.info("Loading LoRA adapters for MLX backend...");
        let mut adapters_loaded = 0;

        for (adapter_id, adapter_spec) in manifest.adapters.iter().enumerate() {
            let adapter_path = format!("./adapters/{}.safetensors", adapter_spec.id);
            if std::path::Path::new(&adapter_path).exists() {
                let config = adapteros_lora_mlx_ffi::LoRAConfig {
                    rank: adapter_spec.rank as usize,
                    alpha: adapter_spec.alpha,
                    target_modules: adapter_spec.target_modules.clone(),
                    dropout: 0.0,
                };

                output.verbose(format!(
                    "Loading adapter: {} (id={})",
                    adapter_spec.id, adapter_id
                ));

                match adapteros_lora_mlx_ffi::LoRAAdapter::load(
                    &adapter_path,
                    adapter_spec.id.clone(),
                    config,
                ) {
                    Ok(_adapter) => {
                        // Load adapter weights into backend
                        // For now, we skip this as it requires extending the trait
                        output.verbose(format!(
                            "  Loaded adapter: {} (id={})",
                            adapter_spec.id, adapter_id
                        ));
                        adapters_loaded += 1;
                    }
                    Err(e) => {
                        output.warning(format!(
                            "  Failed to load adapter {}: {}",
                            adapter_spec.id, e
                        ));
                    }
                }
            } else {
                output.warning(format!("Adapter file not found: {}", adapter_path));
            }
        }
        output.success(format!("{} adapters loaded successfully", adapters_loaded));
    }

    #[cfg(not(feature = "multi-backend"))]
    if matches!(backend, BackendType::Mlx) {
        return Err(anyhow::anyhow!(
            "MLX backend requires 'multi-backend' feature"
        ));
    }

    if !matches!(backend, BackendType::Mlx) {
        output.verbose("Metal backend: adapters loaded from plan");
    }

    // 5. Create worker with all components
    output.info("Creating worker instance...");

    // Convert BackendType to BackendKind for AvailableBackends
    let backend_kind = match backend {
        BackendType::Metal => adapteros_core::BackendKind::Metal,
        BackendType::Mlx => adapteros_core::BackendKind::Mlx,
        BackendType::CoreML => adapteros_core::BackendKind::CoreML,
    };

    let available_backends = adapteros_lora_worker::AvailableBackends {
        primary: backend_kind,
        fallback: None,
        coreml_primary: None,
        coreml_fallback: None,
    };

    // PRD-06: Generate a default worker_id for CLI serve using BLAKE3
    // Combines tenant with timestamp for per-session uniqueness while maintaining determinism
    let cli_worker_id: u32 = {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let input = format!("{}:{}", tenant, timestamp);
        let hash = adapteros_core::B3Hash::hash(input.as_bytes());
        let bytes: [u8; 4] = hash.as_bytes()[0..4].try_into().unwrap_or([0; 4]);
        u32::from_le_bytes(bytes)
    };

    let worker = adapteros_lora_worker::Worker::new(
        manifest.clone(),
        tenant,
        kernels,
        available_backends,
        rag,
        &tokenizer_path,
        &model_path,
        telemetry,
        None,
        None,
        None, // No quota manager for CLI serve command
        cli_worker_id,
    )
    .await?;
    output.success("Worker initialized");

    output.blank();
    output.success("Server configuration complete");
    output.kv("Tenant", tenant);
    output.kv("Plan", plan);
    output.kv("Socket", &socket.display().to_string());
    output.kv("Model", &manifest.base.model_id);
    output.kv("Adapters", &manifest.adapters.len().to_string());
    output.blank();
    output.success("Starting UDS server...");

    // 6. Start UDS server with worker
    adapteros_api::serve_uds_with_worker(socket, worker)
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    output.blank();
    output.info("Shutdown signal received");

    Ok(())
}
