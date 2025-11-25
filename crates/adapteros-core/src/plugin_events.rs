//! Plugin Event Payloads
//!
//! This module defines event payloads for the plugin event hook system.
//! Plugins can subscribe to specific event types and receive structured
//! event data when those events occur.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin event wrapper containing all possible event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum PluginEvent {
    /// Training job status changed
    TrainingJob(TrainingJobEvent),
    /// Adapter registered in the system
    AdapterRegistered(AdapterEvent),
    /// Adapter loaded into memory
    AdapterLoaded(AdapterEvent),
    /// Adapter unloaded from memory
    AdapterUnloaded(AdapterEvent),
    /// Audit event occurred
    Audit(AuditEvent),
    /// Periodic metrics tick
    MetricsTick(MetricsTickEvent),
    /// Inference request completed
    InferenceComplete(InferenceEvent),
    /// Policy violation detected
    PolicyViolation(PolicyViolationEvent),
}

/// Training job event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingJobEvent {
    /// Unique job identifier
    pub job_id: String,
    /// Job status (pending, running, completed, failed, cancelled)
    pub status: String,
    /// Progress percentage (0-100)
    pub progress_pct: Option<f64>,
    /// Current loss value
    pub loss: Option<f64>,
    /// Tokens processed per second
    pub tokens_per_sec: Option<f64>,
    /// Dataset identifier
    pub dataset_id: Option<String>,
    /// Adapter ID being trained
    pub adapter_id: Option<String>,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Adapter lifecycle event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEvent {
    /// Adapter identifier
    pub adapter_id: String,
    /// Action performed (registered, loaded, unloaded)
    pub action: String,
    /// Adapter hash (BLAKE3)
    pub hash: Option<String>,
    /// Adapter tier (tier_1, tier_2, tier_3)
    pub tier: Option<String>,
    /// Adapter rank (LoRA rank parameter)
    pub rank: Option<i32>,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// Lifecycle state (unloaded, cold, warm, hot, resident)
    pub lifecycle_state: Option<String>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Audit event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// User ID who performed the action
    pub user_id: String,
    /// Action performed (e.g., adapter.register, adapter.load)
    pub action: String,
    /// Resource type (e.g., adapter, tenant, policy)
    pub resource_type: String,
    /// Resource identifier
    pub resource_id: Option<String>,
    /// Action status (success, failure)
    pub status: String,
    /// IP address of the requester
    pub ip_address: Option<String>,
    /// User agent string
    pub user_agent: Option<String>,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Metrics tick event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsTickEvent {
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// CPU usage percentage (0-100)
    pub cpu_percent: Option<f64>,
    /// Memory usage in bytes
    pub memory_bytes: Option<u64>,
    /// Memory usage percentage (0-100)
    pub memory_percent: Option<f64>,
    /// Number of active adapters
    pub active_adapters: Option<u64>,
    /// Number of loaded adapters
    pub loaded_adapters: Option<u64>,
    /// Total inference requests since start
    pub inference_requests: Option<u64>,
    /// Average inference latency in milliseconds
    pub avg_latency_ms: Option<f64>,
    /// GPU memory usage in bytes (if applicable)
    pub gpu_memory_bytes: Option<u64>,
    /// GPU utilization percentage (0-100)
    pub gpu_percent: Option<f64>,
    /// Additional system metrics
    pub metrics: HashMap<String, serde_json::Value>,
}

/// Inference completion event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEvent {
    /// Request identifier
    pub request_id: String,
    /// Adapter IDs used in inference
    pub adapter_ids: Vec<String>,
    /// Adapter stack ID (if applicable)
    pub stack_id: Option<String>,
    /// Input prompt
    pub prompt: Option<String>,
    /// Generated output
    pub output: Option<String>,
    /// Total latency in milliseconds
    pub latency_ms: f64,
    /// Tokens generated
    pub tokens_generated: Option<u64>,
    /// Tokens per second
    pub tokens_per_sec: Option<f64>,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// Model identifier
    pub model: Option<String>,
    /// Whether streaming was used
    pub streaming: bool,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Policy violation event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolationEvent {
    /// Policy identifier (CPID)
    pub policy_id: String,
    /// Policy name
    pub policy_name: Option<String>,
    /// Resource type that violated the policy
    pub resource_type: String,
    /// Resource identifier
    pub resource_id: String,
    /// Violation severity (low, medium, high, critical)
    pub severity: String,
    /// Detailed violation description
    pub details: String,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// User ID (if applicable)
    pub user_id: Option<String>,
    /// Action taken (blocked, logged, alerted)
    pub action_taken: String,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginEvent {
    /// Returns the event type as a string
    pub fn event_type(&self) -> &'static str {
        match self {
            PluginEvent::TrainingJob(_) => "training_job",
            PluginEvent::AdapterRegistered(_) => "adapter_registered",
            PluginEvent::AdapterLoaded(_) => "adapter_loaded",
            PluginEvent::AdapterUnloaded(_) => "adapter_unloaded",
            PluginEvent::Audit(_) => "audit",
            PluginEvent::MetricsTick(_) => "metrics_tick",
            PluginEvent::InferenceComplete(_) => "inference_complete",
            PluginEvent::PolicyViolation(_) => "policy_violation",
        }
    }

    /// Returns the timestamp of the event
    pub fn timestamp(&self) -> &str {
        match self {
            PluginEvent::TrainingJob(e) => &e.timestamp,
            PluginEvent::AdapterRegistered(e) => &e.timestamp,
            PluginEvent::AdapterLoaded(e) => &e.timestamp,
            PluginEvent::AdapterUnloaded(e) => &e.timestamp,
            PluginEvent::Audit(e) => &e.timestamp,
            PluginEvent::MetricsTick(e) => &e.timestamp,
            PluginEvent::InferenceComplete(e) => &e.timestamp,
            PluginEvent::PolicyViolation(e) => &e.timestamp,
        }
    }

    /// Returns the tenant ID if available
    pub fn tenant_id(&self) -> Option<&str> {
        match self {
            PluginEvent::TrainingJob(e) => e.tenant_id.as_deref(),
            PluginEvent::AdapterRegistered(e) => e.tenant_id.as_deref(),
            PluginEvent::AdapterLoaded(e) => e.tenant_id.as_deref(),
            PluginEvent::AdapterUnloaded(e) => e.tenant_id.as_deref(),
            PluginEvent::Audit(e) => e.tenant_id.as_deref(),
            PluginEvent::MetricsTick(_) => None,
            PluginEvent::InferenceComplete(e) => e.tenant_id.as_deref(),
            PluginEvent::PolicyViolation(e) => e.tenant_id.as_deref(),
        }
    }
}
