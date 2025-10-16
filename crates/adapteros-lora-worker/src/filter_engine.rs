use std::collections::VecDeque;

/// Supported digital filter types used by the telemetry adapter. The focus is
/// on deterministic filters that can be executed both offline and in a streaming
/// context without relying on dynamic allocations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterType {
    MovingAverage { window: usize },
    Exponential { alpha: f32 },
    LowPass { alpha: f32 },
    HighPass { alpha: f32 },
    Median { window: usize },
}

#[derive(Debug, Clone)]
pub struct FilterStageConfig {
    pub filter_type: FilterType,
}

#[derive(Debug, Clone)]
pub struct FilterEngineConfig {
    pub sampling_rate_hz: f32,
    pub stages: Vec<FilterStageConfig>,
}

impl Default for FilterEngineConfig {
    fn default() -> Self {
        Self {
            sampling_rate_hz: 100.0,
            stages: vec![
                FilterStageConfig {
                    filter_type: FilterType::MovingAverage { window: 5 },
                },
                FilterStageConfig {
                    filter_type: FilterType::LowPass { alpha: 0.2 },
                },
            ],
        }
    }
}

#[derive(Debug, Clone)]
struct FilterState {
    kind: FilterType,
    buffer: VecDeque<f32>,
    previous_input: f32,
    previous_output: f32,
}

impl FilterState {
    fn new(kind: FilterType) -> Self {
        Self {
            kind,
            buffer: VecDeque::new(),
            previous_input: 0.0,
            previous_output: 0.0,
        }
    }

    fn apply(&mut self, value: f32) -> f32 {
        match self.kind {
            FilterType::MovingAverage { window } => {
                if window == 0 {
                    return value;
                }
                self.buffer.push_back(value);
                if self.buffer.len() > window {
                    self.buffer.pop_front();
                }
                let sum: f32 = self.buffer.iter().sum();
                sum / self.buffer.len() as f32
            }
            FilterType::Exponential { alpha } => {
                let output = alpha * value + (1.0 - alpha) * self.previous_output;
                self.previous_output = output;
                output
            }
            FilterType::LowPass { alpha } => {
                let output = self.previous_output + alpha * (value - self.previous_output);
                self.previous_output = output;
                output
            }
            FilterType::HighPass { alpha } => {
                let output = alpha * (self.previous_output + value - self.previous_input);
                self.previous_input = value;
                self.previous_output = output;
                output
            }
            FilterType::Median { window } => {
                if window <= 1 {
                    return value;
                }
                self.buffer.push_back(value);
                if self.buffer.len() > window {
                    self.buffer.pop_front();
                }
                let mut sorted: Vec<f32> = self.buffer.iter().copied().collect();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                sorted[sorted.len() / 2]
            }
        }
    }
}

/// Filter engine orchestrating multiple filter stages. Each stage maintains its
/// own state so that the pipeline can be reused across batches without losing
/// continuity.
#[derive(Debug, Clone)]
pub struct FilterEngine {
    config: FilterEngineConfig,
    stages: Vec<FilterState>,
}

impl FilterEngine {
    pub fn new(config: FilterEngineConfig) -> Self {
        let stages = config
            .stages
            .iter()
            .map(|stage| FilterState::new(stage.filter_type))
            .collect();
        Self { config, stages }
    }

    /// Apply the configured filters to the entire signal and return the filtered
    /// output. The method is deterministic because it performs per-stage
    /// filtering sequentially with fixed precision.
    pub fn process_batch(&mut self, signal: &[f32]) -> Vec<f32> {
        let mut output = signal.to_vec();
        for stage in &mut self.stages {
            output = output.into_iter().map(|value| stage.apply(value)).collect();
        }
        output
    }

    /// Stream-friendly filtering. Values are processed incrementally, updating
    /// the internal state without allocating intermediate buffers.
    pub fn process_value(&mut self, value: f32) -> f32 {
        let mut current = value;
        for stage in &mut self.stages {
            current = stage.apply(current);
        }
        current
    }

    pub fn sampling_rate(&self) -> f32 {
        self.config.sampling_rate_hz
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub fn stage_types(&self) -> Vec<FilterType> {
        self.stages.iter().map(|state| state.kind).collect()
    }

    pub fn set_stage(&mut self, index: usize, filter_type: FilterType) -> bool {
        if let Some(stage) = self.stages.get_mut(index) {
            *stage = FilterState::new(filter_type);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_average_and_low_pass_pipeline() {
        let config = FilterEngineConfig::default();
        let mut engine = FilterEngine::new(config);
        let signal: Vec<f32> = (0..10).map(|v| v as f32).collect();
        let filtered = engine.process_batch(&signal);

        assert_eq!(filtered.len(), signal.len());
        // Low-pass filtering ensures the output is monotonically increasing but
        // smoother than the input ramp.
        for window in filtered.windows(2) {
            assert!(window[0] <= window[1] + 1e-6);
        }
    }

    #[test]
    fn incremental_processing_matches_batch_mode() {
        let config = FilterEngineConfig {
            sampling_rate_hz: 200.0,
            stages: vec![
                FilterStageConfig {
                    filter_type: FilterType::Median { window: 3 },
                },
                FilterStageConfig {
                    filter_type: FilterType::HighPass { alpha: 0.5 },
                },
            ],
        };
        let mut engine = FilterEngine::new(config.clone());
        let mut incremental_engine = FilterEngine::new(config);
        let signal = [1.0, 2.0, 1.5, 3.0, 2.5, 2.0];

        let batch = engine.process_batch(&signal);
        let mut incremental = Vec::new();
        for &value in &signal {
            incremental.push(incremental_engine.process_value(value));
        }

        for (a, b) in batch.iter().zip(incremental.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
