//! Unified telemetry event schema for AdapterOS
//!
//! Provides a centralized event schema that consolidates all telemetry
//! across the system into a single, canonical format.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - CLAUDE.md L132: "Telemetry via `TelemetryWriter::log(event_type, data)`"

use adapteros_core::{identity::IdentityEnvelope, B3Hash};
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

    /// Log level
    pub level: LogLevel,

    /// Human-readable message
    pub message: String,

    /// Component that generated the event
    pub component: Option<String>,

    /// Required identity envelope for all events
    pub identity: IdentityEnvelope,

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

/// Log levels for telemetry events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

/// Unified event types across the system
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    AdapterExpired,       // Citation: Agent G Stability Reinforcement Plan
    AdapterDeleteBlocked, // Citation: Agent G Stability Reinforcement Plan

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

    // Plugin events
    PluginStarted,
    PluginStopped,
    PluginDegraded,
    PluginPanic,
    PluginTimeout,
    PluginHealthCheck,
    PluginRestart,
    PluginDisabled,
    PluginEnabled,

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
            EventType::PluginStarted => "plugin.started",
            EventType::PluginStopped => "plugin.stopped",
            EventType::PluginDegraded => "plugin.degraded",
            EventType::PluginPanic => "plugin.panic",
            EventType::PluginTimeout => "plugin.timeout",
            EventType::PluginHealthCheck => "plugin.health_check",
            EventType::PluginRestart => "plugin.restart",
            EventType::PluginDisabled => "plugin.disabled",
            EventType::PluginEnabled => "plugin.enabled",
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
    /// Create a new event builder - requires identity envelope
    pub fn new(
        event_type: EventType,
        level: LogLevel,
        message: String,
        identity: IdentityEnvelope,
    ) -> Self {
        Self {
            event: TelemetryEvent {
                id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
                timestamp: Utc::now(),
                event_type: event_type.as_str().to_string(),
                level,
                message,
                component: None,
                identity,
                user_id: None,
                metadata: None,
                trace_id: None,
                span_id: None,
                event_hash: None,
            },
        }
    }

    /// Set the component that generated the event
    pub fn component(mut self, component: String) -> Self {
        self.event.component = Some(component);
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

    /// Build the final event and compute hash
    pub fn build(mut self) -> TelemetryEvent {
        // Validate identity
        if let Err(e) = self.event.identity.validate() {
            panic!("Invalid identity envelope: {}", e);
        }

        // Compute event hash for integrity
        let event_data = serde_json::to_vec(&self.event).unwrap();
        self.event.event_hash = Some(B3Hash::hash(&event_data));

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
        assert_eq!(
            EventType::Custom("custom.event".to_string()).as_str(),
            "custom.event"
        );
    }

    #[test]
    fn test_telemetry_event_builder() {
        let identity = IdentityEnvelope::new(
            "test-tenant".to_string(),
            "test-domain".to_string(),
            "test-purpose".to_string(),
            "test-rev".to_string(),
        );
        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "System started successfully".to_string(),
            identity,
        )
        .component("adapteros-core".to_string())
        .user_id("test-user".to_string())
        .build();

        assert_eq!(event.event_type, "SystemStart");
        assert_eq!(event.message, "System started successfully");
        assert_eq!(event.component, Some("adapteros-core".to_string()));
        assert_eq!(event.user_id, Some("test-user".to_string()));
        assert_eq!(event.identity.tenant_id, "test-tenant");
        assert!(event.event_hash.is_some());
    }

    #[test]
    fn test_telemetry_filters_default() {
        let filters = TelemetryFilters::default();
        assert_eq!(filters.limit, Some(100));
        assert!(filters.tenant_id.is_none());
        assert!(filters.user_id.is_none());
    }
}
