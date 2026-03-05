//! Adapter Swap UDS Command Tests
//!
//! Tests verifying the swap handler's UDS command dispatch and format
//! compatibility with the worker's AdapterCommand parser.
//!
//! - Serialization tests: verify JSON format matches between API client and worker
//! - Lifecycle tests: verify swap handler behavior when lifecycle_manager is absent

use adapteros_core::B3Hash;
use adapteros_lora_worker::{AdapterCommand, AdapterCommandResult};

/// Verify the AdapterCommand::Swap that the swap handler constructs
/// serializes to JSON the worker can parse.
#[test]
fn swap_command_matches_worker_format() {
    // This mirrors what swap_adapters constructs in handlers/adapters/swap.rs
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["new-adapter".to_string()],
        remove_ids: vec!["old-adapter".to_string()],
        expected_stack_hash: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();

    // Worker expects {"type":"swap","add_ids":...,"remove_ids":...}
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "swap");
    assert_eq!(parsed["add_ids"][0], "new-adapter");
    assert_eq!(parsed["remove_ids"][0], "old-adapter");

    // Verify full round-trip through worker's type
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    match roundtrip {
        AdapterCommand::Swap {
            add_ids,
            remove_ids,
            expected_stack_hash,
        } => {
            assert_eq!(add_ids, vec!["new-adapter"]);
            assert_eq!(remove_ids, vec!["old-adapter"]);
            assert!(expected_stack_hash.is_none());
        }
        _ => panic!("expected Swap variant"),
    }
}

/// Verify the AdapterCommand::Preload that preload_adapters_for_inference constructs
/// serializes to JSON the worker can parse.
#[test]
fn preload_command_matches_worker_format() {
    // This mirrors what preload_adapters_for_inference constructs in streaming_infer.rs
    let cmd = AdapterCommand::Preload {
        adapter_id: "test-adapter".to_string(),
        hash: B3Hash::default(),
    };
    let json = serde_json::to_string(&cmd).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "preload");
    assert_eq!(parsed["adapter_id"], "test-adapter");

    // Round-trip
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    match roundtrip {
        AdapterCommand::Preload { adapter_id, .. } => {
            assert_eq!(adapter_id, "test-adapter");
        }
        _ => panic!("expected Preload variant"),
    }
}

/// Verify AdapterCommandResult deserialization handles all fields including
/// the optional reject_reason field added for memory pressure scenarios.
#[test]
fn adapter_command_result_with_reject_reason() {
    let worker_response = serde_json::json!({
        "success": false,
        "message": "Memory pressure too high",
        "vram_delta_mb": null,
        "duration_ms": 0,
        "stack_hash": null,
        "memory_state": null,
        "reject_reason": "vram_headroom_insufficient"
    });

    let result: AdapterCommandResult = serde_json::from_value(worker_response)
        .expect("should deserialize worker response with reject_reason");
    assert!(!result.success);
    assert_eq!(
        result.reject_reason.as_deref(),
        Some("vram_headroom_insufficient")
    );
}

/// Verify AdapterCommandResult deserialization handles minimal worker responses
/// (older workers may not include all fields).
#[test]
fn adapter_command_result_minimal_response() {
    let worker_response = serde_json::json!({
        "success": true,
        "message": "OK",
        "duration_ms": 5
    });

    let result: AdapterCommandResult = serde_json::from_value(worker_response)
        .expect("should deserialize minimal worker response");
    assert!(result.success);
    assert_eq!(result.message, "OK");
    assert!(result.vram_delta_mb.is_none());
    assert!(result.stack_hash.is_none());
    assert!(result.memory_state.is_none());
    assert!(result.reject_reason.is_none());
}

/// Verify swap command format for multi-adapter swap (add multiple, remove multiple).
#[test]
fn swap_command_multi_adapter_format() {
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["a1".into(), "a2".into(), "a3".into()],
        remove_ids: vec!["b1".into(), "b2".into()],
        expected_stack_hash: Some(B3Hash::hash(b"expected")),
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();

    match roundtrip {
        AdapterCommand::Swap {
            add_ids,
            remove_ids,
            expected_stack_hash,
        } => {
            assert_eq!(add_ids.len(), 3);
            assert_eq!(remove_ids.len(), 2);
            assert!(expected_stack_hash.is_some());
        }
        _ => panic!("expected Swap variant"),
    }
}
