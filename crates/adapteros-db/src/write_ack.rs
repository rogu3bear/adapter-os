//! WriteAck - Dual-write acknowledgment tracking for deterministic consistency
//!
//! This module provides types and utilities for tracking the outcome of dual-write
//! operations (SQL + KV). It enables:
//! 1. Explicit tracking of which stores succeeded/failed
//! 2. Strict mode: fail-fast on any store failure
//! 3. Relaxed mode: continue with degraded status, enable repair
//! 4. Cross-check validation on read to detect drift

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// WriteStatus - Individual store outcome
// =============================================================================

/// Status of a write operation to a single store (SQL or KV).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WriteStatus {
    /// Write succeeded
    Ok,
    /// Write failed with error message
    Failed {
        /// Error message from the failed operation
        error: String,
    },
    /// Write is pending (in-progress)
    #[default]
    Pending,
    /// Store is unavailable (not configured or disabled)
    Unavailable,
}

impl WriteStatus {
    /// Check if this status represents success
    pub fn is_ok(&self) -> bool {
        matches!(self, WriteStatus::Ok)
    }

    /// Check if this status represents failure
    pub fn is_failed(&self) -> bool {
        matches!(self, WriteStatus::Failed { .. })
    }

    /// Check if this status represents a pending operation
    pub fn is_pending(&self) -> bool {
        matches!(self, WriteStatus::Pending)
    }
}

// =============================================================================
// WriteAck - Dual-write acknowledgment record
// =============================================================================

/// Acknowledgment record for a dual-write operation.
///
/// Tracks the outcome of both SQL and KV writes for a single operation,
/// enabling consistency checking and repair workflows.
///
/// # Invariants
///
/// 1. **Operation ID is unique**: Each dual-write gets a distinct operation_id
/// 2. **Entity tracking**: entity_type + entity_id identify what was written
/// 3. **Status completeness**: Both sql_status and kv_status reflect final state
/// 4. **Degraded marking**: If stores disagree, degraded flag indicates need for repair
///
/// # Example
///
/// ```ignore
/// use adapteros_db::write_ack::{WriteAck, WriteStatus};
///
/// let mut ack = WriteAck::new("adapter", "adapter-123");
///
/// // SQL write succeeds
/// ack.sql_status = WriteStatus::Ok;
///
/// // KV write fails
/// ack.kv_status = WriteStatus::Failed { error: "Connection refused".into() };
/// ack.mark_degraded("KV write failed");
///
/// // In strict mode, this would fail the operation
/// if ack.requires_rollback() {
///     // Rollback SQL write
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteAck {
    /// Unique identifier for this operation
    pub operation_id: Uuid,
    /// Type of entity being written (e.g., "adapter", "trace", "session")
    pub entity_type: String,
    /// ID of the entity being written
    pub entity_id: String,
    /// Status of the SQL write
    pub sql_status: WriteStatus,
    /// Status of the KV write
    pub kv_status: WriteStatus,
    /// Whether this write is in a degraded state (stores disagree)
    pub degraded: bool,
    /// Reason for degraded status, if any
    pub degraded_reason: Option<String>,
    /// Optional content hash for cross-check validation
    pub content_hash: Option<B3Hash>,
    /// Timestamp when the write was initiated
    pub created_at: DateTime<Utc>,
    /// Timestamp when the write was completed (or last updated)
    pub completed_at: Option<DateTime<Utc>>,
}

impl WriteAck {
    /// Create a new pending WriteAck for an operation.
    pub fn new(entity_type: impl Into<String>, entity_id: impl Into<String>) -> Self {
        Self {
            operation_id: Uuid::new_v4(),
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            sql_status: WriteStatus::Pending,
            kv_status: WriteStatus::Pending,
            degraded: false,
            degraded_reason: None,
            content_hash: None,
            created_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Create a new WriteAck with a specific operation ID.
    pub fn with_operation_id(
        operation_id: Uuid,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
    ) -> Self {
        Self {
            operation_id,
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            sql_status: WriteStatus::Pending,
            kv_status: WriteStatus::Pending,
            degraded: false,
            degraded_reason: None,
            content_hash: None,
            created_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Check if both writes succeeded
    pub fn is_fully_ok(&self) -> bool {
        self.sql_status.is_ok()
            && (self.kv_status.is_ok() || matches!(self.kv_status, WriteStatus::Unavailable))
    }

    /// Check if SQL write succeeded
    pub fn sql_ok(&self) -> bool {
        self.sql_status.is_ok()
    }

    /// Check if KV write succeeded
    pub fn kv_ok(&self) -> bool {
        self.kv_status.is_ok()
    }

    /// Check if any write failed (not just pending or unavailable)
    pub fn has_failure(&self) -> bool {
        self.sql_status.is_failed() || self.kv_status.is_failed()
    }

    /// Check if a rollback is required (for strict mode)
    ///
    /// Returns true if KV failed but SQL succeeded, indicating SQL needs rollback.
    pub fn requires_rollback(&self) -> bool {
        self.sql_status.is_ok() && self.kv_status.is_failed()
    }

    /// Mark the write as degraded with a reason.
    pub fn mark_degraded(&mut self, reason: impl Into<String>) {
        self.degraded = true;
        self.degraded_reason = Some(reason.into());
    }

    /// Mark both writes as complete and set completion timestamp.
    pub fn complete(&mut self) {
        self.completed_at = Some(Utc::now());
    }

    /// Set the content hash for cross-check validation.
    pub fn with_content_hash(mut self, hash: B3Hash) -> Self {
        self.content_hash = Some(hash);
        self
    }

    /// Validate that this WriteAck represents a successful operation.
    ///
    /// In strict mode, returns Err if any write failed.
    /// In best-effort mode, returns Ok even if KV failed (but degraded is set).
    pub fn validate_strict(&self) -> Result<()> {
        if self.sql_status.is_failed() {
            return Err(AosError::Database(format!(
                "SQL write failed for {}/{}: {:?}",
                self.entity_type, self.entity_id, self.sql_status
            )));
        }
        if self.kv_status.is_failed() {
            return Err(AosError::DualWriteInconsistency {
                entity_type: self.entity_type.clone(),
                entity_id: self.entity_id.clone(),
                reason: format!("KV write failed: {:?}", self.kv_status),
            });
        }
        Ok(())
    }

    /// Validate in best-effort mode.
    ///
    /// Only returns Err if SQL failed. KV failures result in degraded state.
    pub fn validate_best_effort(&mut self) -> Result<()> {
        if self.sql_status.is_failed() {
            return Err(AosError::Database(format!(
                "SQL write failed for {}/{}: {:?}",
                self.entity_type, self.entity_id, self.sql_status
            )));
        }
        if self.kv_status.is_failed() {
            self.mark_degraded(format!("KV write failed: {:?}", self.kv_status));
        }
        Ok(())
    }
}

impl std::fmt::Display for WriteAck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WriteAck({}/{}: sql={:?}, kv={:?}{})",
            self.entity_type,
            self.entity_id,
            self.sql_status,
            self.kv_status,
            if self.degraded { " [DEGRADED]" } else { "" }
        )
    }
}

// =============================================================================
// WriteAckStore - Trait for persisting WriteAck records
// =============================================================================

/// Trait for storing and retrieving WriteAck records.
///
/// Implementations can store acks in SQL, KV, or in-memory for testing.
#[async_trait::async_trait]
pub trait WriteAckStore: Send + Sync {
    /// Store a WriteAck record
    async fn store_ack(&self, ack: &WriteAck) -> Result<()>;

    /// Retrieve a WriteAck by operation ID
    async fn get_ack(&self, operation_id: Uuid) -> Result<Option<WriteAck>>;

    /// List degraded WriteAcks for repair queue
    async fn list_degraded(&self, limit: usize) -> Result<Vec<WriteAck>>;

    /// Delete a WriteAck after successful repair
    async fn delete_ack(&self, operation_id: Uuid) -> Result<()>;
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_status_is_ok() {
        assert!(WriteStatus::Ok.is_ok());
        assert!(!WriteStatus::Failed {
            error: "test".into()
        }
        .is_ok());
        assert!(!WriteStatus::Pending.is_ok());
    }

    #[test]
    fn write_ack_new() {
        let ack = WriteAck::new("adapter", "test-123");
        assert_eq!(ack.entity_type, "adapter");
        assert_eq!(ack.entity_id, "test-123");
        assert!(ack.sql_status.is_pending());
        assert!(ack.kv_status.is_pending());
        assert!(!ack.degraded);
    }

    #[test]
    fn write_ack_fully_ok() {
        let mut ack = WriteAck::new("adapter", "test-123");
        assert!(!ack.is_fully_ok());

        ack.sql_status = WriteStatus::Ok;
        assert!(!ack.is_fully_ok()); // KV still pending

        ack.kv_status = WriteStatus::Ok;
        assert!(ack.is_fully_ok());
    }

    #[test]
    fn write_ack_degraded_when_kv_unavailable() {
        let mut ack = WriteAck::new("adapter", "test-123");
        ack.sql_status = WriteStatus::Ok;
        ack.kv_status = WriteStatus::Unavailable;

        // Should be considered OK (KV not required when unavailable)
        assert!(ack.is_fully_ok());
    }

    #[test]
    fn write_ack_requires_rollback() {
        let mut ack = WriteAck::new("adapter", "test-123");
        ack.sql_status = WriteStatus::Ok;
        ack.kv_status = WriteStatus::Failed {
            error: "test".into(),
        };

        assert!(ack.requires_rollback());
    }

    #[test]
    fn write_ack_validate_strict() {
        let mut ack = WriteAck::new("adapter", "test-123");
        ack.sql_status = WriteStatus::Ok;
        ack.kv_status = WriteStatus::Failed {
            error: "test".into(),
        };

        assert!(ack.validate_strict().is_err());
    }

    #[test]
    fn write_ack_validate_best_effort() {
        let mut ack = WriteAck::new("adapter", "test-123");
        ack.sql_status = WriteStatus::Ok;
        ack.kv_status = WriteStatus::Failed {
            error: "test".into(),
        };

        assert!(ack.validate_best_effort().is_ok());
        assert!(ack.degraded);
    }

    #[test]
    fn write_ack_serialization_roundtrip() {
        let mut ack = WriteAck::new("adapter", "test-123");
        ack.sql_status = WriteStatus::Ok;
        ack.kv_status = WriteStatus::Failed {
            error: "connection refused".into(),
        };
        ack.mark_degraded("KV unavailable");

        let json = serde_json::to_string(&ack).expect("serialize");
        let deserialized: WriteAck = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.entity_type, ack.entity_type);
        assert_eq!(deserialized.entity_id, ack.entity_id);
        assert!(deserialized.degraded);
    }
}
