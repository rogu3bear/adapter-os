//! Fused QKV kernel implementation with GQA support
//!
//! This module implements the fused QKV kernel with Grouped Query Attention
//! and LoRA adapter support for efficient Metal execution.
//!
//! References:
//! - GQA: https://arxiv.org/abs/2305.13245
//! - Flash Attention: https://arxiv.org/abs/2205.14135
//! - LoRA: https://arxiv.org/abs/2106.09685
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

use adapteros_core::{AosError, Result};
use metal::*;
use std::sync::Arc;

use super::ring_buffer::RingBuffer;
use super::AdapterWeights;

/// GQA configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GqaConfig {
    /// Number of attention heads
    pub num_attention_heads: u32,
    /// Number of key-value heads
    pub num_key_value_heads: u32,
    /// Head dimension
    pub head_dim: u32,
    /// Key-value width
    pub kv_width: u32,
    /// Hidden size
    pub hidden_size: u32,
    /// RoPE base frequency (10000.0 for Qwen2.5-7B)
    pub rope_theta: f32,
    /// Attention scaling factor (0.0 = use sqrt(head_dim) default)
    pub attention_scale: f32,
    /// Dropout rate for attention (0.0 = no dropout)
    pub dropout_rate: f32,
}

impl Default for GqaConfig {
    fn default() -> Self {
        Self {
            num_attention_heads: 32,
            num_key_value_heads: 4,
            head_dim: 128,
            kv_width: 512,
            hidden_size: 4096,
            rope_theta: 10000.0,  // Qwen default
            attention_scale: 0.0, // Use sqrt scaling
            dropout_rate: 0.0,    // No dropout for inference
        }
    }
}

/// LoRA configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoraConfig {
    /// LoRA rank
    pub rank: u32,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Target module identifier
    pub target_module: u32,
    /// Dropout rate (0.0 = no dropout)
    pub dropout_rate: f32,
}

impl Default for LoraConfig {
    fn default() -> Self {
        Self {
            rank: 16,
            alpha: 32.0,
            target_module: 0,
            dropout_rate: 0.0, // No dropout for inference
        }
    }
}

/// Fused QKV kernel with GQA support
pub struct FusedQkvKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    gqa_config: GqaConfig,
}

impl FusedQkvKernel {
    /// Create a new fused QKV kernel
    pub fn new(device: Arc<Device>, gqa_config: GqaConfig) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load library and create pipeline
        let library = device
            .new_library_with_data(include_bytes!("../shaders/mplora_kernels.metallib"))
            .map_err(|e| AosError::Kernel(format!("Failed to load library: {}", e)))?;

        let function = library
            .get_function("fused_qkv_gqa", None)
            .map_err(|e| AosError::Kernel(format!("Function not found: {}", e)))?;

        let pipeline_state = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Kernel(format!("Failed to create pipeline: {}", e)))?;

        Ok(Self {
            device,
            command_queue,
            pipeline_state,
            gqa_config,
        })
    }

    /// Execute the fused QKV kernel with actual adapter weights
    ///
    /// # Arguments
    /// * `adapter_weights` - Slice of references to loaded adapter weights (GPU buffers)
    /// * `adapters` - Active adapters with IDs and Q15 gates (must match adapter_weights length)
    pub fn execute(
        &self,
        input: &Buffer,
        q_weight: &Buffer,
        k_weight: &Buffer,
        v_weight: &Buffer,
        q_output: &Buffer,
        k_output: &Buffer,
        v_output: &Buffer,
        adapter_weights: &[&AdapterWeights],
        adapters: &[super::ring_buffer::ActiveAdapter],
        ring_buffer: &RingBuffer,
    ) -> Result<()> {
        // Validate adapter_weights and adapters match
        if adapter_weights.len() != adapters.len() {
            return Err(AosError::Validation(format!(
                "Adapter count mismatch: {} weights but {} adapters",
                adapter_weights.len(),
                adapters.len()
            )));
        }

        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set base model weight buffers
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(q_weight), 0);
        encoder.set_buffer(2, Some(k_weight), 0);
        encoder.set_buffer(3, Some(v_weight), 0);
        encoder.set_buffer(4, Some(q_output), 0);
        encoder.set_buffer(5, Some(k_output), 0);
        encoder.set_buffer(6, Some(v_output), 0);

        // Pass actual LoRA weight buffers to Metal shader
        // Metal shader (aos_kernels.metal) expects buffers 8-13 for LoRA weights:
        //   Buffer 8: q_lora_a, Buffer 9: q_lora_b
        //   Buffer 10: k_lora_a, Buffer 11: k_lora_b
        //   Buffer 12: v_lora_a, Buffer 13: v_lora_b
        //
        // Our buffer layout: [q_proj_A(0), k_proj_A(1), v_proj_A(2), mlp_down_A(3), mlp_up_A(4)]
        //
        // Note: The Metal shader uses a non-standard indexing scheme. For now, we pass
        // the first adapter's weights directly. Multi-adapter support will require buffer concatenation.

        if !adapter_weights.is_empty() {
            let first_adapter = adapter_weights[0];

            // Pass Q projection LoRA weights (index 0)
            if first_adapter.lora_a_buffers.len() > 0 && first_adapter.lora_b_buffers.len() > 0 {
                encoder.set_buffer(8, Some(&first_adapter.lora_a_buffers[0]), 0); // q_lora_a
                encoder.set_buffer(9, Some(&first_adapter.lora_b_buffers[0]), 0);
                // q_lora_b
            }

            // Pass K projection LoRA weights (index 1)
            if first_adapter.lora_a_buffers.len() > 1 && first_adapter.lora_b_buffers.len() > 1 {
                encoder.set_buffer(10, Some(&first_adapter.lora_a_buffers[1]), 0); // k_lora_a
                encoder.set_buffer(11, Some(&first_adapter.lora_b_buffers[1]), 0);
                // k_lora_b
            }

            // Pass V projection LoRA weights (index 2)
            if first_adapter.lora_a_buffers.len() > 2 && first_adapter.lora_b_buffers.len() > 2 {
                encoder.set_buffer(12, Some(&first_adapter.lora_a_buffers[2]), 0); // v_lora_a
                encoder.set_buffer(13, Some(&first_adapter.lora_b_buffers[2]), 0);
                // v_lora_b
            }
        }

        // Set GQA configuration (buffer 14)
        let gqa_config_bytes =
            serde_json::to_vec(&self.gqa_config).map_err(AosError::Serialization)?;
        let gqa_config_buffer = self.device.new_buffer_with_data(
            gqa_config_bytes.as_ptr() as *const std::ffi::c_void,
            gqa_config_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(14, Some(&gqa_config_buffer), 0);

        // Set LoRA configuration (buffer 15)
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
        encoder.set_buffer(15, Some(&lora_config_buffer), 0);

        // Set ring buffer (buffer 16)
        encoder.set_buffer(16, ring_buffer.get_buffer().map(|v| &**v), 0);

        // Calculate threadgroup size optimized for GQA
        let threadgroup_size = MTLSize::new(32, 8, 1);
        let grid_size = MTLSize::new(
            input.length() / 4,
            self.gqa_config.num_attention_heads as u64,
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

    /// Get GQA configuration
    pub fn gqa_config(&self) -> &GqaConfig {
        &self.gqa_config
    }
}

/// Flash Attention kernel for memory-efficient attention computation
pub struct FlashAttentionKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    gqa_config: GqaConfig,
}

impl FlashAttentionKernel {
    /// Create a new Flash Attention kernel
    pub fn new(device: Arc<Device>, gqa_config: GqaConfig) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load library and create pipeline
        let library = device
            .new_library_with_data(include_bytes!("../shaders/mplora_kernels.metallib"))
            .map_err(|e| AosError::Kernel(format!("Failed to load library: {}", e)))?;

        let function = library
            .get_function("flash_attention", None)
            .map_err(|e| AosError::Kernel(format!("Function not found: {}", e)))?;

        let pipeline_state = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Kernel(format!("Failed to create pipeline: {}", e)))?;

        Ok(Self {
            device,
            command_queue,
            pipeline_state,
            gqa_config,
        })
    }

    /// Execute the Flash Attention kernel
    pub fn execute(&self, q: &Buffer, k: &Buffer, v: &Buffer, output: &Buffer) -> Result<()> {
        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set buffers
        encoder.set_buffer(0, Some(q), 0);
        encoder.set_buffer(1, Some(k), 0);
        encoder.set_buffer(2, Some(v), 0);
        encoder.set_buffer(3, Some(output), 0);

        // Set GQA configuration
        let gqa_config_bytes =
            serde_json::to_vec(&self.gqa_config).map_err(AosError::Serialization)?;
        let gqa_config_buffer = self.device.new_buffer_with_data(
            gqa_config_bytes.as_ptr() as *const std::ffi::c_void,
            gqa_config_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(4, Some(&gqa_config_buffer), 0);

        // Calculate threadgroup size
        let threadgroup_size = MTLSize::new(16, 16, 1);
        let grid_size = MTLSize::new(
            q.length() / 4,
            self.gqa_config.num_attention_heads as u64,
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
    fn test_gqa_config() {
        let config = GqaConfig::default();
        assert_eq!(config.num_attention_heads, 32);
        assert_eq!(config.num_key_value_heads, 4);
        assert_eq!(config.head_dim, 128);
        assert_eq!(config.rope_theta, 10000.0);
        assert_eq!(config.attention_scale, 0.0); // Use default sqrt scaling
        assert_eq!(config.dropout_rate, 0.0);
    }

    #[test]
    fn test_lora_config() {
        let config = LoraConfig::default();
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.dropout_rate, 0.0);
    }

    #[test]
    fn test_fused_qkv_creation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let gqa_config = GqaConfig::default();
        let kernel = FusedQkvKernel::new(Arc::new(device), gqa_config)
            .expect("FusedQkvKernel creation should succeed");
        assert!(!kernel.device_name().is_empty());
    }

    #[test]
    fn test_flash_attention_creation() {
        let device = Device::system_default().expect("Metal device should be available for test");
        let gqa_config = GqaConfig::default();
        let kernel = FlashAttentionKernel::new(Arc::new(device), gqa_config)
            .expect("FlashAttentionKernel creation should succeed");
        assert!(!kernel.device_name().is_empty());
    }
}
