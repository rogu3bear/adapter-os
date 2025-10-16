//! Metric collection and aggregation for adapters

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Rolling window size for metric aggregation
const WINDOW_SIZE: usize = 1000;

/// Metrics for a single adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetrics {
    pub adapter_id: String,
    pub activation_count: usize,
    pub total_tokens: usize,
    pub activation_pct: f32,
    pub avg_latency_us: f32,
    pub latency_p95_us: f32,
    pub latency_p99_us: f32,
    pub memory_bytes: usize,
    pub peak_memory_bytes: usize,
    pub memory_fragmentation: f32,
    pub gpu_utilization_pct: f32,
    pub gpu_memory_bytes: usize,
    pub quality_delta: f32,
}

impl AdapterMetrics {
    pub fn new(adapter_id: String) -> Self {
        Self {
            adapter_id,
            activation_count: 0,
            total_tokens: 0,
            activation_pct: 0.0,
            avg_latency_us: 0.0,
            latency_p95_us: 0.0,
            latency_p99_us: 0.0,
            memory_bytes: 0,
            peak_memory_bytes: 0,
            memory_fragmentation: 0.0,
            gpu_utilization_pct: 0.0,
            gpu_memory_bytes: 0,
            quality_delta: 0.0,
        }
    }
}

/// Rolling window of activation events
#[derive(Debug)]
pub struct ActivationWindow {
    /// Ring buffer of (adapter_id, timestamp)
    events: VecDeque<(u16, Instant)>,
    max_size: usize,
}

impl ActivationWindow {
    pub fn new(max_size: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Record an activation event
    pub fn record(&mut self, adapter_id: u16) {
        if self.events.len() >= self.max_size {
            self.events.pop_front();
        }
        self.events.push_back((adapter_id, Instant::now()));
    }

    /// Get activation count for adapter in window
    pub fn count(&self, adapter_id: u16) -> usize {
        self.events
            .iter()
            .filter(|(id, _)| *id == adapter_id)
            .count()
    }

    /// Get total events in window
    pub fn total(&self) -> usize {
        self.events.len()
    }

    /// Get activation percentage for adapter
    pub fn activation_pct(&self, adapter_id: u16) -> f32 {
        if self.events.is_empty() {
            return 0.0;
        }
        (self.count(adapter_id) as f32 / self.events.len() as f32) * 100.0
    }

    /// Prune events older than duration
    pub fn prune_older_than(&mut self, duration: Duration) {
        let now = Instant::now();
        while let Some((_, timestamp)) = self.events.front() {
            if now.duration_since(*timestamp) > duration {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Rolling window of latency measurements
#[derive(Debug)]
pub struct LatencyWindow {
    /// Ring buffer of (adapter_id, latency_us)
    measurements: VecDeque<(u16, u64)>,
    max_size: usize,
}

impl LatencyWindow {
    pub fn new(max_size: usize) -> Self {
        Self {
            measurements: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Record a latency measurement
    pub fn record(&mut self, adapter_id: u16, latency: Duration) {
        if self.measurements.len() >= self.max_size {
            self.measurements.pop_front();
        }
        self.measurements
            .push_back((adapter_id, latency.as_micros() as u64));
    }

    /// Get average latency for adapter in microseconds
    pub fn avg_latency_us(&self, adapter_id: u16) -> f32 {
        let measurements: Vec<u64> = self
            .measurements
            .iter()
            .filter(|(id, _)| *id == adapter_id)
            .map(|(_, latency)| *latency)
            .collect();

        if measurements.is_empty() {
            return 0.0;
        }

        measurements.iter().sum::<u64>() as f32 / measurements.len() as f32
    }

    /// Compute percentile latency for adapter
    pub fn latency_percentile(&self, adapter_id: u16, percentile: f32) -> f32 {
        let mut measurements: Vec<u64> = self
            .measurements
            .iter()
            .filter(|(id, _)| *id == adapter_id)
            .map(|(_, latency)| *latency)
            .collect();

        if measurements.is_empty() {
            return 0.0;
        }

        measurements.sort_unstable();
        let clamped = percentile.clamp(0.0, 100.0) / 100.0;
        let index = ((measurements.len() - 1) as f32 * clamped).round() as usize;
        measurements[index] as f32
    }
}

/// Memory usage tracker
#[derive(Debug)]
pub struct MemoryTracker {
    /// Map of adapter_id -> memory_bytes
    usage: Vec<usize>,
    /// Track peak usage for each adapter
    peak_usage: Vec<usize>,
    /// Rolling history for fragmentation computation
    history: Vec<VecDeque<usize>>,
    history_window: usize,
}

impl MemoryTracker {
    pub fn new(num_adapters: usize) -> Self {
        Self {
            usage: vec![0; num_adapters],
            peak_usage: vec![0; num_adapters],
            history: (0..num_adapters)
                .map(|_| VecDeque::with_capacity(32))
                .collect(),
            history_window: 32,
        }
    }

    /// Update memory usage for adapter
    pub fn update(&mut self, adapter_id: u16, bytes: usize) {
        if let Some(entry) = self.usage.get_mut(adapter_id as usize) {
            *entry = bytes;
            if let Some(peak) = self.peak_usage.get_mut(adapter_id as usize) {
                *peak = (*peak).max(bytes);
            }
            if let Some(history) = self.history.get_mut(adapter_id as usize) {
                if history.len() == self.history_window {
                    history.pop_front();
                }
                history.push_back(bytes);
            }
        }
    }

    /// Get memory usage for adapter
    pub fn get(&self, adapter_id: u16) -> usize {
        self.usage.get(adapter_id as usize).copied().unwrap_or(0)
    }

    /// Get peak memory usage observed for adapter
    pub fn peak(&self, adapter_id: u16) -> usize {
        self.peak_usage
            .get(adapter_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Estimate memory fragmentation as coefficient of variation of recent samples.
    pub fn fragmentation(&self, adapter_id: u16) -> f32 {
        let Some(history) = self.history.get(adapter_id as usize) else {
            return 0.0;
        };
        if history.len() < 2 {
            return 0.0;
        }

        let mean = history.iter().copied().sum::<usize>() as f32 / history.len() as f32;
        if mean == 0.0 {
            return 0.0;
        }

        let variance = history
            .iter()
            .map(|value| {
                let diff = *value as f32 - mean;
                diff * diff
            })
            .sum::<f32>()
            / history.len() as f32;
        (variance.sqrt() / mean).min(1.0)
    }

    /// Get total memory usage
    pub fn total(&self) -> usize {
        self.usage.iter().sum()
    }
}

#[derive(Debug, Clone, Copy)]
struct GpuSample {
    adapter_id: u16,
    utilization_pct: f32,
    memory_bytes: usize,
    timestamp: Instant,
}

/// GPU utilization window for adapters
#[derive(Debug)]
pub struct GpuWindow {
    samples: VecDeque<GpuSample>,
    max_size: usize,
}

impl GpuWindow {
    pub fn new(max_size: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn record(&mut self, adapter_id: u16, utilization_pct: f32, memory_bytes: usize) {
        if self.samples.len() >= self.max_size {
            self.samples.pop_front();
        }
        self.samples.push_back(GpuSample {
            adapter_id,
            utilization_pct,
            memory_bytes,
            timestamp: Instant::now(),
        });
    }

    pub fn avg_utilization(&self, adapter_id: u16) -> f32 {
        let mut count = 0usize;
        let total: f32 = self
            .samples
            .iter()
            .filter(|sample| sample.adapter_id == adapter_id)
            .map(|sample| {
                count += 1;
                sample.utilization_pct
            })
            .sum();

        if count == 0 {
            0.0
        } else {
            total / count as f32
        }
    }

    pub fn avg_memory(&self, adapter_id: u16) -> usize {
        let mut count = 0usize;
        let total: usize = self
            .samples
            .iter()
            .filter(|sample| sample.adapter_id == adapter_id)
            .map(|sample| {
                count += 1;
                sample.memory_bytes
            })
            .sum();

        if count == 0 {
            0
        } else {
            total / count
        }
    }

    pub fn prune_older_than(&mut self, duration: Duration) {
        let now = Instant::now();
        while let Some(sample) = self.samples.front() {
            if now.duration_since(sample.timestamp) > duration {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Quality delta tracker (placeholder for future quality measurement)
#[derive(Debug)]
pub struct QualityTracker {
    /// Map of adapter_id -> quality_delta
    deltas: Vec<f32>,
}

impl QualityTracker {
    pub fn new(num_adapters: usize) -> Self {
        Self {
            deltas: vec![0.0; num_adapters],
        }
    }

    /// Update quality delta for adapter
    pub fn update(&mut self, adapter_id: u16, delta: f32) {
        if let Some(entry) = self.deltas.get_mut(adapter_id as usize) {
            *entry = delta;
        }
    }

    /// Get quality delta for adapter
    pub fn get(&self, adapter_id: u16) -> f32 {
        self.deltas.get(adapter_id as usize).copied().unwrap_or(0.0)
    }
}

/// Aggregated metrics collector
pub struct MetricsAggregator {
    activation_window: ActivationWindow,
    latency_window: LatencyWindow,
    memory_tracker: MemoryTracker,
    quality_tracker: QualityTracker,
    gpu_window: GpuWindow,
    num_adapters: usize,
}

impl MetricsAggregator {
    pub fn new(num_adapters: usize) -> Self {
        Self {
            activation_window: ActivationWindow::new(WINDOW_SIZE),
            latency_window: LatencyWindow::new(WINDOW_SIZE),
            memory_tracker: MemoryTracker::new(num_adapters),
            quality_tracker: QualityTracker::new(num_adapters),
            gpu_window: GpuWindow::new(WINDOW_SIZE),
            num_adapters,
        }
    }

    /// Record an activation event
    pub fn record_activation(&mut self, adapter_id: u16) {
        self.activation_window.record(adapter_id);
    }

    /// Record a latency measurement
    pub fn record_latency(&mut self, adapter_id: u16, latency: Duration) {
        self.latency_window.record(adapter_id, latency);
    }

    /// Update memory usage
    pub fn update_memory(&mut self, adapter_id: u16, bytes: usize) {
        self.memory_tracker.update(adapter_id, bytes);
    }

    /// Update quality delta
    pub fn update_quality(&mut self, adapter_id: u16, delta: f32) {
        self.quality_tracker.update(adapter_id, delta);
    }

    /// Record GPU utilization and memory samples for an adapter.
    pub fn record_gpu_metrics(
        &mut self,
        adapter_id: u16,
        utilization_pct: f32,
        memory_bytes: usize,
    ) {
        self.gpu_window
            .record(adapter_id, utilization_pct.clamp(0.0, 100.0), memory_bytes);
    }

    /// Get metrics for a specific adapter
    pub fn get_metrics(&self, adapter_id: u16, adapter_name: String) -> AdapterMetrics {
        AdapterMetrics {
            adapter_id: adapter_name,
            activation_count: self.activation_window.count(adapter_id),
            total_tokens: self.activation_window.total(),
            activation_pct: self.activation_window.activation_pct(adapter_id),
            avg_latency_us: self.latency_window.avg_latency_us(adapter_id),
            latency_p95_us: self.latency_window.latency_percentile(adapter_id, 95.0),
            latency_p99_us: self.latency_window.latency_percentile(adapter_id, 99.0),
            memory_bytes: self.memory_tracker.get(adapter_id),
            peak_memory_bytes: self.memory_tracker.peak(adapter_id),
            memory_fragmentation: self.memory_tracker.fragmentation(adapter_id),
            gpu_utilization_pct: self.gpu_window.avg_utilization(adapter_id),
            gpu_memory_bytes: self.gpu_window.avg_memory(adapter_id),
            quality_delta: self.quality_tracker.get(adapter_id),
        }
    }

    /// Get metrics for all adapters
    pub fn get_all_metrics(&self, adapter_names: &[String]) -> Vec<AdapterMetrics> {
        (0..self.num_adapters)
            .map(|i| {
                let name = adapter_names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("adapter_{}", i));
                self.get_metrics(i as u16, name)
            })
            .collect()
    }

    /// Prune old events
    pub fn prune_old(&mut self, duration: Duration) {
        self.activation_window.prune_older_than(duration);
        self.gpu_window.prune_older_than(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activation_window() {
        let mut window = ActivationWindow::new(10);

        // Record some activations
        for _ in 0..5 {
            window.record(0);
        }
        for _ in 0..3 {
            window.record(1);
        }

        assert_eq!(window.count(0), 5);
        assert_eq!(window.count(1), 3);
        assert_eq!(window.total(), 8);
        assert!((window.activation_pct(0) - 62.5).abs() < 0.1);
        assert!((window.activation_pct(1) - 37.5).abs() < 0.1);
    }

    #[test]
    fn test_latency_window() {
        let mut window = LatencyWindow::new(10);

        window.record(0, Duration::from_micros(100));
        window.record(0, Duration::from_micros(200));
        window.record(1, Duration::from_micros(300));

        assert!((window.avg_latency_us(0) - 150.0).abs() < 0.1);
        assert!((window.avg_latency_us(1) - 300.0).abs() < 0.1);
        assert!((window.latency_percentile(0, 95.0) - 200.0).abs() < 0.1);
    }

    #[test]
    fn test_memory_tracker() {
        let mut tracker = MemoryTracker::new(5);

        tracker.update(0, 1000);
        tracker.update(1, 2000);
        tracker.update(0, 1500);

        assert_eq!(tracker.get(0), 1500);
        assert_eq!(tracker.get(1), 2000);
        assert_eq!(tracker.total(), 3500);
        assert_eq!(tracker.peak(0), 1500);
        assert!(tracker.fragmentation(0) >= 0.0);
    }

    #[test]
    fn test_gpu_window() {
        let mut window = GpuWindow::new(8);
        window.record(0, 80.0, 1_000_000);
        window.record(0, 60.0, 1_200_000);
        window.record(1, 50.0, 500_000);

        assert!((window.avg_utilization(0) - 70.0).abs() < 0.1);
        assert_eq!(window.avg_memory(1), 500_000);
    }
}
