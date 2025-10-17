//! Base model state tracking for persistent UX visibility
//!
//! Tracks the loading state of base models (Layer 1) and persists status
//! to database for UI consumption. Follows existing adapter lifecycle patterns.

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Base model loading status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseModelStatus {
    Loading,
    Loaded,
    Unloading,
    Unloaded,
    Error,
}

impl BaseModelStatus {
    /// Convert to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Loaded => "loaded",
            Self::Unloading => "unloading",
            Self::Unloaded => "unloaded",
            Self::Error => "error",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "loading" => Ok(Self::Loading),
            "loaded" => Ok(Self::Loaded),
            "unloading" => Ok(Self::Unloading),
            "unloaded" => Ok(Self::Unloaded),
            "error" => Ok(Self::Error),
            _ => Err(AosError::Worker(format!(
                "Invalid base model status: {}",
                s
            ))),
        }
    }

    /// Check if model is currently loaded
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded)
    }

    /// Check if model is in transition state
    pub fn is_transitioning(&self) -> bool {
        matches!(self, Self::Loading | Self::Unloading)
    }
}

/// Base model state tracking
#[derive(Clone)]
pub struct BaseModelState {
    /// Model identifier
    pub model_id: String,
    /// Current loading status
    pub status: BaseModelStatus,
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
            status: BaseModelStatus::Unloaded,
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
        status: BaseModelStatus,
        error_message: Option<String>,
        memory_usage_mb: Option<u32>,
    ) -> Result<()> {
        let old_status = self.status;
        self.status = status;
        self.error_message = error_message;
        self.memory_usage_mb = memory_usage_mb;

        // Update timestamps based on status
        match status {
            BaseModelStatus::Loaded => {
                self.loaded_at = Some(Instant::now());
                info!("Base model {} loaded successfully", self.model_id);
            }
            BaseModelStatus::Unloaded => {
                self.loaded_at = None;
                info!("Base model {} unloaded", self.model_id);
            }
            BaseModelStatus::Error => {
                warn!(
                    "Base model {} error: {:?}",
                    self.model_id, self.error_message
                );
            }
            _ => {
                debug!(
                    "Base model {} status changed: {:?} -> {:?}",
                    self.model_id, old_status, status
                );
            }
        }

        // Persist to database
        self.persist_status().await?;

        Ok(())
    }

    /// Mark model as loading
    pub async fn mark_loading(&mut self) -> Result<()> {
        self.update_status(BaseModelStatus::Loading, None, None)
            .await
    }

    /// Mark model as loaded
    pub async fn mark_loaded(&mut self, memory_usage_mb: u32) -> Result<()> {
        self.update_status(BaseModelStatus::Loaded, None, Some(memory_usage_mb))
            .await
    }

    /// Mark model as unloading
    pub async fn mark_unloading(&mut self) -> Result<()> {
        self.update_status(BaseModelStatus::Unloading, None, None)
            .await
    }

    /// Mark model as unloaded
    pub async fn mark_unloaded(&mut self) -> Result<()> {
        self.update_status(BaseModelStatus::Unloaded, None, None)
            .await
    }

    /// Mark model as error
    pub async fn mark_error(&mut self, error_message: String) -> Result<()> {
        self.update_status(BaseModelStatus::Error, Some(error_message), None)
            .await
    }

    /// Get current status
    pub fn status(&self) -> BaseModelStatus {
        self.status
    }

    /// Check if model is loaded
    pub fn is_loaded(&self) -> bool {
        self.status.is_loaded()
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
                self.status.as_str(),
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
            self.status = BaseModelStatus::from_str(&status_record.status)?;
            self.error_message = status_record.error_message;
            self.memory_usage_mb = status_record.memory_usage_mb.map(|mb| mb as u32);

            // Restore loaded_at if model is currently loaded
            if self.status.is_loaded() && status_record.loaded_at.is_some() {
                // Parse the timestamp (simplified - in production would use proper parsing)
                self.loaded_at = Some(Instant::now()); // Simplified for now
            }

            debug!("Loaded base model status from database: {:?}", self.status);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::sync::Arc; // unused

    #[test]
    fn test_base_model_status_conversion() {
        assert_eq!(BaseModelStatus::Loading.as_str(), "loading");
        assert_eq!(BaseModelStatus::Loaded.as_str(), "loaded");
        assert_eq!(BaseModelStatus::Unloading.as_str(), "unloading");
        assert_eq!(BaseModelStatus::Unloaded.as_str(), "unloaded");
        assert_eq!(BaseModelStatus::Error.as_str(), "error");

        assert_eq!(
            BaseModelStatus::from_str("loading").unwrap(),
            BaseModelStatus::Loading
        );
        assert_eq!(
            BaseModelStatus::from_str("loaded").unwrap(),
            BaseModelStatus::Loaded
        );
        assert_eq!(
            BaseModelStatus::from_str("unloading").unwrap(),
            BaseModelStatus::Unloading
        );
        assert_eq!(
            BaseModelStatus::from_str("unloaded").unwrap(),
            BaseModelStatus::Unloaded
        );
        assert_eq!(
            BaseModelStatus::from_str("error").unwrap(),
            BaseModelStatus::Error
        );
    }

    #[test]
    fn test_base_model_status_checks() {
        assert!(BaseModelStatus::Loaded.is_loaded());
        assert!(!BaseModelStatus::Loading.is_loaded());
        assert!(!BaseModelStatus::Unloaded.is_loaded());

        assert!(BaseModelStatus::Loading.is_transitioning());
        assert!(BaseModelStatus::Unloading.is_transitioning());
        assert!(!BaseModelStatus::Loaded.is_transitioning());
        assert!(!BaseModelStatus::Unloaded.is_transitioning());
    }

    #[test]
    fn test_base_model_state_creation() {
        // Note: In-memory database testing would require proper Db implementation
        // For now, just test the status enum functionality
        let status = BaseModelStatus::Unloaded;
        assert_eq!(status, BaseModelStatus::Unloaded);
        assert!(!status.is_loaded());
        assert!(!status.is_transitioning());
    }
}
