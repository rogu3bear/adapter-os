//! Digital filter implementation for telemetry preprocessing.

use std::collections::VecDeque;

use adapteros_core::{AosError, Result};

/// Supported filter types for the telemetry adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterKind {
    LowPass { cutoff_hz: f32 },
    HighPass { cutoff_hz: f32 },
    BandPass { low_cut_hz: f32, high_cut_hz: f32 },
    MovingAverage { window: usize },
    Median { window: usize },
}

/// Configuration of the filter engine.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub sample_rate_hz: f32,
    pub kind: FilterKind,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 100.0,
            kind: FilterKind::LowPass { cutoff_hz: 5.0 },
        }
    }
}

#[derive(Debug, Clone)]
struct IirState {
    prev_input: f32,
    prev_output: f32,
}

impl Default for IirState {
    fn default() -> Self {
        Self {
            prev_input: 0.0,
            prev_output: 0.0,
        }
    }
}

/// Streaming filter engine that supports real-time processing.
#[derive(Debug, Clone)]
pub struct FilterEngine {
    config: FilterConfig,
    iir: IirState,
    mov_avg_window: VecDeque<f32>,
    mov_avg_sum: f32,
    median_window: VecDeque<f32>,
}

impl FilterEngine {
    pub fn new(config: FilterConfig) -> Result<Self> {
        if config.sample_rate_hz <= 0.0 {
            return Err(AosError::Validation("sample rate must be positive".into()));
        }

        Ok(Self {
            config,
            iir: IirState::default(),
            mov_avg_window: VecDeque::new(),
            mov_avg_sum: 0.0,
            median_window: VecDeque::new(),
        })
    }

    /// Reset the filter state.
    pub fn reset(&mut self) {
        self.iir = IirState::default();
        self.mov_avg_window.clear();
        self.mov_avg_sum = 0.0;
        self.median_window.clear();
    }

    /// Process a single sample.
    pub fn apply_sample(&mut self, sample: f32) -> f32 {
        match self.config.kind {
            FilterKind::LowPass { cutoff_hz } => self.apply_low_pass(sample, cutoff_hz),
            FilterKind::HighPass { cutoff_hz } => self.apply_high_pass(sample, cutoff_hz),
            FilterKind::BandPass {
                low_cut_hz,
                high_cut_hz,
            } => self.apply_band_pass(sample, low_cut_hz, high_cut_hz),
            FilterKind::MovingAverage { window } => self.apply_moving_average(sample, window),
            FilterKind::Median { window } => self.apply_median(sample, window),
        }
    }

    /// Process an entire signal and return the filtered result.
    pub fn process_signal(&mut self, signal: &[f32]) -> Vec<f32> {
        signal
            .iter()
            .map(|&value| self.apply_sample(value))
            .collect()
    }

    fn apply_low_pass(&mut self, sample: f32, cutoff_hz: f32) -> f32 {
        let dt = 1.0 / self.config.sample_rate_hz;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz.max(1e-6));
        let alpha = dt / (rc + dt);
        let output = self.iir.prev_output + alpha * (sample - self.iir.prev_output);
        self.iir.prev_input = sample;
        self.iir.prev_output = output;
        output
    }

    fn apply_high_pass(&mut self, sample: f32, cutoff_hz: f32) -> f32 {
        let dt = 1.0 / self.config.sample_rate_hz;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz.max(1e-6));
        let alpha = rc / (rc + dt);
        let output = alpha * (self.iir.prev_output + sample - self.iir.prev_input);
        self.iir.prev_input = sample;
        self.iir.prev_output = output;
        output
    }

    fn apply_band_pass(&mut self, sample: f32, low_cut_hz: f32, high_cut_hz: f32) -> f32 {
        // Sequential combination of high-pass then low-pass.
        let high_passed = self.apply_high_pass(sample, low_cut_hz);
        self.apply_low_pass(high_passed, high_cut_hz)
    }

    fn apply_moving_average(&mut self, sample: f32, window: usize) -> f32 {
        if window == 0 {
            return sample;
        }

        self.mov_avg_window.push_back(sample);
        self.mov_avg_sum += sample;
        if self.mov_avg_window.len() > window {
            if let Some(oldest) = self.mov_avg_window.pop_front() {
                self.mov_avg_sum -= oldest;
            }
        }

        self.mov_avg_sum / self.mov_avg_window.len() as f32
    }

    fn apply_median(&mut self, sample: f32, window: usize) -> f32 {
        if window == 0 {
            return sample;
        }

        self.median_window.push_back(sample);
        if self.median_window.len() > window {
            self.median_window.pop_front();
        }

        let mut values: Vec<f32> = self.median_window.iter().copied().collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        values[values.len() / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_low_pass_filter_smooths_signal() {
        let mut engine = FilterEngine::new(FilterConfig {
            sample_rate_hz: 100.0,
            kind: FilterKind::LowPass { cutoff_hz: 5.0 },
        })
        .unwrap();

        for _ in 0..10 {
            engine.apply_sample(0.0);
        }

        // Inject a spike and ensure it is dampened.
        let spike = engine.apply_sample(10.0);
        assert!(spike < 10.0);
    }

    #[test]
    fn test_moving_average_filter() {
        let mut engine = FilterEngine::new(FilterConfig {
            sample_rate_hz: 1.0,
            kind: FilterKind::MovingAverage { window: 4 },
        })
        .unwrap();

        let input = [1.0, 2.0, 3.0, 4.0, 5.0];
        let output = engine.process_signal(&input);
        assert_eq!(output.len(), input.len());
        assert!((output[3] - 2.5).abs() < 1e-6);
    }
}
