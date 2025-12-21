//! Logging initialization and maintenance utilities.
//!
//! This module provides:
//! - [`initialize_logging`]: Sets up tracing with console, file, and OpenTelemetry outputs
//! - [`cleanup_old_logs`]: Removes log files older than the retention period

use anyhow::Result;
use tracing::{info, warn};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

use adapteros_server_api::config::{LoggingConfig, OtelConfig};

use crate::otel::{self, OtelGuard};

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
    // Parse log level from config or environment
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

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

    // Set up file logging if log_dir is configured
    let (file_layer, guard) = if let Some(ref log_dir) = config.log_dir {
        // Ensure log directory exists
        std::fs::create_dir_all(log_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create log directory {}: {}", log_dir, e))?;

        let file_appender = RollingFileAppender::new(rotation, log_dir, &config.log_prefix);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = if config.json_format {
            fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_span_events(FmtSpan::CLOSE)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .boxed()
        } else {
            fmt::layer()
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
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false) // Cleaner console output
        .with_file(false)
        .with_line_number(false);

    // Try to initialize OpenTelemetry (graceful degradation on failure)
    let (otel_tracer, otel_guard) = match otel::init_otel(otel_config) {
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
