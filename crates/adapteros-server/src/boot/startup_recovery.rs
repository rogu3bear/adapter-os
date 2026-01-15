//! Startup recovery tasks for adapterOS control plane.
//!
//! This module handles recovery of orphaned or stuck resources that may have been
//! left in inconsistent states due to previous server crashes or ungraceful shutdowns.
//!
//! ## Recovery Tasks
//!
//! 1. **Orphaned Training Jobs**: Jobs stuck in "running" state with no recent progress
//!    are transitioned to "interrupted" state, allowing retry via the normal retry chain.
//!
//! ## Design Principles (ANCHOR, AUDIT, RECTIFY)
//!
//! - **ANCHOR**: Recovery runs as a mandatory boot phase before accepting requests
//! - **AUDIT**: All recovery actions logged via structured tracing for observability
//! - **RECTIFY**: Orphaned resources are transitioned to recoverable states

use adapteros_db::Db;
use adapteros_orchestrator::training::{OrphanedJobRecoveryReport, TrainingService};
use anyhow::Result;
use std::time::Duration;
use tracing::{error, info, warn};

/// Default staleness threshold for orphaned training jobs.
/// Jobs in "running" state without progress for this duration are considered orphaned.
pub const DEFAULT_ORPHANED_JOB_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes

/// Result of all startup recovery operations
#[derive(Debug, Clone)]
pub struct StartupRecoveryReport {
    /// Training job recovery results
    pub training_jobs: Option<OrphanedJobRecoveryReport>,
    /// Whether any recovery actions were taken
    pub had_recoveries: bool,
    /// Total duration of recovery phase
    pub duration_ms: u64,
}

/// Run all startup recovery tasks.
///
/// This function should be called during boot, after database migrations complete
/// but before the server starts accepting new requests.
///
/// # Arguments
///
/// * `db` - Database connection
///
/// # Returns
///
/// A report summarizing all recovery actions taken.
pub async fn run_startup_recovery(db: &Db) -> Result<StartupRecoveryReport> {
    let start = std::time::Instant::now();
    info!(target: "boot", "Starting startup recovery phase");

    let mut report = StartupRecoveryReport {
        training_jobs: None,
        had_recoveries: false,
        duration_ms: 0,
    };

    // Recovery Task 1: Orphaned Training Jobs
    match recover_orphaned_training_jobs(db).await {
        Ok(job_report) => {
            if job_report.recovered_count > 0 {
                report.had_recoveries = true;
                // AUDIT: Log recovered jobs with structured fields for observability
                warn!(
                    target: "boot",
                    recovered_count = job_report.recovered_count,
                    job_ids = ?job_report.recovered_job_ids,
                    threshold_secs = DEFAULT_ORPHANED_JOB_THRESHOLD.as_secs(),
                    "Recovered orphaned training jobs - transitioned to interrupted state"
                );
            } else {
                info!(target: "boot", "No orphaned training jobs found");
            }
            report.training_jobs = Some(job_report);
        }
        Err(e) => {
            // Log but don't fail boot - recovery is best-effort
            error!(target: "boot", error = %e, "Failed to recover orphaned training jobs");
        }
    }

    report.duration_ms = start.elapsed().as_millis() as u64;

    // AUDIT: Final recovery summary with structured fields
    if report.had_recoveries {
        info!(
            target: "boot",
            duration_ms = report.duration_ms,
            training_jobs_recovered = report.training_jobs.as_ref().map(|r| r.recovered_count).unwrap_or(0),
            "Startup recovery completed with recoveries"
        );
    } else {
        info!(
            target: "boot",
            duration_ms = report.duration_ms,
            "Startup recovery completed - no orphaned resources found"
        );
    }

    Ok(report)
}

/// Recover orphaned training jobs that were left in "running" state.
///
/// Jobs are considered orphaned if they are in "running" state but haven't
/// had any progress updates within the threshold duration. This typically
/// indicates the previous server instance crashed while the job was executing.
///
/// RECTIFY: Orphaned jobs are transitioned to "interrupted" state, allowing
/// them to be retried via the normal retry_of_job_id chain.
async fn recover_orphaned_training_jobs(db: &Db) -> Result<OrphanedJobRecoveryReport> {
    // Create a minimal training service just for recovery
    let mut service = TrainingService::new();
    service.set_db(db.clone());

    let report = service
        .recover_orphaned_jobs(DEFAULT_ORPHANED_JOB_THRESHOLD)
        .await?;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_threshold() {
        assert_eq!(DEFAULT_ORPHANED_JOB_THRESHOLD, Duration::from_secs(300));
    }
}
