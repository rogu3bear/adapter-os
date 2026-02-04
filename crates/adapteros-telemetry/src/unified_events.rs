//! Unified telemetry event schema for adapterOS
//!
//! Provides a centralized event schema that consolidates all telemetry
//! across the system into a single, canonical format.
//!
//! # Citations
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//! - AGENTS.md L132: "Telemetry via `TelemetryWriter::log(event_type, data)`"

use adapteros_core::{identity::IdentityEnvelope, AosError, B3Hash};
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
    pub hash: Option<String>,

    /// Sampling rate applied to this event
    pub sampling_rate: Option<f32>,
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

impl std::str::FromStr for LogLevel {
    type Err = AosError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(LogLevel::Debug),
            "info" => Ok(LogLevel::Info),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            "critical" => Ok(LogLevel::Critical),
            _ => Err(AosError::Validation(format!("Invalid log level: {}", s))),
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

impl TelemetryFilters {
    /// Construct filters with a required tenant identifier.
    pub fn with_tenant(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            ..Self::default()
        }
    }

    /// Validate that required tenant context is present.
    pub fn validate(&self) -> std::result::Result<(), AosError> {
        match self
            .tenant_id
            .as_ref()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
        {
            Some(_) => Ok(()),
            None => Err(AosError::Validation(
                "telemetry tenant_id is required for queries".to_string(),
            )),
        }
    }
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
                id: adapteros_core::ids::generate_id(
                    adapteros_core::ids::IdKind::Event,
                    "telemetry",
                ),
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
                hash: None,
                sampling_rate: None,
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
    pub fn build(mut self) -> Result<TelemetryEvent, AosError> {
        // Validate identity
        self.event
            .identity
            .validate()
            .map_err(|e| AosError::Validation(format!("Invalid identity envelope: {}", e)))?;

        // Compute event hash for integrity
        let event_data = serde_json::to_vec(&self.event).unwrap();
        self.event.hash = Some(B3Hash::hash(&event_data).to_string());

        Ok(self.event)
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
        .build()
        .unwrap();

        assert_eq!(event.event_type, "system.start");
        assert_eq!(event.message, "System started successfully");
        assert_eq!(event.component, Some("adapteros-core".to_string()));
        assert_eq!(event.user_id, Some("test-user".to_string()));
        assert_eq!(event.identity.tenant_id, "test-tenant");
        assert!(event.hash.is_some());
        assert!(event.sampling_rate.is_none());
    }

    #[test]
    fn telemetry_event_builder_rejects_empty_tenant() {
        let identity = IdentityEnvelope::new(
            "".to_string(),
            "domain".to_string(),
            "purpose".to_string(),
            "rev".to_string(),
        );
        let result = TelemetryEventBuilder::new(
            EventType::SystemError,
            LogLevel::Error,
            "missing tenant".to_string(),
            identity,
        )
        .build();

        assert!(result.is_err());
    }

    #[test]
    fn telemetry_event_builder_rejects_whitespace_tenant() {
        let identity = IdentityEnvelope::new(
            "   ".to_string(),
            "domain".to_string(),
            "purpose".to_string(),
            "rev".to_string(),
        );
        let result = TelemetryEventBuilder::new(
            EventType::SystemError,
            LogLevel::Error,
            "missing tenant".to_string(),
            identity,
        )
        .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_telemetry_filters_default() {
        let filters = TelemetryFilters::default();
        assert_eq!(filters.limit, Some(100));
        assert!(filters.tenant_id.is_none());
        assert!(filters.user_id.is_none());
        assert!(filters.validate().is_err());
    }

    #[test]
    fn telemetry_filters_require_tenant() {
        let filters = TelemetryFilters::with_tenant("tenant-123");
        assert!(filters.validate().is_ok());
        assert_eq!(filters.limit, Some(100));
    }
}
