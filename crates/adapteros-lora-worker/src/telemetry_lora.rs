use std::sync::Arc;

use adapteros_core::AosError;

use crate::anomaly_detection::{AnomalyDetectorConfig, DetectionAlgorithm};
use crate::filter_engine::{FilterEngine, FilterType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TelemetryTask {
    PredictiveMaintenance,
    ResourceMonitoring,
    NetworkHealth,
}

#[derive(Debug, Clone)]
pub struct FilterAdjustment {
    pub stage_index: usize,
    pub filter: FilterType,
}

#[derive(Debug, Clone)]
pub struct DetectionAdjustment {
    pub algorithm_index: usize,
    pub algorithm: DetectionAlgorithm,
}

#[derive(Debug, Clone)]
pub struct TelemetryLoRAWeights {
    task: TelemetryTask,
    filter_adjustments: Arc<[FilterAdjustment]>,
    detection_adjustments: Arc<[DetectionAdjustment]>,
}

impl TelemetryLoRAWeights {
    pub fn new(
        task: TelemetryTask,
        filter_adjustments: Vec<FilterAdjustment>,
        detection_adjustments: Vec<DetectionAdjustment>,
    ) -> Self {
        Self {
            task,
            filter_adjustments: filter_adjustments.into(),
            detection_adjustments: detection_adjustments.into(),
        }
    }

    pub fn task(&self) -> TelemetryTask {
        self.task
    }

    pub fn filter_adjustments(&self) -> &[FilterAdjustment] {
        &self.filter_adjustments
    }

    pub fn detection_adjustments(&self) -> &[DetectionAdjustment] {
        &self.detection_adjustments
    }

    pub fn apply_to_filter_engine(&self, engine: &mut FilterEngine) {
        for adjustment in self.filter_adjustments.iter() {
            if !engine.set_stage(adjustment.stage_index, adjustment.filter) {
                tracing::warn!(
                    stage = adjustment.stage_index,
                    "ignored filter adjustment for out-of-range stage"
                );
            }
        }
    }

    pub fn apply_to_detector_config(
        &self,
        config: &mut AnomalyDetectorConfig,
    ) -> std::result::Result<(), AosError> {
        for adjustment in self.detection_adjustments.iter() {
            if let Some(target) = config.algorithms.get_mut(adjustment.algorithm_index) {
                *target = adjustment.algorithm;
            } else {
                return Err(AosError::Adapter(format!(
                    "detection adjustment index {} out of range",
                    adjustment.algorithm_index
                )));
            }
        }
        Ok(())
    }

    pub fn parameter_count(&self) -> usize {
        self.filter_adjustments.len() + self.detection_adjustments.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter_engine::{FilterEngineConfig, FilterStageConfig};

    #[test]
    fn adjustments_update_pipeline_components() {
        let mut engine = FilterEngine::new(FilterEngineConfig {
            sampling_rate_hz: 100.0,
            stages: vec![
                FilterStageConfig {
                    filter_type: FilterType::MovingAverage { window: 3 },
                },
                FilterStageConfig {
                    filter_type: FilterType::LowPass { alpha: 0.1 },
                },
            ],
        });

        let mut detector = AnomalyDetectorConfig::default();
        detector.algorithms.push(DetectionAlgorithm::Threshold {
            min: -1.0,
            max: 1.0,
        });

        let lora = TelemetryLoRAWeights::new(
            TelemetryTask::NetworkHealth,
            vec![FilterAdjustment {
                stage_index: 1,
                filter: FilterType::HighPass { alpha: 0.4 },
            }],
            vec![DetectionAdjustment {
                algorithm_index: 1,
                algorithm: DetectionAlgorithm::Threshold {
                    min: -0.5,
                    max: 0.5,
                },
            }],
        );

        lora.apply_to_filter_engine(&mut engine);
        assert_eq!(engine.stage_types()[1], FilterType::HighPass { alpha: 0.4 });

        lora.apply_to_detector_config(&mut detector)
            .expect("detector update");
        match detector.algorithms[1] {
            DetectionAlgorithm::Threshold { min, max } => {
                assert_eq!(min, -0.5);
                assert_eq!(max, 0.5);
            }
            _ => panic!("unexpected algorithm"),
        }
    }
}
