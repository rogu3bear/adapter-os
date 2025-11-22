//! Memory integration module for Metal backend
//!
//! Integrates GPU memory pool with:
//! - adapteros-memory unified tracker
//! - Telemetry reporting
//! - Automatic cleanup scheduling

use crate::gpu_memory_pool::{
    GpuMemoryPool, GpuMemoryPoolConfig, GpuMemoryStats, MemoryPressureEvent,
};
use crate::vram::VramTracker;
use adapteros_core::{AosError, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[cfg(target_os = "macos")]
use metal::Device;

/// Memory telemetry event for GPU operations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuMemoryTelemetryEvent {
    /// Event type
    pub event_type: GpuMemoryEventType,
    /// Timestamp (epoch seconds)
    pub timestamp: u64,
    /// Current stats snapshot
    pub stats: GpuMemoryStatsSnapshot,
    /// Additional context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Types of GPU memory events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GpuMemoryEventType {
    /// Buffer allocated
    Allocation { size: u64, allocation_id: u64 },
    /// Buffer released
    Release { size: u64, allocation_id: u64 },
    /// Buffer reused from pool
    PoolHit { size: u64, allocation_id: u64 },
    /// Memory pressure detected
    PressureDetected { level: f32, bytes_to_free: u64 },
    /// Cleanup performed
    Cleanup { bytes_freed: u64, reason: String },
    /// High memory usage alert
    HighUsageAlert { usage_pct: f32, threshold_pct: f32 },
}

/// Snapshot of memory stats for telemetry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuMemoryStatsSnapshot {
    /// Active memory (bytes)
    pub active_bytes: u64,
    /// Pooled memory (bytes)
    pub pooled_bytes: u64,
    /// Total memory (bytes)
    pub total_bytes: u64,
    /// Pool hit rate
    pub hit_rate: f32,
    /// Pooled buffer count
    pub pooled_count: usize,
}

impl From<&GpuMemoryStats> for GpuMemoryStatsSnapshot {
    fn from(stats: &GpuMemoryStats) -> Self {
        let total_requests = stats.pool_hits + stats.pool_misses;
        let hit_rate = if total_requests > 0 {
            stats.pool_hits as f32 / total_requests as f32
        } else {
            0.0
        };

        Self {
            active_bytes: stats.total_active_bytes,
            pooled_bytes: stats.total_pooled_bytes,
            total_bytes: stats.total_active_bytes + stats.total_pooled_bytes,
            hit_rate,
            pooled_count: stats.pooled_buffer_count,
        }
    }
}

/// GPU memory manager that integrates pool, VRAM tracker, and telemetry
#[cfg(target_os = "macos")]
pub struct GpuMemoryManager {
    /// Memory pool
    pool: Arc<GpuMemoryPool>,
    /// VRAM tracker for adapter attribution
    vram_tracker: Arc<parking_lot::RwLock<VramTracker>>,
    /// Telemetry sink
    telemetry_sink: Option<Arc<dyn TelemetrySink>>,
    /// Cleanup interval handle
    cleanup_handle: Option<std::thread::JoinHandle<()>>,
    /// Shutdown signal
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

/// Telemetry sink trait for reporting events
pub trait TelemetrySink: Send + Sync {
    /// Send a telemetry event
    fn send(&self, event: GpuMemoryTelemetryEvent);
}

#[cfg(target_os = "macos")]
impl GpuMemoryManager {
    /// Create a new GPU memory manager
    pub fn new(
        device: Arc<Device>,
        config: GpuMemoryPoolConfig,
        vram_tracker: VramTracker,
    ) -> Self {
        let pool = Arc::new(GpuMemoryPool::new(device, config));
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));

        Self {
            pool,
            vram_tracker: Arc::new(parking_lot::RwLock::new(vram_tracker)),
            telemetry_sink: None,
            cleanup_handle: None,
            shutdown,
        }
    }

    /// Set telemetry sink for event reporting
    pub fn set_telemetry_sink(&mut self, sink: Arc<dyn TelemetrySink>) {
        // Register pressure callback
        let sink_clone = Arc::clone(&sink);
        self.pool
            .register_pressure_callback(Box::new(move |event: MemoryPressureEvent| {
                let stats = GpuMemoryStatsSnapshot {
                    active_bytes: event.current_usage,
                    pooled_bytes: 0, // Not available in pressure event
                    total_bytes: event.current_usage,
                    hit_rate: 0.0,
                    pooled_count: 0,
                };

                let telemetry_event = GpuMemoryTelemetryEvent {
                    event_type: GpuMemoryEventType::PressureDetected {
                        level: event.pressure_level,
                        bytes_to_free: event.bytes_to_free,
                    },
                    timestamp: event.timestamp,
                    stats,
                    context: None,
                };

                sink_clone.send(telemetry_event);
            }));

        self.telemetry_sink = Some(sink);
    }

    /// Start automatic cleanup scheduler
    pub fn start_cleanup_scheduler(&mut self, interval: Duration) {
        let pool = Arc::clone(&self.pool);
        let shutdown = Arc::clone(&self.shutdown);

        let handle = std::thread::spawn(move || {
            while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(interval);

                if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                let freed = pool.cleanup_idle_buffers();
                if freed > 0 {
                    debug!(bytes_freed = freed, "Scheduled cleanup completed");
                }
            }
            info!("GPU memory cleanup scheduler stopped");
        });

        self.cleanup_handle = Some(handle);
        info!(
            interval_secs = interval.as_secs(),
            "Started cleanup scheduler"
        );
    }

    /// Stop cleanup scheduler
    pub fn stop_cleanup_scheduler(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.cleanup_handle.take() {
            let _ = handle.join();
        }
    }

    /// Get memory pool reference
    pub fn pool(&self) -> &Arc<GpuMemoryPool> {
        &self.pool
    }

    /// Get VRAM tracker reference
    pub fn vram_tracker(&self) -> &Arc<parking_lot::RwLock<VramTracker>> {
        &self.vram_tracker
    }

    /// Check memory pressure and handle if needed
    pub fn check_and_handle_pressure(&self) -> Result<u64> {
        let stats = self.pool.stats();
        let (active, pooled) = (stats.total_active_bytes, stats.total_pooled_bytes);
        let total = active + pooled;

        // Get approximate total device memory from peak usage
        let estimated_total = stats.peak_memory_usage.max(total * 2);
        let usage_ratio = total as f32 / estimated_total as f32;

        if usage_ratio > 0.85 {
            let target_usage = (estimated_total as f32 * 0.70) as u64;
            let bytes_to_free = total.saturating_sub(target_usage);

            if bytes_to_free > 0 {
                let freed = self.pool.handle_memory_pressure(bytes_to_free);

                // Report to telemetry
                if let Some(ref sink) = self.telemetry_sink {
                    let event = GpuMemoryTelemetryEvent {
                        event_type: GpuMemoryEventType::Cleanup {
                            bytes_freed: freed,
                            reason: "pressure".to_string(),
                        },
                        timestamp: current_timestamp(),
                        stats: GpuMemoryStatsSnapshot::from(&self.pool.stats()),
                        context: None,
                    };
                    sink.send(event);
                }

                return Ok(freed);
            }
        }

        Ok(0)
    }

    /// Report high memory usage alert
    pub fn report_high_usage(&self, usage_pct: f32, threshold_pct: f32) {
        if let Some(ref sink) = self.telemetry_sink {
            let event = GpuMemoryTelemetryEvent {
                event_type: GpuMemoryEventType::HighUsageAlert {
                    usage_pct,
                    threshold_pct,
                },
                timestamp: current_timestamp(),
                stats: GpuMemoryStatsSnapshot::from(&self.pool.stats()),
                context: None,
            };
            sink.send(event);
        }

        warn!(
            usage_pct = usage_pct,
            threshold_pct = threshold_pct,
            "High GPU memory usage alert"
        );
    }

    /// Get comprehensive memory report
    pub fn memory_report(&self) -> GpuMemoryReport {
        let pool_stats = self.pool.stats();
        let pool_info = self.pool.pool_info();
        let vram = self.vram_tracker.read();

        GpuMemoryReport {
            pool_stats,
            pool_buckets: pool_info,
            adapter_count: vram.adapter_count(),
            adapter_vram_total: vram.get_total_vram(),
            adapter_allocations: vram.get_all_allocations(),
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for GpuMemoryManager {
    fn drop(&mut self) {
        self.stop_cleanup_scheduler();
        self.pool.clear_pool();
    }
}

/// Comprehensive GPU memory report
#[derive(Debug, Clone)]
pub struct GpuMemoryReport {
    /// Pool statistics
    pub pool_stats: GpuMemoryStats,
    /// Pool buckets (bucket_size, count, total_bytes)
    pub pool_buckets: Vec<(u64, usize, u64)>,
    /// Number of tracked adapters
    pub adapter_count: usize,
    /// Total VRAM used by adapters
    pub adapter_vram_total: u64,
    /// Individual adapter allocations
    pub adapter_allocations: Vec<(u32, u64)>,
}

/// Non-macOS stub
#[cfg(not(target_os = "macos"))]
pub struct GpuMemoryManager {
    vram_tracker: VramTracker,
}

#[cfg(not(target_os = "macos"))]
impl GpuMemoryManager {
    pub fn new(vram_tracker: VramTracker) -> Self {
        Self { vram_tracker }
    }

    pub fn check_and_handle_pressure(&self) -> Result<u64> {
        Ok(0)
    }

    pub fn memory_report(&self) -> GpuMemoryReport {
        GpuMemoryReport {
            pool_stats: GpuMemoryStats::default(),
            pool_buckets: vec![],
            adapter_count: 0,
            adapter_vram_total: 0,
            adapter_allocations: vec![],
        }
    }
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_snapshot_conversion() {
        let stats = GpuMemoryStats {
            total_allocations: 100,
            total_deallocations: 50,
            pooled_buffer_count: 10,
            total_pooled_bytes: 1024,
            total_active_bytes: 2048,
            pool_hits: 30,
            pool_misses: 70,
            timeout_cleanups: 5,
            pressure_cleanups: 2,
            peak_memory_usage: 4096,
        };

        let snapshot = GpuMemoryStatsSnapshot::from(&stats);
        assert_eq!(snapshot.active_bytes, 2048);
        assert_eq!(snapshot.pooled_bytes, 1024);
        assert_eq!(snapshot.total_bytes, 3072);
        assert!((snapshot.hit_rate - 0.3).abs() < 0.001);
    }
}
