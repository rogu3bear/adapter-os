//! CLI telemetry helpers

use anyhow::Result;
use serde_json::json;
use std::time::SystemTime;

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

    let event = json!({
        "event_id": event_id,
        "event_type": "cli.error",
        "code": code,
        "command": command,
        "tenant": tenant,
        "error_message": error_message,
        "timestamp": timestamp,
    });

    // Log to tracing (if enabled)
    tracing::error!(
        code = code,
        command = command,
        tenant = tenant,
        "CLI error: {}",
        error_message
    );

    // In a real implementation, this would write to the telemetry system
    // For now, we'll just log it. The actual telemetry integration would
    // happen in mplora-telemetry crate and be called from here.

    // TODO: Integrate with mplora-telemetry::emit_event when available
    // For now, just write to a simple log file if var/telemetry exists
    if let Ok(telemetry_dir) = std::env::var("AOS_TELEMETRY_DIR") {
        let log_path = std::path::Path::new(&telemetry_dir).join("cli_errors.jsonl");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", event);
        }
    }

    Ok(event_id)
}

/// Emit a CLI command execution event
pub async fn emit_cli_command(command: &str, tenant: Option<&str>, success: bool) -> Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    let event = json!({
        "event_type": "cli.command",
        "command": command,
        "tenant": tenant,
        "success": success,
        "timestamp": timestamp,
    });

    tracing::info!(
        command = command,
        tenant = tenant,
        success = success,
        "CLI command executed"
    );

    if let Ok(telemetry_dir) = std::env::var("AOS_TELEMETRY_DIR") {
        let log_path = std::path::Path::new(&telemetry_dir).join("cli_commands.jsonl");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", event);
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
