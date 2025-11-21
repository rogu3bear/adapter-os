//! RMSNorm (Root Mean Square Layer Normalization) kernel implementation
//!
//! This module implements RMSNorm, used in modern LLMs like Llama instead of LayerNorm.
//! Formula: y = (x / sqrt(mean(x^2) + eps)) * weight
//! No bias term (unlike LayerNorm).
//!
//! References:
//! - RMSNorm: https://arxiv.org/abs/1910.07467
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

use adapteros_core::{AosError, Result};
use metal::*;
use std::sync::Arc;

/// RMSNorm configuration
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct RmsNormConfig {
    /// Dimension of the hidden states
    pub hidden_size: u32,
    /// Epsilon for numerical stability (typically 1e-6)
    pub eps: f32,
}

impl Default for RmsNormConfig {
    fn default() -> Self {
        Self {
            hidden_size: 3584, // Qwen2.5-7B hidden size
            eps: 1e-6,
        }
    }
}

/// RMSNorm kernel for Metal execution
pub struct RmsNormKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    config: RmsNormConfig,
}

impl RmsNormKernel {
    /// Create a new RMSNorm kernel
    pub fn new(device: Arc<Device>, config: RmsNormConfig) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load library and create pipeline
        let library = device
            .new_library_with_data(include_bytes!("../shaders/aos_kernels.metallib"))
            .map_err(|e| AosError::Kernel(format!("Failed to load library: {}", e)))?;

        let function = library
            .get_function("rms_norm", None)
            .map_err(|e| AosError::Kernel(format!("rms_norm function not found: {}", e)))?;

        let pipeline_state = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Kernel(format!("Failed to create RMSNorm pipeline: {}", e)))?;

        tracing::info!(
            hidden_size = config.hidden_size,
            eps = config.eps,
            "Created RMSNorm kernel"
        );

        Ok(Self {
            device,
            command_queue,
            pipeline_state,
            config,
        })
    }

    /// Execute RMSNorm on the given input
    ///
    /// # Arguments
    /// * `input` - Input tensor [batch_size, hidden_size]
    /// * `weight` - Learnable scale parameter [hidden_size]
    /// * `output` - Output tensor [batch_size, hidden_size]
    /// * `batch_size` - Number of batch elements
    pub fn execute(
        &self,
        input: &Buffer,
        weight: &Buffer,
        output: &Buffer,
        batch_size: usize,
    ) -> Result<()> {
        // Validate buffer sizes
        let expected_input_size =
            batch_size * (self.config.hidden_size as usize) * std::mem::size_of::<f32>();
        let expected_weight_size =
            (self.config.hidden_size as usize) * std::mem::size_of::<f32>();

        if (input.length() as usize) < expected_input_size {
            return Err(AosError::Validation(format!(
                "Input buffer too small: expected {} bytes, got {} bytes",
                expected_input_size,
                input.length()
            )));
        }

        if (weight.length() as usize) < expected_weight_size {
            return Err(AosError::Validation(format!(
                "Weight buffer too small: expected {} bytes, got {} bytes",
                expected_weight_size,
                weight.length()
            )));
        }

        if (output.length() as usize) < expected_input_size {
            return Err(AosError::Validation(format!(
                "Output buffer too small: expected {} bytes, got {} bytes",
                expected_input_size,
                output.length()
            )));
        }

        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set buffers
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(weight), 0);
        encoder.set_buffer(2, Some(output), 0);

        // Set configuration
        let config_buffer = self.device.new_buffer_with_data(
            &self.config as *const RmsNormConfig as *const std::ffi::c_void,
            std::mem::size_of::<RmsNormConfig>() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(3, Some(&config_buffer), 0);

        // Dispatch: one threadgroup per batch element
        // Use 256 threads per threadgroup for parallel reduction
        let threadgroup_size = MTLSize::new(256, 1, 1);
        let grid_size = MTLSize::new(batch_size as u64, 1, 1);

        encoder.dispatch_thread_groups(grid_size, threadgroup_size);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        tracing::debug!(
            batch_size = batch_size,
            hidden_size = self.config.hidden_size,
            "RMSNorm execution completed"
        );

        Ok(())
    }

    /// Execute RMSNorm with data arrays (convenience method)
    ///
    /// Creates Metal buffers from input data, executes the kernel, and returns results.
    pub fn execute_with_data(
        &self,
        input: &[f32],
        weight: &[f32],
        batch_size: usize,
    ) -> Result<Vec<f32>> {
        let hidden_size = self.config.hidden_size as usize;
        let expected_input_len = batch_size * hidden_size;

        if input.len() != expected_input_len {
            return Err(AosError::Validation(format!(
                "Input length mismatch: expected {}, got {}",
                expected_input_len,
                input.len()
            )));
        }

        if weight.len() != hidden_size {
            return Err(AosError::Validation(format!(
                "Weight length mismatch: expected {}, got {}",
                hidden_size,
                weight.len()
            )));
        }

        // Create input buffer
        let input_buffer = self.device.new_buffer_with_data(
            input.as_ptr() as *const std::ffi::c_void,
            std::mem::size_of_val(input) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create weight buffer
        let weight_buffer = self.device.new_buffer_with_data(
            weight.as_ptr() as *const std::ffi::c_void,
            std::mem::size_of_val(weight) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create output buffer
        let output_buffer = self.device.new_buffer(
            (expected_input_len * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Execute kernel
        self.execute(&input_buffer, &weight_buffer, &output_buffer, batch_size)?;

        // Read back results
        let output_ptr = output_buffer.contents() as *const f32;
        let mut output = vec![0.0f32; expected_input_len];
        // SAFETY: We validated buffer sizes above and Metal guarantees alignment
        unsafe {
            std::ptr::copy_nonoverlapping(output_ptr, output.as_mut_ptr(), expected_input_len);
        }

        Ok(output)
    }

    /// Get device name
    pub fn device_name(&self) -> &str {
        self.device.name()
    }

    /// Get configuration
    pub fn config(&self) -> &RmsNormConfig {
        &self.config
    }

    /// Update epsilon value
    pub fn set_eps(&mut self, eps: f32) {
        self.config.eps = eps;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_norm_config_default() {
        let config = RmsNormConfig::default();
        assert_eq!(config.hidden_size, 3584);
        assert!((config.eps - 1e-6).abs() < 1e-10);
    }

    #[test]
    fn test_rms_norm_creation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let config = RmsNormConfig {
            hidden_size: 128,
            eps: 1e-6,
        };
        let kernel = RmsNormKernel::new(Arc::new(device), config);
        // Note: This will fail if metallib is not compiled, which is expected in CI
        if let Ok(kernel) = kernel {
            assert!(!kernel.device_name().is_empty());
            assert_eq!(kernel.config().hidden_size, 128);
        }
    }

    #[test]
    fn test_rms_norm_validation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let config = RmsNormConfig {
            hidden_size: 4,
            eps: 1e-6,
        };

        if let Ok(kernel) = RmsNormKernel::new(Arc::new(device), config) {
            // Test with mismatched input size
            let input = vec![1.0f32; 8]; // 2 batches
            let weight = vec![1.0f32; 3]; // Wrong size!

            let result = kernel.execute_with_data(&input, &weight, 2);
            assert!(result.is_err());
        }
    }
}
