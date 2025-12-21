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
use super::AdapterWeights;

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

    /// Execute the fused MLP kernel with actual adapter weights
    ///
    /// # Arguments
    /// * `adapter_weights` - Slice of references to loaded adapter weights (GPU buffers)
    /// * `adapters` - Active adapters with IDs and Q15 gates (must match adapter_weights length)
    pub fn execute(
        &mut self,
        input: &Buffer,
        gate_weight: &Buffer,
        up_weight: &Buffer,
        down_weight: &Buffer,
        output: &Buffer,
        adapter_weights: &[&AdapterWeights],
        adapters: &[super::ring_buffer::ActiveAdapter],
    ) -> Result<()> {
        // Validate adapter_weights and adapters match
        if adapter_weights.len() != adapters.len() {
            return Err(AosError::Validation(format!(
                "Adapter count mismatch: {} weights but {} adapters",
                adapter_weights.len(),
                adapters.len()
            )));
        }

        // Update ring buffer with active adapters
        self.ring_buffer.update(adapters)?;

        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set base model weight buffers
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(gate_weight), 0);
        encoder.set_buffer(2, Some(up_weight), 0);
        encoder.set_buffer(3, Some(down_weight), 0);
        // Note: Metal shader expects bias buffers at 4-6, but we skip them (nullable)
        encoder.set_buffer(7, Some(output), 0);

        // Pass actual LoRA weight buffers to Metal shader for ALL K adapters
        // Metal shader (aos_kernels.metal) expects buffers 8-13 for LoRA weights:
        //   Buffer 8: gate_lora_a, Buffer 9: gate_lora_b
        //   Buffer 10: up_lora_a, Buffer 11: up_lora_b
        //   Buffer 12: down_lora_a, Buffer 13: down_lora_b
        //
        // For MLP projections:
        //   - gate_proj uses lora_a_buffers[3] and lora_b_buffers[3] (actually mlp_down in our layout)
        //   - up_proj uses lora_a_buffers[4] and lora_b_buffers[4] (actually mlp_up in our layout)
        //   - down_proj: Uses index 3
        //
        // Multi-adapter routing: Iterate over ALL adapters and apply gate-weighted contributions.
        // The RingBuffer contains Q15 gates for each adapter. The final output is:
        //   output = W_base @ x + Σᵢ (gateᵢ / 32767) * (alpha / rank) * (Bᵢ @ (Aᵢ @ x))
        //
        // Buffer layout per adapter (K adapters, interleaved):
        //   Buffers 8-13 contain concatenated weights for gate/up/down projections
        //   Buffer 17+ contain additional adapter weights (K-1 adapters)

        if !adapter_weights.is_empty() {
            // Iterate over ALL adapters in the router ring, applying gate-weighted LoRA
            for (adapter_idx, (adapter, active)) in
                adapter_weights.iter().zip(adapters.iter()).enumerate()
            {
                // Calculate buffer offset for this adapter
                // First adapter uses buffers 8-13, subsequent adapters use 17+
                let base_buffer_idx = if adapter_idx == 0 {
                    8
                } else {
                    17 + (adapter_idx - 1) * 6
                };

                // Log adapter activation for debugging
                tracing::trace!(
                    adapter_id = active.id,
                    gate_q15 = active.gate,
                    gate_f32 = RingBuffer::q15_to_float(active.gate),
                    buffer_offset = base_buffer_idx,
                    "Binding adapter weights for multi-adapter routing"
                );

                // MLP has 3 projections: gate, up, down
                // Our buffer layout: [q_proj_A(0), k_proj_A(1), v_proj_A(2), mlp_down_A(3), mlp_up_A(4)]

                // Pass gate projection LoRA weights (using index 3 = mlp_down)
                if adapter.lora_a_buffers.len() > 3 && adapter.lora_b_buffers.len() > 3 {
                    encoder.set_buffer(base_buffer_idx as u64, Some(&adapter.lora_a_buffers[3]), 0);
                    encoder.set_buffer(
                        (base_buffer_idx + 1) as u64,
                        Some(&adapter.lora_b_buffers[3]),
                        0,
                    );
                }

                // Pass up projection LoRA weights (using index 4 = mlp_up)
                if adapter.lora_a_buffers.len() > 4 && adapter.lora_b_buffers.len() > 4 {
                    encoder.set_buffer(
                        (base_buffer_idx + 2) as u64,
                        Some(&adapter.lora_a_buffers[4]),
                        0,
                    );
                    encoder.set_buffer(
                        (base_buffer_idx + 3) as u64,
                        Some(&adapter.lora_b_buffers[4]),
                        0,
                    );
                }

                // Pass down projection LoRA weights (using index 3)
                if adapter.lora_a_buffers.len() > 3 && adapter.lora_b_buffers.len() > 3 {
                    encoder.set_buffer(
                        (base_buffer_idx + 4) as u64,
                        Some(&adapter.lora_a_buffers[3]),
                        0,
                    );
                    encoder.set_buffer(
                        (base_buffer_idx + 5) as u64,
                        Some(&adapter.lora_b_buffers[3]),
                        0,
                    );
                }
            }
        }

        // Note: Adapter count is already available in the RingBuffer (top_k field)
        // which is passed to the shader at buffer 15. The shader iterates using
        // ring.top_k and ring.adapter_indices to access per-adapter weights.

        // Set LoRA configuration
        let lora_config = if adapter_weights.is_empty() {
            LoraConfig::default()
        } else {
            LoraConfig {
                rank: adapter_weights[0].rank as u32,
                alpha: adapter_weights[0].alpha,
                target_module: 0,
                dropout_rate: 0.0,
            }
        };

        let lora_config_bytes =
            serde_json::to_vec(&lora_config).map_err(AosError::Serialization)?;
        let lora_config_buffer = self.device.new_buffer_with_data(
            lora_config_bytes.as_ptr() as *const std::ffi::c_void,
            lora_config_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(14, Some(&lora_config_buffer), 0); // Buffer 14 for lora_config

        // Set ring buffer
        encoder.set_buffer(15, self.ring_buffer.get_buffer().map(|v| &**v), 0);

        // Set dropout seed
        let dropout_seed: u32 = 0; // Deterministic seed (should come from HKDF)
        let dropout_seed_buffer = self.device.new_buffer_with_data(
            &dropout_seed as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(16, Some(&dropout_seed_buffer), 0);

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
