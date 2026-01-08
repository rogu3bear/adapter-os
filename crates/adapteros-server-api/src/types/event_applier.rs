//! API types for tenant event application.
//!
//! These types define the request/response schema for the tenant event endpoint,
//! which allows applying configuration events to a tenant's state.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to apply a single event to a tenant.
///
/// Events modify tenant state deterministically and are recorded in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApplyEventRequest {
    /// Event type identifier (e.g., "adapter.registered", "stack.created")
    pub event_type: String,
    /// Event metadata payload specific to the event type
    pub metadata: serde_json::Value,
    /// Optional event identity for idempotency and tracing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<serde_json::Value>,
}

/// Response after successfully applying an event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApplyEventResponse {
    /// Whether the event was applied successfully
    pub success: bool,
    /// The event type that was applied
    pub event_type: String,
    /// Timestamp when the event was applied (ISO 8601)
    pub applied_at: String,
    /// Identity label extracted from the event, if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_label: Option<String>,
}

/// Request to apply multiple events atomically.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApplyEventsBatchRequest {
    /// List of events to apply in order
    pub events: Vec<ApplyEventRequest>,
    /// If true, validate events without applying (dry run)
    #[serde(default)]
    pub dry_run: bool,
}

/// Response after applying a batch of events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApplyEventsBatchResponse {
    /// Whether all events were applied successfully
    pub success: bool,
    /// Number of events successfully applied
    pub applied_count: usize,
    /// Errors encountered during application, if any
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<EventApplicationError>,
}

/// Details about a failed event application.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventApplicationError {
    /// Index of the failed event in the batch
    pub index: usize,
    /// The event type that failed
    pub event_type: String,
    /// Error message describing the failure
    pub error: String,
}

/// Supported event types for reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TenantEventType {
    /// Register a new adapter
    #[serde(rename = "adapter.registered")]
    AdapterRegistered,
    /// Create a new adapter stack
    #[serde(rename = "stack.created")]
    StackCreated,
    /// Update a policy
    #[serde(rename = "policy.updated")]
    PolicyUpdated,
    /// Update tenant configuration
    #[serde(rename = "config.updated")]
    ConfigUpdated,
    /// Update plugin configuration
    #[serde(rename = "plugin.config.updated")]
    PluginConfigUpdated,
    /// Toggle a feature flag
    #[serde(rename = "feature.flag.toggled")]
    FeatureFlagToggled,
}
