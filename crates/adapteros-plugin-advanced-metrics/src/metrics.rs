//! Metrics Collection Logic
//!
//! This module implements the core metrics collection functionality,
//! tracking performance data from inference, training, and system events.

use adapteros_core::{plugin_events::*, Result};
use prometheus::{CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Statistics about collected metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsStats {
    pub tracked_adapters: usize,
    pub inference_events: u64,
    pub training_events: u64,
    pub metrics_ticks: u64,
}

/// Metrics collector for advanced performance tracking
pub struct MetricsCollector {
    // Inference latency percentiles (p50, p95, p99) per adapter
    inference_latency: HistogramVec,

    // Training job duration histograms
    training_duration: HistogramVec,

    // Adapter activation patterns (counter)
    adapter_activations: CounterVec,

    // Token throughput per tenant (counter for total tokens, gauge for rate)
    tenant_tokens_total: CounterVec,
    tenant_tokens_per_sec: GaugeVec,

    // System metrics from metrics tick
    system_cpu_percent: GaugeVec,
    system_memory_bytes: GaugeVec,
    system_active_adapters: GaugeVec,

    // Internal tracking
    tracked_adapters: HashMap<String, AdapterMetrics>,
    inference_count: u64,
    training_count: u64,
    metrics_tick_count: u64,

    // Training job start times for duration calculation
    training_job_starts: HashMap<String, std::time::Instant>,
}

#[derive(Debug, Clone)]
struct AdapterMetrics {
    adapter_id: String,
    activation_count: u64,
    total_latency_ms: f64,
    last_used: std::time::Instant,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            inference_latency: HistogramVec::new(
                HistogramOpts::new(
                    "adapteros_inference_latency_ms",
                    "Inference latency in milliseconds by adapter",
                )
                .buckets(vec![
                    10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
                ]),
                &["adapter_id", "tenant_id"],
            )
            .expect("Failed to create inference_latency metric"),
            training_duration: HistogramVec::new(
                HistogramOpts::new(
                    "adapteros_training_duration_seconds",
                    "Training job duration in seconds",
                )
                .buckets(vec![
                    60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 14400.0, 28800.0, 86400.0,
                ]),
                &["adapter_id", "tenant_id", "status"],
            )
            .expect("Failed to create training_duration metric"),
            adapter_activations: CounterVec::new(
                Opts::new(
                    "adapteros_adapter_activations_total",
                    "Total number of adapter activations",
                ),
                &["adapter_id", "tenant_id"],
            )
            .expect("Failed to create adapter_activations metric"),
            tenant_tokens_total: CounterVec::new(
                Opts::new(
                    "adapteros_tenant_tokens_total",
                    "Total tokens processed by tenant",
                ),
                &["tenant_id"],
            )
            .expect("Failed to create tenant_tokens_total metric"),
            tenant_tokens_per_sec: GaugeVec::new(
                Opts::new(
                    "adapteros_tenant_tokens_per_sec",
                    "Current token throughput per tenant",
                ),
                &["tenant_id"],
            )
            .expect("Failed to create tenant_tokens_per_sec metric"),
            system_cpu_percent: GaugeVec::new(
                Opts::new(
                    "adapteros_system_cpu_percent",
                    "System CPU usage percentage",
                ),
                &["node"],
            )
            .expect("Failed to create system_cpu_percent metric"),
            system_memory_bytes: GaugeVec::new(
                Opts::new(
                    "adapteros_system_memory_bytes",
                    "System memory usage in bytes",
                ),
                &["node", "type"],
            )
            .expect("Failed to create system_memory_bytes metric"),
            system_active_adapters: GaugeVec::new(
                Opts::new(
                    "adapteros_system_active_adapters",
                    "Number of active adapters",
                ),
                &["node"],
            )
            .expect("Failed to create system_active_adapters metric"),
            tracked_adapters: HashMap::new(),
            inference_count: 0,
            training_count: 0,
            metrics_tick_count: 0,
            training_job_starts: HashMap::new(),
        }
    }

    /// Initialize the metrics collector (register metrics with Prometheus)
    pub fn initialize(&mut self) -> Result<()> {
        debug!("Initializing advanced metrics collector");

        // Register metrics with Prometheus global registry
        // Ignore duplicate registration errors in tests (metrics persist across test runs)
        let _ = prometheus::register(Box::new(self.inference_latency.clone()));
        let _ = prometheus::register(Box::new(self.training_duration.clone()));
        let _ = prometheus::register(Box::new(self.adapter_activations.clone()));
        let _ = prometheus::register(Box::new(self.tenant_tokens_total.clone()));
        let _ = prometheus::register(Box::new(self.tenant_tokens_per_sec.clone()));
        let _ = prometheus::register(Box::new(self.system_cpu_percent.clone()));
        let _ = prometheus::register(Box::new(self.system_memory_bytes.clone()));
        let _ = prometheus::register(Box::new(self.system_active_adapters.clone()));

        debug!("Advanced metrics collector initialized");
        Ok(())
    }

    /// Record an inference completion event
    pub fn record_inference_complete(&mut self, event: &InferenceEvent) -> Result<()> {
        self.inference_count += 1;

        let tenant_id = event.tenant_id.as_deref().unwrap_or("unknown");

        // Record latency for each adapter used
        for adapter_id in &event.adapter_ids {
            self.inference_latency
                .with_label_values(&[adapter_id.as_str(), tenant_id])
                .observe(event.latency_ms);

            self.adapter_activations
                .with_label_values(&[adapter_id.as_str(), tenant_id])
                .inc();

            // Update internal tracking
            self.tracked_adapters
                .entry(adapter_id.clone())
                .and_modify(|m| {
                    m.activation_count += 1;
                    m.total_latency_ms += event.latency_ms;
                    m.last_used = std::time::Instant::now();
                })
                .or_insert(AdapterMetrics {
                    adapter_id: adapter_id.clone(),
                    activation_count: 1,
                    total_latency_ms: event.latency_ms,
                    last_used: std::time::Instant::now(),
                });
        }

        // Track tenant token throughput
        if let Some(tokens) = event.tokens_generated {
            self.tenant_tokens_total
                .with_label_values(&[tenant_id])
                .inc_by(tokens as f64);
        }

        if let Some(tokens_per_sec) = event.tokens_per_sec {
            self.tenant_tokens_per_sec
                .with_label_values(&[tenant_id])
                .set(tokens_per_sec);
        }

        Ok(())
    }

    /// Record a training job event
    pub fn record_training_job_event(&mut self, event: &TrainingJobEvent) -> Result<()> {
        self.training_count += 1;

        // Track job start time for duration calculation
        if event.status == "running" {
            self.training_job_starts
                .insert(event.job_id.clone(), std::time::Instant::now());
        }

        // Record duration when job completes
        if matches!(event.status.as_str(), "completed" | "failed" | "cancelled") {
            if let Some(start_time) = self.training_job_starts.remove(&event.job_id) {
                let duration_secs = start_time.elapsed().as_secs_f64();

                let adapter_id = event.adapter_id.as_deref().unwrap_or("unknown");
                let tenant_id = event.tenant_id.as_deref().unwrap_or("unknown");

                self.training_duration
                    .with_label_values(&[adapter_id, tenant_id, event.status.as_str()])
                    .observe(duration_secs);
            } else {
                warn!(
                    job_id = %event.job_id,
                    status = %event.status,
                    "Training job completed without recorded start time"
                );
            }
        }

        Ok(())
    }

    /// Record a metrics tick event
    pub fn record_metrics_tick(&mut self, event: &MetricsTickEvent) -> Result<()> {
        self.metrics_tick_count += 1;

        let node = "local"; // Could be made configurable

        // System metrics
        if let Some(cpu) = event.cpu_percent {
            self.system_cpu_percent.with_label_values(&[node]).set(cpu);
        }

        if let Some(mem_bytes) = event.memory_bytes {
            self.system_memory_bytes
                .with_label_values(&[node, "used"])
                .set(mem_bytes as f64);
        }

        if let Some(gpu_bytes) = event.gpu_memory_bytes {
            self.system_memory_bytes
                .with_label_values(&[node, "gpu"])
                .set(gpu_bytes as f64);
        }

        if let Some(active) = event.active_adapters {
            self.system_active_adapters
                .with_label_values(&[node])
                .set(active as f64);
        }

        Ok(())
    }

    /// Get current statistics
    pub fn get_stats(&self) -> MetricsStats {
        MetricsStats {
            tracked_adapters: self.tracked_adapters.len(),
            inference_events: self.inference_count,
            training_events: self.training_count,
            metrics_ticks: self.metrics_tick_count,
        }
    }

    /// Get tracked adapter metrics
    pub fn get_adapter_metrics(&self) -> Vec<(String, u64, f64)> {
        self.tracked_adapters
            .values()
            .map(|m| {
                (
                    m.adapter_id.clone(),
                    m.activation_count,
                    m.total_latency_ms / m.activation_count as f64,
                )
            })
            .collect()
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_initialization() {
        let mut collector = MetricsCollector::new();
        assert!(collector.initialize().is_ok());

        let stats = collector.get_stats();
        assert_eq!(stats.tracked_adapters, 0);
        assert_eq!(stats.inference_events, 0);
        assert_eq!(stats.training_events, 0);
    }

    #[test]
    fn test_inference_event_recording() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        let event = InferenceEvent {
            request_id: "req-1".to_string(),
            adapter_ids: vec!["adapter-1".to_string()],
            stack_id: None,
            prompt: None,
            output: None,
            latency_ms: 100.0,
            tokens_generated: Some(50),
            tokens_per_sec: Some(100.0),
            tenant_id: Some("tenant-1".to_string()),
            model: None,
            streaming: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        assert!(collector.record_inference_complete(&event).is_ok());

        let stats = collector.get_stats();
        assert_eq!(stats.inference_events, 1);
        assert_eq!(stats.tracked_adapters, 1);

        let adapter_metrics = collector.get_adapter_metrics();
        assert_eq!(adapter_metrics.len(), 1);
        assert_eq!(adapter_metrics[0].0, "adapter-1");
        assert_eq!(adapter_metrics[0].1, 1); // activation count
        assert_eq!(adapter_metrics[0].2, 100.0); // avg latency
    }

    #[test]
    fn test_training_event_recording() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        // Start event
        let start_event = TrainingJobEvent {
            job_id: "job-1".to_string(),
            status: "running".to_string(),
            progress_pct: Some(0.0),
            loss: None,
            tokens_per_sec: None,
            dataset_id: Some("dataset-1".to_string()),
            adapter_id: Some("adapter-1".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        assert!(collector.record_training_job_event(&start_event).is_ok());
        assert_eq!(collector.training_job_starts.len(), 1);

        // Complete event
        let complete_event = TrainingJobEvent {
            job_id: "job-1".to_string(),
            status: "completed".to_string(),
            progress_pct: Some(100.0),
            loss: Some(0.1),
            tokens_per_sec: Some(1000.0),
            dataset_id: Some("dataset-1".to_string()),
            adapter_id: Some("adapter-1".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: HashMap::new(),
        };

        assert!(collector.record_training_job_event(&complete_event).is_ok());
        assert_eq!(collector.training_job_starts.len(), 0); // Cleaned up

        let stats = collector.get_stats();
        assert_eq!(stats.training_events, 2);
    }

    #[test]
    fn test_metrics_tick_recording() {
        let mut collector = MetricsCollector::new();
        collector.initialize().unwrap();

        let event = MetricsTickEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_percent: Some(50.0),
            memory_bytes: Some(1024 * 1024 * 1024),
            memory_percent: Some(50.0),
            active_adapters: Some(5),
            loaded_adapters: Some(10),
            inference_requests: Some(100),
            avg_latency_ms: Some(150.0),
            gpu_memory_bytes: Some(2048 * 1024 * 1024),
            gpu_percent: Some(75.0),
            metrics: HashMap::new(),
        };

        assert!(collector.record_metrics_tick(&event).is_ok());

        let stats = collector.get_stats();
        assert_eq!(stats.metrics_ticks, 1);
    }
}
