//! Memory telemetry integration
//!
//! Provides structured telemetry events for memory operations:
//! - Memory allocation/deallocation
//! - Pressure detection and eviction
//! - Backend-specific memory usage
//! - Buffer pool statistics

use crate::buffer_pool::BufferPoolStats;
use crate::pressure_manager::{EvictedAdapter, MemoryPressureReport, MemoryStats};
use crate::unified_tracker::{BackendType, PressureLevel};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Memory telemetry writer (thin wrapper around TelemetryWriter)
pub struct MemoryTelemetryWriter {
    writer: Option<Arc<dyn TelemetryEventSink>>,
}

impl MemoryTelemetryWriter {
    /// Create new telemetry writer
    pub fn new(writer: Option<Arc<dyn TelemetryEventSink>>) -> Self {
        Self { writer }
    }

    /// Emit adapter allocation event
    pub fn emit_allocation(
        &self,
        adapter_id: u32,
        backend: BackendType,
        buffer_bytes: u64,
        kv_cache_bytes: u64,
    ) {
        if let Some(ref writer) = self.writer {
            let event = MemoryAllocationEvent {
                adapter_id,
                backend,
                buffer_bytes,
                kv_cache_bytes,
                total_bytes: buffer_bytes + kv_cache_bytes,
                timestamp: current_timestamp(),
            };

            writer.emit_event("memory.allocation", serde_json::to_value(&event).unwrap());

            info!(
                adapter_id = adapter_id,
                backend = backend.as_str(),
                buffer_bytes = buffer_bytes,
                kv_cache_bytes = kv_cache_bytes,
                "Memory allocation event"
            );
        }
    }

    /// Emit adapter deallocation event
    pub fn emit_deallocation(&self, adapter_id: u32, bytes_freed: u64) {
        if let Some(ref writer) = self.writer {
            let event = MemoryDeallocationEvent {
                adapter_id,
                bytes_freed,
                timestamp: current_timestamp(),
            };

            writer.emit_event("memory.deallocation", serde_json::to_value(&event).unwrap());

            info!(
                adapter_id = adapter_id,
                bytes_freed = bytes_freed,
                "Memory deallocation event"
            );
        }
    }

    /// Emit memory pressure event
    pub fn emit_pressure(&self, report: &MemoryPressureReport) {
        if let Some(ref writer) = self.writer {
            let event = MemoryPressureEvent {
                pressure_level: report.pressure_level,
                action_taken: report.action_taken,
                adapters_evicted_count: report.adapters_evicted.len(),
                bytes_freed: report.bytes_freed,
                headroom_before: report.headroom_before,
                headroom_after: report.headroom_after,
                timestamp: current_timestamp(),
            };

            let level = match report.pressure_level {
                PressureLevel::Low => "info",
                PressureLevel::Medium => "info",
                PressureLevel::High => "warn",
                PressureLevel::Critical => "error",
            };

            writer.emit_event(
                &format!("memory.pressure.{}", level),
                serde_json::to_value(&event).unwrap(),
            );

            match report.pressure_level {
                PressureLevel::Critical => warn!(
                    pressure_level = ?report.pressure_level,
                    action = ?report.action_taken,
                    bytes_freed = report.bytes_freed,
                    "Critical memory pressure event"
                ),
                PressureLevel::High => warn!(
                    pressure_level = ?report.pressure_level,
                    action = ?report.action_taken,
                    bytes_freed = report.bytes_freed,
                    "High memory pressure event"
                ),
                _ => info!(
                    pressure_level = ?report.pressure_level,
                    action = ?report.action_taken,
                    "Memory pressure event"
                ),
            }
        }
    }

    /// Emit eviction event
    pub fn emit_eviction(&self, evicted: &EvictedAdapter) {
        if let Some(ref writer) = self.writer {
            let event = MemoryEvictionEvent {
                adapter_id: evicted.adapter_id,
                backend: evicted.backend,
                bytes_freed: evicted.bytes_freed,
                timestamp: current_timestamp(),
            };

            writer.emit_event("memory.eviction", serde_json::to_value(&event).unwrap());

            warn!(
                adapter_id = evicted.adapter_id,
                backend = evicted.backend.as_str(),
                bytes_freed = evicted.bytes_freed,
                "Memory eviction event"
            );
        }
    }

    /// Emit memory statistics snapshot
    pub fn emit_stats(&self, stats: &MemoryStats) {
        if let Some(ref writer) = self.writer {
            let event = MemoryStatsEvent {
                total_memory_used: stats.total_memory_used,
                metal_memory_used: stats.metal_memory_used,
                coreml_memory_used: stats.coreml_memory_used,
                mlx_memory_used: stats.mlx_memory_used,
                pressure_level: stats.pressure_level,
                headroom_pct: stats.headroom_pct,
                pinned_adapter_count: stats.pinned_adapter_count,
                total_adapter_count: stats.total_adapter_count,
                timestamp: current_timestamp(),
            };

            writer.emit_event("memory.stats", serde_json::to_value(&event).unwrap());
        }
    }

    /// Emit buffer pool statistics
    pub fn emit_buffer_pool_stats(&self, stats: &BufferPoolStats) {
        if let Some(ref writer) = self.writer {
            let event = BufferPoolStatsEvent {
                buffer_count: stats.buffer_count,
                total_pooled_bytes: stats.total_pooled_bytes,
                cache_entries: stats.cache_entries,
                total_cache_bytes: stats.total_cache_bytes,
                timestamp: current_timestamp(),
            };

            writer.emit_event(
                "memory.buffer_pool.stats",
                serde_json::to_value(&event).unwrap(),
            );
        }
    }

    /// Emit fingerprint verification event
    pub fn emit_fingerprint_verification(
        &self,
        adapter_id: u32,
        verified: bool,
        buffer_bytes: u64,
    ) {
        if let Some(ref writer) = self.writer {
            let event = FingerprintVerificationEvent {
                adapter_id,
                verified,
                buffer_bytes,
                timestamp: current_timestamp(),
            };

            writer.emit_event(
                "memory.fingerprint.verification",
                serde_json::to_value(&event).unwrap(),
            );

            if !verified {
                warn!(
                    adapter_id = adapter_id,
                    buffer_bytes = buffer_bytes,
                    "GPU buffer fingerprint verification failed"
                );
            }
        }
    }

    /// Emit memory footprint anomaly event
    pub fn emit_footprint_anomaly(
        &self,
        adapter_id: u32,
        buffer_bytes: u64,
        z_score: f64,
        within_tolerance: bool,
    ) {
        if let Some(ref writer) = self.writer {
            let event = FootprintAnomalyEvent {
                adapter_id,
                buffer_bytes,
                z_score,
                within_tolerance,
                timestamp: current_timestamp(),
            };

            writer.emit_event(
                "memory.footprint.anomaly",
                serde_json::to_value(&event).unwrap(),
            );

            if !within_tolerance {
                warn!(
                    adapter_id = adapter_id,
                    buffer_bytes = buffer_bytes,
                    z_score = z_score,
                    "Memory footprint anomaly detected"
                );
            }
        }
    }
}

/// Trait for telemetry event sinks
pub trait TelemetryEventSink: Send + Sync {
    /// Emit a telemetry event
    fn emit_event(&self, event_type: &str, event: serde_json::Value);
}

/// Memory allocation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAllocationEvent {
    pub adapter_id: u32,
    pub backend: BackendType,
    pub buffer_bytes: u64,
    pub kv_cache_bytes: u64,
    pub total_bytes: u64,
    pub timestamp: u64,
}

/// Memory deallocation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDeallocationEvent {
    pub adapter_id: u32,
    pub bytes_freed: u64,
    pub timestamp: u64,
}

/// Memory pressure event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPressureEvent {
    pub pressure_level: PressureLevel,
    pub action_taken: crate::unified_tracker::EvictionStrategy,
    pub adapters_evicted_count: usize,
    pub bytes_freed: u64,
    pub headroom_before: f32,
    pub headroom_after: f32,
    pub timestamp: u64,
}

/// Memory eviction event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvictionEvent {
    pub adapter_id: u32,
    pub backend: BackendType,
    pub bytes_freed: u64,
    pub timestamp: u64,
}

/// Memory statistics event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatsEvent {
    pub total_memory_used: u64,
    pub metal_memory_used: u64,
    pub coreml_memory_used: u64,
    pub mlx_memory_used: u64,
    pub pressure_level: PressureLevel,
    pub headroom_pct: f32,
    pub pinned_adapter_count: usize,
    pub total_adapter_count: usize,
    pub timestamp: u64,
}

/// Buffer pool statistics event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferPoolStatsEvent {
    pub buffer_count: usize,
    pub total_pooled_bytes: usize,
    pub cache_entries: usize,
    pub total_cache_bytes: usize,
    pub timestamp: u64,
}

/// Fingerprint verification event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintVerificationEvent {
    pub adapter_id: u32,
    pub verified: bool,
    pub buffer_bytes: u64,
    pub timestamp: u64,
}

/// Memory footprint anomaly event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootprintAnomalyEvent {
    pub adapter_id: u32,
    pub buffer_bytes: u64,
    pub z_score: f64,
    pub within_tolerance: bool,
    pub timestamp: u64,
}

/// Get current timestamp (seconds since epoch)
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock telemetry sink for testing
    struct MockTelemetrySink {
        events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl MockTelemetrySink {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_events(&self) -> Vec<(String, serde_json::Value)> {
            self.events.lock().unwrap().clone()
        }
    }

    impl TelemetryEventSink for MockTelemetrySink {
        fn emit_event(&self, event_type: &str, event: serde_json::Value) {
            self.events
                .lock()
                .unwrap()
                .push((event_type.to_string(), event));
        }
    }

    #[test]
    fn test_allocation_event() {
        let sink = Arc::new(MockTelemetrySink::new());
        let writer = MemoryTelemetryWriter::new(Some(sink.clone() as Arc<dyn TelemetryEventSink>));

        writer.emit_allocation(1, BackendType::Metal, 1024, 512);

        let events = sink.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "memory.allocation");
    }

    #[test]
    fn test_deallocation_event() {
        let sink = Arc::new(MockTelemetrySink::new());
        let writer = MemoryTelemetryWriter::new(Some(sink.clone() as Arc<dyn TelemetryEventSink>));

        writer.emit_deallocation(1, 1536);

        let events = sink.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "memory.deallocation");
    }

    #[test]
    fn test_pressure_event_levels() {
        let sink = Arc::new(MockTelemetrySink::new());
        let writer = MemoryTelemetryWriter::new(Some(sink.clone() as Arc<dyn TelemetryEventSink>));

        let report = MemoryPressureReport {
            pressure_level: PressureLevel::Critical,
            action_taken: crate::unified_tracker::EvictionStrategy::EmergencyEvict,
            adapters_evicted: vec![],
            bytes_freed: 0,
            headroom_before: 10.0,
            headroom_after: 20.0,
        };

        writer.emit_pressure(&report);

        let events = sink.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "memory.pressure.error");
    }

    #[test]
    fn test_fingerprint_verification_event() {
        let sink = Arc::new(MockTelemetrySink::new());
        let writer = MemoryTelemetryWriter::new(Some(sink.clone() as Arc<dyn TelemetryEventSink>));

        writer.emit_fingerprint_verification(1, true, 1024);

        let events = sink.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "memory.fingerprint.verification");
    }

    #[test]
    fn test_no_telemetry_writer() {
        let writer = MemoryTelemetryWriter::new(None);

        // Should not panic when no writer configured
        writer.emit_allocation(1, BackendType::Metal, 1024, 512);
        writer.emit_deallocation(1, 1536);
    }
}
