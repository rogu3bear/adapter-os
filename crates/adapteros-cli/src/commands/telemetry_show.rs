//! Show telemetry events in human-readable format

use anyhow::{Context, Result};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub async fn run(
    bundle_path: &Path,
    human: bool,
    filter: Option<&str>,
    limit: Option<usize>,
) -> Result<()> {
    let mode = crate::output::OutputMode::from_env();

    if mode.is_verbose() {
        println!("Reading telemetry from: {}", bundle_path.display());
    }

    // Determine if path is a file or directory
    let files = if bundle_path.is_dir() {
        // Collect all .ndjson files
        std::fs::read_dir(bundle_path)
            .context("Failed to read directory")?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("ndjson"))
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    } else {
        vec![bundle_path.to_path_buf()]
    };

    if files.is_empty() {
        anyhow::bail!("No telemetry bundle files found");
    }

    let mut total_events = 0;
    let mut displayed_events = 0;

    for file_path in files {
        if mode.is_verbose() {
            let file_name = file_path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid bundle file path"))?;
            println!("\n📦 Bundle: {}", file_name.to_string_lossy());
        }

        let file = File::open(&file_path)
            .with_context(|| format!("Failed to open {}", file_path.display()))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.context("Failed to read line")?;
            if line.trim().is_empty() {
                continue;
            }

            total_events += 1;

            let event: Value =
                serde_json::from_str(&line).context("Failed to parse telemetry event")?;

            // Apply filter if specified
            if let Some(filter_type) = filter {
                if let Some(event_type) = event["event_type"].as_str() {
                    if !event_type.contains(filter_type) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Check limit
            if let Some(max) = limit {
                if displayed_events >= max {
                    break;
                }
            }

            if human {
                display_human(&event)?;
            } else {
                display_json(&event)?;
            }

            displayed_events += 1;
        }

        // Check if we've hit the limit
        if let Some(max) = limit {
            if displayed_events >= max {
                break;
            }
        }
    }

    // Summary
    if mode.is_verbose() {
        println!("\n📊 Summary:");
        println!("  Total events: {}", total_events);
        println!("  Displayed: {}", displayed_events);
        if let Some(filter_val) = filter {
            println!("  Filtered by: {}", filter_val);
        }
    } else {
        println!("Events: {}/{}", displayed_events, total_events);
    }

    Ok(())
}

fn display_human(event: &Value) -> Result<()> {
    let timestamp_ns = event["timestamp"].as_u64().unwrap_or(0);
    let timestamp_ms = timestamp_ns / 1_000_000;
    let event_type = event["event_type"].as_str().unwrap_or("unknown");
    let payload = &event["payload"];

    // Convert nanoseconds to datetime
    let dt = chrono::DateTime::from_timestamp_millis(timestamp_ms as i64)
        .unwrap_or_else(|| chrono::Utc::now());

    println!(
        "\n[{}] {}",
        dt.format("%Y-%m-%d %H:%M:%S%.3f"),
        event_type.to_uppercase()
    );

    // Format payload based on event type
    match event_type {
        "router.decision" => {
            if let Some(adapters) = payload["adapters"].as_array() {
                print!("  Adapters: [");
                for (i, adapter) in adapters.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    let id = adapter["id"].as_str().unwrap_or("?");
                    let gate = adapter["gate"].as_f64().unwrap_or(0.0);
                    print!("{} ({:.2})", id, gate);
                }
                println!("]");
            }
            if let Some(k) = payload["k"].as_u64() {
                println!("  K: {}", k);
            }
            if let Some(token) = payload["token"].as_u64() {
                println!("  Token: {}", token);
            }
        }
        "inference.token" => {
            if let Some(token_id) = payload["token_id"].as_u64() {
                println!("  Token ID: {}", token_id);
            }
            if let Some(latency) = payload["latency_ms"].as_f64() {
                println!("  Latency: {:.2}ms", latency);
            }
        }
        "security" => {
            if let Some(event_type) = payload["event_type"].as_str() {
                println!("  Type: {}", event_type);
            }
            if let Some(details) = payload["details"].as_str() {
                println!("  Details: {}", details);
            }
            if let Some(blocked) = payload["blocked"].as_bool() {
                println!("  Blocked: {}", blocked);
            }
        }
        "policy.abstain" => {
            if let Some(reason) = payload["reason"].as_str() {
                println!("  Reason: {}", reason);
            }
            if let Some(missing_fields) = payload["missing_fields"].as_array() {
                print!("  Missing: [");
                for (i, field) in missing_fields.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    print!("{}", field.as_str().unwrap_or("?"));
                }
                println!("]");
            }
        }
        "adapter.evict" => {
            if let Some(adapter_id) = payload["adapter_id"].as_str() {
                println!("  Adapter: {}", adapter_id);
            }
            if let Some(reason) = payload["reason"].as_str() {
                println!("  Reason: {}", reason);
            }
        }
        "memory.pressure" => {
            if let Some(used) = payload["used_mb"].as_f64() {
                println!("  Used: {:.2} MB", used);
            }
            if let Some(total) = payload["total_mb"].as_f64() {
                println!("  Total: {:.2} MB", total);
            }
            if let Some(headroom) = payload["headroom_pct"].as_f64() {
                println!("  Headroom: {:.1}%", headroom);
            }
        }
        _ => {
            // Generic display for unknown event types
            if payload.is_object() {
                if let Some(obj) = payload.as_object() {
                    for (key, value) in obj {
                        println!("  {}: {}", key, format_value(value));
                    }
                }
            }
        }
    }

    Ok(())
}

fn display_json(event: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(event)?);
    Ok(())
}

fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(arr) => format!("[{} items]", arr.len()),
        Value::Object(obj) => format!("{{...}}",),
        Value::Null => "null".to_string(),
    }
}
