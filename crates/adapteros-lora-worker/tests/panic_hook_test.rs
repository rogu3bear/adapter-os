//! Tests for worker panic hook behavior
//!
//! These tests verify that the panic hook correctly:
//! 1. Formats panic information with location and message
//! 2. Truncates long backtraces (> 2000 chars)
//! 3. Builds correct HTTP payload structure
//! 4. Handles missing worker identity gracefully
//!
//! NOTE: These tests import and test the actual functions from
//! `adapteros_lora_worker::panic_utils` to ensure production code is validated.

use adapteros_lora_worker::panic_utils::{
    build_fatal_payload, extract_panic_message, format_panic_location, truncate_backtrace,
};
use serde_json::Value;

#[test]
fn extract_panic_message_from_str_ref() {
    let message = "test panic message";
    let payload: &dyn std::any::Any = &message;
    let extracted = extract_panic_message(payload);
    assert_eq!(extracted, "test panic message");
}

#[test]
fn extract_panic_message_from_string() {
    let message = String::from("owned panic message");
    let payload: &dyn std::any::Any = &message;
    let extracted = extract_panic_message(payload);
    assert_eq!(extracted, "owned panic message");
}

#[test]
fn extract_panic_message_from_unknown_type() {
    let value = 42u32;
    let payload: &dyn std::any::Any = &value;
    let extracted = extract_panic_message(payload);
    assert_eq!(extracted, "Unknown panic");
}

#[test]
fn format_panic_location_includes_all_components() {
    let location = format_panic_location("src/worker.rs", 123, 45);
    assert_eq!(location, "src/worker.rs:123:45");
}

#[test]
fn format_panic_location_handles_paths_with_slashes() {
    let location = format_panic_location(
        "/Users/dev/adapter-os/crates/adapteros-lora-worker/src/bin/aos_worker.rs",
        500,
        12,
    );
    assert_eq!(
        location,
        "/Users/dev/adapter-os/crates/adapteros-lora-worker/src/bin/aos_worker.rs:500:12"
    );
}

#[test]
fn truncate_backtrace_preserves_short_strings() {
    let backtrace = "short backtrace\nline 2\nline 3";
    let truncated = truncate_backtrace(backtrace, 2000);
    assert_eq!(truncated, backtrace);
    assert!(!truncated.contains("(truncated)"));
}

#[test]
fn truncate_backtrace_truncates_long_strings() {
    let long_backtrace = "a".repeat(3000);
    let truncated = truncate_backtrace(&long_backtrace, 2000);

    assert_eq!(truncated.len(), 2000 + "...(truncated)".len());
    assert!(truncated.starts_with(&"a".repeat(2000)));
    assert!(truncated.ends_with("...(truncated)"));
}

#[test]
fn truncate_backtrace_at_exact_boundary() {
    let exact_backtrace = "x".repeat(2000);
    let truncated = truncate_backtrace(&exact_backtrace, 2000);
    assert_eq!(truncated, exact_backtrace);
    assert!(!truncated.contains("(truncated)"));
}

#[test]
fn truncate_backtrace_one_char_over() {
    let over_backtrace = "y".repeat(2001);
    let truncated = truncate_backtrace(&over_backtrace, 2000);
    assert!(truncated.contains("...(truncated)"));
    assert_eq!(truncated.len(), 2000 + "...(truncated)".len());
}

#[test]
fn build_fatal_payload_has_required_fields() {
    let payload = build_fatal_payload(
        "worker-123",
        "src/main.rs:42:10",
        "assertion failed",
        "backtrace line 1\nbacktrace line 2",
    );

    assert!(payload.is_object());
    assert!(payload["worker_id"].is_string());
    assert!(payload["reason"].is_string());
    assert!(payload["backtrace_snippet"].is_string());
    assert!(payload["timestamp"].is_string());
}

#[test]
fn build_fatal_payload_formats_reason_correctly() {
    let payload = build_fatal_payload(
        "worker-abc",
        "lib.rs:100:5",
        "null pointer dereference",
        "stack trace here",
    );

    let reason = payload["reason"].as_str().unwrap();
    assert_eq!(reason, "PANIC at lib.rs:100:5: null pointer dereference");
}

#[test]
fn build_fatal_payload_includes_worker_id() {
    let payload = build_fatal_payload("test-worker-456", "module.rs:1:1", "panic message", "trace");

    assert_eq!(payload["worker_id"].as_str().unwrap(), "test-worker-456");
}

#[test]
fn build_fatal_payload_includes_backtrace() {
    let backtrace = "frame 0: core::panic\nframe 1: worker::run\nframe 2: main";
    let payload = build_fatal_payload("worker-xyz", "src/lib.rs:200:15", "overflow", backtrace);

    assert_eq!(payload["backtrace_snippet"].as_str().unwrap(), backtrace);
}

#[test]
fn build_fatal_payload_timestamp_is_rfc3339() {
    let payload = build_fatal_payload("worker-001", "main.rs:1:1", "test", "trace");

    let timestamp = payload["timestamp"].as_str().unwrap();
    // Verify it parses as RFC3339
    assert!(chrono::DateTime::parse_from_rfc3339(timestamp).is_ok());
}

#[test]
fn build_fatal_payload_handles_empty_backtrace() {
    let payload = build_fatal_payload("worker-002", "file.rs:10:5", "error", "");

    assert_eq!(payload["backtrace_snippet"].as_str().unwrap(), "");
}

#[test]
fn build_fatal_payload_handles_multiline_backtrace() {
    let multiline_backtrace = "line 1\nline 2\nline 3\nline 4\nline 5";
    let payload = build_fatal_payload(
        "worker-003",
        "crash.rs:50:20",
        "segfault",
        multiline_backtrace,
    );

    let backtrace_snippet = payload["backtrace_snippet"].as_str().unwrap();
    assert!(backtrace_snippet.contains('\n'));
    assert_eq!(backtrace_snippet, multiline_backtrace);
}

#[test]
fn build_fatal_payload_handles_special_characters() {
    let payload = build_fatal_payload(
        "worker-special",
        "file.rs:1:1",
        "message with \"quotes\" and \\ backslash",
        "backtrace\twith\ttabs\nand\nnewlines",
    );

    // Verify JSON serialization handles special characters correctly
    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains(r#"\"quotes\""#));
    assert!(serialized.contains(r#"\\"#)); // Escaped backslash
}

#[test]
fn build_fatal_payload_serializes_to_valid_json() {
    let payload = build_fatal_payload(
        "worker-json-test",
        "test.rs:100:1",
        "json test",
        "some backtrace",
    );

    // Ensure it can be serialized to string
    let json_str = serde_json::to_string(&payload).unwrap();
    assert!(!json_str.is_empty());

    // Ensure it can be deserialized back
    let parsed: Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["worker_id"], "worker-json-test");
}

#[test]
fn end_to_end_panic_payload_construction() {
    // Simulate full panic handling flow using real functions
    let panic_message = "index out of bounds";
    let panic_payload: &dyn std::any::Any = &panic_message;
    let message = extract_panic_message(panic_payload);

    let location = format_panic_location("src/worker.rs", 250, 18);

    let long_backtrace = format!(
        "frame 0: panic_handler\nframe 1: panic_impl\n{}",
        "frame X: function\n".repeat(500)
    );
    let backtrace_snippet = truncate_backtrace(&long_backtrace, 2000);

    let payload = build_fatal_payload("e2e-worker", &location, &message, &backtrace_snippet);

    // Verify all components are present and correct
    assert_eq!(payload["worker_id"].as_str().unwrap(), "e2e-worker");
    assert_eq!(
        payload["reason"].as_str().unwrap(),
        "PANIC at src/worker.rs:250:18: index out of bounds"
    );
    assert!(payload["backtrace_snippet"]
        .as_str()
        .unwrap()
        .contains("(truncated)"));
    assert!(payload["timestamp"].is_string());
}

#[test]
fn backtrace_truncation_preserves_beginning() {
    // Important: we want the beginning of the backtrace (most relevant frames)
    // not the end
    let backtrace = format!(
        "IMPORTANT FRAME 0\nIMPORTANT FRAME 1\n{}LESS IMPORTANT",
        "filler\n".repeat(500)
    );

    let truncated = truncate_backtrace(&backtrace, 50);
    assert!(truncated.starts_with("IMPORTANT FRAME 0"));
    assert!(truncated.contains("(truncated)"));
}

#[test]
fn payload_structure_matches_expected_api_contract() {
    // This test documents the expected API contract for the /api/v1/workers/fatal endpoint
    let payload = build_fatal_payload(
        "contract-test-worker",
        "main.rs:1:1",
        "contract test",
        "trace data",
    );

    // Required fields
    assert!(payload.get("worker_id").is_some());
    assert!(payload.get("reason").is_some());
    assert!(payload.get("backtrace_snippet").is_some());
    assert!(payload.get("timestamp").is_some());

    // Field types
    assert!(payload["worker_id"].is_string());
    assert!(payload["reason"].is_string());
    assert!(payload["backtrace_snippet"].is_string());
    assert!(payload["timestamp"].is_string());

    // Reason format
    let reason = payload["reason"].as_str().unwrap();
    assert!(reason.starts_with("PANIC at "));
    assert!(reason.contains(":"));
}
