//! Executor initialization for deterministic execution.
//!
//! This module handles:
//! - Manifest path resolution and loading
//! - Deterministic seed derivation from manifest hash
//! - Global executor initialization
//! - MLX runtime initialization (feature-gated)
//! - Shutdown coordinator and background task tracking

use crate::cli::Cli;
use crate::shutdown::ShutdownCoordinator;
use adapteros_config::{resolve_manifest_path, ConfigLoader};
use adapteros_core::{derive_seed, AosError, B3Hash};
use adapteros_deterministic_exec::{init_global_executor, EnforcementMode, ExecutorConfig};
use adapteros_model_hub::manifest::ManifestV3;
use adapteros_server_api::config::Config;
use adapteros_server_api::state::BackgroundTaskTracker;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, instrument, warn};

/// Context returned from executor initialization containing shutdown
/// coordination, background task tracking, and manifest hash.
pub struct ExecutorContext {
    pub shutdown_coordinator: ShutdownCoordinator,
    pub background_tasks: Arc<BackgroundTaskTracker>,
    pub manifest_hash: Option<B3Hash>,
}

/// Initializes the deterministic executor with manifest-derived seed.
///
/// This function:
/// 1. Resolves manifest path from env, CLI, or config
/// 2. Creates shutdown coordinator and background task tracker
/// 3. Loads and validates manifest
/// 4. Derives deterministic seed from manifest hash using HKDF
/// 5. Initializes global executor with derived seed
/// 6. Optionally initializes MLX runtime (feature-gated)
///
/// # Arguments
///
/// * `config` - Server configuration (wrapped in Arc<RwLock>)
/// * `cli` - CLI arguments containing manifest path and config path
///
/// # Returns
///
/// Returns `ExecutorContext` containing shutdown coordinator, background tasks,
/// and the manifest hash (if loaded successfully).
///
/// # Errors
///
/// Returns error if:
/// - Manifest path resolution fails
/// - Production mode is enabled but manifest is invalid
/// - Executor initialization fails
#[instrument(skip_all)]
pub async fn initialize_executor(
    config: Arc<RwLock<Config>>,
    cli: &Cli,
) -> Result<ExecutorContext> {
    info!(target: "boot", phase = 3, name = "executor", "═══ BOOT PHASE 3/12: Deterministic Executor ═══");

    // Resolve manifest path with precedence: env > CLI > config > dev fallback (debug-only)
    let config_manifest_path = {
        let loader = ConfigLoader::new();
        match loader.load(vec![], Some(cli.config.clone())) {
            Ok(cfg) => cfg.get("manifest.path").map(PathBuf::from),
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to load manifest.path from config; continuing without config override"
                );
                None
            }
        }
    };

    let manifest_resolution =
        resolve_manifest_path(cli.manifest_path.as_ref(), config_manifest_path.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to resolve manifest path: {}", e))?;
    let manifest_path = manifest_resolution.path.clone();
    info!(
        path = %manifest_path.display(),
        source = %manifest_resolution.source,
        dev_fallback = manifest_resolution.used_dev_fallback,
        "Resolved manifest path for executor seeding"
    );

    // Initialize shutdown coordinator for graceful lifecycle management
    let shutdown_coordinator = ShutdownCoordinator::new();
    let background_tasks = Arc::new(BackgroundTaskTracker::default());

    // Initialize deterministic executor with manifest-derived seed
    info!("Initializing deterministic executor");

    // Load manifest for deterministic seeding
    let manifest_hash = if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path) {
            Ok(json) => match serde_json::from_str::<ManifestV3>(&json) {
                Ok(manifest) => {
                    // Validate manifest before using for seeding
                    if let Err(e) = manifest.validate() {
                        warn!(
                            error = %e,
                            path = %manifest_path.display(),
                            "Manifest validation failed, using default seed"
                        );
                        None
                    } else {
                        match manifest.compute_hash() {
                            Ok(hash) => {
                                info!(
                                    manifest_hash = %hash.to_hex()[..16],
                                    path = %manifest_path.display(),
                                    "Loaded and validated manifest for executor seeding"
                                );
                                Some(hash)
                            }
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    path = %manifest_path.display(),
                                    "Failed to compute manifest hash, using default seed"
                                );
                                None
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        path = %manifest_path.display(),
                        "Failed to parse manifest, using default seed"
                    );
                    None
                }
            },
            Err(e) => {
                warn!(
                    error = %e,
                    path = %manifest_path.display(),
                    "Failed to read manifest, using default seed"
                );
                None
            }
        }
    } else {
        warn!(
            path = %manifest_path.display(),
            "Manifest not found, using default seed (development mode)"
        );
        None
    };

    // Production mode enforcement: require valid manifest
    {
        let cfg = config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;

        if cfg.security.require_pf_deny && manifest_hash.is_none() {
            return Err(AosError::Config(
                format!(
                    "Production mode (require_pf_deny=true) requires valid manifest for executor seeding. \
                     Manifest path: {} \n\
                     Set --manifest-path or AOS_MANIFEST_PATH environment variable, or disable production mode.",
                    manifest_path.display()
                )
            ).into());
        }
    }

    // Derive executor seed using HKDF from manifest hash
    let (derived_hash, global_seed) = derive_executor_seed(manifest_hash);

    info!(
        seed_hash = %derived_hash.to_hex()[..16],
        manifest_based = manifest_hash.is_some(),
        hkdf_label = "executor",
        "Derived deterministic executor seed"
    );

    let executor_config = ExecutorConfig {
        global_seed,
        enable_event_logging: true,
        max_ticks_per_task: 1_000_000,
        enforcement_mode: EnforcementMode::AuditOnly,
        ..Default::default()
    };

    // Note: Tick ledger will be initialized after DB connection and attached via init_global_executor_with_ledger
    // For now, initialize executor without ledger
    init_global_executor(executor_config.clone())
        .map_err(|e| anyhow::anyhow!("Deterministic executor init failed: {}", e))?;
    info!("Deterministic executor initialized with manifest-derived seed");

    // Initialize MLX runtime (idempotent, safe to call multiple times)
    #[cfg(feature = "multi-backend")]
    {
        if let Err(e) = adapteros_lora_worker::mlx_runtime_init() {
            tracing::warn!(
                "MLX runtime initialization failed: {}. Continuing with Metal/CoreML fallback.",
                e
            );
        } else {
            let impl_name = adapteros_lora_worker::mlx_selected_implementation()
                .map(|imp| imp.as_str())
                .unwrap_or("unknown");
            tracing::info!(
                implementation = impl_name,
                "MLX runtime initialized successfully"
            );
        }
    }

    Ok(ExecutorContext {
        shutdown_coordinator,
        background_tasks,
        manifest_hash,
    })
}

/// Derives executor seed from manifest hash using HKDF.
///
/// # Arguments
///
/// * `manifest_hash` - Optional manifest hash to derive seed from
///
/// # Returns
///
/// Returns tuple of (derived_hash, seed_bytes):
/// - `derived_hash`: B3Hash of the derived seed (for logging)
/// - `seed_bytes`: 32-byte deterministic seed for executor
///
/// # Details
///
/// Uses HKDF with label "executor" to derive a deterministic seed.
/// If no manifest hash is provided, uses a default non-production seed.
pub fn derive_executor_seed(manifest_hash: Option<B3Hash>) -> (B3Hash, [u8; 32]) {
    let base_seed = manifest_hash.unwrap_or_else(|| B3Hash::hash(b"default-seed-non-production"));
    let global_seed = derive_seed(&base_seed, "executor");
    let derived_hash = B3Hash::hash(&global_seed);
    (derived_hash, global_seed)
}
