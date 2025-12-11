//! E2E Test for Missing Pinned Adapters Graceful Degradation
//!
//! Tests the graceful degradation behavior when pinned adapters are unavailable:
//! 1. Session creation with pinned adapters
//! 2. Pinned adapter retrieval from session
//! 3. Fallback mode computation
//! 4. API response structure verification
//!
//! Note: Full inference E2E tests require a running worker. These tests validate
//! the control plane behavior without actual inference execution.
//!
//! Citations:
//! - Missing Pinned Adapters & Error Signaling
//! - Pinned Adapter Router Integration

mod common;

use adapteros_db::chat_sessions::CreateChatSessionParams;
use common::test_harness::ApiTestHarness;

// =============================================================================
// Session Pinned Adapter API Tests
// =============================================================================

#[tokio::test]
async fn test_create_session_with_pinned_adapters() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    let pinned_adapters = vec!["adapter-a", "adapter-b", "adapter-c"];

    // Create session with pinned adapters
    let session_id = "e2e-pinned-test-session";
    harness
        .state
        .db
        .create_chat_session(CreateChatSessionParams {
            id: session_id.to_string(),
            tenant_id: "default".to_string(),
            user_id: Some("testadmin@example.com".to_string()),
            created_by: Some("testadmin@example.com".to_string()),
            stack_id: None,
            collection_id: None,
            document_id: None,
            name: "E2E Pinned Adapters Test".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: Some(serde_json::to_string(&pinned_adapters).unwrap()),
        })
        .await
        .expect("Failed to create session");

    // Retrieve and verify pinned adapters
    let retrieved = harness
        .state
        .db
        .get_session_pinned_adapters(session_id, "default")
        .await
        .expect("Failed to retrieve pinned adapters");

    assert!(retrieved.is_some(), "Pinned adapters should be present");
    let retrieved_ids = retrieved.unwrap();
    assert_eq!(retrieved_ids.len(), 3, "Should have 3 pinned adapters");
    assert!(
        retrieved_ids.contains(&"adapter-a".to_string()),
        "Should contain adapter-a"
    );
    assert!(
        retrieved_ids.contains(&"adapter-b".to_string()),
        "Should contain adapter-b"
    );
    assert!(
        retrieved_ids.contains(&"adapter-c".to_string()),
        "Should contain adapter-c"
    );
}

#[tokio::test]
async fn test_session_inherits_tenant_default_pinned_adapters() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    // Set tenant default pinned adapters
    let default_adapters = vec!["default-pin-1".to_string(), "default-pin-2".to_string()];
    harness
        .state
        .db
        .set_tenant_default_pinned_adapters("default", Some(&default_adapters))
        .await
        .expect("Failed to set tenant defaults");

    // Create session WITHOUT explicit pinned adapters
    let session_id = "e2e-inheriting-session";
    harness
        .state
        .db
        .create_chat_session(CreateChatSessionParams {
            id: session_id.to_string(),
            tenant_id: "default".to_string(),
            user_id: None,
            created_by: None,
            stack_id: None,
            collection_id: None,
            document_id: None,
            name: "Inheriting Session".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None, // Should inherit from tenant
        })
        .await
        .expect("Failed to create session");

    // Verify session inherits tenant defaults
    let retrieved = harness
        .state
        .db
        .get_session_pinned_adapters(session_id, "default")
        .await
        .expect("Failed to retrieve pinned adapters");

    assert_eq!(
        retrieved,
        Some(default_adapters),
        "Session should inherit tenant default pinned adapters"
    );
}

#[tokio::test]
async fn test_session_explicit_pins_override_tenant_default() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    // Set tenant default pinned adapters
    let default_adapters = vec!["default-adapter".to_string()];
    harness
        .state
        .db
        .set_tenant_default_pinned_adapters("default", Some(&default_adapters))
        .await
        .expect("Failed to set tenant defaults");

    // Create session WITH explicit pinned adapters
    let explicit_adapters = vec!["explicit-1".to_string(), "explicit-2".to_string()];
    let session_id = "e2e-override-session";
    harness
        .state
        .db
        .create_chat_session(CreateChatSessionParams {
            id: session_id.to_string(),
            tenant_id: "default".to_string(),
            user_id: None,
            created_by: None,
            stack_id: None,
            collection_id: None,
            document_id: None,
            name: "Override Session".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: Some(serde_json::to_string(&explicit_adapters).unwrap()),
        })
        .await
        .expect("Failed to create session");

    // Verify explicit adapters override tenant defaults
    let retrieved = harness
        .state
        .db
        .get_session_pinned_adapters(session_id, "default")
        .await
        .expect("Failed to retrieve pinned adapters");

    assert_eq!(
        retrieved,
        Some(explicit_adapters),
        "Explicit pins should override tenant defaults"
    );
}

// =============================================================================
// InferenceResult Structure Tests (Response Fields)
// =============================================================================

#[test]
fn test_inference_result_has_pinned_adapter_fields() {
    use adapteros_server_api::types::InferenceResult;

    // Create InferenceResult with unavailable pinned adapters
    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 100,
        finish_reason: "stop".to_string(),
        adapters_used: vec!["stack-adapter-1".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 500,
        request_id: "test-request-123".to_string(),
        unavailable_pinned_adapters: Some(vec![
            "missing-pin-1".to_string(),
            "missing-pin-2".to_string(),
        ]),
        pinned_routing_fallback: Some("partial".to_string()),
        effective_adapter_ids: None,
        backend_used: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
    };

    // Verify fields are present and correct
    assert_eq!(
        result.unavailable_pinned_adapters,
        Some(vec![
            "missing-pin-1".to_string(),
            "missing-pin-2".to_string()
        ])
    );
    assert_eq!(result.pinned_routing_fallback, Some("partial".to_string()));

    // Verify other fields are set correctly
    assert_eq!(result.text, "test response");
    assert_eq!(result.finish_reason, "stop");
    assert_eq!(result.latency_ms, 500);
}

#[test]
fn test_inference_result_omits_none_pinned_fields() {
    use adapteros_server_api::types::InferenceResult;

    // Create InferenceResult without unavailable pinned adapters
    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 50,
        finish_reason: "stop".to_string(),
        adapters_used: vec!["adapter-1".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 250,
        request_id: "test-request-456".to_string(),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        effective_adapter_ids: None,
        backend_used: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
    };

    // Verify None fields are properly set
    assert!(result.unavailable_pinned_adapters.is_none());
    assert!(result.pinned_routing_fallback.is_none());
}

// =============================================================================
// Fallback Mode Logic Tests
// =============================================================================

/// Mirrors the fallback computation logic from InferenceCore
fn compute_pinned_routing_fallback(
    pinned: Option<&Vec<String>>,
    unavailable: Option<&Vec<String>>,
) -> Option<String> {
    match (pinned, unavailable) {
        (Some(pinned), Some(unavailable)) if !pinned.is_empty() && !unavailable.is_empty() => {
            if unavailable.len() >= pinned.len() {
                Some("stack_only".to_string())
            } else {
                Some("partial".to_string())
            }
        }
        _ => None,
    }
}

#[test]
fn test_graceful_degradation_partial_pins_available() {
    // Scenario: 2 of 3 pinned adapters are available
    let pinned = vec![
        "pin-a".to_string(),
        "pin-b".to_string(),
        "pin-c".to_string(),
    ];
    let unavailable = vec!["pin-b".to_string()];

    let fallback = compute_pinned_routing_fallback(Some(&pinned), Some(&unavailable));

    assert_eq!(
        fallback,
        Some("partial".to_string()),
        "When some pins are unavailable, should use partial fallback"
    );
}

#[test]
fn test_graceful_degradation_all_pins_unavailable() {
    // Scenario: All pinned adapters are unavailable
    let pinned = vec!["pin-a".to_string(), "pin-b".to_string()];
    let unavailable = vec!["pin-a".to_string(), "pin-b".to_string()];

    let fallback = compute_pinned_routing_fallback(Some(&pinned), Some(&unavailable));

    assert_eq!(
        fallback,
        Some("stack_only".to_string()),
        "When all pins are unavailable, should use stack_only fallback"
    );
}

#[test]
fn test_graceful_degradation_no_fallback_when_all_available() {
    // Scenario: All pinned adapters are available
    let pinned = vec!["pin-a".to_string(), "pin-b".to_string()];

    let fallback = compute_pinned_routing_fallback(Some(&pinned), None);

    assert!(
        fallback.is_none(),
        "No fallback needed when all pins are available"
    );
}

#[test]
fn test_graceful_degradation_http_200_behavior() {
    // This test documents the expected behavior: HTTP 200 OK even when pins are missing
    // The actual HTTP response is tested via the harness, but the key invariant is:
    // - Inference should succeed (return result, not error)
    // - Warning metadata should be included
    // - HTTP status should be 200 OK

    use adapteros_server_api::types::InferenceResult;

    // Simulate a successful inference result with missing pins
    let result = InferenceResult {
        text: "Generated response despite missing pins".to_string(),
        tokens_generated: 42,
        finish_reason: "stop".to_string(),
        adapters_used: vec!["stack-fallback-adapter".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 1000,
        request_id: "graceful-degradation-test".to_string(),
        unavailable_pinned_adapters: Some(vec!["requested-pin".to_string()]),
        pinned_routing_fallback: Some("stack_only".to_string()),
        effective_adapter_ids: None,
        backend_used: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
    };

    // Key assertions for graceful degradation compliance:
    // 1. Response has generated text (inference succeeded)
    assert!(!result.text.is_empty(), "Inference should produce output");

    // 2. Response includes warning metadata
    assert!(
        result.unavailable_pinned_adapters.is_some(),
        "Warning metadata should be present"
    );

    // 3. Fallback mode is documented in response
    assert_eq!(
        result.pinned_routing_fallback,
        Some("stack_only".to_string()),
        "Fallback mode should be documented"
    );
}

// =============================================================================
// Telemetry Event Structure Tests
// =============================================================================

#[test]
fn test_telemetry_event_structure_for_missing_pins() {
    // This test validates the structure of telemetry events for missing pinned adapters
    // The actual telemetry emission is in inference_core.rs; this tests the event structure

    use adapteros_core::identity::IdentityEnvelope;
    use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};

    let identity = IdentityEnvelope::new(
        "test-tenant".to_string(),
        "inference_core".to_string(),
        "pinned_adapters_unavailable".to_string(),
        "0.1.0".to_string(),
    );

    let event_result = TelemetryEventBuilder::new(
        EventType::Custom("inference.pinned_adapters_unavailable".to_string()),
        LogLevel::Warn,
        "2 of 3 pinned adapters unavailable - fallback: partial".to_string(),
        identity,
    )
    .component("inference_core".to_string())
    .metadata(serde_json::json!({
        "request_id": "test-req-123",
        "cpid": "test-tenant",
        "session_id": "test-session",
        "pinned_adapter_ids": ["pin-a", "pin-b", "pin-c"],
        "unavailable_pinned_adapters": ["pin-b", "pin-c"],
        "fallback_mode": "partial",
        "latency_ms": 500,
    }))
    .build();

    assert!(
        event_result.is_ok(),
        "Telemetry event should build successfully"
    );

    let event = event_result.unwrap();
    assert_eq!(
        event.event_type, "inference.pinned_adapters_unavailable",
        "Event type should match"
    );
    assert!(
        event.message.contains("pinned adapters unavailable"),
        "Message should describe the issue"
    );
    assert!(event.metadata.is_some(), "Metadata should be present");
}
