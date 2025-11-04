//! Progress Service Trait for AdapterOS
//!
//! Defines the interface for progress tracking that can be implemented
//! by different services (server-api, orchestrator, etc.)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Progress event types across all operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProgressEventType {
    /// Adapter/Model operations (load, unload, etc.)
    Operation(String),
    /// Training job progress
    Training(String),
    /// Background tasks and maintenance
    Background(String),
    /// Custom application-specific progress
    Custom(String),
}

/// Progress status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProgressStatus {
    /// Operation is queued
    Queued,
    /// Operation is running
    Running,
    /// Operation completed successfully
    Completed,
    /// Operation failed
    Failed,
    /// Operation was cancelled
    Cancelled,
}

/// Trait for progress service implementations
#[async_trait::async_trait]
pub trait ProgressService: Send + Sync {
    /// Start tracking a new operation
    async fn start_operation(
        &self,
        operation_id: &str,
        tenant_id: &str,
        event_type: ProgressEventType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Complete an operation
    async fn complete_operation(
        &self,
        operation_id: &str,
        success: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// No-op progress service for when progress tracking is disabled
pub struct NoOpProgressService;

#[async_trait::async_trait]
impl ProgressService for NoOpProgressService {
    async fn start_operation(
        &self,
        _operation_id: &str,
        _tenant_id: &str,
        _event_type: ProgressEventType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn complete_operation(
        &self,
        _operation_id: &str,
        _success: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}