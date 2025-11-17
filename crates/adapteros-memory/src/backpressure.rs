//! Memory Backpressure Tracking (PRD 5)
//!
//! Provides comprehensive memory tracking across host, GPU, and KV cache dimensions
//! with tiered eviction and backpressure signaling.
//!
//! # Invariants
//! - Memory snapshots MUST emit at a fixed interval (5s default)
//! - Backpressure MUST follow clear policy: WARNING → drop caches, CRITICAL → block requests
//! - KV cache usage MUST be tracked separately
//! - KV cache OOM MUST surface as telemetry, not opaque errors
//!
//! # Citations
//! - PRD 5: Memory & Backpressure (Host, GPU, KV)
//! - CLAUDE.md L1073-L1128: Tiered eviction in LifecycleManager

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn, error};

/// Memory snapshot capturing all dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Host memory used (bytes)
    pub host_used_bytes: u64,
    /// Host memory total (bytes)
    pub host_total_bytes: u64,
    /// GPU memory used (bytes, best-effort)
    pub gpu_used_bytes: u64,
    /// GPU memory total (bytes, best-effort)
    pub gpu_total_bytes: u64,
    /// KV cache memory used (bytes)
    pub kv_used_bytes: u64,
    /// Timestamp (microseconds since UNIX epoch)
    pub ts_us: i64,
}

impl MemorySnapshot {
    /// Create new snapshot
    pub fn new(
        host_used: u64,
        host_total: u64,
        gpu_used: u64,
        gpu_total: u64,
        kv_used: u64,
    ) -> Self {
        let ts_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as i64;

        Self {
            host_used_bytes: host_used,
            host_total_bytes: host_total,
            gpu_used_bytes: gpu_used,
            gpu_total_bytes: gpu_total,
            kv_used_bytes: kv_used,
            ts_us,
        }
    }

    /// Get host memory usage percentage
    pub fn host_usage_pct(&self) -> f32 {
        if self.host_total_bytes == 0 {
            0.0
        } else {
            (self.host_used_bytes as f32 / self.host_total_bytes as f32) * 100.0
        }
    }

    /// Get GPU memory usage percentage (returns 0.0 if GPU metrics unavailable)
    pub fn gpu_usage_pct(&self) -> f32 {
        if self.gpu_total_bytes == 0 {
            0.0
        } else {
            (self.gpu_used_bytes as f32 / self.gpu_total_bytes as f32) * 100.0
        }
    }

    /// Get overall memory pressure level
    pub fn pressure_level(&self) -> MemoryPressureLevel {
        // Use host memory as primary indicator (GPU is best-effort)
        let host_pct = self.host_usage_pct();

        if host_pct >= 95.0 {
            MemoryPressureLevel::Critical
        } else if host_pct >= 85.0 {
            MemoryPressureLevel::High
        } else if host_pct >= 70.0 {
            MemoryPressureLevel::Medium
        } else {
            MemoryPressureLevel::Low
        }
    }
}

/// Memory tier for eviction prioritization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryTier {
    /// Critical adapters - only evict on CRITICAL pressure
    Critical,
    /// Extra adapters - evict on HIGH pressure
    Extra,
    /// Cache data - drop on WARNING
    Cache,
}

/// Eviction actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionAction {
    /// Drop cache data
    DropCache { bytes_freed: u64 },
    /// Unload idle adapters
    UnloadIdleAdapters { count: u32, bytes_freed: u64 },
    /// Block new non-critical requests
    BlockNewRequests,
}

/// Memory pressure levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryPressureLevel {
    /// Low pressure - normal operation
    Low,
    /// Medium pressure - monitor closely
    Medium,
    /// High pressure - begin evicting Extra tier
    High,
    /// Critical pressure - evict Critical tier, block new requests
    Critical,
}

impl MemoryPressureLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

/// Backpressure monitor collecting memory snapshots
pub struct BackpressureMonitor {
    /// Latest snapshot
    snapshot: Arc<RwLock<Option<MemorySnapshot>>>,
    /// Snapshot interval (default 5s)
    interval_secs: u64,
    /// Eviction policy
    policy: EvictionPolicy,
    /// Task handle
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Eviction policy based on pressure levels
#[derive(Debug, Clone)]
pub struct EvictionPolicy {
    /// WARNING threshold (usage %)
    pub warning_threshold: f32,
    /// CRITICAL threshold (usage %)
    pub critical_threshold: f32,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        Self {
            warning_threshold: 85.0,  // Drop caches at 85%
            critical_threshold: 95.0, // Block requests at 95%
        }
    }
}

impl BackpressureMonitor {
    /// Create new backpressure monitor
    pub fn new(interval_secs: u64, policy: EvictionPolicy) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(None)),
            interval_secs,
            policy,
            task_handle: None,
        }
    }

    /// Start background snapshot collection
    ///
    /// Collects memory snapshots at fixed interval and emits telemetry
    pub async fn start<F>(&mut self, snapshot_fn: F)
    where
        F: Fn() -> MemorySnapshot + Send + 'static,
    {
        let snapshot_arc = self.snapshot.clone();
        let interval_secs = self.interval_secs;

        let handle = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));

            loop {
                ticker.tick().await;

                // Collect snapshot
                let snap = snapshot_fn();
                let pressure = snap.pressure_level();

                // Log snapshot
                debug!(
                    host_used_mb = snap.host_used_bytes / (1024 * 1024),
                    host_total_mb = snap.host_total_bytes / (1024 * 1024),
                    gpu_used_mb = snap.gpu_used_bytes / (1024 * 1024),
                    gpu_total_mb = snap.gpu_total_bytes / (1024 * 1024),
                    kv_used_mb = snap.kv_used_bytes / (1024 * 1024),
                    pressure = pressure.as_str(),
                    "Memory snapshot collected"
                );

                // Emit telemetry for non-low pressure
                if pressure != MemoryPressureLevel::Low {
                    emit_pressure_telemetry(&snap, pressure).await;
                }

                // Store snapshot
                *snapshot_arc.write().await = Some(snap);
            }
        });

        self.task_handle = Some(handle);
        info!(interval_secs = interval_secs, "Backpressure monitor started");
    }

    /// Get latest snapshot
    pub async fn get_snapshot(&self) -> Option<MemorySnapshot> {
        self.snapshot.read().await.clone()
    }

    /// Check if backpressure is active
    ///
    /// Returns true if memory pressure requires backpressure signaling
    pub async fn is_backpressure_active(&self) -> bool {
        if let Some(snap) = self.get_snapshot().await {
            matches!(
                snap.pressure_level(),
                MemoryPressureLevel::High | MemoryPressureLevel::Critical
            )
        } else {
            false
        }
    }

    /// Determine required eviction action based on current pressure
    pub async fn get_eviction_action(&self) -> Option<EvictionAction> {
        let snap = self.get_snapshot().await?;
        let pressure = snap.pressure_level();

        match pressure {
            MemoryPressureLevel::Critical => Some(EvictionAction::BlockNewRequests),
            MemoryPressureLevel::High => Some(EvictionAction::UnloadIdleAdapters {
                count: 0,
                bytes_freed: 0,
            }),
            MemoryPressureLevel::Medium => Some(EvictionAction::DropCache { bytes_freed: 0 }),
            MemoryPressureLevel::Low => None,
        }
    }

    /// Stop background collection
    pub async fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            info!("Backpressure monitor stopped");
        }
    }
}

/// Emit memory pressure telemetry event
async fn emit_pressure_telemetry(snap: &MemorySnapshot, level: MemoryPressureLevel) {
    // TODO: Integrate with TelemetryWriter once available
    // For now, log at appropriate level
    match level {
        MemoryPressureLevel::Critical => {
            error!(
                host_used_mb = snap.host_used_bytes / (1024 * 1024),
                host_total_mb = snap.host_total_bytes / (1024 * 1024),
                host_usage_pct = snap.host_usage_pct(),
                gpu_used_mb = snap.gpu_used_bytes / (1024 * 1024),
                kv_used_mb = snap.kv_used_bytes / (1024 * 1024),
                pressure = level.as_str(),
                "CRITICAL memory pressure - blocking new requests"
            );
        }
        MemoryPressureLevel::High => {
            warn!(
                host_used_mb = snap.host_used_bytes / (1024 * 1024),
                host_total_mb = snap.host_total_bytes / (1024 * 1024),
                host_usage_pct = snap.host_usage_pct(),
                gpu_used_mb = snap.gpu_used_bytes / (1024 * 1024),
                kv_used_mb = snap.kv_used_bytes / (1024 * 1024),
                pressure = level.as_str(),
                "HIGH memory pressure - evicting idle adapters"
            );
        }
        MemoryPressureLevel::Medium => {
            info!(
                host_used_mb = snap.host_used_bytes / (1024 * 1024),
                host_total_mb = snap.host_total_bytes / (1024 * 1024),
                host_usage_pct = snap.host_usage_pct(),
                pressure = level.as_str(),
                "MEDIUM memory pressure - dropping caches"
            );
        }
        _ => {}
    }
}

/// KV cache OOM error with telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvCacheOom {
    /// Requested bytes
    pub requested_bytes: u64,
    /// Available bytes
    pub available_bytes: u64,
    /// Total capacity bytes
    pub total_capacity_bytes: u64,
    /// Timestamp
    pub timestamp: i64,
}

impl KvCacheOom {
    /// Create new KV cache OOM error
    pub fn new(requested: u64, available: u64, capacity: u64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Self {
            requested_bytes: requested,
            available_bytes: available,
            total_capacity_bytes: capacity,
            timestamp,
        }
    }

    /// Emit telemetry event for KV cache OOM
    pub async fn emit_telemetry(&self) {
        error!(
            requested_mb = self.requested_bytes / (1024 * 1024),
            available_mb = self.available_bytes / (1024 * 1024),
            capacity_mb = self.total_capacity_bytes / (1024 * 1024),
            usage_pct = (self.requested_bytes as f32 / self.total_capacity_bytes as f32) * 100.0,
            "KV cache OOM - allocation failed"
        );
    }

    /// Convert to AosError
    pub fn to_error(&self) -> AosError {
        AosError::MemoryPressure(format!(
            "KV cache OOM: requested {} MB, available {} MB / {} MB capacity",
            self.requested_bytes / (1024 * 1024),
            self.available_bytes / (1024 * 1024),
            self.total_capacity_bytes / (1024 * 1024)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_snapshot_creation() {
        let snap = MemorySnapshot::new(
            8 * 1024 * 1024 * 1024,  // 8 GB used
            16 * 1024 * 1024 * 1024, // 16 GB total
            4 * 1024 * 1024 * 1024,  // 4 GB GPU used
            8 * 1024 * 1024 * 1024,  // 8 GB GPU total
            512 * 1024 * 1024,       // 512 MB KV cache
        );

        assert_eq!(snap.host_usage_pct(), 50.0);
        assert_eq!(snap.gpu_usage_pct(), 50.0);
        assert!(snap.ts_us > 0);
    }

    #[test]
    fn test_pressure_levels() {
        // Low pressure
        let snap1 = MemorySnapshot::new(
            1 * 1024 * 1024 * 1024,  // 1 GB used
            16 * 1024 * 1024 * 1024, // 16 GB total (6.25% usage)
            0,
            0,
            0,
        );
        assert_eq!(snap1.pressure_level(), MemoryPressureLevel::Low);

        // Medium pressure
        let snap2 = MemorySnapshot::new(
            12 * 1024 * 1024 * 1024, // 12 GB used
            16 * 1024 * 1024 * 1024, // 16 GB total (75% usage)
            0,
            0,
            0,
        );
        assert_eq!(snap2.pressure_level(), MemoryPressureLevel::Medium);

        // High pressure
        let snap3 = MemorySnapshot::new(
            14 * 1024 * 1024 * 1024, // 14 GB used
            16 * 1024 * 1024 * 1024, // 16 GB total (87.5% usage)
            0,
            0,
            0,
        );
        assert_eq!(snap3.pressure_level(), MemoryPressureLevel::High);

        // Critical pressure
        let snap4 = MemorySnapshot::new(
            15 * 1024 * 1024 * 1024, // 15.5 GB used
            16 * 1024 * 1024 * 1024, // 16 GB total (96.875% usage)
            0,
            0,
            0,
        );
        assert_eq!(snap4.pressure_level(), MemoryPressureLevel::Critical);
    }

    #[test]
    fn test_gpu_metrics_unavailable() {
        // GPU metrics set to 0 (unavailable)
        let snap = MemorySnapshot::new(
            8 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
            0, // GPU unavailable
            0,
            0,
        );

        assert_eq!(snap.gpu_usage_pct(), 0.0);
        // Should still determine pressure based on host memory
        assert_eq!(snap.pressure_level(), MemoryPressureLevel::Medium);
    }

    #[test]
    fn test_kv_cache_oom() {
        let oom = KvCacheOom::new(
            512 * 1024 * 1024,      // Requested 512 MB
            100 * 1024 * 1024,      // Only 100 MB available
            1024 * 1024 * 1024,     // 1 GB total
        );

        assert_eq!(oom.requested_bytes, 512 * 1024 * 1024);
        assert_eq!(oom.available_bytes, 100 * 1024 * 1024);
        assert!(oom.timestamp > 0);

        let err = oom.to_error();
        assert!(matches!(err, AosError::MemoryPressure(_)));
    }

    #[tokio::test]
    async fn test_backpressure_monitor_creation() {
        let policy = EvictionPolicy::default();
        let monitor = BackpressureMonitor::new(5, policy);

        assert!(monitor.get_snapshot().await.is_none());
        assert!(!monitor.is_backpressure_active().await);
    }

    #[tokio::test]
    async fn test_eviction_action_determination() {
        let policy = EvictionPolicy::default();
        let mut monitor = BackpressureMonitor::new(1, policy);

        // Start with a snapshot function that returns critical pressure
        let critical_snap = MemorySnapshot::new(
            15 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
            0,
            0,
            0,
        );

        *monitor.snapshot.write().await = Some(critical_snap);

        let action = monitor.get_eviction_action().await;
        assert!(matches!(action, Some(EvictionAction::BlockNewRequests)));
        assert!(monitor.is_backpressure_active().await);
    }
}
