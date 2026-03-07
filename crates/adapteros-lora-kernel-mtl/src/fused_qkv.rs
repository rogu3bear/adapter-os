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

use crate::KernelError;
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

impl GqaConfig {
    /// Create GqaConfig from raw model parameters with validation
    ///
    /// Use this when loading model configuration from config.json.
    /// This ensures GQA parameters match the actual model architecture
    /// rather than using hardcoded defaults.
    ///
    /// # Arguments
    /// * `num_attention_heads` - Number of attention heads (e.g., 28 for Qwen2.5-7B)
    /// * `num_key_value_heads` - Number of KV heads for GQA (e.g., 4 for Qwen2.5-7B)
    /// * `hidden_size` - Hidden dimension (e.g., 3584 for Qwen2.5-7B)
    /// * `rope_theta` - RoPE base frequency (e.g., 1_000_000.0 for Qwen2.5)
    ///
    /// # Errors
    /// Returns `AosError::Validation` if:
    /// - `num_attention_heads` is 0
    /// - `num_key_value_heads` is 0
    /// - `hidden_size` is 0
    /// - `hidden_size` is not divisible by `num_attention_heads`
    /// - `num_attention_heads` is not divisible by `num_key_value_heads` (GQA requirement)
    /// - `rope_theta` is not positive and finite
    ///
    /// # Example
    /// ```rust,ignore
    /// let gqa_config = GqaConfig::try_from_params(28, 4, 3584, 1_000_000.0)?;
    /// ```
    pub fn try_from_params(
        num_attention_heads: usize,
        num_key_value_heads: usize,
        hidden_size: usize,
        rope_theta: f32,
    ) -> Result<Self> {
        // Validate non-zero values
        if num_attention_heads == 0 {
            return Err(AosError::Validation(
                "num_attention_heads must be > 0".to_string(),
            ));
        }
        if num_key_value_heads == 0 {
            return Err(AosError::Validation(
                "num_key_value_heads must be > 0".to_string(),
            ));
        }
        if hidden_size == 0 {
            return Err(AosError::Validation("hidden_size must be > 0".to_string()));
        }

        // Validate divisibility
        if !hidden_size.is_multiple_of(num_attention_heads) {
            return Err(AosError::Validation(
                "hidden_size must be divisible by num_attention_heads".to_string(),
            ));
        }
        if !num_attention_heads.is_multiple_of(num_key_value_heads) {
            return Err(AosError::Validation(
                "num_attention_heads must be divisible by num_key_value_heads (GQA)".to_string(),
            ));
        }

        // Validate rope_theta
        if !rope_theta.is_finite() || rope_theta <= 0.0 {
            return Err(AosError::Validation(
                "rope_theta must be positive and finite".to_string(),
            ));
        }

        let head_dim = (hidden_size / num_attention_heads) as u32;
        Ok(Self {
            num_attention_heads: num_attention_heads as u32,
            num_key_value_heads: num_key_value_heads as u32,
            head_dim,
            kv_width: num_key_value_heads as u32 * head_dim,
            hidden_size: hidden_size as u32,
            rope_theta,
            attention_scale: 0.0,
            dropout_rate: 0.0,
        })
    }

    /// Create GqaConfig from raw model parameters (panics on invalid input)
    ///
    /// For a non-panicking version, use [`try_from_params`](Self::try_from_params).
    ///
    /// # Panics
    /// Panics if any validation fails. See [`try_from_params`](Self::try_from_params)
    /// for the list of validations performed.
    ///
    /// # Example
    /// ```rust,ignore
    /// let gqa_config = GqaConfig::from_params(28, 4, 3584, 1_000_000.0);
    /// ```
    pub fn from_params(
        num_attention_heads: usize,
        num_key_value_heads: usize,
        hidden_size: usize,
        rope_theta: f32,
    ) -> Self {
        Self::try_from_params(
            num_attention_heads,
            num_key_value_heads,
            hidden_size,
            rope_theta,
        )
        .expect("GqaConfig::from_params called with invalid parameters")
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

        let input_bytes = input.length() as usize;
        let output_buffers: [(&str, &Buffer); 3] = [
            ("q_output", q_output),
            ("k_output", k_output),
            ("v_output", v_output),
        ];
        for (buffer_name, buffer) in output_buffers {
            let available = buffer.length() as usize;
            if input_bytes > available {
                return Err(KernelError::BufferTooSmall {
                    buffer: buffer_name,
                    required: input_bytes,
                    available,
                }
                .into_aos());
            }
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

        // Pass actual LoRA weight buffers to Metal shader for ALL K adapters
        // Metal shader (aos_kernels.metal) expects buffers 8-13 for LoRA weights:
        //   Buffer 8: q_lora_a, Buffer 9: q_lora_b
        //   Buffer 10: k_lora_a, Buffer 11: k_lora_b
        //   Buffer 12: v_lora_a, Buffer 13: v_lora_b
        //
        // Our buffer layout: [q_proj_A(0), k_proj_A(1), v_proj_A(2), mlp_down_A(3), mlp_up_A(4)]
        //
        // Multi-adapter routing: Iterate over ALL adapters and apply gate-weighted contributions.
        // The RingBuffer contains Q15 gates for each adapter. The final output is:
        //   output = W_base @ x + Σᵢ (gateᵢ / 32767) * (alpha / rank) * (Bᵢ @ (Aᵢ @ x))
        //
        // Buffer layout per adapter (K adapters, interleaved):
        //   Buffers 8-13 contain QKV weights for first adapter
        //   Buffer 17+ contain additional adapter weights (K-1 adapters)

        if !adapter_weights.is_empty() {
            // Iterate over ALL adapters in the router ring, applying gate-weighted LoRA
            for (adapter_idx, (adapter, active)) in
                adapter_weights.iter().zip(adapters.iter()).enumerate()
            {
                // Calculate buffer offset for this adapter
                // First adapter uses buffers 8-13, subsequent adapters use 18+
                let base_buffer_idx = if adapter_idx == 0 {
                    8
                } else {
                    18 + (adapter_idx - 1) * 6
                };

                // Log adapter activation for debugging
                tracing::trace!(
                    adapter_id = active.id,
                    gate_q15 = active.gate,
                    gate_f32 = super::ring_buffer::RingBuffer::q15_to_float(active.gate),
                    buffer_offset = base_buffer_idx,
                    "Binding QKV adapter weights for multi-adapter routing"
                );

                // Pass Q projection LoRA weights (index 0)
                if !adapter.lora_a_buffers.is_empty() && !adapter.lora_b_buffers.is_empty() {
                    encoder.set_buffer(base_buffer_idx as u64, Some(&adapter.lora_a_buffers[0]), 0);
                    encoder.set_buffer(
                        (base_buffer_idx + 1) as u64,
                        Some(&adapter.lora_b_buffers[0]),
                        0,
                    );
                }

                // Pass K projection LoRA weights (index 1)
                if adapter.lora_a_buffers.len() > 1 && adapter.lora_b_buffers.len() > 1 {
                    encoder.set_buffer(
                        (base_buffer_idx + 2) as u64,
                        Some(&adapter.lora_a_buffers[1]),
                        0,
                    );
                    encoder.set_buffer(
                        (base_buffer_idx + 3) as u64,
                        Some(&adapter.lora_b_buffers[1]),
                        0,
                    );
                }

                // Pass V projection LoRA weights (index 2)
                if adapter.lora_a_buffers.len() > 2 && adapter.lora_b_buffers.len() > 2 {
                    encoder.set_buffer(
                        (base_buffer_idx + 4) as u64,
                        Some(&adapter.lora_a_buffers[2]),
                        0,
                    );
                    encoder.set_buffer(
                        (base_buffer_idx + 5) as u64,
                        Some(&adapter.lora_b_buffers[2]),
                        0,
                    );
                }
            }
        }

        // Note: Adapter count is already available in the RingBuffer (top_k field)
        // which is passed to the shader at buffer 16. The shader iterates using
        // ring.top_k and ring.adapter_indices to access per-adapter weights.

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

        // Check for GPU execution errors
        if command_buffer.status() == MTLCommandBufferStatus::Error {
            return Err(AosError::Kernel(
                "Fused QKV kernel execution failed: GPU command buffer error".to_string(),
            ));
        }

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
        let required_bytes = q.length() as usize;
        let output_bytes = output.length() as usize;
        if required_bytes > output_bytes {
            return Err(KernelError::BufferTooSmall {
                buffer: "output",
                required: required_bytes,
                available: output_bytes,
            }
            .into_aos());
        }

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

        // Check for GPU execution errors
        if command_buffer.status() == MTLCommandBufferStatus::Error {
            return Err(AosError::Kernel(
                "Flash attention kernel execution failed: GPU command buffer error".to_string(),
            ));
        }

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

    // =========================================================================
    // GqaConfig::from_params validation tests
    // =========================================================================

    #[test]
    fn test_gqa_config_from_params_valid() {
        // Qwen2.5-7B parameters
        let config = GqaConfig::try_from_params(28, 4, 3584, 1_000_000.0);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.num_attention_heads, 28);
        assert_eq!(config.num_key_value_heads, 4);
        assert_eq!(config.head_dim, 128); // 3584 / 28 = 128
        assert_eq!(config.hidden_size, 3584);
        assert_eq!(config.rope_theta, 1_000_000.0);
    }

    #[test]
    fn test_gqa_config_from_params_llama() {
        // Llama-like parameters (no GQA, all heads equal)
        let config = GqaConfig::try_from_params(32, 32, 4096, 10_000.0);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.num_attention_heads, 32);
        assert_eq!(config.num_key_value_heads, 32);
        assert_eq!(config.head_dim, 128); // 4096 / 32 = 128
    }

    #[test]
    fn test_gqa_config_zero_heads() {
        let result = GqaConfig::try_from_params(0, 4, 3584, 1_000_000.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("num_attention_heads must be > 0"));
    }

    #[test]
    fn test_gqa_config_zero_kv_heads() {
        let result = GqaConfig::try_from_params(28, 0, 3584, 1_000_000.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("num_key_value_heads must be > 0"));
    }

    #[test]
    fn test_gqa_config_zero_hidden() {
        let result = GqaConfig::try_from_params(28, 4, 0, 1_000_000.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("hidden_size must be > 0"));
    }

    #[test]
    fn test_gqa_config_hidden_not_divisible() {
        // 3583 is not divisible by 28
        let result = GqaConfig::try_from_params(28, 4, 3583, 1_000_000.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("hidden_size must be divisible"));
    }

    #[test]
    fn test_gqa_config_heads_not_divisible_by_kv() {
        // 28 is not divisible by 5
        let result = GqaConfig::try_from_params(28, 5, 3584, 1_000_000.0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("num_attention_heads must be divisible by num_key_value_heads"));
    }

    #[test]
    fn test_gqa_config_invalid_rope_theta() {
        // Zero theta
        let result = GqaConfig::try_from_params(28, 4, 3584, 0.0);
        assert!(result.is_err());

        // Negative theta
        let result = GqaConfig::try_from_params(28, 4, 3584, -1.0);
        assert!(result.is_err());

        // NaN theta
        let result = GqaConfig::try_from_params(28, 4, 3584, f32::NAN);
        assert!(result.is_err());

        // Infinity theta
        let result = GqaConfig::try_from_params(28, 4, 3584, f32::INFINITY);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "GqaConfig::from_params called with invalid parameters")]
    fn test_gqa_config_from_params_panics_on_invalid() {
        // This should panic due to invalid parameters
        let _ = GqaConfig::from_params(0, 4, 3584, 1_000_000.0);
    }
}
