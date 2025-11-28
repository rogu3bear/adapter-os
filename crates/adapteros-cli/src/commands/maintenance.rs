//! Maintenance commands for aosctl.
//!
//! Currently provides:
//! - `aosctl maintenance gc-bundles` – prune telemetry bundles on disk
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
use std::path::{Path, PathBuf};

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

/// Dispatch maintenance command.
pub async fn run(cmd: MaintenanceCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        MaintenanceSubcommand::GcBundles(args) => gc_bundles(args, output).await,
    }
}

async fn gc_bundles(args: GcBundlesArgs, output: &OutputWriter) -> Result<()> {
    let bundles_path = &args.bundles_path;
    let db_path = &args.db_path;

    if !bundles_path.is_dir() {
        anyhow::bail!("Bundles directory not found: {}", bundles_path.display());
    }

    // Connect to database explicitly (do not rely on DATABASE_URL)
    let db_path_str = db_path.to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {}", db_path.display()))?;
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
