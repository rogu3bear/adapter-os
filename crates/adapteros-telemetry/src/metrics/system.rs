//! System metrics helpers and emitters.
//!
//! Provides a real system metrics emitter that produces `system.metrics` events
//! into the unified telemetry NDJSON pipeline.
//!
//! This module includes a lightweight metrics collector that uses `sysinfo` directly,
//! avoiding circular dependencies with adapteros-system-metrics.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::FutureExt;
use serde_json::json;
use sysinfo::{Disks, Networks, System};
use tracing::warn;

/// Lightweight system metrics collector for telemetry purposes.
///
/// This is a minimal collector that uses `sysinfo` directly to avoid
/// circular dependencies with the `adapteros-system-metrics` crate.
/// For full system metrics (including GPU via Metal/MLX), use
/// `adapteros_system_metrics::SystemMetricsCollector` instead.
pub struct TelemetryMetricsCollector {
    sys: System,
    #[allow(dead_code)]
    last_collection: SystemTime,
}

impl TelemetryMetricsCollector {
    /// Create a new telemetry metrics collector.
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            last_collection: SystemTime::now(),
        }
    }

    /// Collect current system metrics and return a telemetry event.
    pub fn collect(&mut self) -> crate::event::SystemMetricsEvent {
        self.sys.refresh_all();
        let now = SystemTime::now();

        let cpu_usage = self.calculate_cpu_usage();
        let memory_usage = self.calculate_memory_usage();
        let (disk_read_bytes, disk_write_bytes) = self.collect_disk_io();
        let (network_rx_bytes, network_tx_bytes) = self.collect_network_io();
        let load = System::load_average();

        self.last_collection = now;

        crate::event::SystemMetricsEvent {
            cpu_usage: cpu_usage as f32,
            memory_usage: memory_usage as f32,
            disk_read_bytes,
            disk_write_bytes,
            network_rx_bytes,
            network_tx_bytes,
            // GPU metrics require Metal/MLX integration from adapteros-system-metrics.
            // For telemetry purposes, we report None and let the full collector
            // provide GPU data when needed.
            gpu_utilization: None,
            gpu_memory_used: None,
            uptime_seconds: System::uptime(),
            process_count: self.sys.processes().len(),
            load_average: crate::event::LoadAverageEvent {
                load_1min: load.one,
                load_5min: load.five,
                load_15min: load.fifteen,
            },
            timestamp: now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        }
    }

    /// Calculate average CPU usage percentage across all cores.
    fn calculate_cpu_usage(&mut self) -> f64 {
        self.sys.refresh_cpu_all();
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return 0.0;
        }
        let total_usage: f32 = cpus.iter().map(|cpu| cpu.cpu_usage()).sum();
        (total_usage / cpus.len() as f32) as f64
    }

    /// Calculate memory usage percentage.
    fn calculate_memory_usage(&self) -> f64 {
        let total_memory = self.sys.total_memory();
        if total_memory == 0 {
            return 0.0;
        }
        let used_memory = self.sys.used_memory();
        ((used_memory as f32 / total_memory as f32) * 100.0) as f64
    }

    /// Collect cumulative disk I/O bytes (read, write).
    /// Note: sysinfo 0.30+ removed I/O counter methods from the Disk API,
    /// so we return 0 for now (space metrics are still available).
    fn collect_disk_io(&self) -> (u64, u64) {
        let _disks = Disks::new_with_refreshed_list();
        // Disk I/O counters require platform-specific implementations.
        // Return 0 as placeholders - full metrics are in adapteros-system-metrics.
        (0, 0)
    }

    /// Collect cumulative network I/O bytes (rx, tx).
    fn collect_network_io(&self) -> (u64, u64) {
        let networks = Networks::new_with_refreshed_list();
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        for (_, network) in &networks {
            total_rx += network.received();
            total_tx += network.transmitted();
        }
        (total_rx, total_tx)
    }
}

impl Default for TelemetryMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a `system.metrics` payload using the telemetry metrics collector.
///
/// This is a convenience function that creates a one-off event. For periodic
/// collection, use `spawn_system_metrics_emitter` which maintains collector state.
pub fn system_metrics_event(
    collector: &mut TelemetryMetricsCollector,
) -> crate::event::SystemMetricsEvent {
    collector.collect()
}

/// Spawn a background task that periodically emits `system.metrics` events.
///
/// - `writer`: unified telemetry writer used to append to the NDJSON pipeline
/// - `interval`: how frequently to emit the event
pub fn spawn_system_metrics_emitter(
    writer: Arc<crate::TelemetryWriter>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(panic) = std::panic::AssertUnwindSafe(async move {
            let mut collector = TelemetryMetricsCollector::new();
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                let payload = collector.collect();
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
                    "System metrics sample".to_string(),
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
        .catch_unwind()
        .await
        {
            tracing::error!(
                task = "system_metrics_emitter",
                "background task panicked: {:?}",
                panic
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_event_serializes() {
        let mut collector = TelemetryMetricsCollector::new();
        let ev = collector.collect();
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains("cpu_usage"));
        assert!(json.contains("load_1min"));
    }

    #[test]
    fn test_collector_creation() {
        let collector = TelemetryMetricsCollector::new();
        // Should have at least one process (this test)
        assert!(!collector.sys.processes().is_empty());
    }

    #[test]
    fn test_cpu_memory_in_range() {
        let mut collector = TelemetryMetricsCollector::new();
        let ev = collector.collect();
        // CPU and memory usage should be valid percentages
        assert!(ev.cpu_usage >= 0.0 && ev.cpu_usage <= 100.0);
        assert!(ev.memory_usage >= 0.0 && ev.memory_usage <= 100.0);
    }
}
