//! Storage mode management commands
//!
//! Provides CLI commands for managing the storage backend mode and migration:
//! - `aosctl storage mode` - Display current storage mode
//! - `aosctl storage set-mode <mode>` - Set storage mode (sql_only, dual_write, kv_primary, kv_only)
//! - `aosctl storage migrate` - Migrate data from SQL to KV backend
//! - `aosctl storage verify` - Verify consistency between SQL and KV backends

use crate::output::OutputWriter;
use adapteros_core::Result;
use adapteros_db::kv_migration::{MigrationCheckpoint, MigrationDomain, MigrationOptions};
use adapteros_db::kv_metrics::{global_kv_metrics, KvMetricsSnapshot};
use adapteros_db::{Db, StorageMode};
use anyhow::Context;
use clap::Subcommand;
use serde::Serialize;
use std::path::PathBuf;
use std::{fs, io};
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

        /// Tenant filter (migrate a single tenant only)
        #[arg(long)]
        tenant: Option<String>,

        /// Batch size for migrations
        #[arg(long, default_value_t = 100)]
        batch_size: usize,

        /// Resume from checkpoint (requires --checkpoint-path)
        #[arg(long)]
        resume: bool,

        /// Path to checkpoint file (JSON)
        #[arg(long, default_value = "./var/aos-migrate.checkpoint.json")]
        checkpoint_path: PathBuf,

        /// Comma-separated domains (adapters,tenants,stacks,plans,auth_sessions,runtime_sessions,rag_artifacts)
        #[arg(long)]
        domains: Option<String>,
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

        /// Repair detected drift by re-migrating domains SQL -> KV
        #[arg(long)]
        repair: bool,

        /// Comma-separated domains to verify/repair (default: all supported)
        #[arg(long)]
        domains: Option<String>,

        /// Exit with non-zero if drift is detected
        #[arg(long)]
        fail_on_drift: bool,
    },

    /// Validate and optionally repair consistency for a tenant
    ///
    /// Runs SQL↔KV parity checks for all adapters in a tenant. With --repair, fixes drift by
    /// syncing SQL → KV using ensure_consistency().
    #[command(after_help = r#"Examples:
  aosctl storage validate-consistency --tenant default --repair
  aosctl storage validate-consistency --tenant default --db-path ./var/aos-cp.sqlite3 --kv-path ./var/aos-kv.redb"#)]
    ValidateConsistency {
        /// Tenant ID to validate
        #[arg(long)]
        tenant: String,

        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// KV database path
        #[arg(long, default_value = "./var/aos-kv.redb")]
        kv_path: PathBuf,

        /// Repair drift by syncing SQL → KV
        #[arg(long, default_value_t = false)]
        repair: bool,
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
struct DomainReport {
    domain: String,
    total: usize,
    migrated: usize,
    skipped: usize,
    failed: usize,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct MigrationReport {
    dry_run: bool,
    tenant: Option<String>,
    batch_size: usize,
    checkpoint_path: String,
    domains: Vec<String>,
    results: Vec<DomainReport>,
    degraded_reason: Option<String>,
    kv_metrics: KvMetricsSnapshot,
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

#[derive(Serialize)]
struct ConsistencyReport {
    tenant: String,
    consistent: usize,
    inconsistent: usize,
    errors: usize,
    repaired: bool,
}

/// Get storage command name for telemetry
fn get_storage_command_name(cmd: &StorageCommand) -> String {
    match cmd {
        StorageCommand::Mode { .. } => "storage_mode".to_string(),
        StorageCommand::SetMode { .. } => "storage_set_mode".to_string(),
        StorageCommand::Migrate { .. } => "storage_migrate".to_string(),
        StorageCommand::Verify { .. } => "storage_verify".to_string(),
        StorageCommand::ValidateConsistency { .. } => "storage_validate_consistency".to_string(),
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
            tenant,
            batch_size,
            resume,
            checkpoint_path,
            domains,
        } => migrate_data(
            db_path,
            kv_path,
            dry_run,
            verify,
            force,
            tenant,
            batch_size,
            resume,
            checkpoint_path,
            domains,
            output,
        )
        .await,
        StorageCommand::Verify {
            db_path,
            kv_path,
            adapters_only,
            tenants_only,
            stacks_only,
            repair,
            domains,
            fail_on_drift,
        } => {
            verify_consistency(
                db_path,
                kv_path,
                adapters_only,
                tenants_only,
                stacks_only,
                repair,
                domains,
                fail_on_drift,
                output,
            )
            .await
        }
        StorageCommand::ValidateConsistency {
            tenant,
            db_path,
            kv_path,
            repair,
        } => validate_consistency(db_path, kv_path, tenant, repair, output).await,
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
    db.set_storage_mode(mode)
        .context("Failed to set storage mode")?;

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
    tenant: Option<String>,
    batch_size: usize,
    resume: bool,
    checkpoint_path: PathBuf,
    domains: Option<String>,
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
    } else if !force && !dry_run && !resume {
        output.warning("KV backend already exists");
        output.warning("Use --force to overwrite existing KV data");
        return Err(anyhow::anyhow!("KV backend already exists").into());
    }

    let domains = parse_domains(domains.as_deref())?;
    output.info(&format!(
        "Domains: {}",
        domains
            .iter()
            .map(|d| d.label())
            .collect::<Vec<_>>()
            .join(",")
    ));
    if let Some(tid) = &tenant {
        output.info(&format!("Tenant filter: {}", tid));
    }

    let mut options = MigrationOptions {
        batch_size,
        dry_run,
        tenant_filter: tenant.clone(),
        checkpoint: None,
    };

    if resume {
        if let Some(cp) = load_checkpoint(&checkpoint_path)? {
            output.info(&format!(
                "Loaded checkpoint from {}",
                checkpoint_path.display()
            ));
            options.checkpoint = Some(cp);
        } else {
            output.info("No checkpoint found; starting fresh");
        }
    }

    let (results, checkpoint) = db
        .migrate_domains(&domains, &options)
        .await
        .context("Migration failed")?;

    // Write checkpoint only when not in dry-run
    if !dry_run {
        save_checkpoint(&checkpoint_path, &checkpoint)?;
        output.info(&format!(
            "Checkpoint saved to {}",
            checkpoint_path.display()
        ));
    } else {
        output.info("Dry-run: checkpoint not written");
    }

    if verify && !dry_run {
        output.info("");
        output.info("Verifying migration (diff_all_supported)...");
        // Reuse existing verifier for supported domains
        let _ = verify_consistency(
            db_path.clone(),
            kv_path.clone(),
            false,
            false,
            false,
            false,
            None,
            false,
            output,
        )
        .await?;
    }

    let kv_snapshot = global_kv_metrics().snapshot();
    let degraded = db.degradation_reason();

    if output.is_json() {
        let report = MigrationReport {
            dry_run,
            tenant,
            batch_size,
            checkpoint_path: checkpoint_path.display().to_string(),
            domains: domains.iter().map(|d| d.label().to_string()).collect(),
            results: results
                .iter()
                .map(|(domain, stats)| DomainReport {
                    domain: domain.label().to_string(),
                    total: stats.total,
                    migrated: stats.migrated,
                    skipped: stats.skipped,
                    failed: stats.failed,
                    errors: stats.errors.clone(),
                })
                .collect(),
            degraded_reason: degraded.clone(),
            kv_metrics: kv_snapshot.clone(),
        };
        output.json(&report)?;
    } else {
        if let Some(reason) = degraded {
            output.warning(&format!("Degraded: {}", reason));
        }
        output.info(&format!(
            "KV fallback ops: {} (drift detections: {}, degraded events: {})",
            kv_snapshot.fallback_operations_total,
            kv_snapshot.drift_detections_total,
            kv_snapshot.degraded_events_total
        ));
        for (domain, stats) in &results {
            output.info(&format!(
                "[{}] migrated={}, skipped={}, failed={}, total={}",
                domain.label(),
                stats.migrated,
                stats.skipped,
                stats.failed,
                stats.total
            ));
            if !stats.errors.is_empty() {
                output.warning(&format!(
                    "[{}] errors: {}",
                    domain.label(),
                    stats.errors.join("; ")
                ));
            }
        }
        output.success("Migration complete");
    }

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
    repair: bool,
    domains: Option<String>,
    fail_on_drift: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info("Storage Consistency Verification");
    output.info("================================");

    let db_url = get_db_url(db_path.as_ref());
    let mut db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    let domains = if adapters_only {
        vec![MigrationDomain::Adapters]
    } else if tenants_only {
        vec![MigrationDomain::Tenants]
    } else if stacks_only {
        vec![MigrationDomain::Stacks]
    } else {
        parse_domains(domains.as_deref())?
    };

    // Ensure KV backend is attached
    if !db.has_kv_backend() {
        output.info(&format!("Attaching KV backend from: {}", kv_path.display()));
        db.init_kv_backend(&kv_path)
            .context("Failed to attach KV backend")?;
    }

    if repair {
        output.info("Repair requested: re-migrating selected domains from SQL to KV");
        let opts = MigrationOptions {
            batch_size: 200,
            dry_run: false,
            tenant_filter: None,
            checkpoint: None,
        };
        let (results, _) = db
            .migrate_domains(&domains, &opts)
            .await
            .context("Repair migration failed")?;
        for (domain, stats) in &results {
            output.info(&format!(
                "[repair:{}] migrated={}, failed={}, skipped={}",
                domain.label(),
                stats.migrated,
                stats.failed,
                stats.skipped
            ));
        }
    }

    let mode = db.storage_mode();
    output.info(&format!("Current mode: {}", mode));
    output.info("");

    let mut issues = Vec::new();
    if adapters_only && !tenants_only && !stacks_only {
        issues.extend(db.diff_adapters().await?);
    } else if tenants_only && !adapters_only && !stacks_only {
        issues.extend(db.diff_tenants().await?);
    } else if stacks_only && !adapters_only && !tenants_only {
        issues.extend(db.diff_stacks().await?);
    } else {
        // Domain-selectable sweep
        for domain in &domains {
            match domain {
                MigrationDomain::Adapters => issues.extend(db.diff_adapters().await?),
                MigrationDomain::Tenants => issues.extend(db.diff_tenants().await?),
                MigrationDomain::Stacks => issues.extend(db.diff_stacks().await?),
                MigrationDomain::Plans => issues.extend(db.diff_plans().await?),
                MigrationDomain::AuthSessions => issues.extend(db.diff_auth_sessions().await?),
                MigrationDomain::RuntimeSessions => issues.extend(db.diff_runtime_sessions().await?),
                MigrationDomain::RagArtifacts => {
                    issues.extend(db.diff_documents().await?);
                    issues.extend(db.diff_collections().await?);
                    issues.extend(db.diff_collection_links().await?);
                }
                MigrationDomain::PolicyAudit => issues.extend(db.diff_policy_audit().await?),
                MigrationDomain::TrainingJobs => issues.extend(db.diff_training_jobs().await?),
                MigrationDomain::ChatSessions => issues.extend(db.diff_chat_sessions().await?),
            }
        }
    }

    if !issues.is_empty() {
        global_kv_metrics().record_drift_detected();
        if fail_on_drift {
            output.error("Drift detected");
            // Still print issues below
        }
    }

    if output.is_json() {
        output.json(&issues)?;
    } else {
        if issues.is_empty() {
            output.success("No discrepancies detected between SQL and KV");
        } else {
            output.info("Discrepancies detected:");
            for issue in &issues {
                output.info(&format!(
                    "- [{domain}] {id} :: {field} sql={sql} kv={kv}",
                    domain = issue.domain,
                    id = issue.id,
                    field = issue.field,
                    sql = issue.sql_value,
                    kv = issue.kv_value
                ));
            }
            if fail_on_drift {
                return Err(anyhow::anyhow!("Drift detected"));
            }
        }
    }

    output.success("Verification complete");

    Ok(())
}

// ============================================================
// Validate Consistency Implementation
// ============================================================

async fn validate_consistency(
    db_path: Option<PathBuf>,
    kv_path: PathBuf,
    tenant: String,
    repair: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info("Tenant Consistency Validation");
    output.info("============================");

    let db_url = get_db_url(db_path.as_ref());
    let mut db = Db::connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    if !db.has_kv_backend() {
        output.info(&format!(
            "KV backend not attached; initializing at {}",
            kv_path.display()
        ));
        db.init_kv_backend(&kv_path)
            .context("Failed to initialize KV backend")?;
    }

    let (consistent, inconsistent, errors) = db
        .validate_tenant_consistency(&tenant, repair)
        .await
        .context("Consistency validation failed")?;

    if output.is_json() {
        let report = ConsistencyReport {
            tenant: tenant.clone(),
            consistent,
            inconsistent,
            errors,
            repaired: repair,
        };
        output.json(&report)?;
    } else {
        output.kv("Tenant", &tenant);
        output.kv("Repair Enabled", if repair { "yes" } else { "no" });
        output.kv("Consistent", &consistent.to_string());
        output.kv("Inconsistent", &inconsistent.to_string());
        output.kv("Errors", &errors.to_string());
    }

    output.success("Consistency validation complete");
    Ok(())
}

// ============================================================
// Helper Functions
// ============================================================

fn parse_domains(domains: Option<&str>) -> Result<Vec<MigrationDomain>> {
    if let Some(raw) = domains {
        let mut parsed = Vec::new();
        for part in raw.split(',') {
            let dom = match part.trim().to_lowercase().as_str() {
                "adapters" => MigrationDomain::Adapters,
                "tenants" => MigrationDomain::Tenants,
                "stacks" => MigrationDomain::Stacks,
                "plans" => MigrationDomain::Plans,
                "auth_sessions" => MigrationDomain::AuthSessions,
                "runtime_sessions" => MigrationDomain::RuntimeSessions,
                "rag_artifacts" | "rag" | "documents" => MigrationDomain::RagArtifacts,
                "policy_audit" => MigrationDomain::PolicyAudit,
                "training_jobs" => MigrationDomain::TrainingJobs,
                "chat_sessions" => MigrationDomain::ChatSessions,
                other => {
                    return Err(adapteros_core::AosError::Config(format!(
                        "Unknown domain '{}'. Valid: adapters, tenants, stacks, plans, auth_sessions, runtime_sessions, rag_artifacts, policy_audit, training_jobs, chat_sessions",
                        other,
                    ))
                    .into())
                }
            };
            parsed.push(dom);
        }
        Ok(parsed)
    } else {
        Ok(default_domains())
    }
}

fn default_domains() -> Vec<MigrationDomain> {
    vec![
        MigrationDomain::Adapters,
        MigrationDomain::Tenants,
        MigrationDomain::Stacks,
        MigrationDomain::Plans,
        MigrationDomain::AuthSessions,
        MigrationDomain::RuntimeSessions,
        MigrationDomain::RagArtifacts,
        MigrationDomain::PolicyAudit,
        MigrationDomain::TrainingJobs,
        MigrationDomain::ChatSessions,
    ]
}

fn load_checkpoint(path: &PathBuf) -> Result<Option<MigrationCheckpoint>> {
    match fs::read(path) {
        Ok(bytes) => {
            let cp: MigrationCheckpoint = serde_json::from_slice(&bytes)
                .context("Failed to parse checkpoint file")?;
            Ok(Some(cp))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(adapteros_core::AosError::Io(format!(
            "Failed to read checkpoint: {}",
            e
        ))
        .into()),
    }
}

fn save_checkpoint(path: &PathBuf, checkpoint: &MigrationCheckpoint) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create dir: {}", e)))?;
    }
    let bytes = serde_json::to_vec_pretty(checkpoint)
        .map_err(|e| adapteros_core::AosError::Serialization(e))?;
    fs::write(path, bytes).map_err(|e| {
        adapteros_core::AosError::Io(format!("Failed to write checkpoint: {}", e))
    })?;
    Ok(())
}

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
                tenant: None,
                batch_size: 100,
                resume: false,
                checkpoint_path: PathBuf::from("./var/aos-migrate.checkpoint.json"),
                domains: None,
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
                repair: false,
                domains: None,
                fail_on_drift: false,
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

    #[test]
    fn test_default_domains_includes_rag() {
        let labels: Vec<_> = default_domains().into_iter().map(|d| d.label()).collect();
        assert!(labels.contains(&"rag_artifacts"));
    }

    #[test]
    fn test_parse_domains_custom() {
        let parsed =
            parse_domains(Some("adapters,rag_artifacts,runtime_sessions,policy_audit,training_jobs,chat_sessions")).unwrap();
        let labels: Vec<_> = parsed.into_iter().map(|d| d.label()).collect();
        assert_eq!(
            labels,
            vec![
                "adapters",
                "rag_artifacts",
                "runtime_sessions",
                "policy_audit",
                "training_jobs",
                "chat_sessions"
            ]
        );
    }

    #[test]
    fn test_parse_domains_unknown() {
        let err = parse_domains(Some("unknown_domain")).unwrap_err();
        assert!(err.to_string().contains("Unknown domain"));
    }
}
