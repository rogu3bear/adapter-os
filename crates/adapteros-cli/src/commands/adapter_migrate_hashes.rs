//! Batch hash migration command
//!
//! Migrates all adapters with missing `content_hash_b3` or `manifest_hash` fields.
//! This is a batch operation designed for migrating legacy adapters that were
//! registered before hash fields became mandatory for preflight.
//!
//! # Usage
//!
//! ```bash
//! # Migrate all adapters for a specific tenant
//! aosctl adapter migrate-hashes --tenant-id tenant-123
//!
//! # Migrate all adapters across all tenants
//! aosctl adapter migrate-hashes --all-tenants
//!
//! # Preview changes without updating (dry-run)
//! aosctl adapter migrate-hashes --tenant-id tenant-123 --dry-run
//!
//! # Control batch size for large migrations
//! aosctl adapter migrate-hashes --all-tenants --batch-size 50
//! ```
//!
//! # Migration Report
//!
//! After migration, a summary is printed showing:
//! - Total adapters processed
//! - Successfully repaired count
//! - Skipped count (already had hashes)
//! - Failed count (missing .aos file or parse errors)
//!
//! Adapters that failed migration are listed with their error details.

use crate::commands::adapter_repair_hashes::{repair_tenant_adapters, HashRepairResult};
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Summary of a batch migration operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSummary {
    pub tenants_processed: usize,
    pub total_adapters: usize,
    pub repaired: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failed_adapters: Vec<FailedAdapter>,
}

/// Details of an adapter that failed migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedAdapter {
    pub adapter_id: String,
    pub tenant_id: String,
    pub aos_file_path: String,
    pub error: String,
}

/// Run the batch hash migration command
pub async fn run(
    tenant_id: Option<&str>,
    all_tenants: bool,
    dry_run: bool,
    batch_size: i64,
    output: &OutputWriter,
) -> Result<()> {
    output.section("Adapter Hash Migration");

    if !all_tenants && tenant_id.is_none() {
        return Err(AosError::Validation(
            "Either --tenant-id or --all-tenants is required".to_string(),
        ));
    }

    let db = Db::connect_env()
        .await
        .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

    let mut summary = MigrationSummary {
        tenants_processed: 0,
        total_adapters: 0,
        repaired: 0,
        skipped: 0,
        failed: 0,
        failed_adapters: Vec::new(),
    };

    if let Some(tid) = tenant_id {
        // Single tenant migration
        output.kv("Mode", "Single tenant");
        output.kv("Tenant ID", tid);
        if dry_run {
            output.kv("Dry-run", "true");
        }
        output.blank();

        let results = migrate_tenant(&db, tid, dry_run, batch_size, output).await?;
        summary.tenants_processed = 1;
        process_results(&results, &mut summary);
    } else if all_tenants {
        // All tenants migration
        output.kv("Mode", "All tenants");
        output.kv("Batch size", &batch_size.to_string());
        if dry_run {
            output.kv("Dry-run", "true");
        }
        output.blank();

        // Get all unique tenant IDs with adapters needing repair
        let tenants = get_tenants_with_missing_hashes(&db, batch_size).await?;

        if tenants.is_empty() {
            output.info("No adapters with missing hashes found across any tenant");
            return Ok(());
        }

        output.info(format!(
            "Found {} tenants with adapters needing repair",
            tenants.len()
        ));
        output.blank();

        for tid in &tenants {
            output.section(format!("Tenant: {}", tid));
            let results = migrate_tenant(&db, tid, dry_run, batch_size, output).await?;
            summary.tenants_processed += 1;
            process_results(&results, &mut summary);
            output.blank();
        }
    }

    // Print final summary
    output.section("Migration Summary");
    output.kv("Tenants processed", &summary.tenants_processed.to_string());
    output.kv("Total adapters", &summary.total_adapters.to_string());
    output.kv("Repaired", &summary.repaired.to_string());
    output.kv("Skipped", &summary.skipped.to_string());
    output.kv("Failed", &summary.failed.to_string());

    if dry_run {
        output.blank();
        output.info("(Dry-run mode - no changes were made)");
    }

    // Print failed adapters if any
    if !summary.failed_adapters.is_empty() {
        output.blank();
        output.section("Failed Adapters");
        output.warning("The following adapters could not be migrated:");
        for failed in &summary.failed_adapters {
            output.error(format!(
                "  {} (tenant: {}) - {}",
                failed.adapter_id, failed.tenant_id, failed.error
            ));
            if !failed.aos_file_path.is_empty() {
                output.info(format!("    .aos path: {}", failed.aos_file_path));
            }
        }
    }

    if output.is_json() {
        output.json(&summary)?;
    }

    info!(
        tenants = summary.tenants_processed,
        total = summary.total_adapters,
        repaired = summary.repaired,
        skipped = summary.skipped,
        failed = summary.failed,
        dry_run = dry_run,
        code = "HASH_MIGRATION_COMPLETE",
        "Hash migration completed"
    );

    Ok(())
}

/// Migrate a single tenant's adapters
async fn migrate_tenant(
    db: &Db,
    tenant_id: &str,
    dry_run: bool,
    batch_size: i64,
    output: &OutputWriter,
) -> Result<Vec<HashRepairResult>> {
    repair_tenant_adapters(db, tenant_id, dry_run, batch_size, output).await
}

/// Process results into summary
fn process_results(results: &[HashRepairResult], summary: &mut MigrationSummary) {
    summary.total_adapters += results.len();

    for result in results {
        if result.repaired {
            summary.repaired += 1;
        } else if let Some(ref error) = result.error {
            summary.failed += 1;
            summary.failed_adapters.push(FailedAdapter {
                adapter_id: result.adapter_id.clone(),
                tenant_id: result.tenant_id.clone(),
                aos_file_path: result.aos_file_path.clone(),
                error: error.clone(),
            });
        } else {
            summary.skipped += 1;
        }
    }
}

/// Get all tenant IDs that have adapters with missing hashes
async fn get_tenants_with_missing_hashes(db: &Db, limit: i64) -> Result<Vec<String>> {
    // Query for distinct tenant IDs with adapters missing hashes
    let adapters = db.find_adapters_with_missing_hashes(None, limit).await?;

    let mut tenants: Vec<String> = adapters.iter().map(|a| a.tenant_id.clone()).collect();

    tenants.sort();
    tenants.dedup();

    Ok(tenants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_results_empty() {
        let results = Vec::new();
        let mut summary = MigrationSummary {
            tenants_processed: 0,
            total_adapters: 0,
            repaired: 0,
            skipped: 0,
            failed: 0,
            failed_adapters: Vec::new(),
        };

        process_results(&results, &mut summary);

        assert_eq!(summary.total_adapters, 0);
        assert_eq!(summary.repaired, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn test_process_results_mixed() {
        let results = vec![
            HashRepairResult {
                adapter_id: "adapter-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                aos_file_path: "/path/1.aos".to_string(),
                content_hash_b3_before: None,
                content_hash_b3_after: Some("hash1".to_string()),
                manifest_hash_before: None,
                manifest_hash_after: Some("hash2".to_string()),
                repaired: true,
                error: None,
            },
            HashRepairResult {
                adapter_id: "adapter-2".to_string(),
                tenant_id: "tenant-1".to_string(),
                aos_file_path: String::new(),
                content_hash_b3_before: Some("existing".to_string()),
                content_hash_b3_after: Some("existing".to_string()),
                manifest_hash_before: Some("existing".to_string()),
                manifest_hash_after: Some("existing".to_string()),
                repaired: false,
                error: None,
            },
            HashRepairResult {
                adapter_id: "adapter-3".to_string(),
                tenant_id: "tenant-1".to_string(),
                aos_file_path: "/path/3.aos".to_string(),
                content_hash_b3_before: None,
                content_hash_b3_after: None,
                manifest_hash_before: None,
                manifest_hash_after: None,
                repaired: false,
                error: Some("File not found".to_string()),
            },
        ];

        let mut summary = MigrationSummary {
            tenants_processed: 0,
            total_adapters: 0,
            repaired: 0,
            skipped: 0,
            failed: 0,
            failed_adapters: Vec::new(),
        };

        process_results(&results, &mut summary);

        assert_eq!(summary.total_adapters, 3);
        assert_eq!(summary.repaired, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.failed_adapters.len(), 1);
        assert_eq!(summary.failed_adapters[0].adapter_id, "adapter-3");
    }
}
