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

use super::ring_buffer::RawRingBuffer;

/// GQA configuration
#[repr(C)]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
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
#[repr(C)]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
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

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AttentionPointerSet {
    input: u64,
    q_output: u64,
    k_output: u64,
    v_output: u64,
    q_weight: u64,
    k_weight: u64,
    v_weight: u64,
    q_lora_a: u64,
    q_lora_b: u64,
    k_lora_a: u64,
    k_lora_b: u64,
    v_lora_a: u64,
    v_lora_b: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MetalAttentionParams {
    input: u64,
    q_output: u64,
    k_output: u64,
    v_output: u64,
    q_weight: u64,
    k_weight: u64,
    v_weight: u64,
    q_lora_a: u64,
    q_lora_b: u64,
    k_lora_a: u64,
    k_lora_b: u64,
    v_lora_a: u64,
    v_lora_b: u64,
    gqa_config: GqaConfig,
    lora_config: LoraConfig,
    ring_buffer: RawRingBuffer,
    batch_size: u32,
    max_adapters: u32,
    _padding: [u32; 2],
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
            .new_library_with_data(include_bytes!("../shaders/adapteros_kernels.metallib"))
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

    /// Execute the fused QKV kernel
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        input: &Buffer,
        q_weight: &Buffer,
        k_weight: &Buffer,
        v_weight: &Buffer,
        q_output: &Buffer,
        k_output: &Buffer,
        v_output: &Buffer,
        q_lora_a: &Buffer,
        q_lora_b: &Buffer,
        k_lora_a: &Buffer,
        k_lora_b: &Buffer,
        v_lora_a: &Buffer,
        v_lora_b: &Buffer,
        lora_config: &LoraConfig,
        ring_state: RawRingBuffer,
        max_adapters: u32,
        batch_size: u32,
    ) -> Result<()> {
        let command_buffer = self.command_queue.new_command_buffer();

        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        let pointer_set = AttentionPointerSet {
            input: input.gpu_address(),
            q_output: q_output.gpu_address(),
            k_output: k_output.gpu_address(),
            v_output: v_output.gpu_address(),
            q_weight: q_weight.gpu_address(),
            k_weight: k_weight.gpu_address(),
            v_weight: v_weight.gpu_address(),
            q_lora_a: q_lora_a.gpu_address(),
            q_lora_b: q_lora_b.gpu_address(),
            k_lora_a: k_lora_a.gpu_address(),
            k_lora_b: k_lora_b.gpu_address(),
            v_lora_a: v_lora_a.gpu_address(),
            v_lora_b: v_lora_b.gpu_address(),
        };

        let params = MetalAttentionParams {
            input: pointer_set.input,
            q_output: pointer_set.q_output,
            k_output: pointer_set.k_output,
            v_output: pointer_set.v_output,
            q_weight: pointer_set.q_weight,
            k_weight: pointer_set.k_weight,
            v_weight: pointer_set.v_weight,
            q_lora_a: pointer_set.q_lora_a,
            q_lora_b: pointer_set.q_lora_b,
            k_lora_a: pointer_set.k_lora_a,
            k_lora_b: pointer_set.k_lora_b,
            v_lora_a: pointer_set.v_lora_a,
            v_lora_b: pointer_set.v_lora_b,
            gqa_config: self.gqa_config,
            lora_config: *lora_config,
            ring_buffer: ring_state,
            batch_size,
            max_adapters,
            _padding: [0; 2],
        };

        let params_buffer = self.device.new_buffer_with_data(
            &params as *const MetalAttentionParams as *const std::ffi::c_void,
            std::mem::size_of::<MetalAttentionParams>() as u64,
            MTLResourceOptions::StorageModeShared,
        );

        encoder.set_buffer(0, Some(&params_buffer), 0);

        let threadgroup_size = MTLSize::new(1, 1, 1);
        let grid_size = MTLSize::new(
            batch_size as u64,
            self.gqa_config.num_attention_heads as u64,
            self.gqa_config.head_dim as u64,
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
