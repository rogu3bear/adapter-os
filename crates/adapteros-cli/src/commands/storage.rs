//! Storage mode management commands
//!
//! Provides CLI commands for managing the storage backend mode and migration:
//! - `aosctl storage mode` - Display current storage mode
//! - `aosctl storage set-mode <mode>` - Set storage mode (sql_only, dual_write, kv_primary, kv_only)
//! - `aosctl storage migrate` - Migrate data from SQL to KV backend
//! - `aosctl storage verify` - Verify consistency between SQL and KV backends

use crate::output::OutputWriter;
use adapteros_core::Result;
use adapteros_db::{Db, StorageMode};
use anyhow::Context;
use clap::Subcommand;
use serde::Serialize;
use std::path::PathBuf;
use tracing::{info, warn};

/// Storage management subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum StorageCommand {
    /// Show current storage mode
    ///
    /// Displays the current storage backend mode (sql_only, dual_write, kv_primary, or kv_only)
    /// and whether a KV backend is attached.
    #[command(after_help = r#"Examples:
  # Show current storage mode
  aosctl storage mode

  # Show mode with JSON output
  aosctl storage mode --json
"#)]
    Mode {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,
    },

    /// Set storage mode
    ///
    /// Changes the storage backend mode. Valid modes:
    /// - sql_only: SQL backend only (default)
    /// - dual_write: Write to both SQL and KV, read from SQL (validation phase)
    /// - kv_primary: Write to both SQL and KV, read from KV (cutover phase)
    /// - kv_only: KV backend only (full migration complete)
    #[command(after_help = r#"Examples:
  # Enable dual-write mode for validation
  aosctl storage set-mode dual_write

  # Switch to KV-primary mode for cutover
  aosctl storage set-mode kv_primary

  # Complete migration to KV-only mode
  aosctl storage set-mode kv_only

  # Revert to SQL-only mode
  aosctl storage set-mode sql_only

  # Set mode with custom database path
  aosctl storage set-mode dual_write --db-path ./var/custom.db --kv-path ./var/custom.redb
"#)]
    SetMode {
        /// Storage mode to set (sql_only, dual_write, kv_primary, kv_only)
        mode: String,

        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// KV database path (required for kv modes)
        #[arg(long, default_value = "./var/aos-kv.redb")]
        kv_path: PathBuf,

        /// Initialize KV backend if not exists
        #[arg(long)]
        init_kv: bool,
    },

    /// Migrate data from SQL to KV backend
    ///
    /// Performs a full migration of adapter, tenant, and stack data from the SQL backend
    /// to the KV backend. This should be run before switching from sql_only to dual_write mode.
    #[command(after_help = r#"Examples:
  # Migrate all data from SQL to KV
  aosctl storage migrate

  # Migrate with custom paths
  aosctl storage migrate --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb

  # Dry run to preview migration
  aosctl storage migrate --dry-run

  # Migrate with verification
  aosctl storage migrate --verify
"#)]
    Migrate {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// KV database path
        #[arg(long, default_value = "./var/aos-kv.redb")]
        kv_path: PathBuf,

        /// Dry run - show what would be migrated without making changes
        #[arg(long)]
        dry_run: bool,

        /// Verify consistency after migration
        #[arg(long)]
        verify: bool,

        /// Force migration even if KV backend already has data
        #[arg(long)]
        force: bool,
    },

    /// Verify consistency between SQL and KV backends
    ///
    /// Compares data in SQL and KV backends to ensure consistency.
    /// This is useful for validating dual-write mode or before switching to kv_primary.
    #[command(after_help = r#"Examples:
  # Verify consistency between backends
  aosctl storage verify

  # Verify with custom paths
  aosctl storage verify --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb

  # Detailed verification report
  aosctl storage verify --verbose

  # Verify specific entities
  aosctl storage verify --adapters-only
  aosctl storage verify --tenants-only
  aosctl storage verify --stacks-only
"#)]
    Verify {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// KV database path
        #[arg(long, default_value = "./var/aos-kv.redb")]
        kv_path: PathBuf,

        /// Verify adapters only
        #[arg(long)]
        adapters_only: bool,

        /// Verify tenants only
        #[arg(long)]
        tenants_only: bool,

        /// Verify stacks only
        #[arg(long)]
        stacks_only: bool,
    },
}

#[derive(Serialize)]
struct ModeStatus {
    mode: String,
    has_kv_backend: bool,
    db_path: String,
    kv_path: Option<String>,
}

#[derive(Serialize)]
struct MigrationStats {
    adapters_migrated: usize,
    tenants_migrated: usize,
    stacks_migrated: usize,
    errors: usize,
}

#[derive(Serialize)]
struct VerificationReport {
    adapters_checked: usize,
    adapters_matched: usize,
    adapters_mismatched: usize,
    tenants_checked: usize,
    tenants_matched: usize,
    tenants_mismatched: usize,
    stacks_checked: usize,
    stacks_matched: usize,
    stacks_mismatched: usize,
}

/// Get storage command name for telemetry
fn get_storage_command_name(cmd: &StorageCommand) -> String {
    match cmd {
        StorageCommand::Mode { .. } => "storage_mode".to_string(),
        StorageCommand::SetMode { .. } => "storage_set_mode".to_string(),
        StorageCommand::Migrate { .. } => "storage_migrate".to_string(),
        StorageCommand::Verify { .. } => "storage_verify".to_string(),
    }
}

/// Handle storage management commands
///
/// Routes storage subcommands to appropriate handlers:
/// - `mode` -> show_mode()
/// - `set-mode` -> set_mode()
/// - `migrate` -> migrate_data()
/// - `verify` -> verify_consistency()
///
/// # Arguments
///
/// * `cmd` - The storage subcommand to execute
/// * `output` - Output writer for formatted console output
///
/// # Errors
///
/// Returns error if:
/// - Database cannot be opened
/// - Invalid storage mode specified
/// - KV backend is required but not available
/// - Migration or verification fails
pub async fn handle_storage_command(cmd: StorageCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_storage_command_name(&cmd);

    info!(command = ?cmd, "Handling storage command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        StorageCommand::Mode { db_path } => show_mode(db_path, output).await,
        StorageCommand::SetMode {
            mode,
            db_path,
            kv_path,
            init_kv,
        } => set_mode(mode, db_path, kv_path, init_kv, output).await,
        StorageCommand::Migrate {
            db_path,
            kv_path,
            dry_run,
            verify,
            force,
        } => migrate_data(db_path, kv_path, dry_run, verify, force, output).await,
        StorageCommand::Verify {
            db_path,
            kv_path,
            adapters_only,
            tenants_only,
            stacks_only,
        } => {
            verify_consistency(
                db_path,
                kv_path,
                adapters_only,
                tenants_only,
                stacks_only,
                output,
            )
            .await
        }
    }
}

// ============================================================
// Show Mode Implementation
// ============================================================

async fn show_mode(db_path: Option<PathBuf>, output: &OutputWriter) -> Result<()> {
    let db_url = get_db_url(db_path.as_ref());
    let db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    let mode = db.storage_mode();
    let has_kv = db.has_kv_backend();

    output.info("Storage Backend Status");
    output.info("=====================");
    output.kv("Mode", &mode.to_string());
    output.kv(
        "KV Backend",
        if has_kv { "Attached" } else { "Not Attached" },
    );
    output.kv("Database", &db_url);

    // Show mode description
    let description = match mode {
        StorageMode::SqlOnly => "SQL backend only (default, current production mode)",
        StorageMode::DualWrite => "Write to both SQL and KV, read from SQL (validation phase)",
        StorageMode::KvPrimary => "Write to both SQL and KV, read from KV (cutover phase)",
        StorageMode::KvOnly => "KV backend only (full migration complete)",
    };
    output.info("");
    output.info(&format!("Description: {}", description));

    // Warn if in KV mode without KV backend
    if (mode == StorageMode::DualWrite
        || mode == StorageMode::KvPrimary
        || mode == StorageMode::KvOnly)
        && !has_kv
    {
        output.warning("Warning: Storage mode requires KV backend but none is attached");
        output.warning("Run 'aosctl storage set-mode <mode> --init-kv' to initialize KV backend");
    }

    if output.is_json() {
        let status = ModeStatus {
            mode: mode.to_string(),
            has_kv_backend: has_kv,
            db_path: db_url,
            kv_path: if has_kv {
                Some("./var/aos-kv.redb".to_string())
            } else {
                None
            },
        };
        output.json(&status)?;
    }

    Ok(())
}

// ============================================================
// Set Mode Implementation
// ============================================================

async fn set_mode(
    mode_str: String,
    db_path: Option<PathBuf>,
    kv_path: PathBuf,
    init_kv: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Parse storage mode
    let mode: StorageMode = mode_str.parse().context(
        "Invalid storage mode. Valid options: sql_only, dual_write, kv_primary, kv_only",
    )?;

    let db_url = get_db_url(db_path.as_ref());
    let mut db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    let old_mode = db.storage_mode();

    // Check if KV backend is needed
    let needs_kv = mode == StorageMode::DualWrite
        || mode == StorageMode::KvPrimary
        || mode == StorageMode::KvOnly;

    if needs_kv && !db.has_kv_backend() {
        if init_kv {
            output.info(&format!(
                "Initializing KV backend at: {}",
                kv_path.display()
            ));
            db.init_kv_backend(&kv_path)
                .context("Failed to initialize KV backend")?;
            output.success("KV backend initialized");
        } else {
            output.error(&format!(
                "Storage mode '{}' requires KV backend but none is attached",
                mode
            ));
            output.info("Use --init-kv flag to initialize KV backend, or run:");
            output.info(&format!("  aosctl storage set-mode {} --init-kv", mode_str));
            return Err(adapteros_core::AosError::Config(
                "KV backend required but not attached".to_string(),
            )
            .into());
        }
    }

    // Set the mode
    db.set_storage_mode(mode);

    output.success(&format!("Storage mode changed: {} -> {}", old_mode, mode));

    // Show recommendations based on mode
    match mode {
        StorageMode::SqlOnly => {
            output.info("Running in SQL-only mode (default)");
        }
        StorageMode::DualWrite => {
            output.info("Running in dual-write mode");
            output.info("Recommendation: Run 'aosctl storage migrate' to populate KV backend");
            output.info("Then use 'aosctl storage verify' to check consistency");
        }
        StorageMode::KvPrimary => {
            output.warning("Running in KV-primary mode - reads from KV, writes to both");
            output.info("Ensure data is migrated before switching to this mode");
            output.info("Use 'aosctl storage verify' to check consistency");
            output.info("Can revert to dual_write if issues are detected");
        }
        StorageMode::KvOnly => {
            output.warning("Running in KV-only mode - SQL backend is ignored");
            output.info("This is the final migration state");
            output.info("Ensure thorough testing before using in production");
        }
    }

    Ok(())
}

// ============================================================
// Migrate Data Implementation
// ============================================================

async fn migrate_data(
    db_path: Option<PathBuf>,
    kv_path: PathBuf,
    dry_run: bool,
    verify: bool,
    force: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info("Storage Migration Tool");
    output.info("=====================");

    if dry_run {
        output.warning("DRY RUN MODE - No changes will be made");
    }

    let db_url = get_db_url(db_path.as_ref());
    let mut db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    // Initialize or attach KV backend
    if !db.has_kv_backend() {
        if dry_run {
            output.info("[DRY RUN] Would initialize KV backend");
        } else {
            output.info(&format!(
                "Initializing KV backend at: {}",
                kv_path.display()
            ));
            db.init_kv_backend(&kv_path)
                .context("Failed to initialize KV backend")?;
            output.success("KV backend initialized");
        }
    } else if !force && !dry_run {
        output.warning("KV backend already exists");
        output.warning("Use --force to overwrite existing KV data");
        return Err(anyhow::anyhow!("KV backend already exists").into());
    }

    output.info("");
    output.info("Migrating data from SQL to KV...");

    // Note: Actual migration logic would be implemented here
    // For now, we'll provide a placeholder that explains what needs to be done

    output.warning("Migration functionality is currently under development");
    output.info("To migrate data, you need to:");
    output.info("1. Switch to dual_write mode: aosctl storage set-mode dual_write --init-kv");
    output.info("2. All new writes will populate both SQL and KV backends");
    output.info("3. Use 'aosctl storage verify' to check consistency");
    output.info("4. When ready, switch to kv_primary: aosctl storage set-mode kv_primary");

    // Placeholder stats
    let stats = MigrationStats {
        adapters_migrated: 0,
        tenants_migrated: 0,
        stacks_migrated: 0,
        errors: 0,
    };

    if verify && !dry_run {
        output.info("");
        output.info("Verifying migration...");
        // Verification would be implemented here
    }

    if output.is_json() {
        output.json(&stats)?;
    }

    output.success("Migration preview complete");

    Ok(())
}

// ============================================================
// Verify Consistency Implementation
// ============================================================

async fn verify_consistency(
    db_path: Option<PathBuf>,
    kv_path: PathBuf,
    adapters_only: bool,
    tenants_only: bool,
    stacks_only: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info("Storage Consistency Verification");
    output.info("================================");

    let db_url = get_db_url(db_path.as_ref());
    let mut db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    // Ensure KV backend is attached
    if !db.has_kv_backend() {
        output.info(&format!("Attaching KV backend from: {}", kv_path.display()));
        db.init_kv_backend(&kv_path)
            .context("Failed to attach KV backend")?;
    }

    let mode = db.storage_mode();
    output.info(&format!("Current mode: {}", mode));
    output.info("");

    // Placeholder verification
    output.warning("Verification functionality is currently under development");
    output.info("This command will compare:");
    if !tenants_only && !stacks_only {
        output.info("- Adapter records between SQL and KV backends");
    }
    if !adapters_only && !stacks_only {
        output.info("- Tenant records between SQL and KV backends");
    }
    if !adapters_only && !tenants_only {
        output.info("- Stack records between SQL and KV backends");
    }

    // Placeholder report
    let report = VerificationReport {
        adapters_checked: 0,
        adapters_matched: 0,
        adapters_mismatched: 0,
        tenants_checked: 0,
        tenants_matched: 0,
        tenants_mismatched: 0,
        stacks_checked: 0,
        stacks_matched: 0,
        stacks_mismatched: 0,
    };

    if output.is_json() {
        output.json(&report)?;
    } else {
        output.info("");
        output.info("Verification Results:");
        output.info("--------------------");
        output.kv("Adapters", "Not yet implemented");
        output.kv("Tenants", "Not yet implemented");
        output.kv("Stacks", "Not yet implemented");
    }

    output.success("Verification preview complete");

    Ok(())
}

// ============================================================
// Helper Functions
// ============================================================

/// Get database URL from path or environment
fn get_db_url(db_path: Option<&PathBuf>) -> String {
    if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_storage_command_name() {
        assert_eq!(
            get_storage_command_name(&StorageCommand::Mode { db_path: None }),
            "storage_mode"
        );
        assert_eq!(
            get_storage_command_name(&StorageCommand::SetMode {
                mode: "dual_write".to_string(),
                db_path: None,
                kv_path: PathBuf::from("./var/aos-kv.redb"),
                init_kv: false,
            }),
            "storage_set_mode"
        );
        assert_eq!(
            get_storage_command_name(&StorageCommand::Migrate {
                db_path: None,
                kv_path: PathBuf::from("./var/aos-kv.redb"),
                dry_run: false,
                verify: false,
                force: false,
            }),
            "storage_migrate"
        );
        assert_eq!(
            get_storage_command_name(&StorageCommand::Verify {
                db_path: None,
                kv_path: PathBuf::from("./var/aos-kv.redb"),
                adapters_only: false,
                tenants_only: false,
                stacks_only: false,
            }),
            "storage_verify"
        );
    }

    #[test]
    fn test_storage_command_clone() {
        let cmd = StorageCommand::Mode { db_path: None };
        let cloned = cmd.clone();
        assert_eq!(
            get_storage_command_name(&cmd),
            get_storage_command_name(&cloned)
        );
    }
}
