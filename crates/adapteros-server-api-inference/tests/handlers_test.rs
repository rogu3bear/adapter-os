//! Unit tests for inference types
//!
//! Tests for request/response types, streaming types, batch types,
//! and serde serialization/deserialization.

use adapteros_server_api_inference::batch::{
    BatchInferItemPayload, BatchInferItemResponse, BatchInferRequest, BatchInferResponse,
    BatchItemError,
};
use adapteros_server_api_inference::provenance::{
    AdapterProvenanceInfo, DocumentProvenanceInfo, ProvenanceResponse,
};
use adapteros_server_api_inference::streaming::{
    Delta, StreamingChoice, StreamingChunk, StreamingInferRequest,
};

/// Test streaming request defaults
#[test]
fn test_streaming_request_defaults() {
    let json = r#"{"prompt": "Hello"}"#;
    let req: StreamingInferRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.prompt, "Hello");
    assert_eq!(req.max_tokens, 512);
    assert!((req.temperature - 0.7).abs() < 0.01);
}

/// Test streaming request with all fields
#[test]
fn test_streaming_request_all_fields() {
    let json = r#"{
        "prompt": "Test prompt",
        "model": "test-model",
        "max_tokens": 100,
        "temperature": 0.8,
        "top_p": 0.9,
        "top_k": 50,
        "stop": ["STOP"],
        "seed": 12345,
        "require_evidence": true,
        "collection_id": "test-collection"
    }"#;

    let req: StreamingInferRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.prompt, "Test prompt");
    assert_eq!(req.model, Some("test-model".to_string()));
    assert_eq!(req.max_tokens, 100);
    assert!((req.temperature - 0.8).abs() < 0.01);
    assert_eq!(req.top_p, Some(0.9));
    assert_eq!(req.top_k, Some(50));
    assert_eq!(req.stop, vec!["STOP"]);
    assert_eq!(req.seed, Some(12345));
    assert!(req.require_evidence);
    assert_eq!(req.collection_id, Some("test-collection".to_string()));
}

/// Test streaming chunk serialization
#[test]
fn test_streaming_chunk_serialization() {
    let chunk = StreamingChunk {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "test-model".to_string(),
        system_fingerprint: None,
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: None,
                content: Some("Hello".to_string()),
            },
            finish_reason: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
        }],
    };

    let json = serde_json::to_string(&chunk).unwrap();
    assert!(json.contains("chat.completion.chunk"));
    assert!(json.contains("Hello"));
}

/// Test streaming chunk with finish reason
#[test]
fn test_streaming_chunk_finish_reason() {
    let chunk = StreamingChunk {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "test-model".to_string(),
        system_fingerprint: None,
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: None,
                content: None,
            },
            finish_reason: Some("stop".to_string()),
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
        }],
    };

    let json = serde_json::to_string(&chunk).unwrap();
    assert!(json.contains("stop"));
}

/// Test delta with role (first chunk)
#[test]
fn test_delta_with_role() {
    let delta = Delta {
        role: Some("assistant".to_string()),
        content: None,
    };

    let json = serde_json::to_string(&delta).unwrap();
    assert!(json.contains("assistant"));
    assert!(!json.contains("content"));
}

/// Test batch request deserialization
#[test]
fn test_batch_request_deserialization() {
    let json = r#"{
        "requests": [
            {
                "id": "item-1",
                "request": {
                    "prompt": "Hello, world!"
                }
            }
        ]
    }"#;

    let req: BatchInferRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.requests.len(), 1);
    assert_eq!(req.requests[0].id, "item-1");
    assert_eq!(req.requests[0].request.prompt, "Hello, world!");
    assert_eq!(req.requests[0].request.max_tokens, 512);
}

/// Test batch response with error
#[test]
fn test_batch_response_with_error() {
    let response = BatchInferResponse {
        responses: vec![BatchInferItemResponse {
            id: "item-1".to_string(),
            response: None,
            error: Some(BatchItemError {
                message: "Test error".to_string(),
                code: Some("TEST_ERROR".to_string()),
                details: None,
            }),
        }],
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("TEST_ERROR"));
    assert!(json.contains("Test error"));
}

/// Test batch item payload defaults
#[test]
fn test_batch_item_payload_defaults() {
    let json = r#"{"prompt": "Test"}"#;
    let payload: BatchInferItemPayload = serde_json::from_str(json).unwrap();

    assert_eq!(payload.prompt, "Test");
    assert_eq!(payload.max_tokens, 512);
    assert!((payload.temperature - 0.7).abs() < 0.01);
}

/// Test provenance response serialization
#[test]
fn test_provenance_response_serialization() {
    let response = ProvenanceResponse {
        trace_id: "trace-123".to_string(),
        tenant_id: "tenant-456".to_string(),
        request_id: Some("req-789".to_string()),
        created_at: Some("2024-01-15T10:30:00Z".to_string()),
        adapters: vec![AdapterProvenanceInfo {
            adapter_id: "adapter-1".to_string(),
            gate: 0.85,
            training_job_id: Some("job-111".to_string()),
            dataset_version_id: Some("ds-v1".to_string()),
        }],
        source_documents: vec![DocumentProvenanceInfo {
            source_file: "docs/guide.md".to_string(),
            content_hash: "abc123".to_string(),
            lines: Some("10-50".to_string()),
        }],
        is_complete: true,
        warnings: vec![],
        confidence: 0.95,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("trace-123"));
    assert!(json.contains("0.85"));
    assert!(json.contains("docs/guide.md"));
}

/// Test provenance with warnings
#[test]
fn test_provenance_with_warnings() {
    let response = ProvenanceResponse {
        trace_id: "trace-456".to_string(),
        tenant_id: "tenant-789".to_string(),
        request_id: None,
        created_at: None,
        adapters: vec![],
        source_documents: vec![],
        is_complete: false,
        warnings: vec![
            "Adapter training lineage not found".to_string(),
            "Source documents could not be resolved".to_string(),
        ],
        confidence: 0.0,
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("is_complete\":false") || json.contains("is_complete\": false"));
    assert!(json.contains("Adapter training lineage not found"));
}

/// Test streaming request clone
#[test]
fn test_streaming_request_clone() {
    let original = StreamingInferRequest {
        prompt: "Clone test".to_string(),
        model: Some("model-1".to_string()),
        coreml_mode: None,
        routing_determinism_mode: None,
        stack_id: None,
        domain: None,
        max_tokens: 50,
        temperature: 0.9,
        top_p: None,
        top_k: None,
        stop: vec![],
        adapter_stack: None,
        adapters: None,
        seed: None,
        adapter_strength_overrides: None,
        require_evidence: false,
        reasoning_mode: false,
        collection_id: None,
        session_id: None,
        effective_adapter_ids: None,
        stop_policy: None,
        context: None,
        bit_identical: false,
    };

    let cloned = original.clone();

    assert_eq!(cloned.prompt, original.prompt);
    assert_eq!(cloned.model, original.model);
    assert_eq!(cloned.max_tokens, original.max_tokens);
}

/// Test adapter provenance info
#[test]
fn test_adapter_provenance_info() {
    let info = AdapterProvenanceInfo {
        adapter_id: "adapter-xyz".to_string(),
        gate: 0.72,
        training_job_id: None,
        dataset_version_id: None,
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("adapter-xyz"));
    assert!(json.contains("0.72"));
}

/// Test document provenance info
#[test]
fn test_document_provenance_info() {
    let info = DocumentProvenanceInfo {
        source_file: "src/main.rs".to_string(),
        content_hash: "deadbeef".to_string(),
        lines: Some("1-100".to_string()),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("src/main.rs"));
    assert!(json.contains("deadbeef"));
    assert!(json.contains("1-100"));
}
