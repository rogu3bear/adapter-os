//! Panic handling utilities for worker fatal error reporting.
//!
//! These functions are extracted to a separate module to enable unit testing
//! of the panic hook logic without requiring actual panics.

use serde_json::Value;

/// Extract panic message from panic payload.
///
/// Attempts to downcast the payload to common panic types (&str, String).
/// Returns "Unknown panic" if the payload type is not recognized.
pub fn extract_panic_message(payload: &dyn std::any::Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Format panic location for reporting.
///
/// Creates a standard "file:line:column" format string.
pub fn format_panic_location(file: &str, line: u32, column: u32) -> String {
    let mut location = String::with_capacity(file.len() + 20);
    let _ = std::fmt::Write::write_fmt(&mut location, format_args!("{file}:{line}:{column}"));
    location
}

/// Truncate backtrace to avoid oversized HTTP messages.
///
/// If the backtrace exceeds `max_len` characters, it is truncated and
/// "...(truncated)" is appended to indicate the truncation.
pub fn truncate_backtrace(backtrace_str: &str, max_len: usize) -> String {
    if backtrace_str.len() > max_len {
        format!("{}...(truncated)", &backtrace_str[..max_len])
    } else {
        backtrace_str.to_string()
    }
}

/// Build fatal error payload for control plane notification.
///
/// Creates a JSON payload containing:
/// - `worker_id`: The worker's unique identifier
/// - `reason`: Formatted as "PANIC at {location}: {message}"
/// - `backtrace_snippet`: Truncated backtrace for debugging
/// - `timestamp`: RFC3339 formatted timestamp
pub fn build_fatal_payload(
    worker_id: &str,
    location: &str,
    message: &str,
    backtrace_snippet: &str,
) -> Value {
    let mut reason = String::with_capacity(location.len() + message.len() + 16);
    reason.push_str("PANIC at ");
    reason.push_str(location);
    reason.push_str(": ");
    reason.push_str(message);

    serde_json::json!({
        "worker_id": worker_id,
        "reason": reason,
        "backtrace_snippet": backtrace_snippet,
        "timestamp": chrono::Utc::now().to_rfc3339()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_panic_message_from_str_ref() {
        let msg: &str = "test panic message";
        let payload: &dyn std::any::Any = &msg;
        assert_eq!(extract_panic_message(payload), "test panic message");
    }

    #[test]
    fn test_extract_panic_message_from_string() {
        let msg = String::from("string panic message");
        let payload: &dyn std::any::Any = &msg;
        assert_eq!(extract_panic_message(payload), "string panic message");
    }

    #[test]
    fn test_extract_panic_message_from_unknown_type() {
        let msg: i32 = 42;
        let payload: &dyn std::any::Any = &msg;
        assert_eq!(extract_panic_message(payload), "Unknown panic");
    }

    #[test]
    fn test_format_panic_location_includes_all_components() {
        let result = format_panic_location("src/main.rs", 42, 10);
        assert_eq!(result, "src/main.rs:42:10");
    }

    #[test]
    fn test_format_panic_location_handles_paths_with_slashes() {
        let result = format_panic_location("/home/user/project/src/lib.rs", 100, 5);
        assert_eq!(result, "/home/user/project/src/lib.rs:100:5");
    }

    #[test]
    fn test_truncate_backtrace_preserves_short_strings() {
        let short = "short backtrace";
        assert_eq!(truncate_backtrace(short, 2000), "short backtrace");
    }

    #[test]
    fn test_truncate_backtrace_truncates_long_strings() {
        let long = "a".repeat(3000);
        let result = truncate_backtrace(&long, 2000);
        assert!(result.len() < long.len());
        assert!(result.ends_with("...(truncated)"));
        assert!(result.starts_with("aaaa"));
    }

    #[test]
    fn test_truncate_backtrace_at_exact_boundary() {
        let exact = "a".repeat(2000);
        let result = truncate_backtrace(&exact, 2000);
        assert_eq!(result, exact); // No truncation needed at exactly max_len
    }

    #[test]
    fn test_truncate_backtrace_one_char_over() {
        let over = "a".repeat(2001);
        let result = truncate_backtrace(&over, 2000);
        assert!(result.ends_with("...(truncated)"));
        assert_eq!(result.len(), 2000 + "...(truncated)".len());
    }

    #[test]
    fn test_build_fatal_payload_has_required_fields() {
        let payload = build_fatal_payload("worker-123", "src/main.rs:10:5", "test panic", "bt");
        assert!(payload.get("worker_id").is_some());
        assert!(payload.get("reason").is_some());
        assert!(payload.get("backtrace_snippet").is_some());
        assert!(payload.get("timestamp").is_some());
    }

    #[test]
    fn test_build_fatal_payload_formats_reason_correctly() {
        let payload = build_fatal_payload("w1", "file.rs:1:1", "message", "bt");
        let reason = payload["reason"].as_str().unwrap();
        assert_eq!(reason, "PANIC at file.rs:1:1: message");
    }

    #[test]
    fn test_build_fatal_payload_includes_worker_id() {
        let payload = build_fatal_payload("my-worker-id", "loc", "msg", "bt");
        assert_eq!(payload["worker_id"].as_str().unwrap(), "my-worker-id");
    }

    #[test]
    fn test_build_fatal_payload_includes_backtrace() {
        let payload = build_fatal_payload("w", "l", "m", "my backtrace content");
        assert_eq!(
            payload["backtrace_snippet"].as_str().unwrap(),
            "my backtrace content"
        );
    }

    #[test]
    fn test_build_fatal_payload_timestamp_is_rfc3339() {
        let payload = build_fatal_payload("w", "l", "m", "bt");
        let ts = payload["timestamp"].as_str().unwrap();
        // RFC3339 timestamps contain 'T' separator and timezone
        assert!(ts.contains('T'));
        assert!(ts.contains('+') || ts.contains('Z'));
    }

    #[test]
    fn test_build_fatal_payload_handles_empty_backtrace() {
        let payload = build_fatal_payload("w", "l", "m", "");
        assert_eq!(payload["backtrace_snippet"].as_str().unwrap(), "");
    }

    #[test]
    fn test_build_fatal_payload_handles_special_characters() {
        let payload = build_fatal_payload(
            "worker\"id",
            "path\\with\\backslashes:10:5",
            "message with \"quotes\" and\ttabs",
            "backtrace\nwith\nnewlines",
        );

        // Should serialize without error
        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(json_str.contains("worker\\\"id"));
    }

    #[test]
    fn test_build_fatal_payload_serializes_to_valid_json() {
        let payload = build_fatal_payload("w", "l", "m", "bt");
        let json_str = serde_json::to_string(&payload);
        assert!(json_str.is_ok());

        // Can deserialize back
        let parsed: Value = serde_json::from_str(&json_str.unwrap()).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_truncate_backtrace_preserves_beginning() {
        let backtrace = "IMPORTANT_START ".to_string() + &"x".repeat(3000);
        let result = truncate_backtrace(&backtrace, 2000);
        assert!(result.starts_with("IMPORTANT_START"));
    }
}
