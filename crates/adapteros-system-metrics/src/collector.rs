//! System metrics collector implementation
//!
//! Provides real-time system resource monitoring using sysinfo crate.
//! Follows AdapterOS patterns for error handling and telemetry integration.

use crate::{DiskMetrics, GpuMetrics, NetworkMetrics, SystemMetrics};
use adapteros_core::{AosError, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, Networks, System};
use tracing::debug;

/// System metrics collector
pub struct SystemMetricsCollector {
    sys: System,
    last_cpu_time: u64,
    last_cpu_count: usize,
    last_disk_read: u64,
    last_disk_write: u64,
    last_network_rx: u64,
    last_network_tx: u64,
    last_collection: SystemTime,
}

impl SystemMetricsCollector {
    /// Create a new system metrics collector
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        Self {
            last_cpu_time: 0,
            last_cpu_count: sys.cpus().len(),
            last_disk_read: 0,
            last_disk_write: 0,
            last_network_rx: 0,
            last_network_tx: 0,
            last_collection: SystemTime::now(),
            sys,
        }
    }

    /// Collect current system metrics
    pub fn collect_metrics(&mut self) -> SystemMetrics {
        self.sys.refresh_all();

        let now = SystemTime::now();
        let time_delta = now
            .duration_since(self.last_collection)
            .unwrap_or_default()
            .as_secs_f32();

        let cpu_usage = self.calculate_cpu_usage();
        let memory_usage = self.calculate_memory_usage();
        let disk_io = self.collect_disk_metrics(time_delta);
        let network_io = self.collect_network_metrics(time_delta);
        let gpu_metrics = self.collect_gpu_metrics();

        self.last_collection = now;

        SystemMetrics {
            cpu_usage,
            memory_usage,
            disk_io,
            network_io,
            gpu_metrics,
            timestamp: now,
        }
    }

    /// Calculate CPU usage percentage
    fn calculate_cpu_usage(&mut self) -> f64 {
        self.sys.refresh_cpu();

        // Calculate average CPU usage across all cores
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return 0.0;
        }

        let total_usage: f32 = cpus.iter().map(|cpu| cpu.cpu_usage()).sum();
        (total_usage / cpus.len() as f32) as f64
    }

    /// Calculate memory usage percentage
    fn calculate_memory_usage(&self) -> f64 {
        let total_memory = self.sys.total_memory();
        if total_memory == 0 {
            return 0.0;
        }

        let used_memory = self.sys.used_memory();
        ((used_memory as f32 / total_memory as f32) * 100.0) as f64
    }

    /// Collect disk I/O metrics
    fn collect_disk_metrics(&mut self, time_delta: f32) -> DiskMetrics {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let mut total_read_bytes = 0u64;
        let mut total_write_bytes = 0u64;
        let mut total_read_ops = 0u64;
        let mut total_write_ops = 0u64;
        let mut total_space = 0u64;
        let mut available_space = 0u64;

        // Note: sysinfo 0.30+ removed I/O counter methods from the Disk API.
        // I/O metrics (read_bytes, write_bytes, read_ops, write_ops) are set to 0
        // as they require platform-specific implementations or process-level tracking.
        // Space metrics (total_space, available_space) are still available.
        for disk in &disks {
            total_space += disk.total_space();
            available_space += disk.available_space();
        }

        // Calculate usage percentage
        let usage_percent = if total_space > 0 {
            ((total_space - available_space) as f32 / total_space as f32) * 100.0
        } else {
            0.0
        };

        DiskMetrics {
            read_bytes: total_read_bytes,
            write_bytes: total_write_bytes,
            read_ops: total_read_ops,
            write_ops: total_write_ops,
            usage_percent,
            available_bytes: available_space,
            total_bytes: total_space,
        }
    }

    /// Collect network I/O metrics
    fn collect_network_metrics(&mut self, time_delta: f32) -> NetworkMetrics {
        let networks = sysinfo::Networks::new_with_refreshed_list();
        let mut total_rx_bytes = 0u64;
        let mut total_tx_bytes = 0u64;
        let mut total_rx_packets = 0u64;
        let mut total_tx_packets = 0u64;

        for (_, network) in &networks {
            total_rx_bytes += network.received();
            total_tx_bytes += network.transmitted();
            total_rx_packets += network.packets_received();
            total_tx_packets += network.packets_transmitted();
        }

        // Calculate bandwidth in Mbps
        let bandwidth_mbps = if time_delta > 0.0 {
            let total_bytes = total_rx_bytes + total_tx_bytes;
            let bytes_per_second = total_bytes as f32 / time_delta;
            (bytes_per_second * 8.0) / 1_000_000.0 // Convert to Mbps
        } else {
            0.0
        };

        NetworkMetrics {
            rx_bytes: total_rx_bytes,
            tx_bytes: total_tx_bytes,
            rx_packets: total_rx_packets,
            tx_packets: total_tx_packets,
            bandwidth_mbps,
        }
    }

    /// Collect GPU metrics using Metal profiler and MLX integration
    fn collect_gpu_metrics(&self) -> GpuMetrics {
        use crate::gpu::GpuMetricsCollector;

        let gpu_collector = GpuMetricsCollector::new();
        gpu_collector.collect_metrics()
    }

    /// Get memory headroom percentage
    pub fn headroom_pct(&self) -> f32 {
        let total_memory = self.sys.total_memory();
        if total_memory == 0 {
            return 100.0;
        }

        let available_memory = self.sys.available_memory();
        (available_memory as f32 / total_memory as f32) * 100.0
    }

    /// Get system uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        sysinfo::System::uptime()
    }

    /// Get number of running processes
    pub fn process_count(&self) -> usize {
        self.sys.processes().len()
    }

    /// Get system load average (1, 5, 15 minutes)
    pub fn load_average(&self) -> (f64, f64, f64) {
        let load = sysinfo::System::load_average();
        (load.one, load.five, load.fifteen)
    }
}

impl Default for SystemMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_creation() {
        let collector = SystemMetricsCollector::new();
        assert!(collector.process_count() > 0);
    }

    #[test]
    fn test_metrics_collection() {
        let mut collector = SystemMetricsCollector::new();
        let metrics = collector.collect_metrics();

        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
        assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
        assert!(metrics.disk_io.read_bytes >= 0);
        assert!(metrics.network_io.rx_bytes >= 0);
    }

    #[test]
    fn test_headroom_calculation() {
        let collector = SystemMetricsCollector::new();
        let headroom = collector.headroom_pct();
        assert!(headroom >= 0.0 && headroom <= 100.0);
    }
}
