use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::metrics::{AdapterMetrics, MetricsAggregator};
use crate::scoring::{rank_adapters, AdapterScorer};

const DEFAULT_ERROR_THRESHOLD: f32 = 0.05;
const LATENCY_ALERT_MULTIPLIER: f32 = 1.5;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct AdapterUsageStats {
    selection_count: usize,
    error_count: usize,
}

#[rustfmt::skip]
impl AdapterUsageStats {
    fn record_selection(&mut self) { self.selection_count += 1; }
    fn record_error(&mut self) { self.error_count += 1; }
    fn error_rate(&self) -> f32 { if self.selection_count == 0 { 0.0 } else { self.error_count as f32 / self.selection_count as f32 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdapterPerformanceEntry {
    pub adapter_id: String,
    pub avg_latency_ms: f32,
    pub p95_latency_ms: f32,
    pub memory_usage_mb: f32,
    pub selection_count: usize,
    pub error_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProblemAdapter {
    pub adapter_id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceReport {
    pub generated_at: SystemTime,
    pub total_selection_count: usize,
    pub total_error_count: usize,
    pub adapters: Vec<AdapterPerformanceEntry>,
    pub problematic_adapters: Vec<ProblemAdapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfilingSnapshot {
    pub metrics: Vec<AdapterPerformanceEntry>,
}

pub struct AdapterProfiler {
    aggregator: Arc<RwLock<MetricsAggregator>>,
    scorer: AdapterScorer,
    adapter_names: Vec<String>,
    telemetry: Option<TelemetryWriter>,
    sample_counter: Arc<RwLock<usize>>,
    usage_stats: Arc<RwLock<Vec<AdapterUsageStats>>>,
}

impl AdapterProfiler {
    #[rustfmt::skip]
    pub fn new(adapter_names: Vec<String>, telemetry: Option<TelemetryWriter>) -> Self {
        let num_adapters = adapter_names.len();
        Self {
            aggregator: Arc::new(RwLock::new(MetricsAggregator::new(num_adapters))),
            scorer: AdapterScorer::default(),
            usage_stats: Arc::new(RwLock::new(vec![AdapterUsageStats::default(); num_adapters])),
            adapter_names,
            telemetry,
            sample_counter: Arc::new(RwLock::new(0)),
        }
    }

    pub fn start_inference(&self) -> InferenceSession {
        InferenceSession {
            start_time: Instant::now(),
            step_times: Vec::new(),
        }
    }

    pub fn record_routing_decision(&self, adapter_ids: &[u16]) {
        let mut agg = self.aggregator.write();
        let mut usage = self.usage_stats.write();
        for &adapter_id in adapter_ids {
            agg.record_activation(adapter_id);
            if let Some(stats) = usage.get_mut(adapter_id as usize) {
                stats.record_selection();
            }
        }
    }

    pub fn record_step_latency(&self, adapter_id: u16, latency: Duration) {
        let mut agg = self.aggregator.write();
        agg.record_latency(adapter_id, latency);
    }

    pub fn record_gpu_metrics(&self, adapter_id: u16, utilization_pct: f32, memory_bytes: usize) {
        let mut agg = self.aggregator.write();
        agg.record_gpu_metrics(adapter_id, utilization_pct, memory_bytes);
    }

    pub fn update_memory_usage(&self, adapter_id: u16, bytes: usize) {
        let mut agg = self.aggregator.write();
        agg.update_memory(adapter_id, bytes);
    }

    pub fn update_quality_delta(&self, adapter_id: u16, delta: f32) {
        let mut agg = self.aggregator.write();
        agg.update_quality(adapter_id, delta);
    }

    pub fn record_inference_error(&self, adapter_id: u16) {
        let mut usage = self.usage_stats.write();
        if let Some(stats) = usage.get_mut(adapter_id as usize) {
            stats.record_error();
        }
    }

    pub fn get_adapter_metrics(&self, adapter_id: u16) -> Option<AdapterMetrics> {
        let agg = self.aggregator.read();
        self.adapter_names
            .get(adapter_id as usize)
            .map(|name| agg.get_metrics(adapter_id, name.clone()))
    }

    pub fn get_all_metrics(&self) -> Vec<AdapterMetrics> {
        let agg = self.aggregator.read();
        agg.get_all_metrics(&self.adapter_names)
    }

    pub fn get_performance_entries(&self) -> Vec<AdapterPerformanceEntry> {
        let metrics = self.get_all_metrics();
        let usage = self.usage_stats.read();

        metrics
            .into_iter()
            .enumerate()
            .map(|(idx, metric)| {
                let stats = usage.get(idx).copied().unwrap_or_default();
                AdapterPerformanceEntry {
                    adapter_id: metric.adapter_id,
                    avg_latency_ms: metric.avg_latency_us / 1000.0,
                    p95_latency_ms: metric.latency_p95_us / 1000.0,
                    memory_usage_mb: (metric.memory_bytes as f64 / (1024.0 * 1024.0)) as f32,
                    selection_count: stats.selection_count,
                    error_rate: stats.error_rate(),
                }
            })
            .collect()
    }

    pub fn get_ranked_adapters(&self) -> Vec<(usize, f32)> {
        let metrics = self.get_all_metrics();
        rank_adapters(&metrics, &self.scorer)
    }

    pub fn should_promote(&self, adapter_id: u16, threshold: f32) -> bool {
        if let Some(metrics) = self.get_adapter_metrics(adapter_id) {
            self.scorer.should_promote(&metrics, threshold)
        } else {
            false
        }
    }

    pub fn should_demote(&self, adapter_id: u16, threshold: f32) -> bool {
        if let Some(metrics) = self.get_adapter_metrics(adapter_id) {
            self.scorer.should_demote(&metrics, threshold)
        } else {
            false
        }
    }

    pub fn maybe_log_snapshot(&self) -> Result<()> {
        let mut counter = self.sample_counter.write();
        *counter += 1;

        if (*counter).is_multiple_of(20) {
            if let Some(ref telemetry) = self.telemetry {
                let metrics = self.get_performance_entries();
                telemetry.log("profiling_snapshot", ProfilingSnapshot { metrics })?;
            }
        }

        Ok(())
    }

    pub fn prune_old(&self, duration: Duration) {
        let mut agg = self.aggregator.write();
        agg.prune_old(duration);
    }

    #[rustfmt::skip]
    pub fn identify_problematic_adapters(&self, latency_threshold_ms: Option<f32>, error_rate_threshold: Option<f32>) -> Vec<ProblemAdapter> {
        let entries = self.get_performance_entries();
        if entries.is_empty() { return Vec::new(); }
        let (latency_sum, latency_count) = entries.iter().fold((0.0, 0usize), |(sum, count), entry| if entry.avg_latency_ms > 0.0 { (sum + entry.avg_latency_ms, count + 1) } else { (sum, count) });
        let latency_threshold = latency_threshold_ms.unwrap_or_else(|| if latency_count == 0 { 0.0 } else { (latency_sum / latency_count as f32) * LATENCY_ALERT_MULTIPLIER });
        let error_threshold = error_rate_threshold.unwrap_or(DEFAULT_ERROR_THRESHOLD);
        entries.into_iter().filter_map(|entry| {
            let mut reasons = Vec::new();
            if latency_threshold > 0.0 && entry.p95_latency_ms > latency_threshold {
                reasons.push(format!("p95 latency {:.2}ms exceeds threshold {:.2}ms", entry.p95_latency_ms, latency_threshold));
            }
            if entry.error_rate > error_threshold {
                reasons.push(format!("error rate {:.2}% exceeds threshold {:.2}%", entry.error_rate * 100.0, error_threshold * 100.0));
            }
            if reasons.is_empty() { None } else { Some(ProblemAdapter { adapter_id: entry.adapter_id, reasons }) }
        }).collect()
    }

    pub fn generate_report(&self) -> PerformanceReport {
        let adapters = self.get_performance_entries();
        let usage = self.usage_stats.read();
        let total_selection_count = usage.iter().map(|stats| stats.selection_count).sum();
        let total_error_count = usage.iter().map(|stats| stats.error_count).sum();

        let problematic_adapters = self.identify_problematic_adapters(None, None);

        PerformanceReport {
            generated_at: SystemTime::now(),
            total_selection_count,
            total_error_count,
            adapters,
            problematic_adapters,
        }
    }

    pub fn export_report_json(&self) -> Result<String> {
        let report = self.generate_report();
        Ok(serde_json::to_string(&report)?)
    }
}
pub struct InferenceSession {
    start_time: Instant,
    step_times: Vec<(u16, Duration)>,
}

impl InferenceSession {
    pub fn record_step(&mut self, adapter_id: u16) {
        let elapsed = self.start_time.elapsed();
        self.step_times.push((adapter_id, elapsed));
        self.start_time = Instant::now();
    }

    pub fn step_timings(&self) -> &[(u16, Duration)] {
        &self.step_times
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_and_error_tracking() {
        let profiler = AdapterProfiler::new(vec!["a".into(), "b".into()], None);
        profiler.record_routing_decision(&[0, 1, 0]);
        profiler.record_inference_error(0);
        profiler.record_routing_decision(&[1]);
        profiler.record_inference_error(1);
        profiler.record_routing_decision(&[0]);
        let entries = profiler.get_performance_entries();
        let a = entries.iter().find(|e| e.adapter_id == "a").unwrap();
        let b = entries.iter().find(|e| e.adapter_id == "b").unwrap();
        assert_eq!(a.selection_count, 3);
        assert!(a.error_rate > 0.0);
        assert_eq!(b.selection_count, 2);
        assert!(b.error_rate > 0.0);
    }

    #[test]
    fn test_identify_problematic_adapters() {
        let profiler = AdapterProfiler::new(vec!["slow".into(), "fast".into()], None);
        for _ in 0..50 {
            profiler.record_routing_decision(&[0, 1]);
            profiler.record_step_latency(0, Duration::from_millis(40));
            profiler.record_step_latency(1, Duration::from_millis(5));
        }
        profiler.record_inference_error(0);
        let problematic = profiler.identify_problematic_adapters(None, Some(0.01));
        assert_eq!(problematic.len(), 1);
        assert_eq!(problematic[0].adapter_id, "slow");
        assert!(problematic[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("p95 latency")));
    }

    #[test]
    fn test_export_report_json() {
        let profiler = AdapterProfiler::new(vec!["adapter".into()], None);
        profiler.record_routing_decision(&[0]);
        profiler.record_step_latency(0, Duration::from_millis(10));
        let json = profiler.export_report_json().unwrap();
        let report: PerformanceReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report.adapters.len(), 1);
        assert_eq!(report.adapters[0].selection_count, 1);
        assert!(report.adapters[0].avg_latency_ms >= 0.0);
    }

    #[test]
    fn test_inference_session() {
        let profiler = AdapterProfiler::new(vec!["adapter".into()], None);
        let mut session = profiler.start_inference();
        std::thread::sleep(Duration::from_millis(5));
        session.record_step(0);
        std::thread::sleep(Duration::from_millis(5));
        session.record_step(0);
        assert_eq!(session.step_timings().len(), 2);
    }
}
