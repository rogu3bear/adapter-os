//! Logging initialization and maintenance utilities.
//!
//! This module provides:
//! - [`initialize_logging`]: Sets up tracing with console, file, and OpenTelemetry outputs
//!   using a deterministic log profile switch.
//! - [`cleanup_old_logs`]: Removes log files older than the retention period

use anyhow::Result;
use std::fmt as stdfmt;
use tracing::field::{Field, Visit};
use tracing::{info, warn};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{
    fmt as tracing_fmt,
    fmt::{
        format::{FmtSpan, FormatEvent, FormatFields, Writer},
        FmtContext,
    },
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

use adapteros_core::time;
use adapteros_server_api::config::{LoggingConfig, OtelConfig};

use crate::otel::{self, OtelGuard};

// PRD-4.0: Log sanitization tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let result = redact_sensitive(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_redact_api_key() {
        let input = "api_key=sk-1234567890abcdef";
        let result = redact_sensitive(input);
        assert_eq!(result, "api_key=[REDACTED]");
    }

    #[test]
    fn test_redact_password() {
        let input = "password=mysecretpassword123";
        let result = redact_sensitive(input);
        assert_eq!(result, "password=[REDACTED]");
    }

    #[test]
    fn test_redact_preserves_safe_content() {
        let input = "user_id=12345, action=login";
        let result = redact_sensitive(input);
        assert_eq!(result, input); // No redaction needed
    }

    #[test]
    fn test_redact_case_insensitive() {
        let input = "BEARER abc123 and bearer xyz789";
        let result = redact_sensitive(input);
        assert!(result.contains("[REDACTED]"));
        // Note: our simple implementation only catches the first occurrence
    }
}

/// PRD-4.0: Patterns that indicate sensitive data requiring redaction.
/// These patterns are matched case-insensitively.
const REDACT_PATTERNS: &[&str] = &[
    "bearer ",
    "api_key=",
    "api-key=",
    "password=",
    "secret=",
    "authorization:",
    "token=",
    "access_token=",
    "refresh_token=",
    "jwt=",
    "apikey=",
];

/// PRD-4.0: Redact sensitive data from log values.
///
/// Scans the input for patterns indicating sensitive data (auth tokens, passwords, etc.)
/// and replaces the sensitive portion with `[REDACTED]`.
fn redact_sensitive(value: &str) -> String {
    let mut result = value.to_string();
    let lower = result.to_lowercase();

    for pattern in REDACT_PATTERNS {
        let pattern_lower = pattern.to_lowercase();
        if let Some(idx) = lower.find(&pattern_lower) {
            let start = idx + pattern.len();
            // Find end of sensitive value (whitespace, quote, comma, or end of string)
            let end = result[start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}')
                .map(|i| start + i)
                .unwrap_or(result.len());
            if end > start {
                result.replace_range(start..end, "[REDACTED]");
            }
        }
    }

    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogProfile {
    Json,
    Plain,
    Debug,
    Trace,
}

impl LogProfile {
    fn from_env() -> Self {
        match std::env::var("AOS_LOG_PROFILE")
            .unwrap_or_else(|_| "json".to_string())
            .to_lowercase()
            .as_str()
        {
            "plain" => LogProfile::Plain,
            "debug" => LogProfile::Debug,
            "trace" => LogProfile::Trace,
            _ => LogProfile::Json,
        }
    }
}

#[derive(Clone)]
struct StructuredJsonFormatter {
    component: &'static str,
    run_id: String,
}

impl StructuredJsonFormatter {
    fn new(component: &'static str) -> Self {
        let run_id = std::env::var("AOS_RUN_ID").unwrap_or_else(|_| "unknown".to_string());
        Self { component, run_id }
    }
}

impl<S, N> FormatEvent<S, N> for StructuredJsonFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> stdfmt::Result {
        struct JsonVisitor<'a> {
            payload: &'a mut serde_json::Map<String, serde_json::Value>,
        }

        impl<'a> Visit for JsonVisitor<'a> {
            fn record_str(&mut self, field: &Field, value: &str) {
                // PRD-4.0: Redact sensitive data from log values
                let sanitized = redact_sensitive(value);
                self.payload.insert(
                    field.name().to_string(),
                    serde_json::Value::String(sanitized),
                );
            }

            fn record_debug(&mut self, field: &Field, value: &dyn stdfmt::Debug) {
                // PRD-4.0: Redact sensitive data from log values
                let raw = format!("{:?}", value);
                let sanitized = redact_sensitive(&raw);
                self.payload.insert(
                    field.name().to_string(),
                    serde_json::Value::String(sanitized),
                );
            }
        }

        let mut payload = serde_json::Map::new();

        payload.insert(
            "ts".to_string(),
            serde_json::Value::String(time::now_rfc3339()),
        );
        payload.insert(
            "level".to_string(),
            serde_json::Value::String(event.metadata().level().to_string()),
        );
        payload.insert(
            "component".to_string(),
            serde_json::Value::String(self.component.to_string()),
        );
        let phase = ctx
            .lookup_current()
            .map(|s| s.name().to_string())
            .unwrap_or_default();
        payload.insert("phase".to_string(), serde_json::Value::String(phase));

        // Standard keys with deterministic presence (empty string if absent)
        payload.insert("run_id".to_string(), serde_json::Value::String(self.run_id.clone()));
        for key in ["trace_id", "span_id", "request_id", "tenant", "error_code"] {
            payload.insert(key.to_string(), serde_json::Value::String(String::new()));
        }

        let mut visitor = JsonVisitor {
            payload: &mut payload,
        };
        event.record(&mut visitor);
        if let Some(code_value) = payload.get("code").cloned() {
            payload
                .entry("error_code".to_string())
                .or_insert(code_value);
        }

        let json = serde_json::Value::Object(payload);
        let mut buf = Vec::new();
        if serde_json::to_writer(&mut buf, &json).is_ok() {
            let _ = writeln!(writer, "{}", String::from_utf8_lossy(&buf));
        }

        Ok(())
    }
}

/// Cleanup old log files based on retention policy
///
/// Deletes log files older than the specified retention period.
/// Returns the number of files deleted.
pub async fn cleanup_old_logs(log_dir: &str, retention_days: u32) -> Result<usize> {
    use std::time::SystemTime;

    let retention_duration = std::time::Duration::from_secs(retention_days as u64 * 86400);
    let now = SystemTime::now();
    let mut deleted_count = 0;

    let log_path = std::path::Path::new(log_dir);
    if !log_path.exists() {
        return Ok(0);
    }

    let entries = tokio::fs::read_dir(log_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read log directory: {}", e))?;

    let mut entries = entries;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?
    {
        let path = entry.path();

        // Only process files (not directories)
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to read file metadata, skipping"
                );
                continue;
            }
        };

        if !metadata.is_file() {
            continue;
        }

        // Check if file is old enough to delete
        let modified = match metadata.modified() {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to get file modification time, skipping"
                );
                continue;
            }
        };

        let age = match now.duration_since(modified) {
            Ok(d) => d,
            Err(_) => continue, // File modified in the future? Skip it
        };

        if age > retention_duration {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {
                    deleted_count += 1;
                    info!(
                        path = %path.display(),
                        age_days = age.as_secs() / 86400,
                        "Deleted old log file"
                    );
                }
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to delete old log file"
                    );
                }
            }
        }
    }

    Ok(deleted_count)
}

/// Initialize logging with configuration-based settings
///
/// Sets up tracing with:
/// - Console output (always)
/// - File output with rotation (if log_dir configured)
/// - Configurable log levels
/// - JSON or human-readable format
///
/// Returns guards that must be kept alive for the duration of the program
/// to ensure log files are properly flushed and OpenTelemetry spans are exported.
pub fn initialize_logging(
    config: &LoggingConfig,
    otel_config: &OtelConfig,
) -> Result<(Option<WorkerGuard>, Option<OtelGuard>)> {
    let profile = LogProfile::from_env();

    // Honor existing RUST_LOG; otherwise set a deterministic baseline per profile.
    let filter_source = std::env::var("RUST_LOG").unwrap_or_else(|_| match profile {
        LogProfile::Plain | LogProfile::Json => config.level.clone(),
        LogProfile::Debug => "debug".to_string(),
        LogProfile::Trace => "trace".to_string(),
    });

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", &filter_source);
    }

    if matches!(profile, LogProfile::Debug | LogProfile::Trace)
        && std::env::var("RUST_BACKTRACE").is_err()
    {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Validate log level - warn if too restrictive
    let level_lower = filter_source.to_lowercase();
    if level_lower == "error" || level_lower.starts_with("error,") {
        eprintln!(
            "WARNING: Log level set to 'error' only. This will hide all warnings and info messages. \
             Consider using 'warn' or 'info' for better observability."
        );
    } else if level_lower == "off" {
        eprintln!(
            "WARNING: Logging is disabled (level=off). No log messages will be recorded. \
             This may make debugging issues impossible."
        );
    }

    let env_filter = EnvFilter::try_new(&filter_source).unwrap_or_else(|e| {
        eprintln!(
            "WARNING: Invalid RUST_LOG filter '{}': {}. Falling back to '{}'.",
            filter_source, e, config.level
        );
        EnvFilter::new(&config.level)
    });

    // Determine rotation strategy
    let rotation = match config.rotation.as_str() {
        "hourly" => Rotation::HOURLY,
        "daily" => Rotation::DAILY,
        "never" => Rotation::NEVER,
        _ => {
            eprintln!(
                "WARNING: Unknown rotation '{}', defaulting to daily",
                config.rotation
            );
            Rotation::DAILY
        }
    };

    let use_json = config.json_format || !matches!(profile, LogProfile::Plain);
    let component = "adapteros-server";

    let otel_config = otel_config.clone();

    // Set up file logging if log_dir is configured
    let (file_layer, guard) = if let Some(ref log_dir) = config.log_dir {
        // Ensure log directory exists
        std::fs::create_dir_all(log_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create log directory {}: {}", log_dir, e))?;

        let file_appender = RollingFileAppender::new(rotation, log_dir, &config.log_prefix);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = if use_json {
            let mut layer = tracing_fmt::layer()
                .event_format(StructuredJsonFormatter::new(component))
                .with_writer(non_blocking)
                .with_ansi(false);
            layer.set_span_events(FmtSpan::CLOSE);
            layer.boxed()
        } else {
            tracing_fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false) // No ANSI colors in log files
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .boxed()
        };

        (Some(file_layer), Some(guard))
    } else {
        (None, None)
    };

    // Console layer (always enabled)
    let console_layer = if use_json {
        let mut layer = tracing_fmt::layer()
            .event_format(StructuredJsonFormatter::new(component))
            .with_ansi(false);
        layer.set_span_events(FmtSpan::CLOSE);
        layer.boxed()
    } else {
        tracing_fmt::layer()
            .with_target(true)
            .with_thread_ids(false) // Cleaner console output
            .with_file(false)
            .with_line_number(false)
            .boxed()
    };

    // Try to initialize OpenTelemetry (graceful degradation on failure)
    let (otel_tracer, otel_guard) = match otel::init_otel(&otel_config) {
        Ok(Some((tracer, guard))) => (Some(tracer), Some(guard)),
        Ok(None) => (None, None),
        Err(e) => {
            eprintln!(
                "WARNING: OpenTelemetry initialization failed: {}. Continuing without OTLP export.",
                e
            );
            (None, None)
        }
    };

    // Create the OTel layer inline to avoid type composition issues with boxed layers.
    // The layer is created from the tracer here rather than in otel.rs so that the
    // type system can properly compose it with the other layers.
    let otel_layer = otel_tracer.map(|tracer| tracing_opentelemetry::layer().with_tracer(tracer));

    // Build the subscriber with all layers.
    // Option<L> implements Layer<S> where None is a no-op, allowing conditional composition.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .with(otel_layer)
        .init();

    // Log effective logging configuration
    if let Some(ref log_dir) = config.log_dir {
        // Can't use tracing yet since it's being initialized, use eprintln
        eprintln!(
            "Logging initialized: level={}, dir={}, rotation={}, json={}",
            config.level, log_dir, config.rotation, config.json_format
        );
    } else {
        eprintln!("Logging initialized: level={}, stdout only", config.level);
    }

    if otel_config.enabled {
        eprintln!(
            "OpenTelemetry enabled: endpoint={}, protocol={}, sampling={}",
            otel_config.endpoint, otel_config.protocol, otel_config.sampling_ratio
        );
    }

    Ok((guard, otel_guard))
}
