//! Telemetry adapter responsible for deterministic signal processing.

use std::collections::HashMap;
use std::time::Instant;

use adapteros_core::{AosError, Result};

use crate::anomaly_detection::{
    AnomalyDetectionConfig, AnomalyDetector, AnomalyScore, DetectionAlgorithm,
};
use crate::filter_engine::{FilterConfig, FilterEngine, FilterKind};
use crate::telemetry_lora::{TelemetryLoraRegistry, TelemetryMergePlan};

/// Sample within a telemetry channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalSample {
    pub timestamp: u64,
    pub value: f32,
}

/// Set of samples for a telemetry channel.
#[derive(Debug, Clone)]
pub struct SignalChannel {
    pub name: String,
    pub samples: Vec<SignalSample>,
}

impl SignalChannel {
    pub fn new(name: impl Into<String>, samples: Vec<SignalSample>) -> Self {
        Self {
            name: name.into(),
            samples,
        }
    }
}

/// Output of telemetry processing for a single channel.
#[derive(Debug, Clone)]
pub struct TelemetryOutput {
    pub name: String,
    pub filtered: Vec<f32>,
    pub anomalies: Vec<AnomalyScore>,
}

/// Configuration for telemetry adapter.
#[derive(Debug, Clone)]
pub struct TelemetryAdapterConfig {
    pub sample_rate_hz: f32,
    pub default_filter: FilterKind,
    pub anomaly: AnomalyDetectionConfig,
}

impl Default for TelemetryAdapterConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 50.0,
            default_filter: FilterKind::LowPass { cutoff_hz: 5.0 },
            anomaly: AnomalyDetectionConfig {
                window_size: 64,
                threshold: 4.0,
                algorithm: DetectionAlgorithm::ZScore,
                min_points: 8,
            },
        }
    }
}

/// Telemetry processing metrics.
#[derive(Debug, Default, Clone)]
pub struct TelemetryAdapterMetrics {
    pub channels_processed: usize,
    pub samples_processed: usize,
    pub last_latency_ms: Option<u128>,
}

/// Adapter performing filtering and anomaly detection on telemetry streams.
#[derive(Debug)]
pub struct TelemetryAdapter {
    config: TelemetryAdapterConfig,
    filters: HashMap<String, FilterEngine>,
    detectors: HashMap<String, AnomalyDetector>,
    metrics: TelemetryAdapterMetrics,
}

impl TelemetryAdapter {
    pub fn new(config: TelemetryAdapterConfig) -> Result<Self> {
        if config.sample_rate_hz <= 0.0 {
            return Err(AosError::Validation(
                "sample rate must be positive".into(),
            ));
        }

        Ok(Self {
            config,
            filters: HashMap::new(),
            detectors: HashMap::new(),
            metrics: TelemetryAdapterMetrics::default(),
        })
    }

    /// Access metrics for observability.
    pub fn metrics(&self) -> &TelemetryAdapterMetrics {
        &self.metrics
    }

    fn get_filter(&mut self, channel: &str) -> Result<&mut FilterEngine> {
        if !self.filters.contains_key(channel) {
            let config = FilterConfig {
                sample_rate_hz: self.config.sample_rate_hz,
                kind: self.config.default_filter,
            };
            self.filters
                .insert(channel.to_string(), FilterEngine::new(config)?);
        }
        Ok(self.filters.get_mut(channel).unwrap())
    }

    fn get_detector(&mut self, channel: &str) -> Result<&mut AnomalyDetector> {
        if !self.detectors.contains_key(channel) {
            self.detectors.insert(
                channel.to_string(),
                AnomalyDetector::new(self.config.anomaly.clone())?,
            );
        }
        Ok(self.detectors.get_mut(channel).unwrap())
    }

    /// Process telemetry channels and return filtered signals and anomalies.
    pub fn process_channels(&mut self, channels: &[SignalChannel]) -> Result<Vec<TelemetryOutput>> {
        if channels.is_empty() {
            return Err(AosError::Validation(
                "no telemetry channels supplied".into(),
            ));
        }

        let start = Instant::now();
        let mut outputs = Vec::with_capacity(channels.len());

        let mut sorted_channels = channels.to_vec();
        sorted_channels.sort_by(|a, b| a.name.cmp(&b.name));

        for channel in sorted_channels.iter() {
            let mut filtered = Vec::with_capacity(channel.samples.len());
            let mut anomalies = Vec::new();

            for sample in &channel.samples {
                let filtered_value = {
                    let filter = self.get_filter(&channel.name)?;
                    filter.apply_sample(sample.value)
                };
                let score = {
                    let detector = self.get_detector(&channel.name)?;
                    detector.observe(filtered_value)
                };
                filtered.push(filtered_value);
                if score.is_anomaly {
                    anomalies.push(score.clone());
                }
            }

            outputs.push(TelemetryOutput {
                name: channel.name.clone(),
                filtered,
                anomalies,
            });

            self.metrics.channels_processed += 1;
            self.metrics.samples_processed += channel.samples.len();
        }

        self.metrics.last_latency_ms = Some(start.elapsed().as_millis());
        Ok(outputs)
    }

    /// Apply a telemetry LoRA merge plan to mutable weights.
    pub fn apply_lora(
        &self,
        registry: &TelemetryLoraRegistry,
        plan: &TelemetryMergePlan,
        weights: &mut [f32],
        bias: &mut [f32],
    ) -> Result<()> {
        plan.apply(registry, weights, bias)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel(name: &str, values: &[f32]) -> SignalChannel {
        let samples = values
            .iter()
            .enumerate()
            .map(|(idx, &value)| SignalSample {
                timestamp: idx as u64,
                value,
            })
            .collect();
        SignalChannel::new(name, samples)
    }

    #[test]
    fn test_process_channels_detects_anomaly() {
        let mut adapter = TelemetryAdapter::new(TelemetryAdapterConfig::default()).unwrap();
        let normal = make_channel("temperature", &[1.0, 1.2, 0.9, 1.1, 5.0, 1.0]);
        let outputs = adapter.process_channels(&[normal]).unwrap();
        assert_eq!(outputs.len(), 1);
        assert!(!outputs[0].anomalies.is_empty());
    }

    #[test]
    fn test_metrics_increment() {
        let mut adapter = TelemetryAdapter::new(TelemetryAdapterConfig::default()).unwrap();
        let channel_a = make_channel("a", &[0.0, 0.1, 0.2]);
        let channel_b = make_channel("b", &[1.0, 1.1, 1.2]);
        adapter.process_channels(&[channel_a, channel_b]).unwrap();
        assert_eq!(adapter.metrics.channels_processed, 2);
        assert_eq!(adapter.metrics.samples_processed, 6);
        assert!(adapter.metrics.last_latency_ms.is_some());
    }
}
