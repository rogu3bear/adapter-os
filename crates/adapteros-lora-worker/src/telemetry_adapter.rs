use std::collections::HashMap;

use adapteros_core::{AosError, Result};

use crate::anomaly_detection::{Anomaly, AnomalyDetector, AnomalyDetectorConfig, BaselineModel};
use crate::filter_engine::FilterEngineConfig;
use crate::filter_engine::{FilterEngine, FilterType};
use crate::telemetry_lora::TelemetryLoRAWeights;

/// Type of telemetry signal supported by the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SignalType {
    Temperature,
    Pressure,
    CpuUsage,
    MemoryUsage,
    Custom(String),
}

impl SignalType {
    fn matches(&self, other: &SignalType) -> bool {
        match (self, other) {
            (SignalType::Custom(lhs), SignalType::Custom(rhs)) => lhs == rhs,
            _ => self == other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelemetrySignal {
    pub signal_type: SignalType,
    pub values: Vec<f32>,
    pub timestamps: Option<Vec<f64>>,
}

#[derive(Debug, Clone)]
pub struct SignalStatistics {
    pub mean: f32,
    pub std_dev: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone)]
pub struct TelemetryOutput {
    pub signal_type: SignalType,
    pub normalized: Vec<f32>,
    pub filtered: Vec<f32>,
    pub anomalies: Vec<Anomaly>,
    pub baseline: BaselineModel,
    pub statistics: SignalStatistics,
}

#[derive(Debug, Clone)]
pub struct TelemetryAdapterConfig {
    pub sample_rate_hz: f32,
    pub window_size: usize,
    pub filter: FilterEngineConfig,
    pub detector: AnomalyDetectorConfig,
    pub supported_signals: Vec<SignalType>,
}

impl Default for TelemetryAdapterConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 100.0,
            window_size: 512,
            filter: FilterEngineConfig::default(),
            detector: AnomalyDetectorConfig::default(),
            supported_signals: vec![SignalType::CpuUsage, SignalType::MemoryUsage],
        }
    }
}

#[derive(Debug)]
pub struct TelemetryAdapter {
    config: TelemetryAdapterConfig,
    filter_config: FilterEngineConfig,
    detector_config: AnomalyDetectorConfig,
    base_filter_config: FilterEngineConfig,
    base_detector_config: AnomalyDetectorConfig,
    detectors: HashMap<SignalType, AnomalyDetector>,
    attached_lora: Option<TelemetryLoRAWeights>,
}

impl TelemetryAdapter {
    pub fn new(config: TelemetryAdapterConfig) -> Self {
        Self {
            filter_config: config.filter.clone(),
            detector_config: config.detector.clone(),
            base_filter_config: config.filter.clone(),
            base_detector_config: config.detector.clone(),
            detectors: HashMap::new(),
            attached_lora: None,
            config,
        }
    }

    pub fn attach_lora(&mut self, lora: TelemetryLoRAWeights) -> Result<()> {
        self.reset_to_baseline();

        for adjustment in lora.filter_adjustments() {
            if let Some(stage) = self.filter_config.stages.get_mut(adjustment.stage_index) {
                stage.filter_type = adjustment.filter;
            } else {
                tracing::warn!(
                    stage = adjustment.stage_index,
                    "ignoring filter adjustment outside configured range"
                );
            }
        }

        let mut detector_config = self.detector_config.clone();
        lora.apply_to_detector_config(&mut detector_config)?;
        self.detector_config = detector_config;
        self.detectors.clear();
        self.attached_lora = Some(lora);
        Ok(())
    }

    pub fn detach_lora(&mut self) {
        if self.attached_lora.is_some() {
            self.reset_to_baseline();
            self.detectors.clear();
            self.attached_lora = None;
        }
    }

    pub fn process_signal(&mut self, signal: &TelemetrySignal) -> Result<TelemetryOutput> {
        self.validate_signal(signal)?;
        let (normalized, stats) = self.normalize_signal(&signal.values)?;

        let mut engine = FilterEngine::new(self.filter_config.clone());
        let filtered = engine.process_batch(&normalized);

        let detector = self
            .detectors
            .entry(signal.signal_type.clone())
            .or_insert_with(|| AnomalyDetector::new(self.detector_config.clone()));
        let report = detector.detect(&filtered);

        Ok(TelemetryOutput {
            signal_type: signal.signal_type.clone(),
            normalized,
            filtered,
            anomalies: report.anomalies,
            baseline: report.baseline,
            statistics: stats,
        })
    }

    pub fn process_batch(&mut self, signals: &[TelemetrySignal]) -> Result<Vec<TelemetryOutput>> {
        signals
            .iter()
            .map(|signal| self.process_signal(signal))
            .collect()
    }

    fn validate_signal(&self, signal: &TelemetrySignal) -> Result<()> {
        if signal.values.is_empty() {
            return Err(AosError::Adapter("empty telemetry signal".to_string()));
        }
        if signal.values.len() > self.config.window_size {
            return Err(AosError::Adapter(format!(
                "signal length {} exceeds window size {}",
                signal.values.len(),
                self.config.window_size
            )));
        }
        if let Some(ts) = &signal.timestamps {
            if ts.len() != signal.values.len() {
                return Err(AosError::Adapter("timestamp length mismatch".to_string()));
            }
        }
        if !self.config.supported_signals.is_empty()
            && !self
                .config
                .supported_signals
                .iter()
                .any(|supported| supported.matches(&signal.signal_type))
        {
            return Err(AosError::Adapter(format!(
                "unsupported signal type: {:?}",
                signal.signal_type
            )));
        }
        Ok(())
    }

    fn normalize_signal(&self, values: &[f32]) -> Result<(Vec<f32>, SignalStatistics)> {
        let len = values.len();
        let mean = values.iter().sum::<f32>() / len as f32;
        let variance = values
            .iter()
            .map(|value| {
                let delta = value - mean;
                delta * delta
            })
            .sum::<f32>()
            / len as f32;
        let std_dev = variance.max(1e-6).sqrt();
        let normalized: Vec<f32> = values
            .iter()
            .map(|value| (value - mean) / std_dev)
            .collect();
        let min = values
            .iter()
            .fold(f32::INFINITY, |acc, value| acc.min(*value));
        let max = values
            .iter()
            .fold(f32::NEG_INFINITY, |acc, value| acc.max(*value));

        Ok((
            normalized,
            SignalStatistics {
                mean,
                std_dev,
                min,
                max,
            },
        ))
    }

    fn reset_to_baseline(&mut self) {
        self.filter_config = self.base_filter_config.clone();
        self.detector_config = self.base_detector_config.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anomaly_detection::DetectionAlgorithm;

    #[test]
    fn telemetry_pipeline_detects_anomalies() {
        let mut config = TelemetryAdapterConfig::default();
        config.supported_signals.push(SignalType::Temperature);
        config.window_size = 64;
        config
            .detector
            .algorithms
            .push(DetectionAlgorithm::Threshold {
                min: -2.0,
                max: 2.0,
            });

        let mut adapter = TelemetryAdapter::new(config);
        let mut values: Vec<f32> = (0..32).map(|v| v as f32 * 0.01).collect();
        values.push(10.0);
        let signal = TelemetrySignal {
            signal_type: SignalType::Temperature,
            values,
            timestamps: None,
        };

        let output = adapter
            .process_signal(&signal)
            .expect("processing succeeded");
        assert!(!output.anomalies.is_empty());
        assert_eq!(output.signal_type, SignalType::Temperature);
    }

    #[test]
    fn attaching_lora_updates_configs() {
        let mut adapter = TelemetryAdapter::new(TelemetryAdapterConfig::default());
        let lora = TelemetryLoRAWeights::new(
            crate::telemetry_lora::TelemetryTask::PredictiveMaintenance,
            vec![crate::telemetry_lora::FilterAdjustment {
                stage_index: 0,
                filter: FilterType::HighPass { alpha: 0.3 },
            }],
            vec![crate::telemetry_lora::DetectionAdjustment {
                algorithm_index: 0,
                algorithm: DetectionAlgorithm::ZScore { threshold: 2.0 },
            }],
        );

        adapter.attach_lora(lora).expect("attach succeeds");
        assert_eq!(
            adapter.filter_config.stages[0].filter_type,
            FilterType::HighPass { alpha: 0.3 }
        );
    }
}
