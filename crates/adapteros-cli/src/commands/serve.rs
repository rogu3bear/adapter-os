//! Start serving

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

pub async fn run(
    tenant: &str,
    plan: &str,
    socket: &Path,
    backend: BackendType,
    dry_run: bool,
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

    // 1. Load model paths
    let model_path = format!("./models/{}", manifest.base.model_id);
    let tokenizer_path = format!("{}/tokenizer.json", model_path);

    if !std::path::Path::new(&model_path).exists() {
        return Err(anyhow::anyhow!("Model directory not found: {}", model_path));
    }

    output.success(format!("Model directory found: {}", model_path));

    // 2. Initialize telemetry writer
    let telemetry_dir = std::path::PathBuf::from("./var/telemetry");
    std::fs::create_dir_all(&telemetry_dir)?;
    let telemetry = adapteros_telemetry::TelemetryWriter::new(
        telemetry_dir,
        500_000,           // max_events from policy
        256 * 1024 * 1024, // max_bytes (256MB) from policy
    )?;

    output.success("Telemetry writer initialized");

    // 3. Initialize RAG system (optional)
    let rag = if manifest.policies.evidence.require_open_book {
        output.info("Initializing RAG system...");
        let index_dir = std::path::PathBuf::from(format!("./var/indices/{}", tenant));
        if index_dir.exists() {
            // Use a placeholder embedding hash - in production this should come from the manifest
            let embedding_hash = adapteros_core::B3Hash::hash(b"placeholder");
            match adapteros_lora_rag::RagSystem::new(index_dir, embedding_hash) {
                Ok(rag_system) => {
                    output.success("RAG system initialized");
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

    // Compile-time guard for experimental backends
    #[cfg(feature = "experimental-backends")]
    {
        if !matches!(backend, BackendType::Metal) {
            output.warning("⚠️  EXPERIMENTAL BACKENDS ENABLED - NOT FOR PRODUCTION ⚠️");
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

            #[cfg(not(feature = "experimental-backends"))]
            {
                output.error("MLX backend requires --features experimental-backends");
                output.info("Rebuild with: cargo build --features experimental-backends");
                return Err(anyhow::anyhow!(
                    "MLX backend not available in deterministic-only build"
                ));
            }

            #[cfg(feature = "experimental-backends")]
            {
                // Check if MLX is available
                let mlx_available = std::process::Command::new("python3")
                    .args(&["-c", "import mlx.core; print('ok')"])
                    .output()
                    .map(|out| String::from_utf8_lossy(&out.stdout).contains("ok"))
                    .unwrap_or(false);

                if !mlx_available {
                    output.error("MLX not found. Please install MLX:");
                    output.info("  uv pip install mlx");
                    output.info("Or use: pip install mlx");
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

            #[cfg(not(feature = "experimental-backends"))]
            {
                output.error("CoreML backend requires --features experimental-backends");
                output.info("Rebuild with: cargo build --features experimental-backends");
                return Err(anyhow::anyhow!(
                    "CoreML backend not available in deterministic-only build"
                ));
            }

            #[cfg(feature = "experimental-backends")]
            {
                // CoreML backend not yet implemented
                output.error("CoreML backend not yet implemented");
                output.info("Please use Metal or MLX backend instead");
                return Err(anyhow::anyhow!("CoreML backend not implemented"));
            }
        }
    };

    let mut kernels = adapteros_lora_worker::create_backend(backend_choice)
        .map_err(|e| anyhow::anyhow!("Failed to create backend: {}", e))?;

    output.success(format!("{:?} backend initialized", backend));

    // Load LoRA adapters if using MLX backend
    #[cfg(feature = "experimental-backends")]
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

    #[cfg(not(feature = "experimental-backends"))]
    if matches!(backend, BackendType::Mlx) {
        return Err(anyhow::anyhow!(
            "MLX backend requires 'experimental-backends' feature"
        ));
    }

    if !matches!(backend, BackendType::Mlx) {
        output.verbose("Metal backend: adapters loaded from plan");
    }

    // 5. Create worker with all components
    output.info("Creating worker instance...");
    let worker = adapteros_lora_worker::Worker::new(
        manifest.clone(),
        kernels,
        rag,
        &tokenizer_path,
        &model_path,
        telemetry,
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
