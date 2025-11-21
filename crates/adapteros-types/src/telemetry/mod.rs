//! Telemetry event types for AdapterOS
//!
//! This module provides canonical telemetry types including:
//! - TelemetryEvent - Core event structure
//! - TelemetryBundle - Collection of events for batching
//! - Event metadata and filtering types
//!
//! # Policy Compliance
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - All events require IdentityEnvelope for audit trail

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unified telemetry event structure
///
/// Core event type used across all AdapterOS components.
/// Supports distributed tracing, sampling, and integrity verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryEvent {
    /// Unique event identifier (UUIDv7)
    pub id: String,

    /// Event timestamp in ISO 8601 format
    pub timestamp: DateTime<Utc>,

    /// Event type identifier (e.g., "adapter.loaded", "inference.complete")
    pub event_type: String,

    /// Log level
    pub level: LogLevel,

    /// Human-readable message
    pub message: String,

    /// Component that generated the event
    pub component: Option<String>,

    /// Tenant ID for multi-tenancy
    pub tenant_id: String,

    /// Domain for the event
    pub domain: String,

    /// User ID (if applicable)
    pub user_id: Option<String>,

    /// Additional metadata as JSON
    pub metadata: Option<serde_json::Value>,

    /// Trace ID for distributed tracing
    pub trace_id: Option<String>,

    /// Span ID for distributed tracing
    pub span_id: Option<String>,

    /// Event hash for integrity verification (BLAKE3)
    pub hash: Option<String>,

    /// Sampling rate applied to this event (0.0-1.0)
    pub sampling_rate: Option<f32>,
}

/// Log levels for telemetry events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Detailed debugging information
    Trace,
    /// Debugging information
    Debug,
    /// General information
    Info,
    /// Warning conditions
    Warn,
    /// Error conditions
    Error,
    /// Critical conditions requiring immediate attention
    Critical,
}

/// Unified event types across the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // System events
    SystemStart,
    SystemStop,
    SystemError,
    SystemWarning,

    // Adapter events
    AdapterLoaded,
    AdapterUnloaded,
    AdapterEvicted,
    AdapterPinned,
    AdapterUnpinned,
    AdapterActivated,
    AdapterDeactivated,
    AdapterExpired,
    AdapterDeleteBlocked,

    // Inference events
    InferenceStart,
    InferenceComplete,
    InferenceError,
    InferenceTimeout,

    // Policy events
    PolicyViolation,
    PolicyEnforcement,
    PolicyCheck,
    PolicyUpdate,

    // Memory events
    MemoryPressure,
    MemoryEviction,
    MemoryAllocation,
    MemoryDeallocation,

    // Training events
    TrainingStart,
    TrainingComplete,
    TrainingError,
    TrainingProgress,

    // User events
    UserLogin,
    UserLogout,
    UserAction,
    UserError,

    // Router events
    RouterDecision,
    RouterCalibration,
    RouterError,

    // Telemetry events
    TelemetryBundleCreated,
    TelemetryBundleSigned,
    TelemetryBundleRotated,

    // Database events
    DatabaseQuery,
    DatabaseError,
    DatabaseMigration,

    // Network events
    NetworkRequest,
    NetworkResponse,
    NetworkError,

    // Security events
    SecurityViolation,
    SecurityCheck,
    SecurityAlert,

    // Performance events
    PerformanceMetric,
    PerformanceAlert,
    PerformanceDegradation,

    // Custom events
    Custom(String),
}

impl EventType {
    /// Convert to string representation
    pub fn as_str(&self) -> &str {
        match self {
            EventType::SystemStart => "system.start",
            EventType::SystemStop => "system.stop",
            EventType::SystemError => "system.error",
            EventType::SystemWarning => "system.warning",
            EventType::AdapterLoaded => "adapter.loaded",
            EventType::AdapterUnloaded => "adapter.unloaded",
            EventType::AdapterEvicted => "adapter.evicted",
            EventType::AdapterPinned => "adapter.pinned",
            EventType::AdapterUnpinned => "adapter.unpinned",
            EventType::AdapterActivated => "adapter.activated",
            EventType::AdapterDeactivated => "adapter.deactivated",
            EventType::AdapterExpired => "adapter.expired",
            EventType::AdapterDeleteBlocked => "adapter.delete.blocked",
            EventType::InferenceStart => "inference.start",
            EventType::InferenceComplete => "inference.complete",
            EventType::InferenceError => "inference.error",
            EventType::InferenceTimeout => "inference.timeout",
            EventType::PolicyViolation => "policy.violation",
            EventType::PolicyEnforcement => "policy.enforcement",
            EventType::PolicyCheck => "policy.check",
            EventType::PolicyUpdate => "policy.update",
            EventType::MemoryPressure => "memory.pressure",
            EventType::MemoryEviction => "memory.eviction",
            EventType::MemoryAllocation => "memory.allocation",
            EventType::MemoryDeallocation => "memory.deallocation",
            EventType::TrainingStart => "training.start",
            EventType::TrainingComplete => "training.complete",
            EventType::TrainingError => "training.error",
            EventType::TrainingProgress => "training.progress",
            EventType::UserLogin => "user.login",
            EventType::UserLogout => "user.logout",
            EventType::UserAction => "user.action",
            EventType::UserError => "user.error",
            EventType::RouterDecision => "router.decision",
            EventType::RouterCalibration => "router.calibration",
            EventType::RouterError => "router.error",
            EventType::TelemetryBundleCreated => "telemetry.bundle.created",
            EventType::TelemetryBundleSigned => "telemetry.bundle.signed",
            EventType::TelemetryBundleRotated => "telemetry.bundle.rotated",
            EventType::DatabaseQuery => "database.query",
            EventType::DatabaseError => "database.error",
            EventType::DatabaseMigration => "database.migration",
            EventType::NetworkRequest => "network.request",
            EventType::NetworkResponse => "network.response",
            EventType::NetworkError => "network.error",
            EventType::SecurityViolation => "security.violation",
            EventType::SecurityCheck => "security.check",
            EventType::SecurityAlert => "security.alert",
            EventType::PerformanceMetric => "performance.metric",
            EventType::PerformanceAlert => "performance.alert",
            EventType::PerformanceDegradation => "performance.degradation",
            EventType::Custom(s) => s,
        }
    }
}

/// Telemetry bundle for batching events
///
/// Used for efficient storage and transmission of multiple events.
/// Includes Merkle root for integrity verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryBundle {
    /// Bundle ID (UUIDv7)
    pub bundle_id: String,

    /// Tenant ID
    pub tenant_id: String,

    /// Bundle creation timestamp
    pub timestamp: DateTime<Utc>,

    /// Events in bundle
    pub events: Vec<TelemetryEvent>,

    /// Bundle hash (BLAKE3)
    pub bundle_hash: String,

    /// Merkle root of all event hashes
    pub merkle_root: String,

    /// Bundle signature (Ed25519)
    pub signature: Option<String>,

    /// Content-addressed path ID
    pub cpid: Option<String>,

    /// Event sequence range
    pub start_seq: Option<i64>,
    pub end_seq: Option<i64>,
}

/// Telemetry filters for event queries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryFilters {
    /// Maximum number of events to return
    pub limit: Option<usize>,

    /// Filter by tenant ID
    pub tenant_id: Option<String>,

    /// Filter by user ID
    pub user_id: Option<String>,

    /// Filter by start time
    pub start_time: Option<DateTime<Utc>>,

    /// Filter by end time
    pub end_time: Option<DateTime<Utc>>,

    /// Filter by event type
    pub event_type: Option<String>,

    /// Filter by log level
    pub level: Option<LogLevel>,

    /// Filter by component
    pub component: Option<String>,

    /// Filter by trace ID
    pub trace_id: Option<String>,
}

impl Default for TelemetryEvent {
    fn default() -> Self {
        Self {
            id: String::new(),
            timestamp: Utc::now(),
            event_type: String::new(),
            level: LogLevel::Info,
            message: String::new(),
            component: None,
            tenant_id: String::new(),
            domain: String::new(),
            user_id: None,
            metadata: None,
            trace_id: None,
            span_id: None,
            hash: None,
            sampling_rate: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_string_conversion() {
        assert_eq!(EventType::SystemStart.as_str(), "system.start");
        assert_eq!(EventType::AdapterLoaded.as_str(), "adapter.loaded");
        assert_eq!(
            EventType::Custom("custom.event".to_string()).as_str(),
            "custom.event"
        );
    }

    #[test]
    fn test_telemetry_event_default() {
        let event = TelemetryEvent::default();
        assert_eq!(event.level, LogLevel::Info);
        assert!(event.metadata.is_none());
    }

    #[test]
    fn test_telemetry_filters_default() {
        let filters = TelemetryFilters::default();
        assert!(filters.limit.is_none());
        assert!(filters.tenant_id.is_none());
    }

    #[test]
    fn test_telemetry_bundle_serialization() {
        let bundle = TelemetryBundle {
            bundle_id: "test-bundle".to_string(),
            tenant_id: "test-tenant".to_string(),
            timestamp: Utc::now(),
            events: vec![],
            bundle_hash: "b3:test".to_string(),
            merkle_root: "merkle:test".to_string(),
            signature: None,
            cpid: None,
            start_seq: Some(1),
            end_seq: Some(100),
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(json.contains("bundle_id"));
        assert!(json.contains("merkle_root"));
    }
}
