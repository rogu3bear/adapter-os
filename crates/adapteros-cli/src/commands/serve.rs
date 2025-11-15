//! Start serving

use adapteros_policy::egress;
use anyhow::Result;
use std::path::Path;

#[cfg(target_os = "macos")]
use nix::unistd::geteuid;

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

#[cfg(feature = "serve-with-config")]
use adapteros_server::config::Config;

pub async fn run(
    tenant: &str,
    plan: &str,
    socket: &Path,
    backend: BackendType,
    dry_run: bool,
    capture_events: Option<&std::path::PathBuf>,
    output: &OutputWriter,
) -> Result<()> {
    output.section("Starting AdapterOS server");

    // Normalize socket path: if default global path is used, select per-tenant path
    let mut socket_path = socket.to_path_buf();
    if socket_path.as_os_str() == "/var/run/aos/aos.sock" {
        // Fallback to per-tenant path; will be refined after manifest load
        socket_path = std::path::PathBuf::from(format!("/var/run/aos/{}/aos.sock", tenant));
    }

    output.kv("Tenant", tenant);
    output.kv("Plan", plan);
    output.kv("Socket", &socket_path.display().to_string());
    output.blank();

    if dry_run {
        output.info("Dry-run mode: validating preflight checks only");
        output.blank();
    }

    // Phase 0: Egress preflight validation is deferred until after manifest load

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

    // Load configuration for MLX settings (if feature enabled)
    #[cfg(feature = "serve-with-config")]
    let mlx_model_path = {
        let config_path = "configs/cp.toml";
        if std::path::Path::new(config_path).exists() {
            match Config::load(config_path) {
                Ok(config) => {
                    output.verbose(format!("Configuration loaded from {}", config_path));

                    // Apply MLX configuration if present
                    if let Some(mlx_config) = &config.mlx {
                        if mlx_config.enabled {
                            // Check for early feature validation
                            #[cfg(not(any(
                                feature = "mlx-ffi-backend",
                                feature = "experimental-backends"
                            )))]
                            {
                                output.warning("MLX backend is enabled in config but --features mlx-ffi-backend not enabled");
                                output.warning("MLX backend will fail at runtime. Rebuild with: cargo build --features mlx-ffi-backend");
                            }

                            mlx_config.model_path.clone()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Err(e) => {
                    output.warning(format!(
                        "Could not load configuration from {}: {}",
                        config_path, e
                    ));
                    output.warning("MLX configuration will be unavailable");
                    None
                }
            }
        } else {
            output.verbose("No configuration file found, using defaults");
            None
        }
    };

    #[cfg(not(feature = "serve-with-config"))]
    let mlx_model_path = {
        output.verbose("Config loading not available, using environment variables only");
        None
    };

    // After manifest load, re-evaluate default socket path using policy uds_root
    if socket.as_os_str() == "/var/run/aos/aos.sock" {
        let uds_root = manifest
            .policies
            .isolation
            .uds_root
            .replace("<tenant>", tenant);
        socket_path = std::path::PathBuf::from(uds_root).join("aos.sock");
        output.verbose(format!(
            "Using policy-derived UDS root for tenant: {}",
            socket_path.display()
        ));
    }

    // Phase 0: Egress preflight (policy-aware, post-manifest)
    let skip_egress = std::env::var("AOS_INSECURE_SKIP_EGRESS")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if manifest.policies.egress.serve_requires_pf && !skip_egress {
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
    } else {
        // Policy disabled or env override set
        output.warning("Skipping PF egress validation (policy or AOS_INSECURE_SKIP_EGRESS)");
        // Best-effort network socket scan for awareness
        let _ = adapteros_policy::egress::validate_no_network_sockets();
    }

    // Strict mode: refuse to run with insecure confidence skip in production
    let strict_mode = std::env::var("AOS_STRICT_MODE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let insecure_skip_conf = std::env::var("AOS_INSECURE_SKIP_CONF")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    if strict_mode && insecure_skip_conf {
        output.error("Strict mode enabled: AOS_INSECURE_SKIP_CONF must not be set");
        return Err(anyhow::anyhow!(
            "Refusing to start with AOS_INSECURE_SKIP_CONF under AOS_STRICT_MODE"
        ));
    }

    if dry_run {
        output.blank();
        output.success("All preflight checks passed");
        output.kv("System status", "ready to serve");
        output.blank();
        output.info("Re-run without --dry-run to start serving.");
        return Ok(());
    }

    // Create UDS server directory
    let socket_dir = socket_path
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
        use adapteros_lora_worker::launcher::{setup_tenant_isolation, TenantIsolation};
        // Only attempt isolation setup if effective root
        if geteuid().as_raw() == 0 {
            let (uid, gid) = get_tenant_credentials(tenant)?;
            let tenant_root = std::path::PathBuf::from("/var/lib/aos").join(tenant);
            let isolation = TenantIsolation {
                tenant_id: tenant.to_string(),
                uid,
                gid,
                root_dir: tenant_root,
                socket_path: socket_path.clone(),
            };
            match setup_tenant_isolation(&isolation) {
                Ok(()) => {
                    output.success(format!("Tenant isolation established for: {}", tenant));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed tenant isolation (continuing dev mode)");
                    output.warning(format!("Failed tenant isolation: {}", e));
                    output.warning("Continuing with current privileges (dev mode)");
                }
            }
        } else {
            output.verbose("Not running as root, skipping privilege drop");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        output.verbose("Note: Privilege dropping only available on macOS");
        if std::env::var("AOS_INSECURE_SKIP_EGRESS").ok().is_none() {
            output.warning(
                "Development mode: egress enforcement unavailable on this platform; do not use in production",
            );
        }
    }

    output.info("Initializing worker...");

    // 1. Load model paths with MLX precedence
    let model_path = if matches!(backend, BackendType::Mlx) {
        // For MLX backend: env var > config > manifest-based
        if let Ok(env_path) = std::env::var("AOS_MLX_FFI_MODEL") {
            output.verbose(format!(
                "Using MLX model path from AOS_MLX_FFI_MODEL: {}",
                env_path
            ));
            env_path
        } else if let Some(config_path) = mlx_model_path {
            output.verbose(format!("Using MLX model path from config: {}", config_path));
            config_path
        } else {
            let default_path = format!("./models/{}", manifest.base.model_id);
            output.verbose(format!(
                "Using default MLX model path from manifest: {}",
                default_path
            ));
            default_path
        }
    } else {
        // For other backends: use manifest-based path
        format!("./models/{}", manifest.base.model_id)
    };

    let tokenizer_path = format!("{}/tokenizer.json", model_path);

    // Validate model path (MLX validation happens in backend selection)
    if !matches!(backend, BackendType::Mlx) && !std::path::Path::new(&model_path).exists() {
        return Err(anyhow::anyhow!("Model directory not found: {}", model_path));
    }

    if !matches!(backend, BackendType::Mlx) {
        output.success(format!("Model directory found: {}", model_path));
    }

    // Validate tokenizer exists early to fail fast with actionable error
    if !std::path::Path::new(&tokenizer_path).exists() {
        return Err(anyhow::anyhow!(
            "Tokenizer file not found: {}",
            tokenizer_path
        ));
    }

    // 2. Initialize telemetry writer
    let default_dir = std::path::PathBuf::from(format!("./var/telemetry/{}", tenant));
    let telemetry_dir = capture_events.cloned().unwrap_or(default_dir);
    std::fs::create_dir_all(&telemetry_dir)?;
    let telemetry = adapteros_telemetry::TelemetryWriter::new(
        telemetry_dir.clone(),
        manifest.telemetry.bundle.max_events,
        manifest.telemetry.bundle.max_bytes,
    )?;

    output.success("Telemetry writer initialized");
    output.kv("TelemetryDir", &telemetry_dir.display().to_string());

    // 3. Initialize RAG system (optional)
    let rag = if manifest.policies.evidence.require_open_book {
        output.info("Initializing RAG system...");

        // Use embedding model hash from manifest policy
        let embedding_hash = manifest.policies.rag.embedding_model_hash;

        // Feature-gated pgvector backend
        #[cfg(feature = "rag-pgvector")]
        {
            // Try Postgres pgvector backend first
            match adapteros_db::postgres::PostgresDb::connect_env().await {
                Ok(db) => {
                    let _ = db.migrate().await; // best-effort migrations
                    let pool = db.pool().clone();
                    // Embedding dimension: configurable via env, defaults to 3584
                    let embedding_dim = std::env::var("RAG_EMBED_DIM")
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(3584);
                    let pg = adapteros_lora_rag::PgVectorIndex::new_postgres(
                        pool,
                        embedding_hash,
                        embedding_dim,
                    );
                    let rag_system =
                        adapteros_lora_rag::RagSystem::from_pg_index(pg, embedding_hash);
                    output.success("RAG system (pgvector) initialized");
                    Some(rag_system)
                }
                Err(e) => {
                    output.warning(format!(
                        "Failed to connect to PostgreSQL (falling back to in-memory RAG): {}",
                        e
                    ));

                    // Fallback to in-memory if Postgres failed
                    let index_dir = std::path::PathBuf::from(format!("./var/indices/{}", tenant));
                    if index_dir.exists() {
                        match adapteros_lora_rag::RagSystem::new(index_dir, embedding_hash) {
                            Ok(rag_system) => {
                                output.success("RAG system (in-memory) initialized");
                                Some(rag_system)
                            }
                            Err(e) => {
                                output.warning(format!("Failed to initialize RAG system: {}", e));
                                output.warning("Continuing without evidence retrieval");
                                None
                            }
                        }
                    } else {
                        output
                            .warning("RAG index not found, continuing without evidence retrieval");
                        None
                    }
                }
            }
        }

        // Default in-memory backend when feature is not enabled
        #[cfg(not(feature = "rag-pgvector"))]
        {
            let index_dir = std::path::PathBuf::from(format!("./var/indices/{}", tenant));
            if index_dir.exists() {
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
        }
    } else {
        output.verbose("RAG system not required by policy");
        None
    };

    // If policy requires open-book evidence, refuse to serve without a RAG backend
    if manifest.policies.evidence.require_open_book && rag.is_none() {
        output.error("Policy requires open-book evidence, but no RAG index/backend is available");
        output.error(
            "Initialize a pgvector index or create a local index under ./var/indices/<tenant>",
        );
        output.info("Docs: see docs/rag-pgvector.md for setup instructions");
        return Err(anyhow::anyhow!(
            "Refusing to serve without evidence backend when policy requires open-book"
        ));
    }

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
            output.verbose("Using MLX backend (C++ FFI)");

            // Validate that we have a model path for MLX
            if !std::path::Path::new(&model_path).exists() {
                output.error(format!("MLX model directory not found: {}", model_path));
                output.info(
                    "Set AOS_MLX_FFI_MODEL environment variable or configure in configs/cp.toml",
                );
                output.info("Or ensure the model directory exists at the expected path");
                return Err(anyhow::anyhow!(
                    "MLX model directory not found: {}",
                    model_path
                ));
            }

            #[cfg(not(any(feature = "mlx-ffi-backend", feature = "experimental-backends")))]
            {
                output.error("MLX backend requires --features mlx-ffi-backend");
                output.info("Rebuild with: cargo build --features mlx-ffi-backend");
                return Err(anyhow::anyhow!(
                    "MLX backend not available in deterministic-only build"
                ));
            }

            #[cfg(any(feature = "mlx-ffi-backend", feature = "experimental-backends"))]
            {
                adapteros_lora_worker::BackendChoice::Mlx {
                    model_path: std::path::PathBuf::from(&model_path),
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

    // Load LoRA adapters if using MLX backend (or any backend supporting hot-swap)
    if matches!(backend, BackendType::Mlx) {
        output.info("Loading LoRA adapters for MLX backend...");
        let mut adapters_loaded = 0;

        for (adapter_id, adapter_spec) in manifest.adapters.iter().enumerate() {
            let packaged = format!("./adapters/{}/weights.safetensors", adapter_spec.id);
            let flat = format!("./adapters/{}.safetensors", adapter_spec.id);
            let adapter_path = if std::path::Path::new(&packaged).exists() {
                packaged
            } else if std::path::Path::new(&flat).exists() {
                flat
            } else {
                output.warning(format!(
                    "Adapter files not found for {} (checked: {}, {})",
                    adapter_spec.id, packaged, flat
                ));
                continue;
            };
            if std::path::Path::new(&adapter_path).exists() {
                output.verbose(format!(
                    "Loading adapter: {} (id={})",
                    adapter_spec.id, adapter_id
                ));

                match std::fs::read(&adapter_path) {
                    Ok(bytes) => {
                        if let Err(e) = kernels.load_adapter(adapter_id as u16, &bytes) {
                            output.warning(format!(
                                "  Backend rejected adapter {}: {}",
                                adapter_spec.id, e
                            ));
                        } else {
                            adapters_loaded += 1;
                            output.verbose(format!(
                                "  Loaded adapter into backend: {}",
                                adapter_spec.id
                            ));
                        }
                    }
                    Err(e) => {
                        output.warning(format!(
                            "  Failed to read adapter {}: {}",
                            adapter_spec.id, e
                        ));
                    }
                }
            }
        }
        output.success(format!("{} adapters loaded successfully", adapters_loaded));
    } else {
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

    // Run a lightweight warmup to validate tokenizer and a single kernel step
    match worker.warmup().await {
        Ok(report) => {
            let steps = report.get("steps").and_then(|v| v.as_u64()).unwrap_or(0);
            let duration_ms = report.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
            output.success(format!(
                "Warmup complete: steps={} duration={}ms",
                steps, duration_ms
            ));
        }
        Err(e) => {
            output.warning(format!("Warmup failed (continuing in dev mode): {}", e));
        }
    }

    output.blank();
    output.success("Server configuration complete");
    output.kv("Tenant", tenant);
    output.kv("Plan", plan);
    output.kv("Socket", &socket_path.display().to_string());
    output.kv("Model", &manifest.base.model_id);
    output.kv("Adapters", &manifest.adapters.len().to_string());
    output.blank();
    output.success("Starting UDS server...");
    // 6. Register worker with control plane (best-effort) and start heartbeats
    {
        let uds_path_str = socket_path.display().to_string();
        let tenant_id = tenant.to_string();
        let plan_id = plan.to_string();
        let cp_base =
            std::env::var("AOS_CP_URL").unwrap_or_else(|_| "http://127.0.0.1:3200".to_string());
        let cp_api = format!("{}/api", cp_base.trim_end_matches('/'));
        let jwt_opt = std::env::var("AOS_CP_JWT").ok();
        let pid = std::process::id() as i32;

        // Perform registration in a task to avoid blocking
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let url = format!("{}/v1/workers/register-local", cp_api);
            let body = serde_json::json!({
                "tenant_id": tenant_id,
                "plan_id": plan_id,
                "node_id": "local",
                "uds_path": uds_path_str,
                "pid": pid,
            });

            let mut req = client.post(&url).json(&body);
            if let Some(jwt) = &jwt_opt {
                req = req.bearer_auth(jwt);
            }

            match req.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        if let Ok(val) = resp.json::<serde_json::Value>().await {
                            if let Some(worker_id) = val.get("id").and_then(|v| v.as_str()) {
                                // Start heartbeat loop
                                let hb_client = reqwest::Client::new();
                                let hb_url =
                                    format!("{}/v1/workers/{}/heartbeat", cp_api, worker_id);
                                let jwt_clone = jwt_opt.clone();
                                tokio::spawn(async move {
                                    let mut interval =
                                        tokio::time::interval(std::time::Duration::from_secs(10));
                                    loop {
                                        interval.tick().await;
                                        let mut req = hb_client.post(&hb_url);
                                        if let Some(jwt) = &jwt_clone {
                                            req = req.bearer_auth(jwt);
                                        }
                                        let _ = req.send().await;
                                    }
                                });
                            }
                        }
                    }
                }
                Err(_e) => {
                    // Best-effort: ignore registration failure in dev mode
                }
            }
        });
    }

    // 7. Start UDS server with worker
    adapteros_api::serve_uds_with_worker(&socket_path, worker)
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    output.blank();
    output.info("Shutdown signal received");

    Ok(())
}
