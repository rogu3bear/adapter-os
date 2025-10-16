use std::cmp::Ordering;

/// Supported anomaly detection algorithms. All thresholds are deterministic and
/// must be configured explicitly to avoid implicit heuristics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectionAlgorithm {
    ZScore { threshold: f32 },
    MedianAbsoluteDeviation { threshold: f32 },
    Threshold { min: f32, max: f32 },
    Derivative { threshold: f32 },
}

#[derive(Debug, Clone)]
pub struct AnomalyDetectorConfig {
    pub window_size: usize,
    pub algorithms: Vec<DetectionAlgorithm>,
    pub min_variance: f32,
}

impl Default for AnomalyDetectorConfig {
    fn default() -> Self {
        Self {
            window_size: 128,
            algorithms: vec![
                DetectionAlgorithm::ZScore { threshold: 3.0 },
                DetectionAlgorithm::MedianAbsoluteDeviation { threshold: 3.5 },
            ],
            min_variance: 1e-6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BaselineModel {
    pub mean: f32,
    pub variance: f32,
    pub std_dev: f32,
    pub median: f32,
    pub mad: f32,
    pub sample_count: usize,
}

impl BaselineModel {
    fn from_signal(signal: &[f32], min_variance: f32) -> Self {
        if signal.is_empty() {
            let variance = min_variance.max(1e-12);
            return Self {
                mean: 0.0,
                variance,
                std_dev: variance.sqrt(),
                median: 0.0,
                mad: variance.sqrt(),
                sample_count: 0,
            };
        }
        let sample_count = signal.len().max(1);
        let mean = signal.iter().sum::<f32>() / sample_count as f32;
        let variance = signal
            .iter()
            .map(|value| {
                let delta = value - mean;
                delta * delta
            })
            .sum::<f32>()
            / sample_count as f32;
        let variance = variance.max(min_variance);
        let std_dev = variance.sqrt();

        let mut sorted: Vec<f32> = signal.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let median = sorted[sorted.len() / 2];
        let mut deviations: Vec<f32> = sorted.iter().map(|value| (value - median).abs()).collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let mad = deviations[deviations.len() / 2] * 1.4826; // Consistent estimator

        Self {
            mean,
            variance,
            std_dev,
            median,
            mad: mad.max(min_variance.sqrt()),
            sample_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Anomaly {
    pub index: usize,
    pub value: f32,
    pub score: f32,
    pub algorithm: DetectionAlgorithm,
}

#[derive(Debug, Clone)]
pub struct AnomalyReport {
    pub anomalies: Vec<Anomaly>,
    pub baseline: BaselineModel,
}

/// Deterministic anomaly detector supporting multiple algorithms. The detector
/// maintains the last computed baseline which can be reused across batches to
/// ensure continuity in streaming scenarios.
#[derive(Debug, Clone)]
pub struct AnomalyDetector {
    config: AnomalyDetectorConfig,
    baseline: Option<BaselineModel>,
}

impl AnomalyDetector {
    pub fn new(config: AnomalyDetectorConfig) -> Self {
        Self {
            config,
            baseline: None,
        }
    }

    pub fn baseline(&self) -> Option<&BaselineModel> {
        self.baseline.as_ref()
    }

    pub fn update_baseline(&mut self, signal: &[f32]) -> BaselineModel {
        let baseline = BaselineModel::from_signal(signal, self.config.min_variance);
        self.baseline = Some(baseline.clone());
        baseline
    }

    pub fn detect(&mut self, signal: &[f32]) -> AnomalyReport {
        let baseline = self
            .baseline
            .clone()
            .unwrap_or_else(|| BaselineModel::from_signal(signal, self.config.min_variance));

        let mut anomalies = Vec::new();
        for (idx, &value) in signal.iter().enumerate() {
            for algorithm in &self.config.algorithms {
                if let Some(anomaly) =
                    self.evaluate_algorithm(algorithm, idx, value, &baseline, signal)
                {
                    anomalies.push(anomaly);
                }
            }
        }

        self.baseline = Some(baseline.clone());
        AnomalyReport {
            anomalies,
            baseline,
        }
    }

    fn evaluate_algorithm(
        &self,
        algorithm: &DetectionAlgorithm,
        index: usize,
        value: f32,
        baseline: &BaselineModel,
        signal: &[f32],
    ) -> Option<Anomaly> {
        match algorithm {
            DetectionAlgorithm::ZScore { threshold } => {
                let score = (value - baseline.mean) / baseline.std_dev;
                if score.abs() >= *threshold {
                    Some(Anomaly {
                        index,
                        value,
                        score,
                        algorithm: *algorithm,
                    })
                } else {
                    None
                }
            }
            DetectionAlgorithm::MedianAbsoluteDeviation { threshold } => {
                let score = (value - baseline.median).abs() / baseline.mad;
                if score >= *threshold {
                    Some(Anomaly {
                        index,
                        value,
                        score,
                        algorithm: *algorithm,
                    })
                } else {
                    None
                }
            }
            DetectionAlgorithm::Threshold { min, max } => {
                if value < *min || value > *max {
                    let score = if value < *min {
                        (*min - value) / (*max - *min).max(1e-6)
                    } else {
                        (value - *max) / (*max - *min).max(1e-6)
                    };
                    Some(Anomaly {
                        index,
                        value,
                        score,
                        algorithm: *algorithm,
                    })
                } else {
                    None
                }
            }
            DetectionAlgorithm::Derivative { threshold } => {
                if index == 0 {
                    return None;
                }
                let diff = value - signal[index - 1];
                if diff.abs() >= *threshold {
                    Some(Anomaly {
                        index,
                        value,
                        score: diff,
                        algorithm: *algorithm,
                    })
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zscore_detection_identifies_outliers() {
        let config = AnomalyDetectorConfig::default();
        let mut detector = AnomalyDetector::new(config);
        let mut signal: Vec<f32> = (0..100).map(|v| v as f32 * 0.1).collect();
        signal[42] = 100.0;

        let report = detector.detect(&signal);
        assert!(!report.anomalies.is_empty());
        assert!(report.anomalies.iter().any(|a| a.index == 42));
    }

    #[test]
    fn derivative_detection_handles_steps() {
        let config = AnomalyDetectorConfig {
            window_size: 64,
            algorithms: vec![DetectionAlgorithm::Derivative { threshold: 1.0 }],
            min_variance: 1e-6,
        };
        let mut detector = AnomalyDetector::new(config);
        let signal = [0.0, 0.1, 0.2, 3.0, 3.1];
        let report = detector.detect(&signal);
        assert_eq!(report.anomalies.len(), 1);
        assert_eq!(report.anomalies[0].index, 3);
    }
}
