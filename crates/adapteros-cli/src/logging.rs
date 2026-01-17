//! Unified logging configuration for AdapterOS CLI
//!
//! Provides centralized logging initialization and configuration for all CLI commands.
//! Replaces println! usage with structured tracing throughout the CLI.
//!
//! # Citations
//! - CONTRIBUTING.md L123: "Use `tracing` for logging (not `println!`)"
//! - AGENTS.md L130: "Use `tracing` for logging (not `println!`)"
//! - Policy Pack #9 (Telemetry): "MUST log events with canonical JSON"
//!
//! # Examples
//!
//! ```rust
//! use adapteros_cli::logging::init_logging;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     init_logging()?;
//!     tracing::info!("CLI started");
//!     // ... rest of CLI logic
//! }
//! ```

use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::Result;
use adapteros_telemetry::{EventType, LogLevel, TelemetryEventBuilder};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize unified logging for CLI commands
///
/// # Returns
///
/// * `Result<()>` - Success or error
///
/// # Policy Compliance
///
/// - Policy Pack #9 (Telemetry): Uses structured logging with canonical JSON
/// - CONTRIBUTING.md L123: Uses `tracing` instead of `println!`
pub fn init_logging() -> Result<()> {
    // Configure tracing subscriber with structured output
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = fmt::layer()
        // Keep logs off stdout so JSON command output stays clean.
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .json() // Canonical JSON per Policy Pack #9
        .with_current_span(false)
        .with_span_list(false);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    info!(component = "cli", "CLI logging initialized");
    Ok(())
}

/// Log CLI command execution with structured context
///
/// # Arguments
///
/// * `command` - CLI command being executed
/// * `args` - Command arguments
/// * `result` - Execution result
///
/// # Policy Compliance
///
/// - Policy Pack #9 (Telemetry): Structured logging with canonical JSON
pub fn log_command_execution(command: &str, args: &[String], result: &Result<()>) {
    match result {
        Ok(_) => {
            let identity = IdentityEnvelope::new(
                "system".to_string(),
                "cli".to_string(),
                "command".to_string(),
                "1.0".to_string(),
            );
            let _event = TelemetryEventBuilder::new(
                EventType::UserAction,
                LogLevel::Info,
                format!("CLI command executed successfully: {}", command),
                identity,
            )
            .component("adapteros-cli".to_string())
            .metadata(serde_json::json!({
                "command": command,
                "args": args,
                "status": "success"
            }))
            .build();

            info!(
                command = command,
                args = ?args,
                "CLI command executed successfully"
            );
        }
        Err(e) => {
            let identity = IdentityEnvelope::new(
                "system".to_string(),
                "cli".to_string(),
                "command".to_string(),
                "1.0".to_string(),
            );
            let _event = TelemetryEventBuilder::new(
                EventType::UserError,
                LogLevel::Error,
                format!("CLI command failed: {}", command),
                identity,
            )
            .component("adapteros-cli".to_string())
            .metadata(serde_json::json!({
                "command": command,
                "args": args,
                "error": e.to_string(),
                "status": "failure"
            }))
            .build();

            error!(
                command = command,
                args = ?args,
                error = %e,
                "CLI command failed"
            );
        }
    }
}

/// Log CLI operation with structured context
///
/// # Arguments
///
/// * `operation` - Operation being performed
/// * `context` - Additional context data
/// * `result` - Operation result
///
/// # Policy Compliance
///
/// - Policy Pack #9 (Telemetry): Structured logging with canonical JSON
pub fn log_operation(operation: &str, context: &serde_json::Value, result: &Result<()>) {
    match result {
        Ok(_) => {
            info!(
                operation = operation,
                context = %context,
                "Operation completed successfully"
            );
        }
        Err(e) => {
            error!(
                operation = operation,
                context = %context,
                error = %e,
                "Operation failed"
            );
        }
    }
}

/// Log CLI warning with structured context
///
/// # Arguments
///
/// * `message` - Warning message
/// * `context` - Additional context data
///
/// # Policy Compliance
///
/// - Policy Pack #9 (Telemetry): Structured logging with canonical JSON
pub fn log_warning(message: &str, context: &serde_json::Value) {
    warn!(
        message = message,
        context = %context,
        "CLI warning"
    );
}

/// Log CLI debug information with structured context
///
/// # Arguments
///
/// * `message` - Debug message
/// * `context` - Additional context data
///
/// # Policy Compliance
///
/// - Policy Pack #9 (Telemetry): Structured logging with canonical JSON
pub fn log_debug(message: &str, context: &serde_json::Value) {
    debug!(
        message = message,
        context = %context,
        "CLI debug information"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::AosError;

    #[test]
    fn test_logging_initialization() {
        // Test that logging can be initialized without errors
        let result = init_logging();
        assert!(result.is_ok());
    }

    #[test]
    fn test_command_logging() {
        // Test command execution logging
        let args = vec!["list".to_string(), "adapters".to_string()];
        log_command_execution("adapter", &args, &Ok(()));
        log_command_execution(
            "adapter",
            &args,
            &Err(AosError::Internal("test error".to_string())),
        );
    }

    #[test]
    fn test_operation_logging() {
        // Test operation logging
        let context = serde_json::json!({"tenant_id": "test"});
        log_operation("list_adapters", &context, &Ok(()));
        log_operation(
            "list_adapters",
            &context,
            &Err(AosError::Internal("test error".to_string())),
        );
    }
}
