//! Fused MLP kernel implementation
//!
//! This module implements the fused MLP kernel with SwiGLU activation
//! and LoRA adapter support for efficient Metal execution.
//!
//! References:
//! - SwiGLU: https://arxiv.org/abs/2002.05202
//! - LoRA: https://arxiv.org/abs/2106.09685
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

use adapteros_core::{AosError, Result};
use metal::*;
use std::sync::Arc;

use super::ring_buffer::RingBuffer;

/// LoRA configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoraConfig {
    /// LoRA rank
    pub rank: u32,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Target module identifier
    pub target_module: u32,
    /// Dropout rate for LoRA layers
    pub dropout_rate: f32,
}

impl Default for LoraConfig {
    fn default() -> Self {
        Self {
            rank: 16,
            alpha: 32.0,
            target_module: 0,
            dropout_rate: 0.0,
        }
    }
}

/// Fused MLP kernel
pub struct FusedMlpKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    ring_buffer: RingBuffer,
}

impl FusedMlpKernel {
    /// Create a new fused MLP kernel
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let command_queue = device.new_command_queue();
        let ring_buffer = RingBuffer::new(device.clone(), 3)?; // K=3

        // Load library and create pipeline
        let library = device
            .new_library_with_data(include_bytes!("../shaders/mplora_kernels.metallib"))
            .map_err(|e| AosError::Kernel(format!("Failed to load library: {}", e)))?;

        let function = library
            .get_function("fused_mlp", None)
            .map_err(|e| AosError::Kernel(format!("Function not found: {}", e)))?;

        let pipeline_state = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Kernel(format!("Failed to create pipeline: {}", e)))?;

        Ok(Self {
            device,
            command_queue,
            pipeline_state,
            ring_buffer,
        })
    }

    /// Execute the fused MLP kernel
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &mut self,
        input: &Buffer,
        gate_weight: &Buffer,
        up_weight: &Buffer,
        down_weight: &Buffer,
        output: &Buffer,
        lora_config: &LoraConfig,
        adapters: &[super::ring_buffer::ActiveAdapter],
    ) -> Result<()> {
        // Update ring buffer with active adapters
        self.ring_buffer.update(adapters)?;

        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set buffers
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(gate_weight), 0);
        encoder.set_buffer(2, Some(up_weight), 0);
        encoder.set_buffer(3, Some(down_weight), 0);
        encoder.set_buffer(4, Some(output), 0);
        encoder.set_buffer(5, self.ring_buffer.get_buffer().map(|v| &**v), 0);

        // Set LoRA configuration
        let lora_config_bytes = serde_json::to_vec(lora_config).map_err(AosError::Serialization)?;
        let lora_config_buffer = self.device.new_buffer_with_data(
            lora_config_bytes.as_ptr() as *const std::ffi::c_void,
            lora_config_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(6, Some(&lora_config_buffer), 0);

        // Calculate threadgroup size
        let threadgroup_size = MTLSize::new(16, 16, 1);
        let grid_size = MTLSize::new(
            input.length() / 4, // FP16 = 2 bytes, 4 elements per thread
            gate_weight.length() / 4,
            1,
        );

        encoder.dispatch_thread_groups(grid_size, threadgroup_size);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        Ok(())
    }

    /// Get device name
    pub fn device_name(&self) -> &str {
        self.device.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fused_mlp_creation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let kernel =
            FusedMlpKernel::new(Arc::new(device)).expect("FusedMlpKernel creation should succeed");
        assert!(!kernel.device_name().is_empty());
    }

    #[test]
    fn test_lora_config() {
        let config = LoraConfig {
            rank: 16,
            alpha: 32.0,
            target_module: 1,
            dropout_rate: 0.0,
        };
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.dropout_rate, 0.0);
    }
}
