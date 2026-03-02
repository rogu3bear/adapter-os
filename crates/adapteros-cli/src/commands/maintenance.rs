//! Maintenance commands for aosctl.
//!
//! Provides:
//! - `aosctl maintenance gc-bundles` – prune telemetry bundles on disk
//! - `aosctl maintenance gc-adapters` – garbage collect archived adapter .aos files
//!
//! Semantics follow `scripts/gc_bundles.sh`:
//! - Keep last K bundles per CPID
//! - Preserve bundles referenced by open incidents
//! - Preserve promotion bundles
//!
//! [source: scripts/gc_bundles.sh L1-L140]

use crate::output::OutputWriter;
use adapteros_db::sqlx;
use adapteros_db::Db;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Top-level maintenance command.
#[derive(Debug, Args, Clone)]
pub struct MaintenanceCommand {
    #[command(subcommand)]
    pub subcommand: MaintenanceSubcommand,
}

/// Maintenance subcommands.
#[derive(Debug, Subcommand, Clone)]
pub enum MaintenanceSubcommand {
    /// Garbage-collect telemetry bundles
    GcBundles(GcBundlesArgs),
    /// Garbage-collect archived adapter .aos files
    GcAdapters(GcAdaptersArgs),
}

/// Arguments for `aosctl maintenance gc-bundles`.
#[derive(Debug, Args, Clone)]
pub struct GcBundlesArgs {
    /// Bundles directory path (bundle_id.ndjson)
    #[arg(long, default_value = "/srv/aos/bundles")]
    pub bundles_path: PathBuf,

    /// Control-plane database path
    #[arg(long, default_value = "var/aos-cp.sqlite3")]
    pub db_path: PathBuf,

    /// Keep last N bundles per CPID
    #[arg(long, default_value_t = 12)]
    pub keep_count: usize,

    /// Dry run – report actions without deleting files
    #[arg(long)]
    pub dry_run: bool,
}

/// JSON summary for gc-bundles.
#[derive(Debug, Serialize)]
pub struct GcBundlesSummary {
    bundles_path: String,
    db_path: String,
    keep_count: usize,
    dry_run: bool,
    deleted: usize,
    kept: usize,
}

/// Arguments for `aosctl maintenance gc-adapters`.
#[derive(Debug, Args, Clone)]
pub struct GcAdaptersArgs {
    /// Control-plane database path
    #[arg(long, default_value = "var/aos-cp.sqlite3")]
    pub db_path: PathBuf,

    /// Adapters directory path
    #[arg(long, default_value = "var/adapters")]
    pub adapters_path: PathBuf,

    /// Minimum days since archival before GC
    #[arg(long, default_value_t = 30)]
    pub min_age_days: u32,

    /// Maximum adapters to process per run
    #[arg(long, default_value_t = 100)]
    pub batch_size: i64,

    /// Dry run – report actions without deleting files
    #[arg(long)]
    pub dry_run: bool,

    /// Process specific tenant only
    #[arg(long)]
    pub tenant_id: Option<String>,
}

/// JSON summary for gc-adapters.
#[derive(Debug, Serialize)]
pub struct GcAdaptersSummary {
    adapters_path: String,
    db_path: String,
    min_age_days: u32,
    dry_run: bool,
    adapters_processed: usize,
    files_deleted: usize,
    bytes_freed: u64,
    errors: Vec<String>,
}

/// Dispatch maintenance command.
pub async fn run(cmd: MaintenanceCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        MaintenanceSubcommand::GcBundles(args) => gc_bundles(args, output).await,
        MaintenanceSubcommand::GcAdapters(args) => gc_adapters(args, output).await,
    }
}

async fn gc_bundles(args: GcBundlesArgs, output: &OutputWriter) -> Result<()> {
    let bundles_path = &args.bundles_path;
    let db_path = &args.db_path;

    if !bundles_path.is_dir() {
        anyhow::bail!("Bundles directory not found: {}", bundles_path.display());
    }

    // Connect to database explicitly (do not rely on DATABASE_URL)
    let db_path_str = db_path.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Database path contains invalid UTF-8: {}",
            db_path.display()
        )
    })?;
    let db = Db::connect(db_path_str)
        .await
        .with_context(|| format!("connecting to database {}", db_path.display()))?;
    let pool = db.pool();

    // Incident-referenced bundles
    let incident_bundles: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT bundle_id FROM incidents WHERE status != 'closed'",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    // Promotion bundles
    let promotion_bundles: HashSet<String> = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT tb.bundle_id
        FROM telemetry_bundles tb
        JOIN cp_pointers cp ON tb.cpid = cp.name
        WHERE cp.promoted = 1
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    // CPIDs
    let cpids: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT cpid FROM telemetry_bundles ORDER BY cpid",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if cpids.is_empty() {
        let summary = GcBundlesSummary {
            bundles_path: bundles_path.display().to_string(),
            db_path: db_path.display().to_string(),
            keep_count: args.keep_count,
            dry_run: args.dry_run,
            deleted: 0,
            kept: 0,
        };
        if output.is_json() {
            output.json(&summary)?;
        } else {
            output.info("No CPIDs found in telemetry_bundles");
        }
        return Ok(());
    }

    let mut total_deleted = 0usize;
    let mut total_kept = 0usize;

    for cpid in cpids {
        // All bundles for CPID, newest first
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT bundle_id, created_at
            FROM telemetry_bundles
            WHERE cpid = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(&cpid)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if rows.is_empty() {
            continue;
        }

        if !output.is_json() && output.is_verbose() {
            output.section(format!("CPID: {}", cpid));
            output.kv("Total bundles", &rows.len().to_string());
        }

        let mut index = 0usize;
        for (bundle_id, created_at) in rows {
            index += 1;
            let mut should_delete = true;
            let mut reason = String::new();

            if index <= args.keep_count {
                should_delete = false;
                reason = format!("within keep window ({}/{})", index, args.keep_count);
            }

            if incident_bundles.contains(&bundle_id) {
                should_delete = false;
                reason = "referenced by open incident".to_string();
            }

            if promotion_bundles.contains(&bundle_id) {
                should_delete = false;
                reason = "promotion bundle".to_string();
            }

            let bundle_file = bundles_path.join(format!("{}.ndjson", bundle_id));

            if should_delete {
                if args.dry_run {
                    if !output.is_json() {
                        output.verbose(format!(
                            "[dry-run] would delete: {} ({})",
                            bundle_id, created_at
                        ));
                    }
                } else if bundle_file.exists() {
                    if let Err(e) = fs::remove_file(&bundle_file) {
                        output.warning(format!(
                            "Failed to delete bundle {}: {}",
                            bundle_file.display(),
                            e
                        ));
                    } else {
                        total_deleted += 1;
                    }
                }
            } else {
                total_kept += 1;
                if args.dry_run && output.is_verbose() && !output.is_json() {
                    output.verbose(format!(
                        "[dry-run] would keep: {} ({}) – {}",
                        bundle_id, created_at, reason
                    ));
                }
            }
        }
    }

    let summary = GcBundlesSummary {
        bundles_path: bundles_path.display().to_string(),
        db_path: db_path.display().to_string(),
        keep_count: args.keep_count,
        dry_run: args.dry_run,
        deleted: total_deleted,
        kept: total_kept,
    };

    if output.is_json() {
        output.json(&summary)?;
    } else {
        output.section("Garbage Collection Summary");
        if args.dry_run {
            output.info("Dry run complete (no files deleted)");
        } else {
            output.kv("Bundles deleted", &total_deleted.to_string());
            output.kv("Bundles kept", &total_kept.to_string());
        }
    }

    Ok(())
}

/// Garbage-collect archived adapter .aos files.
///
/// Finds adapters that have been archived for at least `min_age_days` days,
/// deletes their .aos files from disk, and marks them as purged in the database.
/// The database record is preserved for audit purposes.
async fn gc_adapters(args: GcAdaptersArgs, output: &OutputWriter) -> Result<()> {
    let adapters_path = &args.adapters_path;
    let db_path = &args.db_path;

    // Connect to database
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8"))?;
    let db = Db::connect(db_path_str)
        .await
        .with_context(|| format!("connecting to database {}", db_path.display()))?;

    // Find GC candidates
    let candidates = db
        .find_archived_adapters_for_gc(args.min_age_days, args.batch_size)
        .await
        .with_context(|| "finding archived adapters for GC")?;

    if candidates.is_empty() {
        let summary = GcAdaptersSummary {
            adapters_path: adapters_path.display().to_string(),
            db_path: db_path.display().to_string(),
            min_age_days: args.min_age_days,
            dry_run: args.dry_run,
            adapters_processed: 0,
            files_deleted: 0,
            bytes_freed: 0,
            errors: Vec::new(),
        };
        if output.is_json() {
            output.json(&summary)?;
        } else {
            output.info("No adapters eligible for garbage collection");
        }
        return Ok(());
    }

    let mut files_deleted = 0usize;
    let mut bytes_freed = 0u64;
    let mut errors: Vec<String> = Vec::new();
    let mut processed = 0usize;

    for adapter in &candidates {
        // Filter by tenant if specified
        if let Some(ref filter_tenant) = args.tenant_id {
            if &adapter.tenant_id != filter_tenant {
                continue;
            }
        }

        processed += 1;

        let adapter_id = match &adapter.adapter_id {
            Some(aid) => aid.clone(),
            None => {
                errors.push(format!("Adapter {} missing adapter_id", adapter.id));
                continue;
            }
        };
        let tenant_id = adapter.tenant_id.as_str();

        // Get file path from adapter record or construct from adapters_path
        let file_path = match &adapter.aos_file_path {
            Some(path) => PathBuf::from(path),
            None => continue, // Already purged or no file reference
        };

        if args.dry_run {
            // Dry run: report what would be deleted
            let file_size = if file_path.exists() {
                fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };

            if !output.is_json() {
                output.verbose(format!(
                    "[dry-run] would delete: {} ({} bytes, archived: {})",
                    file_path.display(),
                    file_size,
                    adapter.archived_at.as_deref().unwrap_or("unknown")
                ));
            }
            files_deleted += 1;
            bytes_freed += file_size;
        } else {
            // Actually delete the file
            if file_path.exists() {
                match fs::metadata(&file_path) {
                    Ok(meta) => {
                        let size = meta.len();
                        match fs::remove_file(&file_path) {
                            Ok(_) => {
                                // Mark as purged in database
                                if let Err(e) = db.mark_adapter_purged(tenant_id, &adapter_id).await
                                {
                                    errors.push(format!(
                                        "Failed to mark {} as purged: {}",
                                        adapter_id, e
                                    ));
                                    continue;
                                }
                                files_deleted += 1;
                                bytes_freed += size;

                                if !output.is_json() && output.is_verbose() {
                                    output.verbose(format!(
                                        "Deleted: {} ({} bytes)",
                                        file_path.display(),
                                        size
                                    ));
                                }
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Failed to delete {}: {}",
                                    file_path.display(),
                                    e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(format!("Failed to stat {}: {}", file_path.display(), e));
                    }
                }
            } else {
                // File doesn't exist on disk, but DB record exists
                // Mark as purged anyway to clean up the stale reference
                if let Err(e) = db.mark_adapter_purged(tenant_id, &adapter_id).await {
                    errors.push(format!(
                        "Failed to mark {} as purged (file missing): {}",
                        adapter_id, e
                    ));
                } else {
                    files_deleted += 1;
                    if !output.is_json() && output.is_verbose() {
                        output.verbose(format!(
                            "Marked as purged (file was missing): {}",
                            adapter_id
                        ));
                    }
                }
            }
        }
    }

    let summary = GcAdaptersSummary {
        adapters_path: adapters_path.display().to_string(),
        db_path: db_path.display().to_string(),
        min_age_days: args.min_age_days,
        dry_run: args.dry_run,
        adapters_processed: processed,
        files_deleted,
        bytes_freed,
        errors: errors.clone(),
    };

    if output.is_json() {
        output.json(&summary)?;
    } else {
        output.section("Adapter Garbage Collection Summary");
        if args.dry_run {
            output.info("Dry run complete (no files deleted)");
        }
        output.kv("Adapters processed", &processed.to_string());
        output.kv("Files deleted", &files_deleted.to_string());
        output.kv("Bytes freed", &format_bytes(bytes_freed));
        if !errors.is_empty() {
            output.warning(format!("{} errors occurred", errors.len()));
            for err in &errors {
                output.warning(err.clone());
            }
        }
    }

    Ok(())
}

/// Format bytes as human-readable string.
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} bytes", bytes)
    }
}
