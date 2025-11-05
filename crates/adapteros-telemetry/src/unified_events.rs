//! Unified telemetry event schema for AdapterOS
//!
//! Provides a centralized event schema that consolidates all telemetry
//! across the system into a single, canonical format.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CLAUDE.md L132: "Telemetry via `TelemetryWriter::log(event_type, data)`"

use adapteros_core::B3Hash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unified telemetry event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Unique event identifier
    pub id: String,

    /// Event timestamp in ISO 8601 format
    pub timestamp: DateTime<Utc>,

    /// Event type identifier
    pub event_type: String,

    /// Structured event kind namespace (e.g., "metrics.system")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<TelemetryEventKind>,

    /// Log level
    pub level: LogLevel,

    /// Human-readable message
    pub message: String,

    /// Component that generated the event
    pub component: Option<String>,

    /// Tenant ID (if applicable)
    pub tenant_id: Option<String>,

    /// User ID (if applicable)
    pub user_id: Option<String>,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,

    /// Trace ID for distributed tracing
    pub trace_id: Option<String>,

    /// Span ID for distributed tracing
    pub span_id: Option<String>,

    /// Event hash for integrity verification
    pub event_hash: Option<B3Hash>,
}

/// Canonical namespaces for telemetry events.
///
/// Wrapper around a string to guarantee that namespaces remain structured
/// without requiring a separate enum for every event family.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct TelemetryEventKind(String);

impl TelemetryEventKind {
    /// Create a new custom namespace.
    pub fn new<N: Into<String>>(namespace: N) -> Self {
        Self(namespace.into())
    }

    /// Return the namespace as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Canonical namespace for system-level metrics events.
    pub fn metrics_system() -> Self {
        Self("metrics.system".to_string())
    }
}

/// Log levels for telemetry events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
            LogLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Unified event types across the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    // System events
    SystemStart,
    SystemStop,
    SystemError,
    SystemWarning,
    /// Periodic system metrics snapshot
    SystemMetrics,

    // Adapter events
    AdapterLoaded,
    AdapterUnloaded,
    AdapterEvicted,
    AdapterPinned,
    AdapterUnpinned,
    AdapterActivated,
    AdapterDeactivated,

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
    PathTraversalBlocked,
    FileSizeLimitExceeded,
    JsonValidationFailed,

    // Shutdown/Cleanup events
    ShutdownStart,
    ShutdownCleanup,
    ShutdownComplete,
    ModelUnload,
    AdapterUnload,

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
            EventType::SystemMetrics => "system.metrics",
            EventType::AdapterLoaded => "adapter.loaded",
            EventType::AdapterUnloaded => "adapter.unloaded",
            EventType::AdapterEvicted => "adapter.evicted",
            EventType::AdapterPinned => "adapter.pinned",
            EventType::AdapterUnpinned => "adapter.unpinned",
            EventType::AdapterActivated => "adapter.activated",
            EventType::AdapterDeactivated => "adapter.deactivated",
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
            EventType::PathTraversalBlocked => "security.path_traversal_blocked",
            EventType::FileSizeLimitExceeded => "security.file_size_limit_exceeded",
            EventType::JsonValidationFailed => "security.json_validation_failed",
            EventType::ShutdownStart => "shutdown.start",
            EventType::ShutdownCleanup => "shutdown.cleanup",
            EventType::ShutdownComplete => "shutdown.complete",
            EventType::ModelUnload => "shutdown.model_unload",
            EventType::AdapterUnload => "shutdown.adapter_unload",
            EventType::PerformanceMetric => "performance.metric",
            EventType::PerformanceAlert => "performance.alert",
            EventType::PerformanceDegradation => "performance.degradation",
            EventType::Custom(s) => s,
        }
    }
}

/// Telemetry filters for event queries
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Telemetry event builder for constructing events
pub struct TelemetryEventBuilder {
    event: TelemetryEvent,
}

impl TelemetryEventBuilder {
    /// Create a new event builder
    pub fn new(event_type: EventType, level: LogLevel, message: String) -> Self {
        Self {
            event: TelemetryEvent {
                id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
                timestamp: Utc::now(),
                event_type: event_type.as_str().to_string(),
                kind: None,
                level,
                message,
                component: None,
                tenant_id: None,
                user_id: None,
                metadata: None,
                trace_id: None,
                span_id: None,
                event_hash: None,
            },
        }
    }

    /// Set the structured telemetry namespace.
    pub fn kind(mut self, kind: TelemetryEventKind) -> Self {
        self.event.kind = Some(kind);
        self
    }

    /// Set the component that generated the event
    pub fn component(mut self, component: String) -> Self {
        self.event.component = Some(component);
        self
    }

    /// Set the tenant ID
    pub fn tenant_id(mut self, tenant_id: String) -> Self {
        self.event.tenant_id = Some(tenant_id);
        self
    }

    /// Set the user ID
    pub fn user_id(mut self, user_id: String) -> Self {
        self.event.user_id = Some(user_id);
        self
    }

    /// Set additional metadata
    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.event.metadata = Some(metadata);
        self
    }

    /// Set trace ID for distributed tracing
    pub fn trace_id(mut self, trace_id: String) -> Self {
        self.event.trace_id = Some(trace_id);
        self
    }

    /// Set span ID for distributed tracing
    pub fn span_id(mut self, span_id: String) -> Self {
        self.event.span_id = Some(span_id);
        self
    }

    /// Build the final event
    pub fn build(mut self) -> TelemetryEvent {
        // Compute event hash for integrity verification
        if let Ok(event_json) = serde_json::to_string(&self.event) {
            let hash_bytes = blake3::hash(event_json.as_bytes());
            self.event.event_hash = Some(B3Hash::from_bytes(hash_bytes.into()));
        }
        self.event
    }
}

impl Default for TelemetryFilters {
    fn default() -> Self {
        Self {
            limit: Some(100),
            tenant_id: None,
            user_id: None,
            start_time: None,
            end_time: None,
            event_type: None,
            level: None,
            component: None,
            trace_id: None,
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
        assert_eq!(EventType::SystemMetrics.as_str(), "system.metrics");
        assert_eq!(
            EventType::Custom("custom.event".to_string()).as_str(),
            "custom.event"
        );
    }

    #[test]
    fn test_telemetry_event_builder() {
        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "System started successfully".to_string(),
        )
        .component("adapteros-core".to_string())
        .tenant_id("default".to_string())
        .build();

        assert_eq!(event.event_type, "system.start");
        assert_eq!(event.level, LogLevel::Info);
        assert_eq!(event.message, "System started successfully");
        assert_eq!(event.component, Some("adapteros-core".to_string()));
        assert_eq!(event.tenant_id, Some("default".to_string()));
        assert!(event.kind.is_none());
        assert!(event.event_hash.is_some());
    }

    #[test]
    fn test_telemetry_event_builder_with_kind() {
        let event = TelemetryEventBuilder::new(
            EventType::Custom("metrics.system".to_string()),
            LogLevel::Info,
            "Placeholder system metrics".to_string(),
        )
        .kind(TelemetryEventKind::metrics_system())
        .build();

        assert_eq!(event.event_type, "metrics.system");
        assert_eq!(
            event.kind.as_ref().map(TelemetryEventKind::as_str),
            Some("metrics.system")
        );
    }

    #[test]
    fn test_telemetry_filters_default() {
        let filters = TelemetryFilters::default();
        assert_eq!(filters.limit, Some(100));
        assert!(filters.tenant_id.is_none());
        assert!(filters.user_id.is_none());
    }
}
