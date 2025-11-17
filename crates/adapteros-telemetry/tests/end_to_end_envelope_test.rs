//! End-to-end golden test for identity envelope in real subsystem events
//!
//! PRD 1 Requirement: "emit telemetry from router, lifecycle, plugin; assert envelope is present and correct in serialized JSON"

use adapteros_core::{Domain, IdentityEnvelope, Purpose, B3Hash};
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};

#[test]
fn test_barrier_event_contains_envelope() {
    // Simulate a barrier event emission (matching multi_agent.rs pattern)
    let identity = IdentityEnvelope::new_with_process_revision(
        "test-tenant".to_string(),
        Domain::Worker,
        Purpose::Maintenance,
    );

    let event = TelemetryEventBuilder::new(
        EventType::Custom("barrier.wait_start".to_string()),
        LogLevel::Debug,
        "Agent entering barrier".to_string(),
        identity,
    )
    .component("adapteros-deterministic-exec".to_string())
    .metadata(serde_json::json!({
        "agent_id": "agent-1",
        "tick": 42,
        "generation": 1,
    }))
    .build();

    // Serialize and verify envelope is present
    let json = serde_json::to_value(&event).unwrap();

    assert!(json.get("identity").is_some(), "Event must have identity field");

    let identity_json = &json["identity"];
    assert_eq!(identity_json["tenant_id"], "test-tenant");
    assert_eq!(identity_json["domain"], "Worker");
    assert_eq!(identity_json["purpose"], "Maintenance");
    assert!(identity_json.get("revision").is_some());

    // Verify event metadata is intact
    assert_eq!(json["event_type"], "barrier.wait_start");
    assert_eq!(json["level"], "Debug");
    assert_eq!(json["component"], "adapteros-deterministic-exec");

    let metadata = json["metadata"].as_object().unwrap();
    assert_eq!(metadata["agent_id"], "agent-1");
    assert_eq!(metadata["tick"], 42);
}

#[test]
fn test_ledger_event_contains_envelope() {
    // Simulate a ledger event emission (matching global_ledger.rs pattern)
    let identity = IdentityEnvelope::new_with_process_revision(
        "tenant-production".to_string(),
        Domain::Worker,
        Purpose::Audit,
    );

    let event = TelemetryEventBuilder::new(
        EventType::Custom("tick_ledger.entry".to_string()),
        LogLevel::Debug,
        "Tick ledger entry recorded".to_string(),
        identity,
    )
    .component("adapteros-deterministic-exec".to_string())
    .metadata(serde_json::json!({
        "tick": 100,
        "tenant_id": "tenant-production",
        "host_id": "host-1",
        "entry_hash": "abc123",
    }))
    .build();

    let json = serde_json::to_value(&event).unwrap();

    // Golden assertions for envelope
    assert!(json.get("identity").is_some());
    let identity_json = &json["identity"];
    assert_eq!(identity_json["tenant_id"], "tenant-production");
    assert_eq!(identity_json["domain"], "Worker");
    assert_eq!(identity_json["purpose"], "Audit");

    // Verify event structure
    assert_eq!(json["event_type"], "tick_ledger.entry");
    assert_eq!(json["component"], "adapteros-deterministic-exec");
}

#[test]
fn test_lifecycle_event_contains_envelope() {
    // Simulate a lifecycle event
    let identity = IdentityEnvelope::new_with_process_revision(
        "tenant-dev".to_string(),
        Domain::Lifecycle,
        Purpose::Maintenance,
    );

    let event = TelemetryEventBuilder::new(
        EventType::AdapterEvicted,
        LogLevel::Info,
        "Adapter evicted due to memory pressure".to_string(),
        identity,
    )
    .component("adapteros-lora-lifecycle".to_string())
    .metadata(serde_json::json!({
        "adapter_id": "adapter-123",
        "tier": "cold",
        "memory_mb": 512,
    }))
    .build();

    let json = serde_json::to_value(&event).unwrap();

    assert!(json.get("identity").is_some());
    let identity_json = &json["identity"];
    assert_eq!(identity_json["tenant_id"], "tenant-dev");
    assert_eq!(identity_json["domain"], "Lifecycle");
    assert_eq!(identity_json["purpose"], "Maintenance");

    assert_eq!(json["event_type"], "adapter.evicted");
}

#[test]
fn test_router_event_contains_envelope() {
    // Simulate a router decision event
    let identity = IdentityEnvelope::new_with_process_revision(
        "tenant-inference".to_string(),
        Domain::Router,
        Purpose::Inference,
    );

    let event = TelemetryEventBuilder::new(
        EventType::RouterDecision,
        LogLevel::Info,
        "Router selected adapters for inference".to_string(),
        identity,
    )
    .component("adapteros-lora-router".to_string())
    .metadata(serde_json::json!({
        "selected_adapters": ["adapter-1", "adapter-2"],
        "k": 2,
        "gates": [0.8, 0.6],
    }))
    .build();

    let json = serde_json::to_value(&event).unwrap();

    assert!(json.get("identity").is_some());
    let identity_json = &json["identity"];
    assert_eq!(identity_json["tenant_id"], "tenant-inference");
    assert_eq!(identity_json["domain"], "Router");
    assert_eq!(identity_json["purpose"], "Inference");

    assert_eq!(json["event_type"], "router.decision");
}

#[test]
fn test_policy_event_contains_envelope() {
    // Simulate a policy enforcement event
    let identity = IdentityEnvelope::new_with_process_revision(
        "tenant-compliance".to_string(),
        Domain::Policy,
        Purpose::Audit,
    );

    let event = TelemetryEventBuilder::new(
        EventType::PolicyViolation,
        LogLevel::Warn,
        "Policy violation detected".to_string(),
        identity,
    )
    .component("adapteros-policy".to_string())
    .metadata(serde_json::json!({
        "policy": "egress",
        "violation_type": "network_attempt",
        "details": "Attempted network call in production mode",
    }))
    .build();

    let json = serde_json::to_value(&event).unwrap();

    assert!(json.get("identity").is_some());
    let identity_json = &json["identity"];
    assert_eq!(identity_json["tenant_id"], "tenant-compliance");
    assert_eq!(identity_json["domain"], "Policy");
    assert_eq!(identity_json["purpose"], "Audit");

    assert_eq!(json["event_type"], "policy.violation");
}

/// Test that ALL events have non-empty tenant_id (PRD 1 invariant)
#[test]
fn test_all_events_have_nonempty_tenant_id() {
    let identity = IdentityEnvelope::new_with_process_revision(
        "tenant-123".to_string(),
        Domain::Telemetry,
        Purpose::Audit,
    );

    let event = TelemetryEventBuilder::new(
        EventType::SystemStart,
        LogLevel::Info,
        "System started".to_string(),
        identity,
    )
    .build();

    assert!(!event.identity.tenant_id.is_empty());
    assert_eq!(event.identity.tenant_id, "tenant-123");

    // Verify serialized form also has non-empty tenant_id
    let json = serde_json::to_value(&event).unwrap();
    let tenant_id_str = json["identity"]["tenant_id"].as_str().unwrap();
    assert!(!tenant_id_str.is_empty());
}
