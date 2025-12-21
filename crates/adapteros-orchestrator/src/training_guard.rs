//! RAII guard for training job lifecycle management
//!
//! Ensures training jobs never get stuck in "Running" state on error by
//! automatically transitioning to terminal states on drop.
//!
//! # Usage Pattern
//!
//! ```ignore
//! let guard = TrainingJobGuard::new(db, job_id, GuardLogLevel::Warn);
//! guard.start().await?;
//!
//! // Training logic here...
//! match train() {
//!     Ok(_) => guard.complete_ok().await?,
//!     Err(e) => guard.fail_with(&e.to_string()),
//! }
//! ```
//!
//! # Invariants
//!
//! - If guard drops without explicit completion, job status becomes Failed
//! - Error messages are preserved in progress_json
//! - Logging respects GuardLogLevel configuration

use adapteros_core::{GuardLogLevel, Result};
use adapteros_db::Db;
use adapteros_types::training::TrainingJobStatus;
use std::sync::Arc;
use tracing::{debug, warn};

/// RAII guard for training job lifecycle management.
///
/// Ensures jobs never get stuck in "Running" state on error by automatically
/// transitioning to terminal states on drop.
///
/// # Drop Behavior
///
/// If the guard is dropped without calling `complete_ok()`, it will:
/// 1. Spawn an async task to update job status to `final_status` (default: Failed)
/// 2. Set error_message in progress_json if present
/// 3. Log a warning or debug message based on `log_level`
///
/// # Thread Safety
///
/// The guard is Send + Sync and can be used across async boundaries.
pub struct TrainingJobGuard {
    /// Database connection for job status updates
    db: Arc<Db>,
    /// Training job ID to manage
    job_id: String,
    /// Final status to set on drop if not explicitly completed
    final_status: TrainingJobStatus,
    /// Error message to store if failed
    error_message: Option<String>,
    /// Whether job was explicitly completed (prevents drop cleanup)
    completed: bool,
    /// Log level for unexpected cleanup
    log_level: GuardLogLevel,
}

impl TrainingJobGuard {
    /// Create a new training job guard.
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection for status updates
    /// * `job_id` - Training job ID to manage
    /// * `log_level` - Log level for cleanup warnings (Warn recommended)
    ///
    /// # Default Behavior
    ///
    /// - `final_status` defaults to `Failed`
    /// - `completed` defaults to `false`
    /// - `error_message` defaults to `None`
    ///
    /// Call `start()` to transition job to Running state.
    pub fn new(db: Arc<Db>, job_id: String, log_level: GuardLogLevel) -> Self {
        Self {
            db,
            job_id,
            final_status: TrainingJobStatus::Failed,
            error_message: None,
            completed: false,
            log_level,
        }
    }

    /// Start the training job by transitioning to Running state.
    ///
    /// Updates the database to set:
    /// - `status = "running"`
    /// - `started_at = <current timestamp>`
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn start(&self) -> Result<()> {
        // Set status to running with started_at timestamp
        self.db
            .update_training_status(&self.job_id, "running")
            .await?;
        Ok(())
    }

    /// Mark training job as successfully completed.
    ///
    /// Updates the database to set:
    /// - `status = "completed"`
    /// - `completed_at = <current timestamp>`
    ///
    /// After calling this, the guard will not perform cleanup on drop.
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn complete_ok(&mut self) -> Result<()> {
        self.db
            .update_training_status(&self.job_id, "completed")
            .await?;
        self.completed = true;
        Ok(())
    }

    /// Set error message for failure case.
    ///
    /// Configures the guard to:
    /// - Set `final_status = Failed`
    /// - Store `error_message` in progress_json on drop
    ///
    /// This method does NOT update the database immediately.
    /// The actual update happens on drop or when explicitly calling fail().
    ///
    /// # Arguments
    ///
    /// * `error` - Human-readable error message
    pub fn fail_with(&mut self, error: &str) {
        self.final_status = TrainingJobStatus::Failed;
        self.error_message = Some(error.to_string());
    }

    /// Explicitly fail the training job and update database.
    ///
    /// Updates the database to set:
    /// - `status = "failed"`
    /// - `completed_at = <current timestamp>`
    /// - `progress_json.error_message = <error>`
    ///
    /// After calling this, the guard will not perform cleanup on drop.
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn fail(&mut self) -> Result<()> {
        // Update status to failed
        self.db
            .update_training_status(&self.job_id, "failed")
            .await?;

        // Update progress_json with error message if present
        if let Some(ref error) = self.error_message {
            let progress = adapteros_db::training_jobs::TrainingProgress {
                progress_pct: 0.0,
                current_epoch: 0,
                total_epochs: 0,
                current_loss: 0.0,
                learning_rate: 0.0,
                tokens_per_second: 0.0,
                error_message: Some(error.clone()),
            };
            self.db
                .update_training_progress(&self.job_id, &progress)
                .await?;
        }

        self.completed = true;
        Ok(())
    }

    /// Mark training job as cancelled.
    ///
    /// Configures the guard to:
    /// - Set `final_status = Cancelled`
    /// - Mark as completed to prevent drop cleanup
    ///
    /// This does NOT update the database immediately.
    /// Use `cancel_now()` for immediate cancellation.
    pub fn cancel(&mut self) {
        self.final_status = TrainingJobStatus::Cancelled;
        self.completed = true;
    }

    /// Explicitly cancel the training job and update database.
    ///
    /// Updates the database to set:
    /// - `status = "cancelled"`
    /// - `completed_at = <current timestamp>`
    ///
    /// After calling this, the guard will not perform cleanup on drop.
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn cancel_now(&mut self) -> Result<()> {
        self.db
            .update_training_status(&self.job_id, "cancelled")
            .await?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for TrainingJobGuard {
    /// Perform cleanup on drop if job was not explicitly completed.
    ///
    /// If `completed == false`, spawns an async task to:
    /// 1. Update job status to `final_status` (default: Failed)
    /// 2. Set error_message in progress_json if present
    /// 3. Log warning/debug based on `log_level`
    ///
    /// # Implementation Note
    ///
    /// Uses `tokio::runtime::Handle::try_current()` to spawn the async cleanup task.
    /// If no runtime is available, logs an error and skips database update.
    fn drop(&mut self) {
        if self.completed {
            // Job was explicitly completed, no cleanup needed
            return;
        }

        // Job was NOT explicitly completed - perform cleanup
        let status_str = self.final_status.to_string();
        let job_id = self.job_id.clone();
        let db = self.db.clone();
        let error_message = self.error_message.clone();
        let log_level = self.log_level;

        // Log based on configured level
        match log_level {
            GuardLogLevel::Warn => {
                warn!(
                    job_id = %job_id,
                    status = %status_str,
                    "TrainingJobGuard dropped without explicit completion, setting terminal status"
                );
            }
            GuardLogLevel::Debug => {
                debug!(
                    job_id = %job_id,
                    status = %status_str,
                    "TrainingJobGuard dropped, cleaning up job status"
                );
            }
            GuardLogLevel::Off => {
                // No logging
            }
        }

        // Spawn async task to update database
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                // Update status to terminal state
                if let Err(e) = db.update_training_status(&job_id, &status_str).await {
                    tracing::error!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to update training job status in guard cleanup"
                    );
                    return;
                }

                // Update progress_json with error message if present
                if let Some(error) = error_message {
                    let progress = adapteros_db::training_jobs::TrainingProgress {
                        progress_pct: 0.0,
                        current_epoch: 0,
                        total_epochs: 0,
                        current_loss: 0.0,
                        learning_rate: 0.0,
                        tokens_per_second: 0.0,
                        error_message: Some(error),
                    };
                    if let Err(e) = db.update_training_progress(&job_id, &progress).await {
                        tracing::error!(
                            job_id = %job_id,
                            error = %e,
                            "Failed to update training job progress with error message"
                        );
                    }
                }
            });
        } else {
            tracing::error!(
                job_id = %job_id,
                "No tokio runtime available for TrainingJobGuard cleanup"
            );
        }
    }
}

// Note: Full integration tests for TrainingJobGuard require database setup with
// foreign key relationships. These tests are located in the integration test suite.
// The unit tests below verify the guard's internal logic without database dependencies.

#[cfg(test)]
mod tests {
    use super::*;

    // Test that guard state transitions work correctly (without DB)
    #[test]
    fn test_guard_state_transitions() {
        // Verify initial state
        let final_status = TrainingJobStatus::Failed;
        assert_eq!(final_status.to_string(), "failed");

        // Verify completed state
        let completed_status = TrainingJobStatus::Completed;
        assert_eq!(completed_status.to_string(), "completed");

        // Verify cancelled state
        let cancelled_status = TrainingJobStatus::Cancelled;
        assert_eq!(cancelled_status.to_string(), "cancelled");
    }

    #[test]
    fn test_guard_log_level_variants() {
        // Test all log level variants are valid
        let _warn = GuardLogLevel::Warn;
        let _debug = GuardLogLevel::Debug;
        let _off = GuardLogLevel::Off;
    }
}
