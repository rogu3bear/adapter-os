//! Base model state tracking for persistent UX visibility
//!
//! Tracks the loading state of base models (Layer 1) and persists status
//! to database for UI consumption. Follows existing adapter lifecycle patterns.

use crate::lifecycle_state::LifecycleState;
use adapteros_core::{AosError, Result, WorkerStatus};
use adapteros_db::Db;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

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
                info!("Base model {} loaded successfully", self.model_id);
            }
            LifecycleState::Unloaded => {
                self.loaded_at = None;
                info!("Base model {} unloaded", self.model_id);
            }
            LifecycleState::Error => {
                warn!(
                    "Base model {} error: {:?}",
                    self.model_id, self.error_message
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
