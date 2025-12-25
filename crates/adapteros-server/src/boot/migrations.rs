//! Database migrations and seeding during boot sequence.
//!
//! This module handles Phase 6 of the boot sequence, which includes:
//! - Running database schema migrations with Ed25519 signature verification
//! - Crash recovery from orphaned adapters and stale state
//! - Seeding development data
//! - Seeding base models from cache
//! - Handling the --migrate-only CLI flag for early exit

use adapteros_api_types::FailureCode;
use adapteros_db::Db;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::config::Config;
use anyhow::Result;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

use crate::cli::Cli;
use crate::model_seeding::seed_models_from_cache_if_empty;

/// Runs database migrations and optional seeding.
///
/// This function executes Phase 6 of the boot sequence:
/// 1. Checks if SQL backend is enabled based on storage mode
/// 2. Runs schema migrations with signature verification
/// 3. Performs crash recovery to clean up orphaned state
/// 4. Seeds development data (if applicable)
/// 5. Seeds base models from cache if the database is empty
/// 6. Handles --migrate-only flag for early exit
///
/// # Arguments
///
/// * `db` - Database connection
/// * `config` - Server configuration (wrapped in Arc<RwLock>)
/// * `cli` - Command-line arguments
/// * `boot_state` - Boot state manager for tracking progress
///
/// # Returns
///
/// * `Ok(true)` - Migrations complete and --migrate-only was specified, caller should exit
/// * `Ok(false)` - Migrations complete, continue with normal boot sequence
/// * `Err(_)` - Migration or crash recovery failed
pub async fn run_migrations(
    db: &Db,
    _config: Arc<RwLock<Config>>,
    cli: &Cli,
    boot_state: &BootStateManager,
) -> Result<bool> {
    let sql_enabled = db.storage_mode().write_to_sql() || db.storage_mode().read_from_sql();

    info!(target: "boot", phase = 6, name = "migrations", "═══ BOOT PHASE 6/12: Database Migrations ═══");

    // Transition boot state: DbConnecting → Migrating
    boot_state.migrating().await;

    if sql_enabled {
        // Run migrations with Ed25519 signature verification
        info!("Running database migrations...");
        if let Err(e) = db.migrate().await {
            error!(
                target: "boot",
                code = %FailureCode::MigrationInvalid.as_str(),
                request_id = "-",
                tenant_id = "system",
                error = %e,
                "Database migrations failed"
            );
            return Err(e.into());
        }

        // Recover from any previous crash (orphaned adapters, stale state)
        info!("Running crash recovery checks...");
        db.recover_from_crash()
            .await
            .map_err(|e| anyhow::anyhow!("Crash recovery failed: {}", e))?;

        // Seed development data
        if let Err(e) = db.seed_dev_data().await {
            warn!(error = %e, "Failed to seed development data");
        }

        if let Err(e) = seed_models_from_cache_if_empty(&db).await {
            warn!(error = %e, "Failed to seed cached base models");
        }
    } else {
        info!("SQL backend disabled; skipping migrations, crash recovery, and SQL seed steps");
    }

    // Transition boot state: Migrating → Seeding (seeding complete)
    boot_state.seeding().await;

    // Transition boot state: Seeding → LoadingPolicies (for Phase 7)
    boot_state.load_policies().await;

    if cli.migrate_only {
        info!("Migrations complete, exiting");
        std::process::exit(0);
    }

    Ok(false)
}
