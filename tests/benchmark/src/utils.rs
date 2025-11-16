<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Benchmark utilities and helper functions

use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use sysinfo::{System, SystemExt};

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub allocated_bytes: usize,
    pub peak_allocated_bytes: usize,
    pub allocation_count: usize,
    pub deallocation_count: usize,
}

/// GPU memory statistics (Metal-specific)
#[cfg(feature = "metal")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMemoryStats {
    pub device_memory_used: usize,
    pub device_memory_total: usize,
    pub buffer_count: usize,
    pub texture_count: usize,
}

/// System resource monitor
pub struct ResourceMonitor {
    system: System,
    start_time: Instant,
    initial_memory: u64,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            system,
            start_time: Instant::now(),
            initial_memory: system.used_memory(),
        }
    }

    pub fn get_memory_usage_mb(&mut self) -> f64 {
        self.system.refresh_memory();
        (self.system.used_memory() - self.initial_memory) as f64 / 1024.0 / 1024.0
    }

    pub fn get_cpu_usage(&mut self) -> f32 {
        self.system.refresh_cpu();
        self.system.global_cpu_info().cpu_usage()
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

/// Deterministic random number generator for reproducible benchmarks
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.state >> 16) as u32
    }

    pub fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    pub fn fill_bytes(&mut self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_mut(4) {
            let val = self.next_u32().to_le_bytes();
            chunk.copy_from_slice(&val[..chunk.len()]);
        }
    }
}

/// Benchmark data generator
pub struct DataGenerator {
    rng: DeterministicRng,
}

impl DataGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: DeterministicRng::new(seed),
        }
    }

    /// Generate random matrix data
    pub fn generate_matrix(&mut self, rows: usize, cols: usize) -> Vec<f32> {
        let mut data = vec![0.0f32; rows * cols];
        for val in &mut data {
            *val = self.rng.next_f32() * 2.0 - 1.0; // Range [-1, 1]
        }
        data
    }

    /// Generate sequential data for deterministic testing
    pub fn generate_sequential(&mut self, size: usize) -> Vec<f32> {
        (0..size).map(|i| i as f32).collect()
    }

    /// Generate sparse data (mostly zeros with some non-zero values)
    pub fn generate_sparse(&mut self, size: usize, density: f32) -> Vec<f32> {
        let mut data = vec![0.0f32; size];
        for val in &mut data {
            if self.rng.next_f32() < density {
                *val = self.rng.next_f32() * 2.0 - 1.0;
            }
        }
        data
    }
}

/// Performance measurement utilities
pub struct PerformanceTimer {
    start: Instant,
    checkpoints: HashMap<String, Duration>,
}

impl PerformanceTimer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            checkpoints: HashMap::new(),
        }
    }

    pub fn checkpoint(&mut self, name: &str) {
        self.checkpoints.insert(name.to_string(), self.start.elapsed());
    }

    pub fn get_checkpoint(&self, name: &str) -> Option<Duration> {
        self.checkpoints.get(name).copied()
    }

    pub fn total_elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn reset(&mut self) {
        self.start = Instant::now();
        self.checkpoints.clear();
    }
}

/// Statistical utilities for benchmark analysis
pub mod stats {
    use std::cmp::Ordering;

    /// Calculate mean of a slice of values
    pub fn mean(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Calculate standard deviation of a slice of values
    pub fn std_dev(values: &[f64]) -> f64 {
        if values.len() <= 1 {
            return 0.0;
        }
        let m = mean(values);
        let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
        variance.sqrt()
    }

    /// Calculate median of a slice of values
    pub fn median(values: &mut [f64]) -> f64 {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let len = values.len();
        if len % 2 == 0 {
            (values[len / 2 - 1] + values[len / 2]) / 2.0
        } else {
            values[len / 2]
        }
    }

    /// Calculate percentiles
    pub fn percentile(values: &mut [f64], p: f64) -> f64 {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let len = values.len();
        if len == 0 {
            return 0.0;
        }
        let index = (p / 100.0 * (len - 1) as f64) as usize;
        values[index]
    }

    /// Calculate coefficient of variation
    pub fn coefficient_of_variation(values: &[f64]) -> f64 {
        let m = mean(values);
        if m == 0.0 {
            return 0.0;
        }
        std_dev(values) / m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_rng() {
        let mut rng1 = DeterministicRng::new(42);
        let mut rng2 = DeterministicRng::new(42);

        for _ in 0..100 {
            assert_eq!(rng1.next_u32(), rng2.next_u32());
        }
    }

    #[test]
    fn test_data_generator() {
        let mut gen = DataGenerator::new(123);
        let matrix = gen.generate_matrix(10, 10);
        assert_eq!(matrix.len(), 100);
        assert!(matrix.iter().all(|&x| x >= -1.0 && x <= 1.0));
    }

    #[test]
    fn test_stats() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(stats::mean(&values), 3.0);
        assert!((stats::std_dev(&values) - 1.58113883).abs() < 1e-6);
        assert_eq!(stats::median(&mut values.clone()), 3.0);
    }
}