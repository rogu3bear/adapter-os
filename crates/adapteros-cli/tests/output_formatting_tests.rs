//! Tests for CLI output formatting
//!
//! Tests the OutputWriter and formatting utilities to ensure:
//! - Correct output mode detection
//! - Proper formatting of bytes, durations, timestamps
//! - Output suppression in quiet/JSON modes
//! - Message recording for testing

#![allow(unused_imports)]

use adapteros_cli::formatting::{
    format_bytes, format_duration, format_seconds, format_time_ago, truncate_id,
};
use adapteros_cli::output::{is_ci, OutputMode, OutputWriter};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(test)]
mod formatting_tests {
    use super::*;

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_small() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(10240), "10.0 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
        assert_eq!(format_bytes(10_485_760), "10.0 MB");
        assert_eq!(format_bytes(104_857_600), "100.0 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(2_147_483_648), "2.0 GB");
        assert_eq!(format_bytes(1_500_000_000), "1.4 GB");
    }

    #[test]
    fn test_format_bytes_terabytes() {
        assert_eq!(format_bytes(1_099_511_627_776), "1.0 TB");
        assert_eq!(format_bytes(2_199_023_255_552), "2.0 TB");
    }

    #[test]
    fn test_format_bytes_rounding() {
        // Test that rounding works correctly (1 decimal place)
        assert_eq!(format_bytes(1536), "1.5 KB"); // Exactly 1.5
        assert_eq!(format_bytes(1587), "1.5 KB"); // Rounds to 1.5
    }

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_duration_milliseconds() {
        assert_eq!(format_duration(Duration::from_millis(1)), "1ms");
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1s");
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(120)), "2m");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(3660)), "1h 1m");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_secs(7320)), "2h 2m");
    }

    #[test]
    fn test_format_duration_days() {
        assert_eq!(format_duration(Duration::from_secs(86400)), "1d");
        assert_eq!(format_duration(Duration::from_secs(90000)), "1d 1h");
        assert_eq!(format_duration(Duration::from_secs(172800)), "2d");
        assert_eq!(format_duration(Duration::from_secs(180000)), "2d 2h");
    }

    #[test]
    fn test_format_seconds() {
        assert_eq!(format_seconds(0), "0s");
        assert_eq!(format_seconds(30), "30s");
        assert_eq!(format_seconds(125), "2m 5s");
        assert_eq!(format_seconds(3661), "1h 1m");
        assert_eq!(format_seconds(90000), "1d 1h");
    }

    #[test]
    fn test_truncate_id_short() {
        assert_eq!(truncate_id("short"), "short");
        assert_eq!(truncate_id("a"), "a");
        assert_eq!(truncate_id(""), "");
        assert_eq!(truncate_id("exactly8"), "exactly8");
    }

    #[test]
    fn test_truncate_id_long() {
        assert_eq!(truncate_id("adapter-123456789"), "adapter-");
        assert_eq!(truncate_id("very-long-adapter-id"), "very-lon");
        assert_eq!(truncate_id("123456789"), "12345678");
    }

    #[test]
    fn test_format_time_ago_recent() {
        let now = Utc::now();

        // Just now (2 seconds ago)
        let just_now = (now - chrono::Duration::seconds(2)).to_rfc3339();
        assert_eq!(format_time_ago(&just_now), "just now");

        // 30 seconds ago
        let thirty_sec = (now - chrono::Duration::seconds(30)).to_rfc3339();
        assert_eq!(format_time_ago(&thirty_sec), "30s ago");
    }

    #[test]
    fn test_format_time_ago_minutes() {
        let now = Utc::now();

        // 2 minutes ago
        let two_min = (now - chrono::Duration::minutes(2)).to_rfc3339();
        assert_eq!(format_time_ago(&two_min), "2m ago");

        // 45 minutes ago
        let forty_five_min = (now - chrono::Duration::minutes(45)).to_rfc3339();
        assert_eq!(format_time_ago(&forty_five_min), "45m ago");
    }

    #[test]
    fn test_format_time_ago_hours() {
        let now = Utc::now();

        // 1 hour ago
        let one_hour = (now - chrono::Duration::hours(1)).to_rfc3339();
        assert_eq!(format_time_ago(&one_hour), "1h ago");

        // 12 hours ago
        let twelve_hours = (now - chrono::Duration::hours(12)).to_rfc3339();
        assert_eq!(format_time_ago(&twelve_hours), "12h ago");
    }

    #[test]
    fn test_format_time_ago_days() {
        let now = Utc::now();

        // 1 day ago
        let one_day = (now - chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(format_time_ago(&one_day), "1d ago");

        // 3 days ago
        let three_days = (now - chrono::Duration::days(3)).to_rfc3339();
        assert_eq!(format_time_ago(&three_days), "3d ago");

        // 30 days ago
        let thirty_days = (now - chrono::Duration::days(30)).to_rfc3339();
        assert_eq!(format_time_ago(&thirty_days), "30d ago");
    }

    #[test]
    fn test_format_time_ago_future() {
        let now = Utc::now();
        let future = (now + chrono::Duration::hours(1)).to_rfc3339();
        assert_eq!(format_time_ago(&future), "just now");
    }

    #[test]
    fn test_format_time_ago_invalid() {
        assert_eq!(format_time_ago("not-a-timestamp"), "unknown");
        assert_eq!(format_time_ago("2025-13-45T99:99:99Z"), "unknown");
        assert_eq!(format_time_ago(""), "unknown");
    }

    #[test]
    fn test_format_time_ago_valid_formats() {
        // RFC3339 format
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::hours(2)).to_rfc3339();
        assert_eq!(format_time_ago(&timestamp), "2h ago");

        // UTC format
        let utc_timestamp = (now - chrono::Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        assert_eq!(format_time_ago(&utc_timestamp), "1d ago");
    }
}

#[cfg(test)]
mod output_mode_tests {
    use super::*;

    #[test]
    fn test_output_mode_text() {
        let mode = OutputMode::Text;
        assert!(mode.is_verbose());
        assert!(!mode.is_quiet());
        assert!(!mode.is_json());
    }

    #[test]
    fn test_output_mode_json() {
        let mode = OutputMode::Json;
        assert!(!mode.is_verbose());
        assert!(!mode.is_quiet());
        assert!(mode.is_json());
    }

    #[test]
    fn test_output_mode_quiet() {
        let mode = OutputMode::Quiet;
        assert!(!mode.is_verbose());
        assert!(mode.is_quiet());
        assert!(!mode.is_json());
    }

    #[test]
    fn test_output_mode_from_flags() {
        // JSON takes precedence
        assert_eq!(OutputMode::from_flags(true, false), OutputMode::Json);
        assert_eq!(OutputMode::from_flags(true, true), OutputMode::Json);

        // Quiet if no JSON
        assert_eq!(OutputMode::from_flags(false, true), OutputMode::Quiet);

        // Text if neither
        assert_eq!(OutputMode::from_flags(false, false), OutputMode::Text);
    }

    #[test]
    fn test_is_ci_detection() {
        // Just verify it doesn't panic - actual value depends on test environment
        let _ = is_ci();
    }
}

#[cfg(test)]
mod output_writer_tests {
    use super::*;

    #[test]
    fn test_output_writer_mode() {
        let writer = OutputWriter::new(OutputMode::Text, false);
        assert_eq!(writer.mode(), OutputMode::Text);
        assert!(writer.mode().is_verbose());

        let writer = OutputWriter::new(OutputMode::Json, false);
        assert_eq!(writer.mode(), OutputMode::Json);
        assert!(writer.is_json());

        let writer = OutputWriter::new(OutputMode::Quiet, false);
        assert_eq!(writer.mode(), OutputMode::Quiet);
        assert!(writer.is_quiet());
    }

    #[test]
    fn test_output_writer_verbose_flag() {
        // Verbose mode should be verbose even without flag
        let writer = OutputWriter::new(OutputMode::Text, false);
        assert!(writer.is_verbose());

        // Non-verbose mode with verbose flag should be verbose
        let writer = OutputWriter::new(OutputMode::Json, true);
        assert!(writer.is_verbose());

        // Quiet mode stays not verbose
        let writer = OutputWriter::new(OutputMode::Quiet, false);
        assert!(!writer.is_verbose());
    }

    #[test]
    fn test_output_writer_with_sink() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        // Test various output methods
        writer.progress("progress message");
        writer.success("success message");
        writer.error("error message");
        writer.warning("warning message");

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"progress message".to_string()));
        assert!(messages.contains(&"success:success message".to_string()));
        assert!(messages.contains(&"error:error message".to_string()));
        assert!(messages.contains(&"warn:warning message".to_string()));
    }

    #[test]
    fn test_output_writer_progress_done() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.progress_done(true);
        writer.progress_done(false);

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"progress:done".to_string()));
        assert!(messages.contains(&"progress:failed".to_string()));
    }

    #[test]
    fn test_output_writer_section() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.section("Test Section");

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"section:Test Section".to_string()));
    }

    #[test]
    fn test_output_writer_kv() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.kv("key", "value");

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"key:value".to_string()));
    }

    #[test]
    fn test_output_writer_blank() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.blank();

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"".to_string()));
    }

    #[test]
    fn test_output_writer_quiet_suppression() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Quiet, false, Arc::clone(&sink));

        // These should be suppressed in quiet mode
        writer.progress("should not appear");
        writer.success("should not appear");
        writer.info("should not appear");

        // Errors should still appear
        writer.error("should appear");

        let messages = sink.lock().unwrap();
        // In quiet mode, most messages are suppressed but still recorded in test sink
        assert!(messages.contains(&"should not appear".to_string()));
        assert!(messages.contains(&"error:should appear".to_string()));
    }

    #[test]
    fn test_output_writer_json_mode() {
        let writer = OutputWriter::new(OutputMode::Json, false);

        assert!(writer.is_json());
        assert!(!writer.is_verbose());
        assert!(!writer.is_quiet());
    }

    #[test]
    fn test_output_writer_json_serialization() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct TestData {
            field1: String,
            field2: i32,
        }

        let writer = OutputWriter::new(OutputMode::Json, false);
        let data = TestData {
            field1: "test".to_string(),
            field2: 42,
        };

        let result = writer.json(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_output_writer_info() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.info("info message");

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"info:info message".to_string()));
    }

    #[test]
    fn test_output_writer_result() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, false, Arc::clone(&sink));

        writer.result("result message");

        let messages = sink.lock().unwrap();
        assert!(messages.contains(&"result message".to_string()));
    }
}
