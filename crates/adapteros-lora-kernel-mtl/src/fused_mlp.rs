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

use super::ring_buffer::RawRingBuffer;

/// LoRA configuration
#[repr(C)]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
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

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MetalMlpParams {
    input: u64,
    output: u64,
    gate_weight: u64,
    up_weight: u64,
    down_weight: u64,
    gate_bias: u64,
    up_bias: u64,
    down_bias: u64,
    gate_lora_a: u64,
    gate_lora_b: u64,
    up_lora_a: u64,
    up_lora_b: u64,
    down_lora_a: u64,
    down_lora_b: u64,
    lora_config: LoraConfig,
    ring_buffer: RawRingBuffer,
    dropout_seed: u32,
    hidden_size: u32,
    intermediate_size: u32,
    batch_size: u32,
    max_adapters: u32,
    _padding: u32,
}

/// Fused MLP kernel
pub struct FusedMlpKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
}

impl FusedMlpKernel {
    /// Create a new fused MLP kernel
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load library and create pipeline
        let library = device
            .new_library_with_data(include_bytes!("../shaders/adapteros_kernels.metallib"))
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
        })
    }

    /// Execute the fused MLP kernel
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        input: &Buffer,
        gate_weight: &Buffer,
        up_weight: &Buffer,
        down_weight: &Buffer,
        output: &Buffer,
        lora_config: &LoraConfig,
        gate_lora_a: &Buffer,
        gate_lora_b: &Buffer,
        up_lora_a: &Buffer,
        up_lora_b: &Buffer,
        down_lora_a: &Buffer,
        down_lora_b: &Buffer,
        ring_state: RawRingBuffer,
        max_adapters: u32,
        batch_size: u32,
        hidden_size: u32,
        intermediate_size: u32,
        dropout_seed: u32,
    ) -> Result<()> {
        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        let params = MetalMlpParams {
            input: input.gpu_address(),
            output: output.gpu_address(),
            gate_weight: gate_weight.gpu_address(),
            up_weight: up_weight.gpu_address(),
            down_weight: down_weight.gpu_address(),
            gate_bias: 0,
            up_bias: 0,
            down_bias: 0,
            gate_lora_a: gate_lora_a.gpu_address(),
            gate_lora_b: gate_lora_b.gpu_address(),
            up_lora_a: up_lora_a.gpu_address(),
            up_lora_b: up_lora_b.gpu_address(),
            down_lora_a: down_lora_a.gpu_address(),
            down_lora_b: down_lora_b.gpu_address(),
            lora_config: *lora_config,
            ring_buffer: ring_state,
            dropout_seed,
            hidden_size,
            intermediate_size,
            batch_size,
            max_adapters,
            _padding: 0,
        };

        let params_buffer = self.device.new_buffer_with_data(
            &params as *const MetalMlpParams as *const std::ffi::c_void,
            std::mem::size_of::<MetalMlpParams>() as u64,
            MTLResourceOptions::StorageModeShared,
        );

        encoder.set_buffer(0, Some(&params_buffer), 0);

        // Threads model output grid: (batch_size, hidden_size)
        // Use 256 threads per group in X; compute required groups.
        let threads_per_group = MTLSize::new(256, 1, 1);
        let groups_x = ((batch_size as u64) + threads_per_group.width - 1) / threads_per_group.width;
        let grid_groups = MTLSize::new(groups_x, hidden_size as u64, 1);

        encoder.dispatch_thread_groups(grid_groups, threads_per_group);
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
