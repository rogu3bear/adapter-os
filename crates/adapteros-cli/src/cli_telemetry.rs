//! CLI telemetry helpers

use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder, TelemetryWriter};
use anyhow::Result;
use once_cell::sync::OnceCell;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Global telemetry writer for CLI operations
static CLI_TELEMETRY_WRITER: OnceCell<Arc<TelemetryWriter>> = OnceCell::new();

/// Initialize the CLI telemetry writer
///
/// Should be called once at CLI startup. Uses var/telemetry as the default output directory.
pub fn init_cli_telemetry() -> Result<()> {
    let telemetry_dir = std::env::var("AOS_TELEMETRY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/telemetry"));

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&telemetry_dir)?;

    let writer = TelemetryWriter::new(
        telemetry_dir,
        1000,    // max events per bundle
        1 << 20, // 1MB max bundle size
    )?;

    CLI_TELEMETRY_WRITER
        .set(Arc::new(writer))
        .map_err(|_| anyhow::anyhow!("CLI telemetry already initialized"))?;

    Ok(())
}

/// Get the CLI telemetry writer (initializes if needed)
fn get_telemetry_writer() -> Option<Arc<TelemetryWriter>> {
    CLI_TELEMETRY_WRITER.get().cloned()
}

/// Create a CLI identity envelope for telemetry events
fn cli_identity(command: &str, tenant: Option<&str>) -> IdentityEnvelope {
    IdentityEnvelope::new(
        tenant.unwrap_or("system").to_string(),
        "cli".to_string(),
        command.to_string(),
        "1.0".to_string(),
    )
}

/// Emit a CLI error event to telemetry and return event ID
pub async fn emit_cli_error(
    code: Option<&str>,
    command: &str,
    tenant: Option<&str>,
    error_message: &str,
) -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    // Generate event ID based on timestamp and hash of error details
    let event_id_source = format!(
        "{}{}{}{}",
        timestamp,
        code.unwrap_or("unknown"),
        command,
        error_message
    );
    let event_id = hex::encode(blake3::hash(event_id_source.as_bytes()).as_bytes());

    // Log to tracing (if enabled)
    tracing::error!(
        code = code,
        command = command,
        tenant = tenant,
        "CLI error: {}",
        error_message
    );

    // Emit to telemetry system via adapteros-telemetry
    if let Some(writer) = get_telemetry_writer() {
        let identity = cli_identity(command, tenant);
        let metadata = json!({
            "event_id": event_id,
            "code": code,
            "error_message": error_message,
            "timestamp": timestamp,
        });

        let event = TelemetryEventBuilder::new(
            EventType::Custom("cli.error".to_string()),
            LogLevel::Error,
            format!("CLI error in {}: {}", command, error_message),
            identity,
        )
        .metadata(metadata)
        .build()
        .ok();

        if let Some(event) = event {
            if let Err(e) = writer.log_event(event) {
                tracing::warn!(error = %e, "Failed to emit CLI error event to telemetry");
            }
        }
    } else {
        // Fallback: write to simple log file if telemetry not initialized
        if let Ok(telemetry_dir) = std::env::var("AOS_TELEMETRY_DIR") {
            let log_path = std::path::Path::new(&telemetry_dir).join("cli_errors.jsonl");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let event = json!({
                    "event_id": event_id,
                    "event_type": "cli.error",
                    "code": code,
                    "command": command,
                    "tenant": tenant,
                    "error_message": error_message,
                    "timestamp": timestamp,
                });
                let _ = writeln!(file, "{}", event);
            }
        }
    }

    Ok(event_id)
}

/// Emit a CLI command execution event
pub async fn emit_cli_command(command: &str, tenant: Option<&str>, success: bool) -> Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    tracing::info!(
        command = command,
        tenant = tenant,
        success = success,
        "CLI command executed"
    );

    // Emit to telemetry system via adapteros-telemetry
    if let Some(writer) = get_telemetry_writer() {
        let identity = cli_identity(command, tenant);
        let metadata = json!({
            "command": command,
            "success": success,
            "timestamp": timestamp,
        });

        let level = if success {
            LogLevel::Info
        } else {
            LogLevel::Warn
        };

        let event = TelemetryEventBuilder::new(
            EventType::Custom("cli.command".to_string()),
            level,
            format!(
                "CLI command {} {}",
                command,
                if success { "succeeded" } else { "failed" }
            ),
            identity,
        )
        .metadata(metadata)
        .build()
        .ok();

        if let Some(event) = event {
            if let Err(e) = writer.log_event(event) {
                tracing::warn!(error = %e, "Failed to emit CLI command event to telemetry");
            }
        }
    } else {
        // Fallback: write to simple log file if telemetry not initialized
        if let Ok(telemetry_dir) = std::env::var("AOS_TELEMETRY_DIR") {
            let log_path = std::path::Path::new(&telemetry_dir).join("cli_commands.jsonl");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let event = json!({
                    "event_type": "cli.command",
                    "command": command,
                    "tenant": tenant,
                    "success": success,
                    "timestamp": timestamp,
                });
                let _ = writeln!(file, "{}", event);
            }
        }
    }

    Ok(())
}

/// Helper to extract error code from AosError
pub fn extract_error_code(error: &anyhow::Error) -> Option<String> {
    let error_str = format!("{:?}", error);

    // Try to extract error code from the error string
    // Look for common AosError patterns
    if error_str.contains("InvalidHash") {
        Some("E1004".to_string())
    } else if error_str.contains("PolicyViolation") {
        Some("E2002".to_string())
    } else if error_str.contains("DeterminismViolation") {
        Some("E2001".to_string())
    } else if error_str.contains("EgressViolation") {
        Some("E2003".to_string())
    } else if error_str.contains("InvalidManifest") {
        Some("E3003".to_string())
    } else if error_str.contains("Kernel") {
        Some("E3002".to_string())
    } else if error_str.contains("Telemetry") {
        Some("E4002".to_string())
    } else if error_str.contains("Artifact") {
        Some("E5001".to_string())
    } else if error_str.contains("Registry") {
        Some("E6001".to_string())
    } else if error_str.contains("ConfigurationError") {
        Some("E6005".to_string())
    } else if error_str.contains("InvalidInput") {
        Some("E6006".to_string())
    } else if error_str.contains("NetworkError") {
        Some("E6005".to_string())
    } else if error_str.contains("SerializationError") {
        Some("E6007".to_string())
    } else if error_str.contains("Database") || error_str.contains("Sqlite") {
        Some("E8003".to_string())
    } else if error_str.contains("permission") || error_str.contains("Permission") {
        Some("E9002".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_error_code() {
        let err = anyhow::anyhow!("InvalidHash: bad hash");
        assert_eq!(extract_error_code(&err), Some("E1004".to_string()));

        let err = anyhow::anyhow!("PolicyViolation: egress blocked");
        assert_eq!(extract_error_code(&err), Some("E2002".to_string()));

        let err = anyhow::anyhow!("Unknown error");
        assert_eq!(extract_error_code(&err), None);
    }
}
