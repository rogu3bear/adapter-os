//! Show telemetry events with filtering and formatting
//!
//! Displays detailed telemetry events from bundle files with support for
//! filtering by time range, event type, and log level.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::output::OutputWriter;

/// Arguments for telemetry show command
#[derive(Debug, Clone)]
pub struct TelemetryShowArgs {
    /// Path to telemetry bundles directory
    pub bundles_dir: String,
    /// Filter by start time (ISO 8601)
    pub start_time: Option<String>,
    /// Filter by end time (ISO 8601)
    pub end_time: Option<String>,
    /// Filter by event type (e.g., "inference.complete", "adapter.loaded")
    pub event_type: Option<String>,
    /// Filter by log level (debug, info, warn, error, critical)
    pub level: Option<String>,
    /// Filter by tenant ID
    pub tenant_id: Option<String>,
    /// Filter by component
    pub component: Option<String>,
    /// Maximum number of events to display
    pub limit: u32,
}

/// Telemetry event structure for display
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DisplayEvent {
    id: String,
    timestamp: String,
    event_type: String,
    level: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

/// Show telemetry events with filtering
pub async fn show_telemetry(args: TelemetryShowArgs, output: &OutputWriter) -> Result<()> {
    let bundles_path = Path::new(&args.bundles_dir);

    if !bundles_path.exists() {
        anyhow::bail!("Bundles directory not found: {}", args.bundles_dir);
    }

    // Parse time filters
    let start_time = args
        .start_time
        .as_ref()
        .map(|s| DateTime::parse_from_rfc3339(s))
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid start_time format: {}", e))?
        .map(|dt| dt.with_timezone(&Utc));

    let end_time = args
        .end_time
        .as_ref()
        .map(|s| DateTime::parse_from_rfc3339(s))
        .transpose()
        .map_err(|e| anyhow::anyhow!("Invalid end_time format: {}", e))?
        .map(|dt| dt.with_timezone(&Utc));

    // Normalize level filter
    let level_filter = args.level.as_ref().map(|l| l.to_lowercase());

    // Collect events from bundle files
    let mut events: Vec<DisplayEvent> = Vec::new();
    let mut bundle_files: Vec<_> = fs::read_dir(bundles_path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "ndjson")
                .unwrap_or(false)
        })
        .collect();

    // Sort by filename (which includes bundle index)
    bundle_files.sort_by_key(|entry| std::cmp::Reverse(entry.path()));

    for entry in bundle_files {
        let content = fs::read_to_string(entry.path())?;

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let event: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Apply filters
            if !matches_filters(&event, &args, &start_time, &end_time, &level_filter) {
                continue;
            }

            // Extract display fields
            let display_event = DisplayEvent {
                id: event["id"].as_str().unwrap_or("-").to_string(),
                timestamp: event["timestamp"].as_str().unwrap_or("-").to_string(),
                event_type: event["event_type"].as_str().unwrap_or("-").to_string(),
                level: event["level"].as_str().unwrap_or("Info").to_string(),
                message: event["message"].as_str().unwrap_or("").to_string(),
                component: event["component"].as_str().map(String::from),
                tenant_id: event["identity"]["tenant_id"]
                    .as_str()
                    .map(String::from)
                    .or_else(|| event["tenant_id"].as_str().map(String::from)),
                metadata: event["metadata"].as_object().map(|m| json!(m)),
            };

            events.push(display_event);

            if events.len() >= args.limit as usize {
                break;
            }
        }

        if events.len() >= args.limit as usize {
            break;
        }
    }

    // Sort events by timestamp (newest first)
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Output results
    if output.is_json() {
        output.print_json(&json!({
            "events": events,
            "count": events.len(),
            "filters": {
                "start_time": args.start_time,
                "end_time": args.end_time,
                "event_type": args.event_type,
                "level": args.level,
                "tenant_id": args.tenant_id,
                "component": args.component,
                "limit": args.limit
            }
        }))?;
    } else {
        output.print_line("Telemetry Events:")?;
        output.print_line(format!(
            "{:<36} {:<24} {:<30} {:<8} {}",
            "ID", "Timestamp", "Event Type", "Level", "Message"
        ))?;
        output.print_line("-".repeat(120))?;

        for event in &events {
            let timestamp_short = if event.timestamp.len() > 19 {
                &event.timestamp[..19]
            } else {
                &event.timestamp
            };

            let message_short = if event.message.len() > 40 {
                format!("{}...", &event.message[..37])
            } else {
                event.message.clone()
            };

            output.print_line(format!(
                "{:<36} {:<24} {:<30} {:<8} {}",
                truncate(&event.id, 36),
                timestamp_short,
                truncate(&event.event_type, 30),
                event.level,
                message_short
            ))?;
        }

        output.print_line("")?;
        output.print_line(format!("Total events: {}", events.len()))?;

        if events.len() >= args.limit as usize {
            output.print_line(format!(
                "Note: Results limited to {} events. Use --limit to increase.",
                args.limit
            ))?;
        }
    }

    Ok(())
}

/// Check if event matches all specified filters
fn matches_filters(
    event: &serde_json::Value,
    args: &TelemetryShowArgs,
    start_time: &Option<DateTime<Utc>>,
    end_time: &Option<DateTime<Utc>>,
    level_filter: &Option<String>,
) -> bool {
    // Time range filter
    if let Some(ts_str) = event["timestamp"].as_str() {
        if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
            let ts_utc = ts.with_timezone(&Utc);
            if let Some(start) = start_time {
                if ts_utc < *start {
                    return false;
                }
            }
            if let Some(end) = end_time {
                if ts_utc > *end {
                    return false;
                }
            }
        }
    }

    // Event type filter
    if let Some(ref filter_type) = args.event_type {
        let event_type = event["event_type"].as_str().unwrap_or("");
        if !event_type.contains(filter_type) && event_type != filter_type {
            return false;
        }
    }

    // Level filter
    if let Some(ref filter_level) = level_filter {
        let event_level = event["level"].as_str().unwrap_or("Info").to_lowercase();
        if event_level != *filter_level {
            return false;
        }
    }

    // Tenant filter
    if let Some(ref filter_tenant) = args.tenant_id {
        let event_tenant = event["identity"]["tenant_id"]
            .as_str()
            .or_else(|| event["tenant_id"].as_str())
            .unwrap_or("");
        if event_tenant != filter_tenant {
            return false;
        }
    }

    // Component filter
    if let Some(ref filter_component) = args.component {
        let event_component = event["component"].as_str().unwrap_or("");
        if event_component != filter_component {
            return false;
        }
    }

    true
}

/// Truncate string to max length
fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a very long string", 10), "this is...");
    }

    #[test]
    fn test_matches_filters_event_type() {
        let event = json!({
            "id": "test",
            "timestamp": "2025-01-01T00:00:00Z",
            "event_type": "adapter.loaded",
            "level": "Info",
            "message": "Test"
        });

        let args = TelemetryShowArgs {
            bundles_dir: ".".to_string(),
            start_time: None,
            end_time: None,
            event_type: Some("adapter".to_string()),
            level: None,
            tenant_id: None,
            component: None,
            limit: 100,
        };

        assert!(matches_filters(&event, &args, &None, &None, &None));

        let args_no_match = TelemetryShowArgs {
            event_type: Some("inference".to_string()),
            ..args.clone()
        };

        assert!(!matches_filters(
            &event,
            &args_no_match,
            &None,
            &None,
            &None
        ));
    }

    #[test]
    fn test_matches_filters_level() {
        let event = json!({
            "id": "test",
            "timestamp": "2025-01-01T00:00:00Z",
            "event_type": "test",
            "level": "Error",
            "message": "Test"
        });

        let args = TelemetryShowArgs {
            bundles_dir: ".".to_string(),
            start_time: None,
            end_time: None,
            event_type: None,
            level: None,
            tenant_id: None,
            component: None,
            limit: 100,
        };

        let level_filter = Some("error".to_string());
        assert!(matches_filters(&event, &args, &None, &None, &level_filter));

        let level_filter_no_match = Some("info".to_string());
        assert!(!matches_filters(
            &event,
            &args,
            &None,
            &None,
            &level_filter_no_match
        ));
    }
}
