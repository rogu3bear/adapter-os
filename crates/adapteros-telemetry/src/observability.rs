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

/// Payload for model load failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLoadFailedEvent {
    pub model_key: String,
    pub backend: String,
    pub error: String,
    pub timestamp_us: u64,
}

/// Payload for adapter load failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterLoadFailedEvent {
    pub adapter_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    pub error: String,
    pub timestamp_us: u64,
}

/// Payload for model cache eviction budget errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEvictionBudgetErrorEvent {
    pub model_key: String,
    pub backend: String,
    pub needed_bytes: u64,
    pub freed_bytes: u64,
    pub pinned_entries: usize,
    pub active_entries: usize,
    pub max_bytes: u64,
    pub timestamp_us: u64,
}

/// Build telemetry event for model load failures.
pub fn build_model_load_failed_event(
    identity: IdentityEnvelope,
    payload: ModelLoadFailedEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("model.load_failed".to_string()),
        LogLevel::Error,
        format!("model load failed for {}", payload.model_key),
        identity,
    )
    .component("model_cache".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Build telemetry event for adapter load failures.
pub fn build_adapter_load_failed_event(
    identity: IdentityEnvelope,
    payload: AdapterLoadFailedEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("adapter.load_failed".to_string()),
        LogLevel::Error,
        format!("adapter load failed for {}", payload.adapter_id),
        identity,
    )
    .component("adapter_loader".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Build telemetry event for eviction budget errors when pinned/active entries block eviction.
pub fn build_model_eviction_budget_error_event(
    identity: IdentityEnvelope,
    payload: ModelEvictionBudgetErrorEvent,
) -> Result<UnifiedTelemetryEvent, AosError> {
    TelemetryEventBuilder::new(
        EventType::Custom("model.eviction_budget_error".to_string()),
        LogLevel::Error,
        format!(
            "eviction budget blocked for {} (needed {} bytes)",
            payload.model_key, payload.needed_bytes
        ),
        identity,
    )
    .component("model_cache".to_string())
    .metadata(serde_json::to_value(payload)?)
    .build()
}

/// Convenience helper to create a ModelLoadFailedEvent with timestamp.
pub fn make_model_load_failed_payload(
    model_key: impl Into<String>,
    backend: impl Into<String>,
    error: impl Into<String>,
) -> ModelLoadFailedEvent {
    ModelLoadFailedEvent {
        model_key: model_key.into(),
        backend: backend.into(),
        error: error.into(),
        timestamp_us: now_us(),
    }
}

/// Convenience helper to create an AdapterLoadFailedEvent with timestamp.
pub fn make_adapter_load_failed_payload(
    adapter_id: impl Into<String>,
    backend: Option<String>,
    error: impl Into<String>,
) -> AdapterLoadFailedEvent {
    AdapterLoadFailedEvent {
        adapter_id: adapter_id.into(),
        backend,
        error: error.into(),
        timestamp_us: now_us(),
    }
}

/// Convenience helper to create a ModelEvictionBudgetErrorEvent with timestamp.
#[allow(clippy::too_many_arguments)]
pub fn make_model_eviction_budget_error_payload(
    model_key: impl Into<String>,
    backend: impl Into<String>,
    needed_bytes: u64,
    freed_bytes: u64,
    pinned_entries: usize,
    active_entries: usize,
    max_bytes: u64,
) -> ModelEvictionBudgetErrorEvent {
    ModelEvictionBudgetErrorEvent {
        model_key: model_key.into(),
        backend: backend.into(),
        needed_bytes,
        freed_bytes,
        pinned_entries,
        active_entries,
        max_bytes,
        timestamp_us: now_us(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;

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
                allowed_mask: Some(vec![true]),
                interval_id: None,
                policy_mask_digest: Some(*B3Hash::hash(b"mask").as_bytes()),
                policy_overrides_applied: Some(adapteros_types::routing::PolicyOverrideFlags {
                    allow_list: true,
                    deny_list: false,
                    trust_state: false,
                }),
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

    #[test]
    fn builds_model_load_failed_event() {
        let payload =
            make_model_load_failed_payload("model-123", "metal", "disk read failure".to_string());
        let event =
            build_model_load_failed_event(test_identity(), payload).expect("model load event");
        assert_eq!(event.event_type, "model.load_failed");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["model_key"], "model-123");
        assert_eq!(meta["backend"], "metal");
        assert_eq!(meta["error"], "disk read failure");
    }

    #[test]
    fn builds_adapter_load_failed_event() {
        let payload =
            make_adapter_load_failed_payload("adapter-9", Some("metal".into()), "oom".to_string());
        let event =
            build_adapter_load_failed_event(test_identity(), payload).expect("adapter event");
        assert_eq!(event.event_type, "adapter.load_failed");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["adapter_id"], "adapter-9");
        assert_eq!(meta["backend"], "metal");
        assert_eq!(meta["error"], "oom");
    }

    #[test]
    fn builds_model_eviction_budget_error_event() {
        let payload =
            make_model_eviction_budget_error_payload("model-abc", "mlx", 1000, 128, 2, 1, 2048);
        let event = build_model_eviction_budget_error_event(test_identity(), payload)
            .expect("budget event");
        assert_eq!(event.event_type, "model.eviction_budget_error");
        let meta = event.metadata.expect("metadata present");
        assert_eq!(meta["model_key"], "model-abc");
        assert_eq!(meta["backend"], "mlx");
        assert_eq!(meta["needed_bytes"], 1000);
        assert_eq!(meta["pinned_entries"], 2);
        assert_eq!(meta["active_entries"], 1);
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
