//! Common Test Utilities and Mocks
//!
//! Shared utilities for CoreML backend tests:
//! - Mock backends
//! - Test data generators
//! - Assertion helpers
//! - CI/CD compatibility utilities
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use adapteros_lora_kernel_api::{
    attestation::{BackendType, DeterminismReport, FloatingPointMode, RngSeedingMethod},
    BackendHealth, BackendMetrics, FusedKernels, IoBuffers, RouterRing,
};
use adapteros_core::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Mock CoreML backend for testing without real hardware
pub struct MockCoreMLBackend {
    device_name: String,
    ane_available: AtomicBool,
    step_count: AtomicU64,
    error_rate: f32,
    metrics: Arc<Mutex<MockMetrics>>,
}

#[derive(Debug, Clone, Default)]
struct MockMetrics {
    total_latency_us: u64,
    peak_memory: u64,
    current_memory: u64,
    error_count: u64,
}

impl MockCoreMLBackend {
    /// Create new mock backend
    pub fn new(ane_available: bool) -> Self {
        let device_name = if ane_available {
            "Mock CoreML (ANE)".to_string()
        } else {
            "Mock CoreML (GPU Fallback)".to_string()
        };

        Self {
            device_name,
            ane_available: AtomicBool::new(ane_available),
            step_count: AtomicU64::new(0),
            error_rate: 0.0,
            metrics: Arc::new(Mutex::new(MockMetrics::default())),
        }
    }

    /// Create mock backend with error injection
    pub fn with_error_rate(ane_available: bool, error_rate: f32) -> Self {
        let mut backend = Self::new(ane_available);
        backend.error_rate = error_rate.clamp(0.0, 1.0);
        backend
    }

    /// Toggle ANE availability at runtime
    pub fn toggle_ane(&self) {
        let current = self.ane_available.load(Ordering::Relaxed);
        self.ane_available.store(!current, Ordering::Relaxed);
    }

    /// Get step count
    pub fn steps_executed(&self) -> u64 {
        self.step_count.load(Ordering::Relaxed)
    }

    /// Check if should inject error
    fn should_inject_error(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f32>() < self.error_rate
    }
}

impl FusedKernels for MockCoreMLBackend {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        if self.should_inject_error() {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.error_count += 1;
            return Err(adapteros_core::AosError::Kernel("Mock error".into()));
        }

        // Generate deterministic output
        for (i, logit) in io.output_logits.iter_mut().enumerate() {
            *logit = (i as f32 * 0.001) % 1.0;
        }

        io.position += 1;
        self.step_count.fetch_add(1, Ordering::Relaxed);

        // Update metrics
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_latency_us += 1000; // 1ms per step
        metrics.current_memory = 256 * 1024 * 1024; // 256MB

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        let ane_available = self.ane_available.load(Ordering::Relaxed);

        Ok(DeterminismReport {
            backend_type: BackendType::CoreML,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: if ane_available {
                RngSeedingMethod::HkdfSeeded
            } else {
                RngSeedingMethod::SystemEntropy
            },
            floating_point_mode: if ane_available {
                FloatingPointMode::Deterministic
            } else {
                FloatingPointMode::Unknown
            },
            compiler_flags: vec!["-fno-fast-math".to_string()],
            deterministic: ane_available,
        })
    }

    fn health_check(&self) -> Result<BackendHealth> {
        let metrics = self.metrics.lock().unwrap();
        let total_ops = self.step_count.load(Ordering::Relaxed);

        if total_ops > 0 && metrics.error_count > (total_ops / 10) {
            Ok(BackendHealth::Degraded {
                reason: format!(
                    "High error rate: {} / {}",
                    metrics.error_count, total_ops
                ),
            })
        } else {
            Ok(BackendHealth::Healthy)
        }
    }

    fn get_metrics(&self) -> BackendMetrics {
        let metrics = self.metrics.lock().unwrap();
        let total_ops = self.step_count.load(Ordering::Relaxed);

        let avg_latency_us = if total_ops > 0 {
            (metrics.total_latency_us as f32) / (total_ops as f32)
        } else {
            0.0
        };

        BackendMetrics {
            total_operations: total_ops,
            avg_latency_us,
            peak_memory_bytes: metrics.peak_memory,
            current_memory_bytes: metrics.current_memory,
            utilization_percent: 75.0,
            error_count: metrics.error_count,
            custom_metrics: HashMap::new(),
        }
    }
}

/// Test data generators
pub mod generators {
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha20Rng;

    /// Generate deterministic test input IDs
    pub fn generate_input_ids(seq_len: usize, seed: u64) -> Vec<u32> {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        (0..seq_len).map(|_| rng.gen_range(0..32000)).collect()
    }

    /// Generate deterministic adapter gates (Q15)
    pub fn generate_adapter_gates(k: usize, seed: u64) -> Vec<i16> {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        (0..k)
            .map(|i| {
                let base = 32767 / (i + 1);
                let noise = rng.gen_range(-100..100);
                (base + noise).max(0).min(32767) as i16
            })
            .collect()
    }

    /// Generate test adapter indices
    pub fn generate_adapter_indices(k: usize) -> Vec<u16> {
        (0..k).map(|i| i as u16).collect()
    }

    /// Generate synthetic logits
    pub fn generate_logits(vocab_size: usize, seed: u64) -> Vec<f32> {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        (0..vocab_size).map(|_| rng.gen_range(-10.0..10.0)).collect()
    }
}

/// Assertion helpers
pub mod assertions {
    /// Assert vectors are approximately equal
    pub fn assert_approx_eq(a: &[f32], b: &[f32], tolerance: f32, msg: &str) {
        assert_eq!(a.len(), b.len(), "{}: length mismatch", msg);

        for (i, (&x, &y)) in a.iter().zip(b.iter()).enumerate() {
            let diff = (x - y).abs();
            assert!(
                diff < tolerance,
                "{}: element {} differs by {} (tolerance: {})",
                msg,
                i,
                diff,
                tolerance
            );
        }
    }

    /// Assert value is within range
    pub fn assert_in_range(value: f32, min: f32, max: f32, msg: &str) {
        assert!(
            value >= min && value <= max,
            "{}: {} not in range [{}, {}]",
            msg,
            value,
            min,
            max
        );
    }

    /// Assert all values are finite
    pub fn assert_all_finite(values: &[f32], msg: &str) {
        for (i, &val) in values.iter().enumerate() {
            assert!(val.is_finite(), "{}: non-finite at index {}: {}", msg, i, val);
        }
    }

    /// Assert probabilities sum to 1.0
    pub fn assert_probabilities(probs: &[f32], msg: &str) {
        assert_all_finite(probs, msg);

        for (i, &p) in probs.iter().enumerate() {
            assert!(
                p >= 0.0 && p <= 1.0,
                "{}: invalid probability at {}: {}",
                msg,
                i,
                p
            );
        }

        let sum: f32 = probs.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "{}: probabilities don't sum to 1.0: {}",
            msg,
            sum
        );
    }
}

/// CI/CD compatibility utilities
pub mod ci {
    use std::env;

    /// Check if running in CI environment
    pub fn is_ci() -> bool {
        env::var("CI").is_ok() || env::var("GITHUB_ACTIONS").is_ok()
    }

    /// Skip test if running in CI without real hardware
    pub fn skip_if_no_hardware(test_name: &str) {
        if is_ci() && !has_real_hardware() {
            println!(
                "Skipping {} in CI (no real hardware available)",
                test_name
            );
        }
    }

    /// Check if real CoreML hardware is available
    pub fn has_real_hardware() -> bool {
        #[cfg(target_os = "macos")]
        {
            // Try to detect ANE
            std::process::Command::new("sysctl")
                .arg("-n")
                .arg("machdep.cpu.brand_string")
                .output()
                .ok()
                .and_then(|output| {
                    let brand = String::from_utf8_lossy(&output.stdout);
                    Some(brand.contains("Apple M"))
                })
                .unwrap_or(false)
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Get CI environment name
    pub fn ci_environment() -> Option<String> {
        if env::var("GITHUB_ACTIONS").is_ok() {
            Some("GitHub Actions".to_string())
        } else if env::var("CI").is_ok() {
            Some("Generic CI".to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_creation() {
        let backend = MockCoreMLBackend::new(true);
        assert!(backend.device_name.contains("ANE"));

        let backend = MockCoreMLBackend::new(false);
        assert!(backend.device_name.contains("GPU"));
    }

    #[test]
    fn test_mock_backend_toggle_ane() {
        let backend = MockCoreMLBackend::new(true);
        assert!(backend.ane_available.load(Ordering::Relaxed));

        backend.toggle_ane();
        assert!(!backend.ane_available.load(Ordering::Relaxed));

        backend.toggle_ane();
        assert!(backend.ane_available.load(Ordering::Relaxed));
    }

    #[test]
    fn test_generator_determinism() {
        let ids1 = generators::generate_input_ids(100, 42);
        let ids2 = generators::generate_input_ids(100, 42);
        assert_eq!(ids1, ids2);

        let ids3 = generators::generate_input_ids(100, 43);
        assert_ne!(ids1, ids3);
    }

    #[test]
    fn test_assertion_helpers() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0001, 1.9999, 3.0001];

        assertions::assert_approx_eq(&a, &b, 0.001, "vectors should be approximately equal");
    }

    #[test]
    fn test_ci_detection() {
        // Should not crash
        let is_ci = ci::is_ci();
        let has_hw = ci::has_real_hardware();

        println!("CI: {}, Hardware: {}", is_ci, has_hw);
    }
}
