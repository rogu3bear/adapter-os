//! Canonical observability event schemas and builders
//!
//! These helpers construct unified telemetry events for health lifecycle,
//! inference metrics, and routing/replay metadata so that control plane and
//! worker code can share a single shape.

use crate::{EventType, LogLevel, TelemetryEventBuilder, UnifiedTelemetryEvent};
use adapteros_core::{identity::IdentityEnvelope, AosError};
use adapteros_types::routing::RouterDecision;
use serde::{Deserialize, Serialize};

/// Lifecycle event kinds for worker health and adapter management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEventKind {
    WorkerRegistered,
    HealthStateChange,
    AdapterSwap,
    FatalError,
}

/// Health lifecycle telemetry payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthLifecycleEvent {
    pub worker_id: String,
    pub tenant_id: String,
    pub kind: HealthEventKind,
    /// Why the status changed (fatal error, restart, swap, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Previous status when applicable (e.g., draining -> serving).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<String>,
    /// New status when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_status: Option<String>,
    /// Adapter IDs involved in the transition (for swaps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Fatal error message when kind == FatalError.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// UTC timestamp in microseconds.
    pub timestamp_us: u64,
}

/// Inference metrics payload (control plane scoped).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetricsEvent {
    pub tenant_id: String,
    pub request_id: String,
    pub model_id: String,
    pub adapter_set: Vec<String>,
    pub seed_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<usize>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Routing telemetry payload that carries per-token RouterDecision metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTelemetryEvent {
    pub tenant_id: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Worker that handled the routed request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    /// Adapter identifiers that were selected during routing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adapter_ids: Vec<String>,
    /// Determinism mode (e.g., strict/debug).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Seed summary or hash used for routing/inference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_hash: Option<String>,
    /// Router decisions captured for this request.
    pub router_decisions: Vec<RouterDecision>,
    /// Chained router decision entries (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_decision_chain: Option<Vec<RouterDecisionChainEntry>>,
    /// Whether this request was a replay execution.
    pub is_replay: bool,
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default()
}

/// Build a unified telemetry event for health lifecycle changes.
pub fn build_health_event(
    identity: IdentityEnvelope,
    payload: HealthLifecycleEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("health.lifecycle".to_string()),
        LogLevel::Info,
        format!("health event {:?} for {}", payload.kind, payload.worker_id),
        identity,
    )
    .component("health".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Build a unified telemetry event for inference metrics.
pub fn build_inference_metrics_event(
    identity: IdentityEnvelope,
    payload: InferenceMetricsEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("inference.metrics".to_string()),
        LogLevel::Info,
        format!("inference metrics for {}", payload.request_id),
        identity,
    )
    .component("inference_core".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Build a unified telemetry event for routing decisions (including replay).
pub fn build_routing_event(
    identity: IdentityEnvelope,
    payload: RoutingTelemetryEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("routing.decision_chain".to_string()),
        LogLevel::Info,
        format!("routing telemetry for {}", payload.request_id),
        identity,
    )
    .component("router".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Auth event payload for login/refresh/logout/revoke flows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEvent {
    pub principal_id: String,
    pub tenant_id: String,
    /// Flow type: login | refresh | logout | revoke
    pub flow_type: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub timestamp_us: u64,
}

/// Build a unified telemetry event for auth flows.
pub fn build_auth_event(
    identity: IdentityEnvelope,
    payload: AuthEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("auth.event".to_string()),
        LogLevel::Info,
        format!(
            "auth {} for principal {}",
            payload.flow_type, payload.principal_id
        ),
        identity,
    )
    .component("auth".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Convenience helper to create an AuthEvent with timestamp.
pub fn make_auth_payload(
    principal_id: impl Into<String>,
    tenant_id: impl Into<String>,
    flow_type: impl Into<String>,
    success: bool,
    error_code: Option<String>,
) -> AuthEvent {
    AuthEvent {
        principal_id: principal_id.into(),
        tenant_id: tenant_id.into(),
        flow_type: flow_type.into(),
        success,
        error_code,
        timestamp_us: now_us(),
    }
}

/// Convenience helper to create a HealthLifecycleEvent with timestamp.
pub fn make_health_payload(
    worker_id: impl Into<String>,
    tenant_id: impl Into<String>,
    kind: HealthEventKind,
    previous_status: Option<String>,
    new_status: Option<String>,
    reason: Option<String>,
    adapters: Option<Vec<String>>,
    error: Option<String>,
) -> HealthLifecycleEvent {
    let resolved_reason = reason.or_else(|| match (&kind, &error) {
        (HealthEventKind::FatalError, Some(err)) => Some(err.clone()),
        (HealthEventKind::FatalError, None) => Some("fatal_error".to_string()),
        (HealthEventKind::AdapterSwap, _) => Some("adapter_swap".to_string()),
        (HealthEventKind::WorkerRegistered, _) => Some("worker_registered".to_string()),
        (HealthEventKind::HealthStateChange, _) => Some("status_change".to_string()),
    });

    HealthLifecycleEvent {
        worker_id: worker_id.into(),
        tenant_id: tenant_id.into(),
        kind,
        reason: resolved_reason,
        previous_status,
        new_status,
        adapters,
        error,
        timestamp_us: now_us(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity() -> IdentityEnvelope {
        IdentityEnvelope::new(
            "tenant-a".to_string(),
            "api".to_string(),
            "inference".to_string(),
            "1.0.0".to_string(),
        )
    }

    #[test]
    fn builds_health_event() {
        let payload = make_health_payload(
            "worker-1",
            "tenant-a",
            HealthEventKind::WorkerRegistered,
            None,
            Some("starting".to_string()),
            None,
            None,
            None,
        );
        let event = build_health_event(test_identity(), payload).expect("health event builds");
        assert_eq!(event.event_type, "health.lifecycle");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["worker_id"], "worker-1");
        assert_eq!(meta["tenant_id"], "tenant-a");
        assert_eq!(meta["reason"], "worker_registered");
    }

    #[test]
    fn builds_inference_metrics_event() {
        let payload = InferenceMetricsEvent {
            tenant_id: "tenant-a".into(),
            request_id: "req-123".into(),
            model_id: "model-x".into(),
            adapter_set: vec!["a1".into(), "a2".into()],
            seed_present: true,
            latency_ms: Some(42),
            input_tokens: Some(4),
            output_tokens: Some(6),
            success: true,
            error: None,
        };

        let event =
            build_inference_metrics_event(test_identity(), payload).expect("metrics event builds");
        assert_eq!(event.event_type, "inference.metrics");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["model_id"], "model-x");
        assert!(meta["seed_present"].as_bool().unwrap());
    }

    #[test]
    fn builds_routing_event() {
        let payload = RoutingTelemetryEvent {
            tenant_id: "tenant-a".into(),
            request_id: "req-456".into(),
            model_id: Some("model-y".into()),
            worker_id: Some("worker-1".into()),
            adapter_ids: vec!["a1".into()],
            determinism_mode: Some("strict".into()),
            seed_hash: Some("seed-abc".into()),
            router_decisions: vec![RouterDecision {
                step: 0,
                input_token_id: Some(1),
                candidate_adapters: vec![adapteros_types::routing::RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 1.0,
                    gate_q15: 100,
                }],
                entropy: 0.1,
                tau: 1.0,
                entropy_floor: 0.01,
                stack_hash: None,
                interval_id: None,
                policy_mask_digest: None,
                policy_overrides_applied: None,
            }],
            router_decision_chain: None,
            is_replay: false,
        };
        let event = build_routing_event(test_identity(), payload).expect("routing event builds");
        assert_eq!(event.event_type, "routing.decision_chain");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["router_decisions"][0]["step"], 0);
    }

    #[test]
    fn builds_auth_event() {
        let payload = make_auth_payload("user-1", "tenant-auth", "login", true, None::<String>);
        let event = build_auth_event(test_identity(), payload).expect("auth event builds");
        assert_eq!(event.event_type, "auth.event");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["principal_id"], "user-1");
        assert_eq!(meta["flow_type"], "login");
        assert_eq!(meta["success"], true);
    }
}

/// Chained router decision entry (per token), localized to avoid a dependency cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterDecisionChainEntry {
    pub step: usize,
    pub input_token_id: Option<u32>,
    pub adapter_indices: Vec<u16>,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<RouterDecisionHash>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub entry_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterDecisionHash {
    pub input_hash: String,
    pub output_hash: String,
    pub combined_hash: String,
    pub tau: f32,
    pub eps: f32,
    pub k: usize,
}
