//! Signal Protocol Implementation
//!
//! Implements Section 5.1 "Signal Protocol" from LLM Interface Specification.
//! Provides bidirectional communication between LLM and runtime during inference.
//!
//! Citation: docs/llm-interface-specification.md §5.1
//!
//! This module enables lightweight, low-level notifications from the LLM to the
//! runtime, allowing dynamic adapter routing, evidence retrieval, and policy
//! enforcement during inference.

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Signal types as defined in Specification §5.1.1
///
/// These signals enable the LLM to communicate state and intent to the runtime
/// during inference, allowing for dynamic adaptation and policy enforcement.
///
/// Citation: docs/llm-interface-specification.md §5.1.1
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    // Adapter routing signals (Specification §5.3)
    /// Request specific adapters for next generation step
    /// Citation: §5.3.1
    AdapterRequest,

    /// Notify runtime that adapter is being used
    /// Citation: §5.3.2
    AdapterActivate,

    /// Release adapter resources
    AdapterRelease,

    // Evidence signals (Specification §5.4)
    /// Indicate that evidence is required for query
    EvidenceRequired,

    /// Log evidence citation in generated text
    /// Citation: §5.4.1
    EvidenceCite,

    /// Signal that available evidence is insufficient to answer
    /// Citation: §5.4.2
    EvidenceInsufficient,

    // Policy signals (Specification §5.5)
    /// Query policy constraints before generating response
    PolicyCheck,

    /// Report policy violation
    PolicyViolation,

    /// Signal intent to refuse query before generating response
    /// Citation: §5.5.1
    RefusalIntent,

    // State signals (Specification §5.2)
    /// Store information in session context
    ContextSave,

    /// Retrieve previously stored context
    ContextLoad,

    /// Request checkpoint creation
    CheckpointRequest,

    // Performance signals (Specification §5.2)
    /// Warn about approaching token budget limit
    TokenBudgetWarning,

    /// Warn about high latency
    LatencyWarning,

    // Error signals (Specification §5.2)
    /// Report error occurrence
    ErrorOccurred,

    /// Request retry of operation
    RetryRequested,

    // Memory signals (Specification §8.2)
    /// Report memory pressure
    MemoryPressure,

    // Contact signals (NEW - Contacts & Streams Implementation)
    /// Contact discovered during inference
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.3
    ContactDiscovered,

    /// Contact metadata updated during inference
    ContactUpdated,

    /// Contact interaction logged (mentioned, invoked, queried)
    ContactInteraction,

    // Training stream signals (NEW - Contacts & Streams Implementation)
    /// Adapter state transition logged
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.2
    AdapterStateTransition,

    /// Training progress update
    TrainingProgress,

    /// Profiler metrics snapshot
    ProfilerMetrics,

    /// Adapter promoted to higher tier
    AdapterPromoted,

    /// Adapter demoted to lower tier
    AdapterDemoted,

    /// Router K value reduced due to memory pressure
    KReduced,

    // Discovery stream signals (NEW - Contacts & Streams Implementation)
    /// Repository scan initiated
    /// Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.2
    RepoScanStarted,

    /// Repository scan progress update
    RepoScanProgress,

    /// Symbol indexed during scan
    SymbolIndexed,

    /// Framework detected during scan
    FrameworkDetected,

    /// Test map updated
    TestMapUpdated,

    /// Repository scan completed
    RepoScanCompleted,
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalType::AdapterRequest => write!(f, "adapter.request"),
            SignalType::AdapterActivate => write!(f, "adapter.activate"),
            SignalType::AdapterRelease => write!(f, "adapter.release"),
            SignalType::EvidenceRequired => write!(f, "evidence.required"),
            SignalType::EvidenceCite => write!(f, "evidence.cite"),
            SignalType::EvidenceInsufficient => write!(f, "evidence.insufficient"),
            SignalType::PolicyCheck => write!(f, "policy.check"),
            SignalType::PolicyViolation => write!(f, "policy.violation"),
            SignalType::RefusalIntent => write!(f, "refusal.intent"),
            SignalType::ContextSave => write!(f, "context.save"),
            SignalType::ContextLoad => write!(f, "context.load"),
            SignalType::CheckpointRequest => write!(f, "checkpoint.request"),
            SignalType::TokenBudgetWarning => write!(f, "token.budget.warning"),
            SignalType::LatencyWarning => write!(f, "latency.warning"),
            SignalType::ErrorOccurred => write!(f, "error.occurred"),
            SignalType::RetryRequested => write!(f, "retry.requested"),
            SignalType::MemoryPressure => write!(f, "memory.pressure"),
            // Contact signals
            SignalType::ContactDiscovered => write!(f, "contact.discovered"),
            SignalType::ContactUpdated => write!(f, "contact.updated"),
            SignalType::ContactInteraction => write!(f, "contact.interaction"),
            // Training stream signals
            SignalType::AdapterStateTransition => write!(f, "adapter.state_transition"),
            SignalType::TrainingProgress => write!(f, "training.progress"),
            SignalType::ProfilerMetrics => write!(f, "profiler.metrics"),
            SignalType::AdapterPromoted => write!(f, "adapter.promoted"),
            SignalType::AdapterDemoted => write!(f, "adapter.demoted"),
            SignalType::KReduced => write!(f, "k.reduced"),
            // Discovery stream signals
            SignalType::RepoScanStarted => write!(f, "repo_scan.started"),
            SignalType::RepoScanProgress => write!(f, "repo_scan.progress"),
            SignalType::SymbolIndexed => write!(f, "symbol.indexed"),
            SignalType::FrameworkDetected => write!(f, "framework.detected"),
            SignalType::TestMapUpdated => write!(f, "test_map.updated"),
            SignalType::RepoScanCompleted => write!(f, "repo_scan.completed"),
        }
    }
}

/// Signal priorities as defined in Specification §5.1.2
///
/// Priority determines processing order and logging behavior.
/// Critical signals are always logged at 100% sampling rate.
///
/// Citation: docs/llm-interface-specification.md §5.1.2
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SignalPriority {
    /// Low priority - advisory signals, may be sampled
    Low,

    /// Normal priority - standard operational signals
    Normal,

    /// High priority - important state changes, logged at 100%
    High,

    /// Critical priority - errors, violations, always logged
    Critical,
}

/// Signal interface as defined in Specification §5.1.2
///
/// Represents a lightweight notification from the LLM to the runtime.
/// Signals carry minimal state and enable dynamic adaptation during inference.
///
/// Citation: docs/llm-interface-specification.md §5.1.2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Type of signal
    #[serde(rename = "type")]
    pub signal_type: SignalType,

    /// Monotonic timestamp in nanoseconds for determinism
    /// Uses UNIX_EPOCH for consistency with telemetry
    pub timestamp: u128,

    /// Signal payload with type-specific data
    pub payload: HashMap<String, serde_json::Value>,

    /// Signal priority for processing and logging
    pub priority: SignalPriority,

    /// Trace ID for correlation with inference traces
    /// Links signal to telemetry bundle for audit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl Signal {
    /// Create a new signal with current timestamp
    pub fn new(signal_type: SignalType, priority: SignalPriority) -> Self {
        Self {
            signal_type,
            timestamp: Self::current_timestamp(),
            payload: HashMap::new(),
            priority,
            trace_id: None,
        }
    }

    /// Create signal with payload
    pub fn with_payload(
        signal_type: SignalType,
        priority: SignalPriority,
        payload: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            signal_type,
            timestamp: Self::current_timestamp(),
            payload,
            priority,
            trace_id: None,
        }
    }

    /// Set trace ID for correlation
    pub fn with_trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    /// Get current monotonic timestamp in nanoseconds
    fn current_timestamp() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_nanos()
    }

    /// Check if signal should be logged at 100% sampling rate
    /// Critical and high priority signals are always logged per Telemetry Ruleset #9
    pub fn requires_full_logging(&self) -> bool {
        matches!(
            self.priority,
            SignalPriority::High | SignalPriority::Critical
        )
    }
}

/// Signal handler trait for runtime components
///
/// Handlers process specific signal types and execute appropriate actions.
/// All handlers must be Send + Sync for use in async context.
///
/// Citation: docs/llm-interface-specification.md §5.1.2
#[async_trait::async_trait]
pub trait SignalHandler: Send + Sync {
    /// Handle a signal
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()>;

    /// Get signal types this handler can process
    fn signal_types(&self) -> Vec<SignalType>;
}

/// Signal dispatcher for routing signals to handlers
///
/// The dispatcher maintains a registry of handlers and routes incoming
/// signals to the appropriate handler based on signal type.
///
/// Citation: docs/llm-interface-specification.md §5.1.2
pub struct SignalDispatcher {
    handlers: HashMap<SignalType, Vec<Box<dyn SignalHandler>>>,
}

impl SignalDispatcher {
    /// Create a new signal dispatcher
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a signal handler
    ///
    /// Each handler instance can only be registered once.
    /// For multiple signal types, the handler must implement all of them.
    pub fn register_handler<H: SignalHandler + 'static>(&mut self, handler: H) {
        let signal_types = handler.signal_types();
        let boxed_handler: Box<dyn SignalHandler> = Box::new(handler);

        // For now, register to first signal type only
        // TODO: Support multiple signal types per handler instance
        if let Some(first_type) = signal_types.first() {
            self.handlers
                .entry(first_type.clone())
                .or_default()
                .push(boxed_handler);
        }
    }

    /// Dispatch a signal to registered handlers
    ///
    /// If no handlers are registered for a signal type, the signal is logged
    /// but not considered an error (following best practices for extensibility).
    pub async fn dispatch(&mut self, signal: &Signal) -> Result<()> {
        if let Some(handlers) = self.handlers.get_mut(&signal.signal_type) {
            for handler in handlers.iter_mut() {
                handler.handle_signal(signal).await?;
            }
        } else {
            // Log unhandled signal for debugging (follows telemetry patterns)
            tracing::debug!(
                "Unhandled signal: {} (priority: {:?})",
                signal.signal_type,
                signal.priority
            );
        }
        Ok(())
    }

    /// Get count of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.values().map(|v| v.len()).sum()
    }
}

impl Default for SignalDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating signals with fluent API
pub struct SignalBuilder {
    signal_type: SignalType,
    priority: SignalPriority,
    payload: HashMap<String, serde_json::Value>,
    trace_id: Option<String>,
}

impl SignalBuilder {
    /// Create a new signal builder
    pub fn new(signal_type: SignalType) -> Self {
        Self {
            signal_type,
            priority: SignalPriority::Normal,
            payload: HashMap::new(),
            trace_id: None,
        }
    }

    /// Set signal priority
    pub fn priority(mut self, priority: SignalPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Add payload field
    pub fn with_field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.payload.insert(key.into(), value);
        self
    }

    /// Set trace ID
    pub fn trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    /// Build the signal
    pub fn build(self) -> Signal {
        let mut signal = Signal::with_payload(self.signal_type, self.priority, self.payload);
        signal.trace_id = self.trace_id;
        signal
    }
}

/// Worker signal types for API communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct WorkerSignal {
    /// Signal type identifier
    pub signal_type: String,
    /// Timestamp when signal was created
    pub timestamp: u128,
    /// Signal payload data
    pub payload: serde_json::Value,
}

impl WorkerSignal {
    /// Create a new worker signal
    pub fn new(signal_type: String, payload: serde_json::Value) -> Self {
        Self {
            signal_type,
            timestamp: chrono::Utc::now().timestamp_millis() as u128,
            payload,
        }
    }

    /// Create an adapter loaded signal
    pub fn adapter_loaded(adapter_id: String) -> Self {
        Self::new(
            "adapter_loaded".to_string(),
            serde_json::json!({ "adapter_id": adapter_id }),
        )
    }

    /// Create an adapter unloaded signal
    pub fn adapter_unloaded(adapter_id: String) -> Self {
        Self::new(
            "adapter_unloaded".to_string(),
            serde_json::json!({ "adapter_id": adapter_id }),
        )
    }

    /// Create a warmup complete signal
    pub fn warmup_complete() -> Self {
        Self::new(
            "warmup_complete".to_string(),
            serde_json::json!({ "status": "complete" }),
        )
    }

    /// Create a health status signal
    pub fn health_status(healthy: bool, message: String) -> Self {
        Self::new(
            "health_status".to_string(),
            serde_json::json!({ "healthy": healthy, "message": message }),
        )
    }

    /// Create an inference complete signal
    pub fn inference_complete(request_id: String) -> Self {
        Self::new(
            "inference_complete".to_string(),
            serde_json::json!({ "request_id": request_id }),
        )
    }

    /// Create an error signal
    pub fn error(message: String) -> Self {
        Self::new(
            "error".to_string(),
            serde_json::json!({ "message": message }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_creation() {
        let signal = Signal::new(SignalType::AdapterRequest, SignalPriority::Normal);
        assert_eq!(signal.signal_type, SignalType::AdapterRequest);
        assert_eq!(signal.priority, SignalPriority::Normal);
        assert!(signal.timestamp > 0);
    }

    #[test]
    fn test_signal_builder() {
        let signal = SignalBuilder::new(SignalType::EvidenceInsufficient)
            .priority(SignalPriority::High)
            .with_field("query", "test query".into())
            .with_field("retrievedSpans", 2.into())
            .trace_id("trace-123")
            .build();

        assert_eq!(signal.signal_type, SignalType::EvidenceInsufficient);
        assert_eq!(signal.priority, SignalPriority::High);
        assert_eq!(signal.payload.len(), 2);
        assert_eq!(signal.trace_id, Some("trace-123".to_string()));
    }

    #[test]
    fn test_signal_full_logging_requirement() {
        let low_signal = Signal::new(SignalType::AdapterActivate, SignalPriority::Low);
        assert!(!low_signal.requires_full_logging());

        let high_signal = Signal::new(SignalType::PolicyViolation, SignalPriority::High);
        assert!(high_signal.requires_full_logging());

        let critical_signal = Signal::new(SignalType::ErrorOccurred, SignalPriority::Critical);
        assert!(critical_signal.requires_full_logging());
    }

    #[test]
    fn test_signal_serialization() {
        let signal = SignalBuilder::new(SignalType::AdapterRequest)
            .with_field("adapterId", "test-adapter".into())
            .build();

        let json = serde_json::to_string(&signal).expect("Test serialization should succeed");
        let deserialized: Signal =
            serde_json::from_str(&json).expect("Test deserialization should succeed");

        assert_eq!(deserialized.signal_type, signal.signal_type);
        assert_eq!(deserialized.priority, signal.priority);
    }

    #[tokio::test]
    async fn test_signal_dispatcher() {
        struct TestHandler;

        #[async_trait::async_trait]
        impl SignalHandler for TestHandler {
            async fn handle_signal(&mut self, _signal: &Signal) -> Result<()> {
                Ok(())
            }

            fn signal_types(&self) -> Vec<SignalType> {
                vec![SignalType::AdapterRequest]
            }
        }

        let mut dispatcher = SignalDispatcher::new();
        dispatcher.register_handler(TestHandler);

        assert_eq!(dispatcher.handler_count(), 1);

        let signal = Signal::new(SignalType::AdapterRequest, SignalPriority::Normal);
        dispatcher
            .dispatch(&signal)
            .await
            .expect("Test signal dispatch should succeed");
    }
}
