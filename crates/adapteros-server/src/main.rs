//! adapterOS Control Plane Server
//!
//! This is the main entry point for the control plane server. Boot logic is
//! modularized in the `boot` module for testability.

// We intentionally use map_err for boot phase error tracking to capture
// errors in the BootStateManager before propagating them. This is clearer
// than using inspect_err when we need to call methods on the error.
#![allow(clippy::manual_inspect)]
// B3Hash and RuntimeMode are Copy but cloning for clarity in state construction
#![allow(clippy::clone_on_copy)]

mod assets;

use adapteros_core::determinism_mode::DeterminismMode;
use adapteros_server::boot::api_config::{build_api_config, spawn_sighup_handler};
use adapteros_server::boot::background_tasks::spawn_all_background_tasks;
use adapteros_server::boot::migrations::run_migrations;
use adapteros_server::boot::{
    bind_and_serve, bind_error_exit_code, build_app_state, check_model_server_readiness,
    enforce_invariants, finalize_boot, initialize_config, initialize_database, initialize_executor,
    initialize_federation, initialize_metrics, initialize_security, log_effective_config,
    log_startup_snapshot, run_preflight_checks, run_startup_recovery, validate_boot_invariants,
    validate_post_db_invariants, validate_production_security_env, write_boot_report, BindMode,
    RetryPolicy, ServerBindConfig, StartupOrchestrator,
};
use adapteros_server::cli::Cli;
use adapteros_server_api::boot_state::failure_codes;
use anyhow::Result;
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tracing::{error, info, instrument, warn};

mod openapi;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn configure_mlx_version_enforcement(
    server_config: &std::sync::Arc<std::sync::RwLock<adapteros_server_api::config::Config>>,
    cli_strict: bool,
) -> Result<()> {
    let strict_mode = {
        let cfg = server_config
            .read()
            .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
        cli_strict || cfg.general.determinism_mode == Some(DeterminismMode::Strict)
    };

    std::env::set_var(
        "AOS_ENFORCE_MLX_VERSION_MATCH",
        if strict_mode { "1" } else { "0" },
    );

    info!(
        strict_mode,
        "Configured MLX runtime/build version enforcement mode"
    );
    Ok(())
}

#[tokio::main]
#[instrument(skip_all)]
async fn main() -> Result<()> {
    // Parse CLI first (before logging, so we know config path)
    let cli = Cli::parse();

    // Early Security Gate: Block dev bypass flags in release builds
    validate_production_security_env()?;

    // =========================================================================
    // Phase 1: Configuration
    // =========================================================================
    let config_ctx = initialize_config(&cli).await?;
    configure_mlx_version_enforcement(&config_ctx.server_config, cli.strict)?;
    let boot_state = config_ctx.boot_state.clone();
    let startup_orchestrator = StartupOrchestrator::new(boot_state.clone());

    // Handle OpenAPI generation (early exit)
    if cli.generate_openapi {
        info!("Generating OpenAPI specification");
        openapi::generate_openapi()?;
        info!("OpenAPI spec written to openapi.json");
        return Ok(());
    }

    let boot_start = std::time::Instant::now();
    let boot_timeout = Duration::from_secs(config_ctx.boot_timeout_secs);
    let boot_state_for_timeout = config_ctx.boot_state.clone();
    let startup_orchestrator_for_boot = startup_orchestrator.clone();

    // Wrap the entire boot sequence in a timeout
    let boot_result = tokio::time::timeout(boot_timeout, async {
        let startup_orchestrator = startup_orchestrator_for_boot.clone();

        // =====================================================================
        // Phase 2: Security Initialization
        // =====================================================================
        let security_ctx = startup_orchestrator
            .run_phase(
                "security_init",
                failure_codes::SECURITY_INIT_FAILED,
                RetryPolicy::with_attempts(2),
                |_| async { initialize_security(config_ctx.server_config.clone(), &cli).await },
                None,
            )
            .await?;

        // Log effective configuration
        log_effective_config(&config_ctx.server_config)?;

        // =====================================================================
        // Phase 3: Deterministic Executor
        // =====================================================================
        let executor_ctx = startup_orchestrator
            .run_phase(
                "executor_init",
                failure_codes::EXECUTOR_INIT_FAILED,
                RetryPolicy::with_attempts(2),
                |_| async { initialize_executor(config_ctx.server_config.clone(), &cli).await },
                None,
            )
            .await?;

        #[cfg(feature = "multi-backend")]
        if std::env::var("AOS_ENFORCE_MLX_VERSION_MATCH").as_deref() == Ok("1") {
            startup_orchestrator
                .run_phase(
                    "mlx_version_guard",
                    failure_codes::EXECUTOR_INIT_FAILED,
                    RetryPolicy::no_retry(),
                    |_| async {
                        adapteros_lora_worker::mlx_runtime_init().map_err(|e| {
                            anyhow::anyhow!(
                                "Strict determinism requires MLX runtime/build version parity: {}",
                                e
                            )
                        })
                    },
                    None,
                )
                .await?;
        }

        startup_orchestrator.mark_determinism_seed_initialized(executor_ctx.manifest_hash.is_some());

        let has_manifest_hash = executor_ctx.manifest_hash.is_some();
        startup_orchestrator
            .run_phase(
                "determinism_seed",
                failure_codes::EXECUTOR_INIT_FAILED,
                RetryPolicy::no_retry(),
                |_| async {
                    if has_manifest_hash {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "Deterministic manifest hash missing; executor seed not initialized"
                        ))
                    }
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 4: Security Preflight Checks
        // =====================================================================
        startup_orchestrator
            .run_phase(
                "preflight",
                failure_codes::PREFLIGHT_FAILED,
                RetryPolicy::with_attempts(2),
                |_| async { run_preflight_checks(config_ctx.server_config.clone(), &cli).await },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 4b: Boot Invariants Validation
        // =====================================================================
        let production_mode = {
            let cfg = config_ctx
                .server_config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            cfg.server.production_mode
        };
        startup_orchestrator
            .run_phase(
                "invariants",
                failure_codes::INVARIANTS_FAILED,
                RetryPolicy::no_retry(),
                |_| async {
                    let invariant_report = validate_boot_invariants(
                        &config_ctx.server_config,
                        executor_ctx.manifest_hash.is_some(),
                    );
                    enforce_invariants(&invariant_report, production_mode)
                        .map_err(|e| anyhow::anyhow!("{}", e))
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 4c: Model Server Readiness (if enabled)
        // =====================================================================
        startup_orchestrator
            .run_phase(
                "model_server_readiness",
                failure_codes::MODEL_SERVER_FAILED,
                RetryPolicy::with_attempts(3),
                |_| async {
                    let mut effective_config = adapteros_config::try_effective_config();
                    if effective_config.is_none() {
                        if let Err(e) =
                            adapteros_config::init_effective_config(Some(&cli.config), vec![])
                        {
                            if production_mode {
                                return Err(anyhow::anyhow!(
                                    "Failed to initialize effective config before model readiness: {}",
                                    e
                                ));
                            }
                            warn!(
                                "Failed to initialize effective config before model readiness; skipping model server readiness in dev: {}",
                                e
                            );
                        }
                        effective_config = adapteros_config::try_effective_config();
                    }
                    if let Some(effective_config) = effective_config {
                        let _model_server_ctx = check_model_server_readiness(effective_config).await?;
                    } else if !production_mode {
                        warn!("Effective config unavailable; skipping model server readiness in dev");
                    }
                    Ok(())
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 5: Database Connection
        // =====================================================================
        let db_ctx = startup_orchestrator
            .run_phase(
                "db_connect",
                failure_codes::DB_CONN_FAILED,
                RetryPolicy::with_attempts(3),
                |_| async {
                    initialize_database(
                        config_ctx.server_config.clone(),
                        config_ctx.boot_state.clone(),
                        &cli,
                    )
                    .await
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 6: Database Migrations
        // =====================================================================
        startup_orchestrator
            .run_phase(
                "migrations",
                failure_codes::MIGRATION_FAILED,
                RetryPolicy::with_attempts(2),
                |_| async {
                    run_migrations(
                        &db_ctx.db,
                        config_ctx.server_config.clone(),
                        &cli,
                        &db_ctx.boot_state,
                    )
                    .await
                    .map(|_| ())
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 6b: Post-DB Invariants Validation
        // =====================================================================
        // These invariants require a live database connection (trigger checks, etc.)
        startup_orchestrator
            .run_phase(
                "post_db_invariants",
                failure_codes::INVARIANTS_FAILED,
                RetryPolicy::no_retry(),
                |_| async {
                    let post_db_report =
                        validate_post_db_invariants(&config_ctx.server_config, db_ctx.db.pool_result()?)
                            .await;
                    enforce_invariants(&post_db_report, production_mode)
                        .map_err(|e| anyhow::anyhow!("{}", e))
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 7: Startup Recovery
        // =====================================================================
        // Recover orphaned resources from previous server crashes/restarts
        // ANCHOR: This phase is best-effort and runs before accepting requests
        startup_orchestrator
            .run_phase(
                "startup_recovery",
                failure_codes::STARTUP_RECOVERY_FAILED,
                RetryPolicy::with_attempts(2),
                |_| async {
                    run_startup_recovery(&db_ctx.db).await.map(|_| ()).map_err(|error| {
                        warn!(error = %error, "Startup recovery encountered errors (degraded)");
                        anyhow::anyhow!(error)
                    })
                },
                Some(()),
            )
            .await?;

        // =====================================================================
        // Phases 8: Policy & Backend (already extracted to separate crates)
        // =====================================================================

        // =====================================================================
        // Phase 9a: API Configuration
        // =====================================================================
        let api_config = startup_orchestrator
            .run_phase(
                "router_build",
                failure_codes::ROUTER_BUILD_FAILED,
                RetryPolicy::no_retry(),
                |_| async { build_api_config(config_ctx.server_config.clone(), &db_ctx.db).await },
                None,
            )
            .await?;

        // Enable dev bypass from config if specified (debug builds only)
        {
            let cfg = api_config
                .read()
                .map_err(|e| anyhow::anyhow!("API config lock poisoned: {}", e))?;
            adapteros_server_api::set_dev_bypass_from_config(cfg.security.dev_bypass);
        }

        // =====================================================================
        // Phase 9b: Federation
        // =====================================================================
        let mut shutdown_coordinator = executor_ctx.shutdown_coordinator;

        // Set up SIGHUP handler for config hot-reload (returns updated coordinator)
        shutdown_coordinator = spawn_sighup_handler(
            config_ctx.server_config.clone(),
            api_config.clone(),
            cli.config.clone(),
            shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
        )?;
        let federation_ctx = initialize_federation(
            &db_ctx.db,
            config_ctx.server_config.clone(),
            &mut shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
        )
        .await?;

        // =====================================================================
        // Phase 9c: Metrics
        // =====================================================================
        let production_mode = {
            let cfg = config_ctx
                .server_config
                .read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            cfg.server.production_mode
        };
        let metrics_ctx = initialize_metrics(
            config_ctx.server_config.clone(),
            &mut shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            &db_ctx.boot_state,
            production_mode,
        )
        .await?;

        // =====================================================================
        // Phase 10a: Application State
        // =====================================================================
        let (state, shutdown_coordinator, diag_receiver, shutdown_rx) = build_app_state(
            db_ctx.db.clone(),
            api_config,
            config_ctx.server_config.clone(),
            federation_ctx.federation_daemon,
            federation_ctx.policy_watcher,
            metrics_ctx.metrics_exporter,
            metrics_ctx.uma_monitor,
            metrics_ctx.jwt_secret,
            security_ctx.worker_keypair.clone(),
            shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            &db_ctx.boot_state,
            db_ctx.runtime_mode,
            db_ctx.tick_ledger.clone(),
            executor_ctx.manifest_hash,
            cli.strict,
        )
        .await?;

        // Deterministic+replay gate must be ready before any request-serving bind.
        startup_orchestrator.mark_replay_ready(state.tick_ledger.is_some() && state.manifest_hash.is_some());
        startup_orchestrator
            .run_phase(
                "runtime_gate",
                failure_codes::INVARIANTS_FAILED,
                RetryPolicy::no_retry(),
                |_| async {
                    startup_orchestrator.ensure_runtime_gates_ready()?;
                    Ok(())
                },
                None,
            )
            .await?;

        // =====================================================================
        // Phase 10b: Background Tasks
        // =====================================================================
        boot_state.start_phase("worker_attach");
        let shutdown_coordinator = spawn_all_background_tasks(
            &state,
            &db_ctx.db,
            shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            &db_ctx.boot_state,
            cli.strict,
            state.metrics_registry.clone(),
            config_ctx.server_config.clone(),
            diag_receiver,
        )
        .await
        .map_err(|e| {
            boot_state.finish_phase_err(
                "worker_attach",
                failure_codes::WORKER_ATTACH_FAILED,
                Some(e.to_string()),
            );
            e
        })?;
        boot_state.finish_phase_ok("worker_attach");

        // =====================================================================
        // Phases 11-12: Finalization
        // =====================================================================
        let ui_routes = assets::routes();
        let boot_artifacts = startup_orchestrator
            .run_phase(
                "finalize_boot",
                failure_codes::ROUTER_BUILD_FAILED,
                RetryPolicy::no_retry(),
                |_| async {
                    finalize_boot(
                        state.clone(),
                        config_ctx.server_config.clone(),
                        ui_routes.clone(),
                        &db_ctx.boot_state,
                    )
                    .await
                },
                None,
            )
            .await?;

        // Write boot report
        write_boot_report(
            config_ctx.server_config.clone(),
            &boot_artifacts.bind_config,
            security_ctx.worker_keypair.as_ref(),
            cli.strict,
        )?;

        log_startup_snapshot(&startup_orchestrator.snapshot());

        Ok::<_, anyhow::Error>((
            db_ctx.boot_state,
            boot_artifacts,
            shutdown_coordinator,
            shutdown_rx,
        ))
    })
    .await;

    // Handle boot timeout
    let (boot_state, boot_artifacts, shutdown_coordinator, shutdown_rx) = match boot_result {
        Ok(Ok(artifacts)) => {
            let boot_duration = boot_start.elapsed();
            info!(
                target: "boot",
                duration_ms = boot_duration.as_millis() as u64,
                duration_secs = format!("{:.1}", boot_duration.as_secs_f64()),
                "╔═══════════════════════════════════════════════════════════════╗"
            );
            info!(target: "boot", "║             BOOT COMPLETE - adapterOS Ready                   ║");
            info!(
                target: "boot",
                duration_secs = format!("{:.1}s", boot_duration.as_secs_f64()),
                "╚═══════════════════════════════════════════════════════════════╝"
            );
            artifacts
        }
        Ok(Err(e)) => {
            error!(error = %e, "Boot sequence failed with error");
            return Err(e);
        }
        Err(_) => {
            let current_state = boot_state_for_timeout.current_state();
            error!(
                timeout_secs = %boot_timeout.as_secs(),
                boot_state = ?current_state,
                "Boot sequence exceeded timeout - server failed to initialize in time"
            );
            eprintln!(
                "FATAL: Boot timeout after {} seconds. Boot was stuck in state: {:?}",
                boot_timeout.as_secs(),
                current_state
            );
            std::process::exit(10);
        }
    };

    // =========================================================================
    // Server Binding & Serving
    // =========================================================================
    let bind_config = &boot_artifacts.bind_config;
    let mode = if bind_config.production_mode {
        let socket_path = bind_config.uds_socket.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Egress policy violation: production_mode requires uds_socket configuration"
            )
        })?;
        BindMode::uds(socket_path)
    } else {
        let bind_ip = bind_config.bind.parse::<IpAddr>().unwrap_or_else(|_| {
            warn!(bind = %bind_config.bind, "Invalid server.bind; falling back to 127.0.0.1");
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        });
        let addr = SocketAddr::from((bind_ip, bind_config.port));
        BindMode::tcp(addr)
    };

    let server_config = ServerBindConfig {
        boot_state: boot_state.clone(),
        shutdown_coordinator,
        drain_timeout: boot_artifacts.bind_config.drain_timeout,
        in_flight_requests: boot_artifacts.in_flight_requests,
        shutdown_rx,
    };

    boot_state.start_phase("bind");
    match bind_and_serve(mode, boot_artifacts.app, server_config).await {
        Ok(()) => boot_state.finish_phase_ok("bind"),
        Err(e) => {
            boot_state.finish_phase_err("bind", failure_codes::BIND_FAILED, Some(e.to_string()));
            let exit_code = bind_error_exit_code(&e);
            if exit_code == adapteros_boot::EXIT_CONFIG_ERROR {
                error!(error = %e, "Bind failed with configuration error");
                eprintln!("{}", e);
                std::process::exit(exit_code);
            }
            return Err(e.into());
        }
    }

    // Final MLX cleanup after all other components
    #[cfg(feature = "multi-backend")]
    {
        adapteros_lora_worker::mlx_runtime_shutdown();
        tracing::info!("MLX runtime shut down");
    }

    Ok(())
}
