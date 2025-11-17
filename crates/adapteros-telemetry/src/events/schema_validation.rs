//! Schema validation telemetry events
//!
//! Events for tracking API response schema validation results.

use crate::{EventType, LogLevel, TelemetryEvent, TelemetryEventBuilder};
use serde_json::json;

/// Schema validation success event
pub fn schema_validation_success(schema_name: &str, response_size: usize, validation_time_us: u64) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.success".to_string()),
        LogLevel::Debug,
        format!("Schema validation passed for '{}' ({} bytes, {}μs)", schema_name, response_size, validation_time_us),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "schema_name": schema_name,
        "response_size_bytes": response_size,
        "validation_time_us": validation_time_us,
        "result": "success"
    }))
    .build()
}

/// Schema validation failure event
pub fn schema_validation_failure(schema_name: &str, error_message: &str, response_size: usize, validation_time_us: u64) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.failure".to_string()),
        LogLevel::Warn,
        format!("Schema validation failed for '{}': {} ({} bytes, {}μs)", schema_name, error_message, response_size, validation_time_us),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "schema_name": schema_name,
        "error_message": error_message,
        "response_size_bytes": response_size,
        "validation_time_us": validation_time_us,
        "result": "failure"
    }))
    .build()
}

/// Schema validation skipped event
pub fn schema_validation_skipped(schema_name: &str, reason: &str) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.skipped".to_string()),
        LogLevel::Debug,
        format!("Schema validation skipped for '{}': {}", schema_name, reason),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "schema_name": schema_name,
        "reason": reason,
        "result": "skipped"
    }))
    .build()
}

/// Schema registration event
pub fn schema_registered(schema_name: &str, version: &str) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.registered".to_string()),
        LogLevel::Info,
        format!("Response schema '{}' v{} registered", schema_name, version),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "schema_name": schema_name,
        "version": version,
        "action": "registered"
    }))
    .build()
}

/// Response validation summary event
pub fn response_validation_summary(
    total_responses: u64,
    valid_responses: u64,
    invalid_responses: u64,
    avg_validation_time_us: f64,
    period_seconds: u64,
) -> TelemetryEvent {
    let error_rate = if total_responses > 0 {
        (invalid_responses as f64 / total_responses as f64) * 100.0
    } else {
        0.0
    };

    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.summary".to_string()),
        LogLevel::Info,
        format!(
            "Response validation summary: {}/{} valid ({:.2}% error rate, {:.0}μs avg)",
            valid_responses, total_responses, error_rate, avg_validation_time_us
        ),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "total_responses": total_responses,
        "valid_responses": valid_responses,
        "invalid_responses": invalid_responses,
        "error_rate_percent": error_rate,
        "avg_validation_time_us": avg_validation_time_us,
        "period_seconds": period_seconds
    }))
    .build()
}

/// Schema compliance alert event
pub fn schema_compliance_alert(schema_name: &str, recent_errors: u32, threshold: u32) -> TelemetryEvent {
    TelemetryEventBuilder::new(
        EventType::Custom("schema_validation.compliance_alert".to_string()),
        LogLevel::Error,
        format!("Schema '{}' compliance alert: {} recent errors (threshold: {})", schema_name, recent_errors, threshold),
    )
    .component("adapteros-server-api".to_string())
    .metadata(json!({
        "schema_name": schema_name,
        "recent_errors": recent_errors,
        "threshold": threshold,
        "alert_type": "compliance_threshold_exceeded"
    }))
    .build()
}
