//! Deterministic anomaly detection utilities for telemetry processing.

use std::collections::VecDeque;

use adapteros_core::{AosError, Result};

/// Supported anomaly detection algorithms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectionAlgorithm {
    /// Z-score based detection using rolling mean and standard deviation.
    ZScore,
    /// Percentage change relative to rolling baseline.
    PercentageChange,
    /// Rolling median absolute deviation.
    MedianDeviation,
}

/// Configuration for the anomaly detector.
#[derive(Debug, Clone)]
pub struct AnomalyDetectionConfig {
    pub window_size: usize,
    pub threshold: f32,
    pub algorithm: DetectionAlgorithm,
    pub min_points: usize,
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            window_size: 64,
            threshold: 3.0,
            algorithm: DetectionAlgorithm::ZScore,
            min_points: 8,
        }
    }
}

/// Result of analysing a sample.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomalyScore {
    pub is_anomaly: bool,
    pub score: f32,
    pub baseline: f32,
}

/// Rolling anomaly detector.
#[derive(Debug, Clone)]
pub struct AnomalyDetector {
    config: AnomalyDetectionConfig,
    history: VecDeque<f32>,
    sum: f32,
    sum_sq: f32,
}

impl AnomalyDetector {
    pub fn new(config: AnomalyDetectionConfig) -> Result<Self> {
        if config.window_size == 0 {
            return Err(AosError::Validation(
                "window size must be greater than zero".into(),
            ));
        }

        Ok(Self {
            config: config.clone(),
            history: VecDeque::with_capacity(config.window_size),
            sum: 0.0,
            sum_sq: 0.0,
        })
    }

    /// Reset the detector to its initial state.
    pub fn reset(&mut self) {
        self.history.clear();
        self.sum = 0.0;
        self.sum_sq = 0.0;
    }

    /// Observe a new sample and return its anomaly score.
    pub fn observe(&mut self, sample: f32) -> AnomalyScore {
        if self.history.len() == self.config.window_size {
            if let Some(oldest) = self.history.pop_front() {
                self.sum -= oldest;
                self.sum_sq -= oldest * oldest;
            }
        }

        self.history.push_back(sample);
        self.sum += sample;
        self.sum_sq += sample * sample;

        if self.history.len() < self.config.min_points {
            return AnomalyScore {
                is_anomaly: false,
                score: 0.0,
                baseline: sample,
            };
        }

        let score = match self.config.algorithm {
            DetectionAlgorithm::ZScore => self.z_score(sample),
            DetectionAlgorithm::PercentageChange => self.percentage_change(sample),
            DetectionAlgorithm::MedianDeviation => self.median_deviation(sample),
        };

        AnomalyScore {
            is_anomaly: score.abs() >= self.config.threshold,
            score,
            baseline: self.baseline(),
        }
    }

    fn baseline(&self) -> f32 {
        if self.history.is_empty() {
            0.0
        } else {
            self.sum / self.history.len() as f32
        }
    }

    fn variance(&self) -> f32 {
        if self.history.len() < 2 {
            0.0
        } else {
            let mean = self.baseline();
            (self.sum_sq / self.history.len() as f32) - mean * mean
        }
    }

    fn z_score(&self, sample: f32) -> f32 {
        let variance = self.variance().max(0.0);
        let std = variance.sqrt();
        if std.abs() < 1e-6 {
            0.0
        } else {
            (sample - self.baseline()) / std
        }
    }

    fn percentage_change(&self, sample: f32) -> f32 {
        let baseline = self.baseline();
        if baseline.abs() < 1e-6 {
            0.0
        } else {
            ((sample - baseline) / baseline) * 100.0
        }
    }

    fn median_deviation(&self, sample: f32) -> f32 {
        let mut history: Vec<f32> = self.history.iter().copied().collect();
        history.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = history[history.len() / 2];
        let mut deviations: Vec<f32> = history.iter().map(|value| (value - median).abs()).collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = deviations[deviations.len() / 2];
        if mad.abs() < 1e-6 {
            0.0
        } else {
            (sample - median) / mad
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zscore_detection() {
        let mut detector = AnomalyDetector::new(AnomalyDetectionConfig {
            window_size: 16,
            threshold: 2.0,
            algorithm: DetectionAlgorithm::ZScore,
            min_points: 4,
        })
        .unwrap();

        for value in [1.0, 1.1, 0.9, 1.05, 1.02] {
            let score = detector.observe(value);
            assert!(!score.is_anomaly);
        }

        let outlier = detector.observe(4.5);
        assert!(outlier.is_anomaly);
    }

    #[test]
    fn test_percentage_change_detection() {
        let mut detector = AnomalyDetector::new(AnomalyDetectionConfig {
            window_size: 8,
            threshold: 50.0,
            algorithm: DetectionAlgorithm::PercentageChange,
            min_points: 3,
        })
        .unwrap();

        for value in [100.0, 102.0, 99.5, 101.2] {
            detector.observe(value);
        }

        let spike = detector.observe(160.0);
        assert!(spike.is_anomaly);
    }

    #[test]
    fn test_median_deviation_detection() {
        let mut detector = AnomalyDetector::new(AnomalyDetectionConfig {
            window_size: 10,
            threshold: 3.0,
            algorithm: DetectionAlgorithm::MedianDeviation,
            min_points: 5,
        })
        .unwrap();

        for value in [5.0, 5.1, 4.9, 5.0, 5.2, 5.1] {
            detector.observe(value);
        }

        let anomaly = detector.observe(8.5);
        assert!(anomaly.is_anomaly);
    }
}
