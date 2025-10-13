//! Mock MLX backend for testing without Python runtime
//!
//! This module provides a mock implementation of the MLX backend that can be used
//! in tests without requiring Python or MLX to be installed.

use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use rand::Rng;

/// Mock MLX backend that returns random logits
pub struct MockMLXBackend {
    vocab_size: usize,
    device_name: String,
}

impl MockMLXBackend {
    /// Create a new mock backend
    ///
    /// # Arguments
    /// * `vocab_size` - Vocabulary size for logits generation
    ///
    /// # Returns
    /// Mock backend instance
    pub fn new(vocab_size: usize) -> Self {
        Self {
            vocab_size,
            device_name: "Mock MLX (Test)".to_string(),
        }
    }
}

impl FusedKernels for MockMLXBackend {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Mock implementation - no-op
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Generate random logits for testing
        let mut rng = rand::thread_rng();

        for logit in &mut io.output_logits {
            *logit = rng.gen_range(-10.0..10.0);
        }

        io.position += 1;
        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_creation() {
        let backend = MockMLXBackend::new(1000);
        assert_eq!(backend.device_name(), "Mock MLX (Test)");
    }

    #[test]
    fn test_mock_backend_run_step() {
        let mut backend = MockMLXBackend::new(1000);
        let mut io = IoBuffers::new(1000);
        io.input_ids = vec![1, 2, 3];

        let ring = RouterRing::new(3);
        backend
            .run_step(&ring, &mut io)
            .expect("Test step should succeed");

        assert_eq!(io.position, 1);
        assert_eq!(io.output_logits.len(), 1000);

        // Check that logits are not all zero (random values)
        let non_zero_count = io.output_logits.iter().filter(|&&x| x != 0.0).count();
        assert!(non_zero_count > 0, "Expected some non-zero logits");
    }
}
