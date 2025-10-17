use crate::alerting::AlertingEngine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::{Duration, SystemTime};

/// Recorded latency sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySample {
    pub component: String,
    pub value_us: u64,
    pub timestamp: SystemTime,
}

/// Throughput sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputSample {
    pub component: String,
    pub tokens_per_second: f32,
    pub timestamp: SystemTime,
}

/// Aggregated performance metrics per component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub component: String,
    pub latency_p95_us: f32,
    pub latency_p99_us: f32,
    pub avg_throughput: f32,
    pub samples: usize,
    pub generated_at: SystemTime,
}

/// Performance monitoring service with latency/throughput tracking.
#[derive(Debug)]
pub struct PerformanceMonitoringService {
    latency_samples: HashMap<String, VecDeque<LatencySample>>,
    throughput_samples: HashMap<String, VecDeque<ThroughputSample>>,
    sample_window: usize,
}

impl PerformanceMonitoringService {
    pub fn new(sample_window: usize) -> Self {
        Self {
            latency_samples: HashMap::new(),
            throughput_samples: HashMap::new(),
            sample_window,
        }
    }

    pub fn record_latency(&mut self, component: impl Into<String>, value: Duration) {
        let component = component.into();
        let entry = self.latency_samples.entry(component.clone()).or_default();
        if entry.len() == self.sample_window {
            entry.pop_front();
        }
        entry.push_back(LatencySample {
            component,
            value_us: value.as_micros() as u64,
            timestamp: SystemTime::now(),
        });
    }

    pub fn record_throughput(&mut self, component: impl Into<String>, tokens_per_second: f32) {
        let component = component.into();
        let entry = self
            .throughput_samples
            .entry(component.clone())
            .or_default();
        if entry.len() == self.sample_window {
            entry.pop_front();
        }
        entry.push_back(ThroughputSample {
            component,
            tokens_per_second,
            timestamp: SystemTime::now(),
        });
    }

    /// Generate performance snapshots for all components.
    pub fn generate_snapshots(&self) -> Vec<PerformanceSnapshot> {
        self.latency_samples
            .keys()
            .chain(self.throughput_samples.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|component| {
                let latency = self.latency_samples.get(component.as_str());
                let throughput = self.throughput_samples.get(component.as_str());
                PerformanceSnapshot {
                    component: component.to_string(),
                    latency_p95_us: percentile(latency, 95.0),
                    latency_p99_us: percentile(latency, 99.0),
                    avg_throughput: avg_throughput(throughput),
                    samples: latency.map(|s| s.len()).unwrap_or(0)
                        + throughput.map(|s| s.len()).unwrap_or(0),
                    generated_at: SystemTime::now(),
                }
            })
            .collect()
    }

    /// Evaluate alerting rules using the most recent snapshots.
    pub fn evaluate_alerts(&self, alerting: &mut AlertingEngine) {
        for snapshot in self.generate_snapshots() {
            alerting.evaluate_metric(
                &format!("latency.{}", snapshot.component),
                snapshot.latency_p95_us as f64,
            );
            alerting.evaluate_metric(
                &format!("throughput.{}", snapshot.component),
                snapshot.avg_throughput as f64,
            );
        }
    }
}

fn percentile(samples: Option<&VecDeque<LatencySample>>, percentile: f32) -> f32 {
    let Some(samples) = samples else {
        return 0.0;
    };
    if samples.is_empty() {
        return 0.0;
    }
    let mut values: Vec<u64> = samples.iter().map(|sample| sample.value_us).collect();
    values.sort_unstable();
    let idx = ((values.len() - 1) as f32 * (percentile.clamp(0.0, 100.0) / 100.0)).round() as usize;
    values[idx] as f32
}

fn avg_throughput(samples: Option<&VecDeque<ThroughputSample>>) -> f32 {
    let Some(samples) = samples else {
        return 0.0;
    };
    if samples.is_empty() {
        return 0.0;
    }
    samples
        .iter()
        .map(|sample| sample.tokens_per_second)
        .sum::<f32>()
        / samples.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_latency_and_throughput() {
        let mut service = PerformanceMonitoringService::new(10);
        service.record_latency("router", Duration::from_micros(100));
        service.record_latency("router", Duration::from_micros(200));
        service.record_throughput("router", 120.0);
        service.record_throughput("router", 100.0);

        let snapshots = service.generate_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].latency_p95_us >= 100.0);
        assert!(snapshots[0].avg_throughput >= 100.0);
    }
}
