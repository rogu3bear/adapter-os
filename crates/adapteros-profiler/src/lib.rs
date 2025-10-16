//! Adapter profiler for MPLoRA lifecycle management
//!
//! Tracks per-adapter metrics:
//! - Activation frequency (how often selected by router)
//! - Latency contribution (time spent in kernels)
//! - Memory footprint (LoRA weights in unified memory)
//! - Quality delta (impact on output quality)

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

pub mod metrics;
pub mod scoring;

pub use metrics::{AdapterMetrics, MetricsAggregator};
pub use scoring::{rank_adapters, AdapterScorer};

/// Profiler for tracking adapter performance
pub struct AdapterProfiler {
    aggregator: Arc<RwLock<MetricsAggregator>>,
    scorer: AdapterScorer,
    adapter_names: Vec<String>,
    telemetry: Option<TelemetryWriter>,
    sample_counter: Arc<RwLock<usize>>,
}

impl AdapterProfiler {
    /// Create a new profiler
    pub fn new(adapter_names: Vec<String>, telemetry: Option<TelemetryWriter>) -> Self {
        let num_adapters = adapter_names.len();
        Self {
            aggregator: Arc::new(RwLock::new(MetricsAggregator::new(num_adapters))),
            scorer: AdapterScorer::default(),
            adapter_names,
            telemetry,
            sample_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Start a new inference session
    pub fn start_inference(&self) -> InferenceSession {
        InferenceSession {
            start_time: Instant::now(),
            step_times: Vec::new(),
        }
    }

    /// Record a routing decision
    pub fn record_routing_decision(&self, adapter_ids: &[u16]) {
        let mut agg = self.aggregator.write();
        for &adapter_id in adapter_ids {
            agg.record_activation(adapter_id);
        }
    }

    /// Record step latency for an adapter
    pub fn record_step_latency(&self, adapter_id: u16, latency: Duration) {
        let mut agg = self.aggregator.write();
        agg.record_latency(adapter_id, latency);
    }

    /// Record GPU metrics for an adapter
    pub fn record_gpu_metrics(&self, adapter_id: u16, utilization_pct: f32, memory_bytes: usize) {
        let mut agg = self.aggregator.write();
        agg.record_gpu_metrics(adapter_id, utilization_pct, memory_bytes);
    }

    /// Update memory usage for an adapter
    pub fn update_memory_usage(&self, adapter_id: u16, bytes: usize) {
        let mut agg = self.aggregator.write();
        agg.update_memory(adapter_id, bytes);
    }

    /// Update quality delta for an adapter
    pub fn update_quality_delta(&self, adapter_id: u16, delta: f32) {
        let mut agg = self.aggregator.write();
        agg.update_quality(adapter_id, delta);
    }

    /// Get metrics for a specific adapter
    pub fn get_adapter_metrics(&self, adapter_id: u16) -> Option<AdapterMetrics> {
        let agg = self.aggregator.read();
        self.adapter_names
            .get(adapter_id as usize)
            .map(|name| agg.get_metrics(adapter_id, name.clone()))
    }

    /// Get metrics for all adapters
    pub fn get_all_metrics(&self) -> Vec<AdapterMetrics> {
        let agg = self.aggregator.read();
        agg.get_all_metrics(&self.adapter_names)
    }

    /// Get ranked adapters by score
    pub fn get_ranked_adapters(&self) -> Vec<(usize, f32)> {
        let metrics = self.get_all_metrics();
        rank_adapters(&metrics, &self.scorer)
    }

    /// Check if adapter should be promoted
    pub fn should_promote(&self, adapter_id: u16, threshold: f32) -> bool {
        if let Some(metrics) = self.get_adapter_metrics(adapter_id) {
            self.scorer.should_promote(&metrics, threshold)
        } else {
            false
        }
    }

    /// Check if adapter should be demoted
    pub fn should_demote(&self, adapter_id: u16, threshold: f32) -> bool {
        if let Some(metrics) = self.get_adapter_metrics(adapter_id) {
            self.scorer.should_demote(&metrics, threshold)
        } else {
            false
        }
    }

    /// Log profiling snapshot to telemetry (sampled at 5%)
    pub fn maybe_log_snapshot(&self) -> Result<()> {
        let mut counter = self.sample_counter.write();
        *counter += 1;

        // Sample at 5% (1 in 20)
        if (*counter).is_multiple_of(20) {
            if let Some(ref telemetry) = self.telemetry {
                let metrics = self.get_all_metrics();
                telemetry.log("profiling_snapshot", ProfilingSnapshot { metrics })?;
            }
        }

        Ok(())
    }

    /// Prune old events (keep last N minutes)
    pub fn prune_old(&self, duration: Duration) {
        let mut agg = self.aggregator.write();
        agg.prune_old(duration);
    }

    /// Generate a performance report snapshot for operators
    pub fn generate_report(&self) -> PerformanceReport {
        let agg = self.aggregator.read();
        let metrics = agg.get_all_metrics(&self.adapter_names);
        let total_activations = metrics.iter().map(|m| m.activation_count).sum();
        let total_memory_bytes = metrics.iter().map(|m| m.memory_bytes).sum();

        PerformanceReport {
            generated_at: SystemTime::now(),
            total_activations,
            total_memory_bytes,
            adapters: metrics,
        }
    }
}

/// Inference session for tracking timing
pub struct InferenceSession {
    start_time: Instant,
    step_times: Vec<(u16, Duration)>,
}

impl InferenceSession {
    /// Record a step timing
    pub fn record_step(&mut self, adapter_id: u16) {
        let elapsed = self.start_time.elapsed();
        self.step_times.push((adapter_id, elapsed));
        self.start_time = Instant::now();
    }

    /// Get step timings
    pub fn step_timings(&self) -> &[(u16, Duration)] {
        &self.step_times
    }
}

/// Profiling snapshot for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingSnapshot {
    pub metrics: Vec<AdapterMetrics>,
}

/// Performance report summarising adapter estate health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub generated_at: SystemTime,
    pub total_activations: usize,
    pub total_memory_bytes: usize,
    pub adapters: Vec<AdapterMetrics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_basic() {
        let adapter_names = vec![
            "adapter_0".to_string(),
            "adapter_1".to_string(),
            "adapter_2".to_string(),
        ];

        let profiler = AdapterProfiler::new(adapter_names, None);

        // Record some activations
        profiler.record_routing_decision(&[0, 1]);
        profiler.record_routing_decision(&[0, 2]);
        profiler.record_routing_decision(&[0, 1]);
        profiler.record_gpu_metrics(0, 80.0, 1_000_000);
        profiler.record_step_latency(0, Duration::from_micros(120));

        let metrics = profiler.get_all_metrics();

        // Adapter 0 should have highest activation
        assert!(metrics[0].activation_count > metrics[2].activation_count);
    }

    #[test]
    fn test_inference_session() {
        let mut session = AdapterProfiler::new(vec![], None).start_inference();

        std::thread::sleep(Duration::from_millis(10));
        session.record_step(0);

        std::thread::sleep(Duration::from_millis(10));
        session.record_step(1);

        let timings = session.step_timings();
        assert_eq!(timings.len(), 2);
    }

    #[test]
    fn test_generate_report() {
        let profiler = AdapterProfiler::new(vec!["adapter".into()], None);
        profiler.record_routing_decision(&[0]);
        profiler.update_memory_usage(0, 1024);

        let report = profiler.generate_report();
        assert_eq!(report.adapters.len(), 1);
        assert_eq!(report.total_activations, 1);
        assert_eq!(report.total_memory_bytes, 1024);
    }
}
