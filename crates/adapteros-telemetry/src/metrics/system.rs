//! System metrics helpers and emitters (placeholder implementation)
//!
//! Provides a minimal, dependency-free placeholder system metrics emitter that
//! produces `system.metrics` events into the unified telemetry NDJSON pipeline.
//!
//! This is intended as a scaffold. Replace the placeholder collectors with a
//! real system metrics collector (e.g., sysinfo) when ready.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tracing::warn;

/// Build a placeholder `system.metrics` payload.
pub fn placeholder_system_metrics_event() -> crate::event::SystemMetricsEvent {
    crate::event::SystemMetricsEvent {
        cpu_usage: 0.0,
        memory_usage: 0.0,
        disk_read_bytes: 0,
        disk_write_bytes: 0,
        network_rx_bytes: 0,
        network_tx_bytes: 0,
        gpu_utilization: None,
        gpu_memory_used: None,
        uptime_seconds: 0,
        process_count: 0,
        load_average: crate::event::LoadAverageEvent {
            load_1min: 0.0,
            load_5min: 0.0,
            load_15min: 0.0,
        },
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }
}

/// Spawn a background task that periodically emits placeholder `system.metrics` events.
///
/// - `writer`: unified telemetry writer used to append to the NDJSON pipeline
/// - `interval`: how frequently to emit the event
pub fn spawn_placeholder_emitter(
    writer: Arc<crate::TelemetryWriter>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let payload = placeholder_system_metrics_event();
            // Attach payload as metadata on unified telemetry event.
            let metadata = match serde_json::to_value(&payload) {
                Ok(val) => val,
                Err(err) => {
                    warn!("Failed to encode system metrics payload: {}", err);
                    json!({
                        "serialization_error": err.to_string(),
                    })
                }
            };

            let identity = adapteros_core::identity::IdentityEnvelope::new(
                "system".to_string(),
                "metrics".to_string(),
                "system-monitoring".to_string(),
                adapteros_core::identity::IdentityEnvelope::default_revision(),
            );
            match crate::TelemetryEventBuilder::new(
                crate::EventType::Custom("metrics.system".to_string()),
                crate::LogLevel::Info,
                "Placeholder system metrics sample".to_string(),
                identity,
            )
            .component("adapteros-server".to_string())
            .metadata(metadata)
            .build()
            {
                Ok(event) => {
                    // Best-effort; keep server hot path resilient.
                    let _ = writer.log_event(event);
                }
                Err(e) => {
                    warn!("Failed to build telemetry event: {}", e);
                    continue;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_event_serializes() {
        let ev = placeholder_system_metrics_event();
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains("cpu_usage"));
        assert!(json.contains("load_1min"));
    }
}
