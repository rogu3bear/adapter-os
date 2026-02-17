//! Integration tests for CHAT-PIN-02: Pinned Adapter Router Integration
//!
//! Tests the full data flow for pinned adapters:
//! 1. Session creation with pinned adapters
//! 2. Pinned adapter IDs flow to InferenceRequestInternal
//! 3. Unavailable pinned adapters are correctly identified
//! 4. Tenant default inheritance works correctly
//!
//! Note: These tests verify the control plane logic without requiring a live worker.
//! Full end-to-end tests with actual inference are in the e2e test suite.

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::Db;
use adapteros_server_api::inference_core::parse_pinned_adapter_ids;
use adapteros_server_api::types::{InferenceRequestInternal, InferenceResult, WorkerInferResponse};

// =============================================================================
// Test Helpers
// =============================================================================

/// Create an in-memory test database
async fn create_test_db() -> Db {
    Db::new_in_memory()
        .await
        .expect("Failed to create in-memory database")
}

/// Create a test tenant
async fn create_test_tenant(db: &Db, name: &str) -> String {
    db.create_tenant(name, false)
        .await
        .expect("Failed to create test tenant")
}

/// Create a test session with optional pinned adapters
async fn create_test_session(
    db: &Db,
    session_id: &str,
    tenant_id: &str,
    pinned_adapter_ids: Option<Vec<String>>,
) {
    db.create_chat_session(CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: None,
        created_by: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        name: "Test Session".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: pinned_adapter_ids.map(|ids| serde_json::to_string(&ids).unwrap()),
        codebase_adapter_id: None,
    })
    .await
    .expect("Failed to create test session");
}

// =============================================================================
// Pinned Adapter Parsing Tests
// =============================================================================

#[test]
fn test_parse_pinned_adapter_ids_valid_json() {
    let json = r#"["adapter-a", "adapter-b", "adapter-c"]"#;
    let result = parse_pinned_adapter_ids(Some(json));

    assert!(result.is_some());
    let ids = result.unwrap();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&"adapter-a".to_string()));
    assert!(ids.contains(&"adapter-b".to_string()));
    assert!(ids.contains(&"adapter-c".to_string()));
}

#[test]
fn test_parse_pinned_adapter_ids_empty_array() {
    let json = r#"[]"#;
    let result = parse_pinned_adapter_ids(Some(json));

    assert!(result.is_some());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_parse_pinned_adapter_ids_none() {
    let result = parse_pinned_adapter_ids(None);
    assert!(result.is_none());
}

#[test]
fn test_parse_pinned_adapter_ids_malformed_json() {
    // Malformed JSON should return None (graceful degradation)
    let result = parse_pinned_adapter_ids(Some("not valid json"));
    assert!(result.is_none());
}

#[test]
fn test_parse_pinned_adapter_ids_wrong_type() {
    // Object instead of array - should return None
    let result = parse_pinned_adapter_ids(Some(r#"{"key": "value"}"#));
    assert!(result.is_none());
}

// =============================================================================
// Session Pinned Adapter Retrieval Tests
// =============================================================================

#[tokio::test]
async fn test_session_with_pinned_adapters() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "Test Tenant").await;
    let session_id = "session-with-pins";

    // Create session with pinned adapters
    let pinned = vec!["adapter-x".to_string(), "adapter-y".to_string()];
    create_test_session(&db, session_id, &tenant_id, Some(pinned.clone())).await;

    // Retrieve session and verify pinned adapters
    let session = db.get_chat_session(session_id).await.unwrap().unwrap();
    let parsed = parse_pinned_adapter_ids(session.pinned_adapter_ids.as_deref());

    assert!(parsed.is_some());
    assert_eq!(parsed.unwrap(), pinned);
}

#[tokio::test]
async fn test_session_without_pinned_adapters() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "Test Tenant").await;
    let session_id = "session-no-pins";

    // Create session without pinned adapters
    create_test_session(&db, session_id, &tenant_id, None).await;

    // Retrieve session and verify no pinned adapters
    let session = db.get_chat_session(session_id).await.unwrap().unwrap();
    let parsed = parse_pinned_adapter_ids(session.pinned_adapter_ids.as_deref());

    assert!(parsed.is_none());
}

// =============================================================================
// InferenceRequestInternal Tests
// =============================================================================

#[test]
fn test_inference_request_internal_with_pinned_adapters() {
    let pinned = vec!["adapter-1".to_string(), "adapter-2".to_string()];

    let request = InferenceRequestInternal {
        request_id: "test-123".to_string(),
        cpid: "tenant-1".to_string(),
        prompt: "test prompt".to_string(),
        max_tokens: 512,
        temperature: 0.7,
        session_id: Some("session-1".to_string()),
        pinned_adapter_ids: Some(pinned.clone()),
        created_at: std::time::Instant::now(),
        ..InferenceRequestInternal::default()
    };

    assert_eq!(request.pinned_adapter_ids, Some(pinned));
    assert_eq!(request.session_id, Some("session-1".to_string()));
}

// =============================================================================
// InferenceResult Tests
// =============================================================================

#[test]
fn test_inference_result_with_unavailable_pinned_adapters() {
    let unavailable = vec![
        "adapter-missing-1".to_string(),
        "adapter-missing-2".to_string(),
    ];

    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 10,
        run_receipt: None,
        token_usage: None,
        finish_reason: "ok".to_string(),
        adapters_used: vec!["adapter-available".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-1".to_string(),
        unavailable_pinned_adapters: Some(unavailable.clone()),
        pinned_routing_fallback: Some("partial".to_string()),
        effective_adapter_ids: None,
        model_type: None,
        backend_used: None,
        deterministic_receipt: None,
        run_envelope: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        abstention: None,
        pending_evidence_ids: Vec::new(),
    };

    assert_eq!(result.unavailable_pinned_adapters, Some(unavailable));
    assert_eq!(result.pinned_routing_fallback, Some("partial".to_string()));
}

#[test]
fn test_inference_result_without_unavailable_pinned_adapters() {
    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 10,
        run_receipt: None,
        token_usage: None,
        finish_reason: "ok".to_string(),
        adapters_used: vec!["adapter-1".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-1".to_string(),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        effective_adapter_ids: None,
        model_type: None,
        backend_used: None,
        deterministic_receipt: None,
        run_envelope: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        abstention: None,
        pending_evidence_ids: Vec::new(),
    };

    assert!(result.unavailable_pinned_adapters.is_none());
    assert!(result.pinned_routing_fallback.is_none());
}

// =============================================================================
// Pinned Routing Fallback Tests (PRD-6A)
// =============================================================================

#[test]
fn test_inference_result_all_pins_unavailable_stack_only_fallback() {
    // When ALL pinned adapters are unavailable, fallback should be "stack_only"
    let unavailable = vec!["pin-1".to_string(), "pin-2".to_string()];

    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 10,
        run_receipt: None,
        token_usage: None,
        finish_reason: "ok".to_string(),
        adapters_used: vec!["stack-adapter".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-1".to_string(),
        unavailable_pinned_adapters: Some(unavailable.clone()),
        pinned_routing_fallback: Some("stack_only".to_string()),
        effective_adapter_ids: None,
        model_type: None,
        backend_used: None,
        deterministic_receipt: None,
        run_envelope: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        abstention: None,
        pending_evidence_ids: Vec::new(),
    };

    assert_eq!(result.unavailable_pinned_adapters, Some(unavailable));
    assert_eq!(
        result.pinned_routing_fallback,
        Some("stack_only".to_string())
    );
}

#[test]
fn test_inference_result_partial_pins_unavailable() {
    // When SOME pinned adapters are unavailable, fallback should be "partial"
    let unavailable = vec!["pin-missing".to_string()];

    let result = InferenceResult {
        text: "test response".to_string(),
        tokens_generated: 10,
        run_receipt: None,
        token_usage: None,
        finish_reason: "ok".to_string(),
        adapters_used: vec!["pin-available".to_string(), "stack-adapter".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-1".to_string(),
        unavailable_pinned_adapters: Some(unavailable.clone()),
        pinned_routing_fallback: Some("partial".to_string()),
        effective_adapter_ids: None,
        model_type: None,
        backend_used: None,
        deterministic_receipt: None,
        run_envelope: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        replay_guarantee: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        abstention: None,
        pending_evidence_ids: Vec::new(),
    };

    assert_eq!(result.unavailable_pinned_adapters, Some(unavailable));
    assert_eq!(result.pinned_routing_fallback, Some("partial".to_string()));
}

// =============================================================================
// WorkerInferResponse Tests
// =============================================================================

#[test]
fn test_worker_response_unavailable_pinned_deserialization() {
    // Simulate worker response with unavailable pinned adapters
    let json = r#"{
        "text": "test output",
        "status": "ok",
        "trace": {
            "router_summary": {
                "adapters_used": ["adapter-1"]
            }
        },
        "unavailable_pinned_adapters": ["adapter-missing"],
        "pinned_routing_fallback": "partial"
    }"#;

    let response: WorkerInferResponse =
        serde_json::from_str(json).expect("Should deserialize worker response");

    assert!(response.unavailable_pinned_adapters.is_some());
    assert_eq!(
        response.unavailable_pinned_adapters.unwrap(),
        vec!["adapter-missing".to_string()]
    );
    assert_eq!(
        response.pinned_routing_fallback,
        Some("partial".to_string())
    );
}

#[test]
fn test_worker_response_no_unavailable_pinned_deserialization() {
    // Simulate worker response without unavailable pinned adapters
    let json = r#"{
        "text": "test output",
        "status": "ok",
        "trace": {
            "router_summary": {
                "adapters_used": ["adapter-1"]
            }
        }
    }"#;

    let response: WorkerInferResponse =
        serde_json::from_str(json).expect("Should deserialize worker response");

    assert!(response.unavailable_pinned_adapters.is_none());
    assert!(response.pinned_routing_fallback.is_none());
}

#[test]
fn test_worker_response_stack_only_fallback() {
    // Simulate worker response with all pins unavailable (stack_only fallback)
    let json = r#"{
        "text": "test output",
        "status": "ok",
        "trace": {
            "router_summary": {
                "adapters_used": ["stack-adapter"]
            }
        },
        "unavailable_pinned_adapters": ["pin-1", "pin-2"],
        "pinned_routing_fallback": "stack_only"
    }"#;

    let response: WorkerInferResponse =
        serde_json::from_str(json).expect("Should deserialize worker response");

    assert!(response.unavailable_pinned_adapters.is_some());
    assert_eq!(response.unavailable_pinned_adapters.unwrap().len(), 2);
    assert_eq!(
        response.pinned_routing_fallback,
        Some("stack_only".to_string())
    );
}

// =============================================================================
// Tenant Default Pinned Adapter Tests
// =============================================================================

#[tokio::test]
async fn test_tenant_default_pinned_adapters_inheritance() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "Test Tenant").await;

    // Set tenant default pinned adapters
    let default_adapters = vec![
        "default-adapter-1".to_string(),
        "default-adapter-2".to_string(),
    ];
    db.set_tenant_default_pinned_adapters(&tenant_id, Some(&default_adapters))
        .await
        .unwrap();

    // Create session WITHOUT explicit pinned adapters - should inherit
    db.create_chat_session(CreateChatSessionParams {
        id: "inheriting-session".to_string(),
        tenant_id: tenant_id.clone(),
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
        pinned_adapter_ids: None, // Not provided - should inherit
        codebase_adapter_id: None,
    })
    .await
    .unwrap();

    // Get session pinned adapters (should return tenant default)
    let pinned = db
        .get_session_pinned_adapters("inheriting-session", &tenant_id)
        .await
        .unwrap();

    assert_eq!(pinned, Some(default_adapters));
}

#[tokio::test]
async fn test_session_explicit_pinned_overrides_tenant_default() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "Test Tenant").await;

    // Set tenant default pinned adapters
    let default_adapters = vec!["default-adapter".to_string()];
    db.set_tenant_default_pinned_adapters(&tenant_id, Some(&default_adapters))
        .await
        .unwrap();

    // Create session WITH explicit pinned adapters - should NOT inherit
    let explicit_adapters = vec![
        "explicit-adapter-1".to_string(),
        "explicit-adapter-2".to_string(),
    ];
    db.create_chat_session(CreateChatSessionParams {
        id: "explicit-session".to_string(),
        tenant_id: tenant_id.clone(),
        user_id: None,
        created_by: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        name: "Explicit Session".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: Some(serde_json::to_string(&explicit_adapters).unwrap()),
        codebase_adapter_id: None,
    })
    .await
    .unwrap();

    // Get session pinned adapters (should return explicit, not default)
    let pinned = db
        .get_session_pinned_adapters("explicit-session", &tenant_id)
        .await
        .unwrap();

    assert_eq!(pinned, Some(explicit_adapters));
}

// =============================================================================
// Pinned Routing Fallback Edge Case Tests (PRD-6A)
// =============================================================================

/// Helper function to compute fallback mode (mirrors InferenceCore logic)
fn compute_fallback_mode(
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
fn test_fallback_mode_no_pins_no_unavailable() {
    // Case: No pinned adapters configured
    let fallback = compute_fallback_mode(None, None);
    assert!(fallback.is_none(), "No fallback when no pins configured");
}

#[test]
fn test_fallback_mode_empty_pins() {
    // Case: Empty pinned list
    let pinned: Vec<String> = vec![];
    let unavailable: Vec<String> = vec![];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert!(fallback.is_none(), "No fallback when pins list is empty");
}

#[test]
fn test_fallback_mode_all_pins_available() {
    // Case: All pinned adapters are available (no unavailable)
    let pinned = vec!["pin-1".to_string(), "pin-2".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), None);
    assert!(fallback.is_none(), "No fallback when all pins available");
}

#[test]
fn test_fallback_mode_partial_one_of_two() {
    // Case: 1 of 2 pinned unavailable -> "partial"
    let pinned = vec!["pin-1".to_string(), "pin-2".to_string()];
    let unavailable = vec!["pin-1".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("partial".to_string()));
}

#[test]
fn test_fallback_mode_partial_one_of_three() {
    // Case: 1 of 3 pinned unavailable -> "partial"
    let pinned = vec![
        "pin-1".to_string(),
        "pin-2".to_string(),
        "pin-3".to_string(),
    ];
    let unavailable = vec!["pin-2".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("partial".to_string()));
}

#[test]
fn test_fallback_mode_partial_two_of_three() {
    // Case: 2 of 3 pinned unavailable -> "partial"
    let pinned = vec![
        "pin-1".to_string(),
        "pin-2".to_string(),
        "pin-3".to_string(),
    ];
    let unavailable = vec!["pin-1".to_string(), "pin-3".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("partial".to_string()));
}

#[test]
fn test_fallback_mode_stack_only_all_two_unavailable() {
    // Case: 2 of 2 pinned unavailable -> "stack_only"
    let pinned = vec!["pin-1".to_string(), "pin-2".to_string()];
    let unavailable = vec!["pin-1".to_string(), "pin-2".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("stack_only".to_string()));
}

#[test]
fn test_fallback_mode_stack_only_all_three_unavailable() {
    // Case: 3 of 3 pinned unavailable -> "stack_only"
    let pinned = vec![
        "pin-1".to_string(),
        "pin-2".to_string(),
        "pin-3".to_string(),
    ];
    let unavailable = vec![
        "pin-1".to_string(),
        "pin-2".to_string(),
        "pin-3".to_string(),
    ];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("stack_only".to_string()));
}

#[test]
fn test_fallback_mode_single_pin_unavailable() {
    // Case: Single pin, and it's unavailable -> "stack_only"
    let pinned = vec!["single-pin".to_string()];
    let unavailable = vec!["single-pin".to_string()];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert_eq!(fallback, Some("stack_only".to_string()));
}

#[test]
fn test_fallback_mode_empty_unavailable() {
    // Case: Pins configured but none unavailable (empty list)
    let pinned = vec!["pin-1".to_string(), "pin-2".to_string()];
    let unavailable: Vec<String> = vec![];
    let fallback = compute_fallback_mode(Some(&pinned), Some(&unavailable));
    assert!(
        fallback.is_none(),
        "No fallback when unavailable list is empty"
    );
}

// =============================================================================
// Streaming Done Event Serialization Tests (PRD-6A)
// =============================================================================

#[test]
fn test_inference_event_done_serializes_pinned_fields() {
    use adapteros_server_api::handlers::streaming_infer::InferenceEvent;

    let event = InferenceEvent::Done {
        total_tokens: 42,
        latency_ms: 1234,
        unavailable_pinned_adapters: Some(vec!["missing-1".to_string(), "missing-2".to_string()]),
        pinned_routing_fallback: Some("partial".to_string()),
        citations: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        pending_evidence_ids: Vec::new(),
    };

    let json = serde_json::to_string(&event).expect("Failed to serialize");

    // Verify all fields are present
    assert!(
        json.contains("\"event\":\"Done\"") || json.contains("\"Done\""),
        "Should contain Done event type"
    );
    assert!(json.contains("42"), "Should contain total_tokens");
    assert!(json.contains("1234"), "Should contain latency_ms");
    assert!(
        json.contains("missing-1"),
        "Should contain first unavailable adapter"
    );
    assert!(
        json.contains("missing-2"),
        "Should contain second unavailable adapter"
    );
    assert!(json.contains("partial"), "Should contain fallback mode");
}

#[test]
fn test_inference_event_done_skips_none_fields() {
    use adapteros_server_api::handlers::streaming_infer::InferenceEvent;

    let event = InferenceEvent::Done {
        total_tokens: 10,
        latency_ms: 500,
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        citations: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        pending_evidence_ids: Vec::new(),
    };

    let json = serde_json::to_string(&event).expect("Failed to serialize");

    // None fields should not be serialized (skip_serializing_if)
    assert!(
        !json.contains("unavailable_pinned_adapters"),
        "Should not contain unavailable_pinned_adapters when None"
    );
    assert!(
        !json.contains("pinned_routing_fallback"),
        "Should not contain pinned_routing_fallback when None"
    );
}

// =============================================================================
// Pin TTL Validation Tests (ISSUE 3)
// =============================================================================

#[tokio::test]
async fn test_pin_adapter_rejects_past_ttl() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Register an adapter first
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("test-adapter")
        .name("test-adapter")
        .hash_b3("b3:somehash")
        .rank(16)
        .tier("ephemeral")
        .category("general")
        .build()
        .expect("adapter params");
    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Try to pin with a TTL in the past
    let past_ttl = "2020-01-01T00:00:00Z";
    let result = db
        .pin_adapter(
            &tenant_id,
            "test-adapter",
            Some(past_ttl),
            "test reason",
            Some("test-user"),
        )
        .await;

    assert!(result.is_err(), "Pin with past TTL should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("TTL is in the past"),
        "Error message should mention TTL in past: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_pin_adapter_accepts_future_ttl() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Register an adapter first
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("test-adapter")
        .name("test-adapter")
        .hash_b3("b3:somehash")
        .rank(16)
        .tier("ephemeral")
        .category("general")
        .build()
        .expect("adapter params");
    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Pin with a TTL in the future
    let future_ttl = "2099-12-31T23:59:59Z";
    let result = db
        .pin_adapter(
            &tenant_id,
            "test-adapter",
            Some(future_ttl),
            "test reason",
            Some("test-user"),
        )
        .await;

    assert!(result.is_ok(), "Pin with future TTL should succeed");
}

#[tokio::test]
async fn test_pin_adapter_accepts_no_ttl() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Register an adapter first
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("test-adapter")
        .name("test-adapter")
        .hash_b3("b3:somehash")
        .rank(16)
        .tier("ephemeral")
        .category("general")
        .build()
        .expect("adapter params");
    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Pin without TTL (indefinite pin)
    let result = db
        .pin_adapter(
            &tenant_id,
            "test-adapter",
            None,
            "test reason",
            Some("test-user"),
        )
        .await;

    assert!(result.is_ok(), "Pin without TTL should succeed");
}

#[tokio::test]
async fn test_pin_adapter_rejects_invalid_ttl_format() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Register an adapter first
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(&tenant_id)
        .adapter_id("test-adapter")
        .name("test-adapter")
        .hash_b3("b3:somehash")
        .rank(16)
        .tier("ephemeral")
        .category("general")
        .build()
        .expect("adapter params");
    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Try to pin with invalid TTL format
    let invalid_ttl = "not-a-valid-timestamp";
    let result = db
        .pin_adapter(
            &tenant_id,
            "test-adapter",
            Some(invalid_ttl),
            "test reason",
            Some("test-user"),
        )
        .await;

    assert!(result.is_err(), "Pin with invalid TTL format should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid pinned_until"),
        "Error message should mention invalid format: {}",
        err_msg
    );
}
