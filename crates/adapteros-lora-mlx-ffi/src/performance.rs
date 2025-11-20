//! Performance Monitoring and Profiling for MLX Backend
//!
//! This module provides Rust-side performance monitoring that wraps the C++ profiling infrastructure.
//! It tracks operation timings, memory usage, and provides analysis tools for optimization.
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

// FFI declarations for C++ performance profiling
extern "C" {
    fn mlx_get_performance_stats() -> *const std::os::raw::c_char;
    fn mlx_reset_performance_counters();
    #[allow(dead_code)]
    fn mlx_set_profiling_enabled(enabled: bool);
}

/// Performance statistics for a single operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStats {
    /// Number of times the operation was called
    pub count: u64,
    /// Average time per call in microseconds
    pub avg_us: f64,
    /// Minimum time observed in microseconds
    pub min_us: f64,
    /// Maximum time observed in microseconds
    pub max_us: f64,
    /// Total time spent in milliseconds
    pub total_ms: f64,
}

impl OperationStats {
    /// Calculate throughput (operations per second)
    pub fn throughput_ops_per_sec(&self) -> f64 {
        if self.total_ms > 0.0 {
            (self.count as f64) / (self.total_ms / 1000.0)
        } else {
            0.0
        }
    }

    /// Get 95th percentile estimate (using max as conservative estimate)
    pub fn p95_us(&self) -> f64 {
        // Conservative estimate: 95th percentile ≈ 0.6 * max + 0.4 * avg
        0.6 * self.max_us + 0.4 * self.avg_us
    }

    /// Check if operation is a performance bottleneck
    /// (high total time and high average time)
    pub fn is_bottleneck(&self, threshold_ms: f64) -> bool {
        self.total_ms > threshold_ms && self.avg_us > 100.0
    }
}

/// Complete performance snapshot from MLX backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Timestamp when snapshot was taken
    pub timestamp: std::time::SystemTime,
    /// Statistics by operation type
    pub operations: HashMap<String, OperationStats>,
    /// Memory usage at snapshot time
    pub memory_usage_bytes: usize,
    /// Number of active allocations
    pub allocation_count: usize,
}

impl PerformanceSnapshot {
    /// Get the current performance snapshot from C++ backend
    pub fn capture() -> Result<Self, Box<dyn std::error::Error>> {
        // Get C++ performance stats
        let stats_ptr = unsafe { mlx_get_performance_stats() };
        if stats_ptr.is_null() {
            return Err("Failed to get performance stats from C++".into());
        }

        let stats_cstr = unsafe { std::ffi::CStr::from_ptr(stats_ptr) };
        let stats_str = stats_cstr.to_str()?;

        // Parse JSON from C++
        let operations: HashMap<String, OperationStats> = serde_json::from_str(stats_str)?;

        // Get memory stats
        let (memory_usage_bytes, allocation_count) = crate::memory::memory_stats();

        Ok(Self {
            timestamp: std::time::SystemTime::now(),
            operations,
            memory_usage_bytes,
            allocation_count,
        })
    }

    /// Get total time spent across all operations
    pub fn total_time_ms(&self) -> f64 {
        self.operations.values().map(|s| s.total_ms).sum()
    }

    /// Get most expensive operations (by total time)
    pub fn top_operations(&self, limit: usize) -> Vec<(&str, &OperationStats)> {
        let mut ops: Vec<_> = self.operations.iter()
            .map(|(name, stats)| (name.as_str(), stats))
            .collect();

        ops.sort_by(|a, b| b.1.total_ms.partial_cmp(&a.1.total_ms).unwrap());
        ops.truncate(limit);
        ops
    }

    /// Identify bottleneck operations
    pub fn bottlenecks(&self) -> Vec<(&str, &OperationStats)> {
        self.operations.iter()
            .filter(|(_, stats)| stats.is_bottleneck(10.0)) // 10ms threshold
            .map(|(name, stats)| (name.as_str(), stats))
            .collect()
    }

    /// Generate a human-readable performance report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== MLX Backend Performance Report ===\n\n");

        // Memory stats
        report.push_str(&format!(
            "Memory Usage: {:.2} MB ({} allocations)\n\n",
            self.memory_usage_bytes as f64 / (1024.0 * 1024.0),
            self.allocation_count
        ));

        // Top operations
        report.push_str("Top Operations by Total Time:\n");
        for (name, stats) in self.top_operations(10) {
            report.push_str(&format!(
                "  {}: {:.2}ms total, {:.2}µs avg ({} calls, {:.0} ops/sec)\n",
                name,
                stats.total_ms,
                stats.avg_us,
                stats.count,
                stats.throughput_ops_per_sec()
            ));
        }

        // Bottlenecks
        let bottlenecks = self.bottlenecks();
        if !bottlenecks.is_empty() {
            report.push_str("\nPerformance Bottlenecks:\n");
            for (name, stats) in bottlenecks {
                report.push_str(&format!(
                    "  {}: {:.2}ms total, {:.2}µs avg, {:.2}µs max\n",
                    name, stats.total_ms, stats.avg_us, stats.max_us
                ));
            }
        }

        report
    }

    /// Export to JSON for external analysis
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Performance profiler that tracks operations over time
#[derive(Clone)]
pub struct PerformanceProfiler {
    snapshots: Arc<RwLock<Vec<PerformanceSnapshot>>>,
    start_time: Instant,
}

impl PerformanceProfiler {
    /// Create a new performance profiler
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    /// Reset all performance counters
    pub fn reset(&self) {
        unsafe {
            mlx_reset_performance_counters();
        }
        self.snapshots.write().clear();
    }

    /// Take a performance snapshot
    pub fn snapshot(&self) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = PerformanceSnapshot::capture()?;
        self.snapshots.write().push(snapshot);
        Ok(())
    }

    /// Get all captured snapshots
    pub fn snapshots(&self) -> Vec<PerformanceSnapshot> {
        self.snapshots.read().clone()
    }

    /// Get the most recent snapshot
    pub fn latest_snapshot(&self) -> Option<PerformanceSnapshot> {
        self.snapshots.read().last().cloned()
    }

    /// Calculate performance delta between two snapshots
    pub fn delta(&self, from_idx: usize, to_idx: usize) -> Option<PerformanceDelta> {
        let snapshots = self.snapshots.read();
        if from_idx >= snapshots.len() || to_idx >= snapshots.len() {
            return None;
        }

        Some(PerformanceDelta::new(&snapshots[from_idx], &snapshots[to_idx]))
    }

    /// Generate a summary report across all snapshots
    pub fn summary_report(&self) -> String {
        let snapshots = self.snapshots.read();
        if snapshots.is_empty() {
            return "No performance data collected yet.".to_string();
        }

        let mut report = String::new();
        report.push_str("=== MLX Backend Performance Summary ===\n\n");
        report.push_str(&format!("Total snapshots: {}\n", snapshots.len()));
        report.push_str(&format!("Duration: {:.2}s\n\n", self.start_time.elapsed().as_secs_f64()));

        // Aggregate stats across snapshots
        if let Some(latest) = snapshots.last() {
            report.push_str(&latest.generate_report());
        }

        report
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance delta between two snapshots
#[derive(Debug, Clone)]
pub struct PerformanceDelta {
    pub operation_deltas: HashMap<String, OperationStatsDelta>,
    pub memory_delta_bytes: i64,
    pub allocation_delta: i64,
}

#[derive(Debug, Clone)]
pub struct OperationStatsDelta {
    pub count_delta: i64,
    pub total_ms_delta: f64,
    pub avg_us_delta: f64,
}

impl PerformanceDelta {
    fn new(from: &PerformanceSnapshot, to: &PerformanceSnapshot) -> Self {
        let mut operation_deltas = HashMap::new();

        for (name, to_stats) in &to.operations {
            if let Some(from_stats) = from.operations.get(name) {
                operation_deltas.insert(
                    name.clone(),
                    OperationStatsDelta {
                        count_delta: to_stats.count as i64 - from_stats.count as i64,
                        total_ms_delta: to_stats.total_ms - from_stats.total_ms,
                        avg_us_delta: to_stats.avg_us - from_stats.avg_us,
                    },
                );
            }
        }

        Self {
            operation_deltas,
            memory_delta_bytes: to.memory_usage_bytes as i64 - from.memory_usage_bytes as i64,
            allocation_delta: to.allocation_count as i64 - from.allocation_count as i64,
        }
    }
}

/// Rust-side operation timer (complements C++ ScopedTimer)
pub struct OperationTimer {
    start: Instant,
    operation_name: String,
}

impl OperationTimer {
    pub fn new(operation_name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            operation_name: operation_name.into(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        tracing::trace!(
            operation = %self.operation_name,
            duration_us = elapsed.as_micros(),
            "Operation completed"
        );
    }
}

/// Performance metrics aggregator
pub struct PerformanceMetrics {
    tokens_generated: AtomicU64,
    total_inference_time_ns: AtomicU64,
    adapter_switches: AtomicU64,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            tokens_generated: AtomicU64::new(0),
            total_inference_time_ns: AtomicU64::new(0),
            adapter_switches: AtomicU64::new(0),
        }
    }

    pub fn record_token_generated(&self, inference_time: Duration) {
        self.tokens_generated.fetch_add(1, Ordering::Relaxed);
        self.total_inference_time_ns.fetch_add(
            inference_time.as_nanos() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn record_adapter_switch(&self) {
        self.adapter_switches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn tokens_per_second(&self) -> f64 {
        let tokens = self.tokens_generated.load(Ordering::Relaxed);
        let time_ns = self.total_inference_time_ns.load(Ordering::Relaxed);

        if time_ns > 0 {
            (tokens as f64) / (time_ns as f64 / 1_000_000_000.0)
        } else {
            0.0
        }
    }

    pub fn avg_latency_ms(&self) -> f64 {
        let tokens = self.tokens_generated.load(Ordering::Relaxed);
        let time_ns = self.total_inference_time_ns.load(Ordering::Relaxed);

        if tokens > 0 {
            (time_ns as f64 / tokens as f64) / 1_000_000.0
        } else {
            0.0
        }
    }

    pub fn reset(&self) {
        self.tokens_generated.store(0, Ordering::Relaxed);
        self.total_inference_time_ns.store(0, Ordering::Relaxed);
        self.adapter_switches.store(0, Ordering::Relaxed);
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_snapshot_creation() {
        // This test will work even without real MLX backend
        // as it tests the data structures
        let stats = OperationStats {
            count: 100,
            avg_us: 50.0,
            min_us: 10.0,
            max_us: 200.0,
            total_ms: 5.0,
        };

        assert_eq!(stats.throughput_ops_per_sec(), 20_000.0);
        assert!(stats.p95_us() > stats.avg_us);
    }

    #[test]
    fn test_performance_profiler() {
        let profiler = PerformanceProfiler::new();
        assert_eq!(profiler.snapshots().len(), 0);

        // Test reset
        profiler.reset();
        assert_eq!(profiler.snapshots().len(), 0);
    }

    #[test]
    fn test_operation_timer() {
        let _timer = OperationTimer::new("test_operation");
        std::thread::sleep(std::time::Duration::from_micros(100));
        // Timer will log on drop
    }

    #[test]
    fn test_performance_metrics() {
        let metrics = PerformanceMetrics::new();

        metrics.record_token_generated(Duration::from_millis(10));
        metrics.record_token_generated(Duration::from_millis(20));

        assert!(metrics.tokens_per_second() > 0.0);
        assert!(metrics.avg_latency_ms() > 0.0);

        metrics.reset();
        assert_eq!(metrics.tokens_per_second(), 0.0);
    }
}
