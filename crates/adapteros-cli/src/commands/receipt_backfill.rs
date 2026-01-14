//! Receipt backfill command for Phase 3 cryptographic receipt system
//!
//! Backfills `crypto_receipt_digest_b3` and `receipt_parity_verified` columns
//! in `inference_trace_receipts` for records that were created before Phase 3.
//!
//! This command:
//! 1. Finds receipts where crypto_receipt_digest_b3 IS NULL or receipt_parity_verified IS NULL
//! 2. For each receipt, recomputes the crypto receipt digest using the canonical algorithm
//! 3. Compares with legacy receipt_digest and sets receipt_parity_verified accordingly
//! 4. Updates the record with the computed values
//!
//! # Usage
//!
//! ```bash
//! # Preview what would be backfilled (dry-run)
//! aosctl receipt backfill --dry-run
//!
//! # Backfill all pending receipts
//! aosctl receipt backfill
//!
//! # Process specific number of records
//! aosctl receipt backfill --limit 1000
//!
//! # Control batch size for large migrations
//! aosctl receipt backfill --batch-size 100
//!
//! # JSON output for scripting
//! aosctl receipt backfill --json
//! ```
//!
//! # Migration Report
//!
//! After backfill, a summary is printed showing:
//! - Total receipts processed
//! - Successfully backfilled count
//! - Parity matches (legacy == crypto)
//! - Parity mismatches (legacy != crypto)
//! - Failed count (errors during processing)

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use adapteros_db::{
    backfill_receipt_digests, count_pending_receipt_backfill, BackfillResult, Db,
};
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Receipt management subcommands
#[derive(Debug, Clone, Subcommand)]
pub enum ReceiptCommand {
    /// Backfill crypto receipt digests for legacy receipts
    #[command(
        after_help = r#"Examples:
  # Preview what would be backfilled (dry-run)
  aosctl receipt backfill --dry-run

  # Backfill all pending receipts
  aosctl receipt backfill

  # Process specific number of records
  aosctl receipt backfill --limit 1000

  # Control batch size for large migrations
  aosctl receipt backfill --batch-size 100

  # JSON output for scripting
  aosctl receipt backfill --json
"#
    )]
    Backfill {
        /// Preview changes without modifying the database
        #[arg(long)]
        dry_run: bool,

        /// Maximum number of records to process (default: all)
        #[arg(long)]
        limit: Option<u32>,

        /// Number of records to process per batch (default: 100)
        #[arg(long, default_value = "100")]
        batch_size: u32,
    },
}

/// Summary of a backfill operation for JSON output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillSummary {
    /// Total receipts found needing backfill
    pub total_pending: u64,
    /// Receipts processed in this run
    pub processed: u32,
    /// Receipts where legacy and crypto digests matched
    pub parity_matched: u32,
    /// Receipts where legacy and crypto digests did not match
    pub parity_mismatched: u32,
    /// Receipts that failed to process
    pub failed: u32,
    /// Parity rate as a percentage
    pub parity_rate: f64,
    /// Trace IDs of failed receipts
    pub failed_trace_ids: Vec<String>,
    /// Trace IDs with parity mismatches (for investigation)
    pub mismatched_trace_ids: Vec<String>,
    /// Whether this was a dry-run
    pub dry_run: bool,
}

impl From<BackfillResult> for BackfillSummary {
    fn from(result: BackfillResult) -> Self {
        Self {
            total_pending: 0, // Will be set separately
            processed: result.processed,
            parity_matched: result.matched,
            parity_mismatched: result.mismatched,
            failed: result.failed,
            parity_rate: result.parity_rate(),
            failed_trace_ids: result.failed_trace_ids,
            mismatched_trace_ids: result.mismatched_trace_ids,
            dry_run: false, // Will be set separately
        }
    }
}

/// Handle receipt commands
pub async fn handle_receipt_command(cmd: ReceiptCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        ReceiptCommand::Backfill {
            dry_run,
            limit,
            batch_size,
        } => run_backfill(dry_run, limit, batch_size, output).await,
    }
}

/// Run the backfill operation
pub async fn run_backfill(
    dry_run: bool,
    limit: Option<u32>,
    batch_size: u32,
    output: &OutputWriter,
) -> Result<()> {
    output.section("Receipt Digest Backfill");

    if dry_run {
        output.info("Dry-run mode - no changes will be made");
    }

    output.kv("Batch size", &batch_size.to_string());
    if let Some(l) = limit {
        output.kv("Limit", &l.to_string());
    }
    output.blank();

    let db = Db::connect_env()
        .await
        .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

    // Get count of pending receipts
    let pending_count = count_pending_receipt_backfill(&db)
        .await
        .map_err(|e| AosError::Database(format!("Failed to count pending receipts: {}", e)))?;

    if pending_count == 0 {
        output.success("No receipts need backfill - all records already have crypto digest values");
        if output.is_json() {
            output.json(&BackfillSummary {
                total_pending: 0,
                processed: 0,
                parity_matched: 0,
                parity_mismatched: 0,
                failed: 0,
                parity_rate: 100.0,
                failed_trace_ids: Vec::new(),
                mismatched_trace_ids: Vec::new(),
                dry_run,
            })?;
        }
        return Ok(());
    }

    let records_to_process = if let Some(l) = limit {
        std::cmp::min(pending_count, l as u64) as u32
    } else {
        pending_count as u32
    };

    output.info(format!(
        "Found {} receipts pending backfill, processing {}",
        pending_count, records_to_process
    ));
    output.blank();

    // Aggregate results across batches
    let mut total_result = BackfillResult::default();
    let mut processed = 0u32;

    // Create progress bar (only if not in JSON/quiet mode)
    let progress = if !output.is_json() && !output.is_quiet() {
        let pb = ProgressBar::new(records_to_process as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.yellow} [{elapsed_precise}] {bar:40.yellow/blue} {pos}/{len} receipts ({msg})")
                .expect("valid template")
                .progress_chars("=>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    while processed < records_to_process {
        let batch_limit = std::cmp::min(batch_size, records_to_process - processed);

        if let Some(ref pb) = progress {
            pb.set_message(format!("batch at {}", processed));
        }

        let batch_result = backfill_receipt_digests(&db, Some(batch_limit), dry_run)
            .await
            .map_err(|e| AosError::Database(format!("Failed to backfill batch: {}", e)))?;

        if batch_result.processed == 0 {
            // No more records to process
            break;
        }

        // Aggregate results
        total_result.processed += batch_result.processed;
        total_result.matched += batch_result.matched;
        total_result.mismatched += batch_result.mismatched;
        total_result.failed += batch_result.failed;
        total_result
            .failed_trace_ids
            .extend(batch_result.failed_trace_ids);
        total_result
            .mismatched_trace_ids
            .extend(batch_result.mismatched_trace_ids);

        processed += batch_result.processed;
        if let Some(ref pb) = progress {
            pb.set_position(processed as u64);
        }

        // In dry-run mode, we can't continue because records aren't updated
        // and we'd process the same records again
        if dry_run {
            break;
        }
    }

    if let Some(pb) = progress {
        pb.finish_with_message("complete");
    }
    output.success("Backfill complete");

    // Build summary for output
    let mut summary = BackfillSummary::from(total_result.clone());
    summary.total_pending = pending_count;
    summary.dry_run = dry_run;

    // Print summary
    output.blank();
    output.section("Backfill Summary");
    output.kv("Total pending", &summary.total_pending.to_string());
    output.kv("Processed", &summary.processed.to_string());
    output.kv("Parity matched", &summary.parity_matched.to_string());
    output.kv("Parity mismatched", &summary.parity_mismatched.to_string());
    output.kv("Failed", &summary.failed.to_string());
    output.kv("Parity rate", &format!("{:.2}%", summary.parity_rate));

    if dry_run {
        output.blank();
        output.info("(Dry-run mode - no changes were made)");
    }

    // Print failed receipts if any
    if !summary.failed_trace_ids.is_empty() {
        output.blank();
        output.section("Failed Receipts");
        output.warning("The following receipts could not be backfilled:");
        for trace_id in summary.failed_trace_ids.iter().take(10) {
            output.error(format!("  {}", trace_id));
        }
        if summary.failed_trace_ids.len() > 10 {
            output.warning(format!(
                "  ... and {} more",
                summary.failed_trace_ids.len() - 10
            ));
        }
    }

    // Print parity warnings if mismatches found
    if summary.parity_mismatched > 0 {
        output.blank();
        output.warning(format!(
            "{} receipts have parity mismatches between legacy and crypto digests",
            summary.parity_mismatched
        ));
        output.info("This may indicate schema version differences or data inconsistencies");
        output.info("Mismatched trace IDs (first 10):");
        for trace_id in summary.mismatched_trace_ids.iter().take(10) {
            output.result(format!("  {}", trace_id));
        }
        if summary.mismatched_trace_ids.len() > 10 {
            output.info(format!(
                "  ... and {} more",
                summary.mismatched_trace_ids.len() - 10
            ));
        }
        output.blank();
        output.info("Run 'aosctl verify receipt --trace-id <id>' to investigate individual receipts");
    }

    if output.is_json() {
        output.json(&summary)?;
    }

    info!(
        total_pending = summary.total_pending,
        processed = summary.processed,
        parity_matched = summary.parity_matched,
        parity_mismatched = summary.parity_mismatched,
        failed = summary.failed,
        parity_rate = summary.parity_rate,
        dry_run = dry_run,
        code = "RECEIPT_BACKFILL_COMPLETE",
        "Receipt backfill completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backfill_summary_serialization() {
        let summary = BackfillSummary {
            total_pending: 100,
            processed: 95,
            parity_matched: 90,
            parity_mismatched: 5,
            failed: 2,
            parity_rate: 94.74,
            failed_trace_ids: vec!["trace-1".to_string()],
            mismatched_trace_ids: vec!["trace-2".to_string()],
            dry_run: false,
        };

        let json = serde_json::to_string(&summary).expect("serialize summary");
        assert!(json.contains("\"total_pending\":100"));
        assert!(json.contains("\"parity_matched\":90"));
    }

    #[test]
    fn test_backfill_summary_empty() {
        let summary = BackfillSummary {
            total_pending: 0,
            processed: 0,
            parity_matched: 0,
            parity_mismatched: 0,
            failed: 0,
            parity_rate: 100.0,
            failed_trace_ids: Vec::new(),
            mismatched_trace_ids: Vec::new(),
            dry_run: true,
        };

        assert_eq!(summary.total_pending, 0);
        assert!(summary.failed_trace_ids.is_empty());
    }

    #[test]
    fn test_backfill_result_conversion() {
        let db_result = BackfillResult {
            processed: 100,
            matched: 95,
            mismatched: 3,
            failed: 2,
            failed_trace_ids: vec!["t1".to_string()],
            mismatched_trace_ids: vec!["t2".to_string()],
        };

        let summary: BackfillSummary = db_result.into();
        assert_eq!(summary.processed, 100);
        assert_eq!(summary.parity_matched, 95);
        assert_eq!(summary.parity_mismatched, 3);
        assert_eq!(summary.failed, 2);
    }
}
