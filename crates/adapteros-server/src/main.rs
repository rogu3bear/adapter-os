//! AdapterOS Control Plane Server
//!
//! This is the main entry point for the control plane server. Boot logic is
//! modularized in the `boot` module for testability.

mod assets;

use adapteros_server::boot::{
    bind_and_serve, build_app_state, finalize_boot, initialize_config, initialize_database,
    initialize_executor, initialize_federation, initialize_metrics, initialize_security,
    log_effective_config, run_preflight_checks, write_boot_report, BindMode, ServerBindConfig,
};
use adapteros_server::boot::api_config::{build_api_config, spawn_sighup_handler};
use adapteros_server::boot::background_tasks::spawn_all_background_tasks;
use adapteros_server::boot::migrations::run_migrations;
use adapteros_server::cli::Cli;
use anyhow::Result;
use clap::Parser;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tracing::{error, info, warn};

mod alerting;
mod openapi;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI first (before logging, so we know config path)
    let cli = Cli::parse();

    // =========================================================================
    // Phase 1: Configuration
    // =========================================================================
    let config_ctx = initialize_config(&cli).await?;

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

    // Wrap the entire boot sequence in a timeout
    let boot_result = tokio::time::timeout(boot_timeout, async {
        // =====================================================================
        // Phase 2: Security Initialization
        // =====================================================================
        let security_ctx = initialize_security(config_ctx.server_config.clone(), &cli).await?;

        // Log effective configuration
        log_effective_config(&config_ctx.server_config)?;

        // =====================================================================
        // Phase 3: Deterministic Executor
        // =====================================================================
        let executor_ctx = initialize_executor(config_ctx.server_config.clone(), &cli).await?;

        // =====================================================================
        // Phase 4: Security Preflight Checks
        // =====================================================================
        run_preflight_checks(config_ctx.server_config.clone(), &cli).await?;

        // =====================================================================
        // Phase 5: Database Connection
        // =====================================================================
        let db_ctx = initialize_database(
            config_ctx.server_config.clone(),
            config_ctx.boot_state.clone(),
            &cli,
        )
        .await?;

        // =====================================================================
        // Phase 6: Database Migrations
        // =====================================================================
        let _migrate_only = run_migrations(
            &db_ctx.db,
            config_ctx.server_config.clone(),
            &cli,
            &db_ctx.boot_state,
        )
        .await?;

        // =====================================================================
        // Phases 7-8: Policy & Backend (already extracted to separate crates)
        // =====================================================================

        // =====================================================================
        // Phase 9a: API Configuration
        // =====================================================================
        let api_config = build_api_config(config_ctx.server_config.clone())?;

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
            let cfg = config_ctx.server_config.read()
                .map_err(|e| anyhow::anyhow!("Config lock poisoned: {}", e))?;
            cfg.server.production_mode
        };
        let metrics_ctx = initialize_metrics(
            config_ctx.server_config.clone(),
            &mut shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            production_mode,
        )
        .await?;

        // =====================================================================
        // Phase 10a: Application State
        // =====================================================================
        let (state, shutdown_coordinator) = build_app_state(
            db_ctx.db.clone(),
            api_config,
            config_ctx.server_config.clone(),
            federation_ctx.federation_daemon,
            metrics_ctx.metrics_exporter,
            metrics_ctx.uma_monitor,
            metrics_ctx.jwt_secret,
            security_ctx.worker_keypair.clone(),
            shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            &db_ctx.boot_state,
            db_ctx.runtime_mode.clone(),
            db_ctx.tick_ledger.clone(),
            executor_ctx.manifest_hash.clone(),
            cli.strict,
        )
        .await?;

        // =====================================================================
        // Phase 10b: Background Tasks
        // =====================================================================
        let shutdown_coordinator = spawn_all_background_tasks(
            &state,
            &db_ctx.db,
            shutdown_coordinator,
            executor_ctx.background_tasks.clone(),
            &db_ctx.boot_state,
            cli.strict,
            state.metrics_registry.clone(),
            config_ctx.server_config.clone(),
        )
        .await?;

        // =====================================================================
        // Phases 11-12: Finalization
        // =====================================================================
        let ui_routes = assets::routes();
        let boot_artifacts = finalize_boot(
            state,
            config_ctx.server_config.clone(),
            ui_routes,
            &db_ctx.boot_state,
        )
        .await?;

        // Write boot report
        write_boot_report(
            config_ctx.server_config.clone(),
            &boot_artifacts.bind_config,
            security_ctx.worker_keypair.as_ref(),
            cli.strict,
        )?;

        Ok::<_, anyhow::Error>((
            db_ctx.boot_state,
            boot_artifacts,
            shutdown_coordinator,
        ))
    })
    .await;

    // Handle boot timeout
    let (boot_state, boot_artifacts, shutdown_coordinator) = match boot_result {
        Ok(Ok(artifacts)) => {
            let boot_duration = boot_start.elapsed();
            info!(
                target: "boot",
                duration_ms = boot_duration.as_millis() as u64,
                duration_secs = format!("{:.1}", boot_duration.as_secs_f64()),
                "╔═══════════════════════════════════════════════════════════════╗"
            );
            info!(target: "boot", "║             BOOT COMPLETE - AdapterOS Ready                   ║");
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
        boot_state,
        shutdown_coordinator,
        drain_timeout: boot_artifacts.bind_config.drain_timeout,
        in_flight_requests: boot_artifacts.in_flight_requests,
    };

    bind_and_serve(mode, boot_artifacts.app, server_config).await?;

    // Final MLX cleanup after all other components
    #[cfg(feature = "multi-backend")]
    {
        adapteros_lora_mlx_ffi::mlx_runtime_shutdown();
        tracing::info!("MLX runtime shut down");
    }

    Ok(())
}
