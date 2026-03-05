//! UDS Adapter Command Integration Tests
//!
//! Tests for the JSON-based AdapterCommand path through the UDS protocol.
//! Verifies serialization compatibility between the API client and worker server.
//!
//! - Non-ignored tests: serialization round-trip (runs in CI)
//! - Ignored tests: full UDS flow (requires running worker with kernel backend)

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::{AdapterCommand, AdapterCommandResult};

/// Verify AdapterCommand::Swap serializes to the format the worker's UDS server expects.
///
/// The worker parses `POST /adapter/command` bodies as tagged JSON:
/// `{"type":"swap","add_ids":[...],"remove_ids":[...]}`
#[test]
fn adapter_command_swap_serializes_correctly() {
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["adapter-new".into()],
        remove_ids: vec!["adapter-old".into()],
        expected_stack_hash: None,
    };
    let json = serde_json::to_string(&cmd).unwrap();

    // Verify tagged enum format
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "swap");
    assert_eq!(parsed["add_ids"][0], "adapter-new");
    assert_eq!(parsed["remove_ids"][0], "adapter-old");

    // Round-trip through AdapterCommand
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    match roundtrip {
        AdapterCommand::Swap {
            add_ids,
            remove_ids,
            expected_stack_hash,
        } => {
            assert_eq!(add_ids, vec!["adapter-new"]);
            assert_eq!(remove_ids, vec!["adapter-old"]);
            assert!(expected_stack_hash.is_none());
        }
        _ => panic!("expected Swap variant after round-trip"),
    }
}

/// Verify AdapterCommand::Preload serializes correctly.
#[test]
fn adapter_command_preload_serializes_correctly() {
    let hash = B3Hash::hash(b"test-adapter");
    let cmd = AdapterCommand::Preload {
        adapter_id: "test-adapter".into(),
        hash: hash.clone(),
    };
    let json = serde_json::to_string(&cmd).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "preload");
    assert_eq!(parsed["adapter_id"], "test-adapter");

    // Round-trip
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    match roundtrip {
        AdapterCommand::Preload {
            adapter_id,
            hash: rt_hash,
        } => {
            assert_eq!(adapter_id, "test-adapter");
            assert_eq!(rt_hash, hash);
        }
        _ => panic!("expected Preload variant after round-trip"),
    }
}

/// Verify AdapterCommand::Rollback serializes correctly.
#[test]
fn adapter_command_rollback_serializes_correctly() {
    let cmd = AdapterCommand::Rollback;
    let json = serde_json::to_string(&cmd).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "rollback");

    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    assert!(matches!(roundtrip, AdapterCommand::Rollback));
}

/// Verify AdapterCommand::VerifyStack serializes correctly.
#[test]
fn adapter_command_verify_stack_serializes_correctly() {
    let cmd = AdapterCommand::VerifyStack;
    let json = serde_json::to_string(&cmd).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "verify_stack");

    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();
    assert!(matches!(roundtrip, AdapterCommand::VerifyStack));
}

/// Verify AdapterCommandResult can be deserialized from worker-format JSON.
#[test]
fn adapter_command_result_deserializes_from_worker_format() {
    let worker_response = serde_json::json!({
        "success": true,
        "message": "Swap completed",
        "vram_delta_mb": 128,
        "duration_ms": 42,
        "stack_hash": null,
        "memory_state": null
    });

    let result: AdapterCommandResult =
        serde_json::from_value(worker_response).expect("should deserialize worker response");
    assert!(result.success);
    assert_eq!(result.message, "Swap completed");
    assert_eq!(result.vram_delta_mb, Some(128));
    assert_eq!(result.duration_ms, 42);
}

/// Verify AdapterCommand::Swap with expected_stack_hash serializes correctly.
#[test]
fn adapter_command_swap_with_expected_hash() {
    let hash = B3Hash::hash(b"expected-stack");
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["a1".into(), "a2".into()],
        remove_ids: vec!["b1".into()],
        expected_stack_hash: Some(hash.clone()),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    let roundtrip: AdapterCommand = serde_json::from_str(&json).unwrap();

    match roundtrip {
        AdapterCommand::Swap {
            add_ids,
            remove_ids,
            expected_stack_hash,
        } => {
            assert_eq!(add_ids, vec!["a1", "a2"]);
            assert_eq!(remove_ids, vec!["b1"]);
            assert_eq!(expected_stack_hash, Some(hash));
        }
        _ => panic!("expected Swap variant"),
    }
}
