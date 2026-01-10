//! Base model state tracking for persistent UX visibility
//!
//! Tracks the loading state of base models (Layer 1) and persists status
//! to database for UI consumption. Follows existing adapter lifecycle patterns.
//!
//! ## Model Error Retry (ANCHOR, AUDIT, RECTIFY)
//!
//! - **ANCHOR**: `is_eligible_for_retry()` enforces: Error state, retries < MAX, elapsed > MIN_INTERVAL
//! - **AUDIT**: Tracks `retry_count`, logs attempts, exposes metrics via accessors
//! - **RECTIFY**: `prepare_for_retry()` transitions to Loading; exhausted retries emit alert

use crate::lifecycle_state::LifecycleState;
use adapteros_core::{AosError, Result, WorkerStatus};
use adapteros_db::Db;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Global counters for model retry metrics (AUDIT pillar)
static MODEL_AUTO_RETRY_COUNT: AtomicU64 = AtomicU64::new(0);
static MODEL_AUTO_RETRY_EXHAUSTED: AtomicU64 = AtomicU64::new(0);

/// Maximum number of automatic retries for error models
pub const MAX_MODEL_AUTO_RETRIES: u32 = 3;
/// Minimum time between retry attempts
pub const MIN_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Base model state tracking
#[derive(Clone)]
pub struct BaseModelState {
    /// Model identifier
    pub model_id: String,
    /// Current lifecycle status
    pub lifecycle: LifecycleState,
    /// When the model was loaded (if currently loaded)
    pub loaded_at: Option<Instant>,
    /// Error message if status is Error
    pub error_message: Option<String>,
    /// Memory usage in MB
    pub memory_usage_mb: Option<u32>,
    /// Database connection for persistence
    db: Arc<Db>,
    /// Tenant ID for multi-tenant support
    tenant_id: String,
    /// Number of automatic retry attempts for error recovery
    retry_count: u32,
    /// When the last error occurred (for retry interval enforcement)
    last_error_at: Option<Instant>,
}

impl BaseModelState {
    /// Create new base model state tracker
    pub fn new(model_id: String, tenant_id: String, db: Arc<Db>) -> Self {
        Self {
            model_id,
            lifecycle: LifecycleState::Unloaded,
            loaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            db,
            tenant_id,
            retry_count: 0,
            last_error_at: None,
        }
    }

    /// Update status and persist to database
    pub async fn update_status(
        &mut self,
        lifecycle: LifecycleState,
        error_message: Option<String>,
        memory_usage_mb: Option<u32>,
    ) -> Result<()> {
        let old_state = self.lifecycle;
        self.lifecycle = lifecycle;
        self.error_message = error_message;
        self.memory_usage_mb = memory_usage_mb;

        // Update timestamps based on lifecycle
        match lifecycle {
            LifecycleState::Active | LifecycleState::Loaded => {
                self.loaded_at = Some(Instant::now());
                // Reset retry count on successful load
                self.retry_count = 0;
                self.last_error_at = None;
                info!("Base model {} loaded successfully", self.model_id);
            }
            LifecycleState::Unloaded => {
                self.loaded_at = None;
                info!("Base model {} unloaded", self.model_id);
            }
            LifecycleState::Error => {
                self.last_error_at = Some(Instant::now());
                self.retry_count += 1;
                warn!(
                    model_id = %self.model_id,
                    error = ?self.error_message,
                    retry_count = self.retry_count,
                    "Base model error (retry {} of {})",
                    self.retry_count,
                    MAX_MODEL_AUTO_RETRIES
                );
            }
            _ => {
                debug!(
                    "Base model {} lifecycle changed: {:?} -> {:?}",
                    self.model_id, old_state, lifecycle
                );
            }
        }

        // Persist to database
        self.persist_status().await?;

        Ok(())
    }

    /// Update based on worker lifecycle status
    pub async fn update_from_worker_status(
        &mut self,
        status: WorkerStatus,
        error_message: Option<String>,
        memory_usage_mb: Option<u32>,
    ) -> Result<()> {
        let lifecycle = match status {
            WorkerStatus::Healthy => LifecycleState::Active,
            WorkerStatus::Pending | WorkerStatus::Registered | WorkerStatus::Created => {
                LifecycleState::Loading
            }
            WorkerStatus::Draining => LifecycleState::Unloading,
            WorkerStatus::Stopped => LifecycleState::Unloaded,
            WorkerStatus::Error => LifecycleState::Error,
        };
        self.update_status(lifecycle, error_message, memory_usage_mb)
            .await
    }

    /// Mark model as loading
    pub async fn mark_loading(&mut self) -> Result<()> {
        self.update_status(LifecycleState::Loading, None, None)
            .await
    }

    /// Mark model as loaded
    pub async fn mark_loaded(&mut self, memory_usage_mb: u32) -> Result<()> {
        self.update_status(LifecycleState::Active, None, Some(memory_usage_mb))
            .await
    }

    /// Mark model as unloading
    pub async fn mark_unloading(&mut self) -> Result<()> {
        self.update_status(LifecycleState::Unloading, None, None)
            .await
    }

    /// Mark model as unloaded
    pub async fn mark_unloaded(&mut self) -> Result<()> {
        self.update_status(LifecycleState::Unloaded, None, None)
            .await
    }

    /// Mark model as error
    pub async fn mark_error(&mut self, error_message: String) -> Result<()> {
        self.update_status(LifecycleState::Error, Some(error_message), None)
            .await
    }

    /// Get current status
    pub fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }

    /// Check if model is loaded
    pub fn is_loaded(&self) -> bool {
        self.lifecycle.is_active()
    }

    /// Get memory usage in MB
    pub fn memory_usage_mb(&self) -> Option<u32> {
        self.memory_usage_mb
    }

    /// Get error message if any
    pub fn error_message(&self) -> Option<&String> {
        self.error_message.as_ref()
    }

    /// Get time since model was loaded
    pub fn time_since_loaded(&self) -> Option<std::time::Duration> {
        self.loaded_at.map(|loaded_at| loaded_at.elapsed())
    }

    /// Check if the model is eligible for automatic retry
    ///
    /// A model is eligible for retry if:
    /// 1. It's in Error state
    /// 2. Retry count is below MAX_MODEL_AUTO_RETRIES
    /// 3. Enough time has elapsed since the last error (MIN_RETRY_INTERVAL)
    pub fn is_eligible_for_retry(&self) -> bool {
        if self.lifecycle != LifecycleState::Error {
            return false;
        }

        if self.retry_count >= MAX_MODEL_AUTO_RETRIES {
            return false;
        }

        if let Some(last_error) = self.last_error_at {
            if last_error.elapsed() < MIN_RETRY_INTERVAL {
                return false;
            }
        }

        true
    }

    /// Check if retry attempts are exhausted
    pub fn is_retry_exhausted(&self) -> bool {
        self.lifecycle == LifecycleState::Error && self.retry_count >= MAX_MODEL_AUTO_RETRIES
    }

    /// Get current retry count
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }

    /// Prepare for retry by transitioning to Loading state
    ///
    /// Call this before attempting to reload the model.
    /// Returns an error if the model is not eligible for retry.
    pub async fn prepare_for_retry(&mut self) -> Result<()> {
        if !self.is_eligible_for_retry() {
            if self.is_retry_exhausted() {
                // AUDIT: Track exhausted retries
                MODEL_AUTO_RETRY_EXHAUSTED.fetch_add(1, Ordering::Relaxed);
                // RECTIFY: Alert for manual intervention
                error!(
                    model_id = %self.model_id,
                    retry_count = self.retry_count,
                    total_exhausted = MODEL_AUTO_RETRY_EXHAUSTED.load(Ordering::Relaxed),
                    "Model retry attempts exhausted - manual intervention required"
                );
                return Err(AosError::Worker(format!(
                    "Model {} retry attempts exhausted ({}/{})",
                    self.model_id, self.retry_count, MAX_MODEL_AUTO_RETRIES
                )));
            }
            return Err(AosError::Worker(format!(
                "Model {} not eligible for retry",
                self.model_id
            )));
        }

        // AUDIT: Track retry attempts
        MODEL_AUTO_RETRY_COUNT.fetch_add(1, Ordering::Relaxed);

        info!(
            model_id = %self.model_id,
            retry_attempt = self.retry_count + 1,
            max_retries = MAX_MODEL_AUTO_RETRIES,
            total_retries = MODEL_AUTO_RETRY_COUNT.load(Ordering::Relaxed),
            "Initiating automatic retry for error model"
        );

        // Transition to Loading state for retry
        // Note: retry_count is NOT incremented here - it's incremented on error
        self.update_status(LifecycleState::Loading, None, None)
            .await
    }

    /// Persist current status to database
    async fn persist_status(&self) -> Result<()> {
        self.db
            .update_base_model_status(
                &self.tenant_id,
                &self.model_id,
                self.lifecycle.to_model_status().as_str(),
                self.error_message.as_deref(),
                self.memory_usage_mb.map(|mb| mb as i32),
            )
            .await
            .map_err(|e| AosError::Worker(format!("Failed to persist base model status: {}", e)))?;

        Ok(())
    }

    /// Load status from database
    pub async fn load_from_db(&mut self) -> Result<()> {
        if let Some(status_record) = self.db.get_base_model_status(&self.tenant_id).await? {
            let model_status =
                adapteros_api_types::ModelLoadStatus::parse_status(&status_record.status);
            self.lifecycle = LifecycleState::from(model_status);
            self.error_message = status_record.error_message;
            self.memory_usage_mb = status_record.memory_usage_mb.map(|mb| mb as u32);

            // Restore loaded_at if model is currently loaded
            if self.lifecycle.is_active() && status_record.loaded_at.is_some() {
                // Parse the timestamp (simplified - in production would use proper parsing)
                self.loaded_at = Some(Instant::now()); // Simplified for now
            }

            debug!(
                "Loaded base model lifecycle from database: {:?}",
                self.lifecycle
            );
        }

        Ok(())
    }
}

/// AUDIT: Get total count of model auto-retry attempts across all models
pub fn model_auto_retry_count() -> u64 {
    MODEL_AUTO_RETRY_COUNT.load(Ordering::Relaxed)
}

/// AUDIT: Get count of models that exhausted all retry attempts
pub fn model_auto_retry_exhausted() -> u64 {
    MODEL_AUTO_RETRY_EXHAUSTED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::sync::Arc; // unused

    #[test]
    fn test_lifecycle_to_model_status_conversion() {
        assert_eq!(
            LifecycleState::Loading.to_model_status().as_str(),
            "loading"
        );
        assert_eq!(LifecycleState::Active.to_model_status().as_str(), "ready");
        assert_eq!(
            LifecycleState::Unloading.to_model_status().as_str(),
            "unloading"
        );
        assert_eq!(
            LifecycleState::Unloaded.to_model_status().as_str(),
            "no-model"
        );
        assert_eq!(LifecycleState::Error.to_model_status().as_str(), "error");
    }

    #[test]
    fn test_lifecycle_activity_checks() {
        assert!(LifecycleState::Active.is_active());
        assert!(LifecycleState::Loaded.is_active());
        assert!(!LifecycleState::Loading.is_active());
        assert!(!LifecycleState::Unloaded.is_active());
    }

    #[test]
    fn test_base_model_state_creation() {
        // Note: In-memory database testing would require proper Db implementation
        // For now, just test the status enum functionality
        let lifecycle = LifecycleState::Unloaded;
        assert_eq!(lifecycle, LifecycleState::Unloaded);
        assert!(!lifecycle.is_active());
    }
}
