# Phase 4: Metal Kernels Implementation Guide

This document outlines the requirements and implementation details for Phase 4 Metal kernels in AdapterOS, specifically designed for Qwen2.5-7B-Instruct with GQA support.

## Overview

Phase 4 implements fused Metal kernels for Apple Silicon M-series chips, providing:
- **Fused MLP**: SwiGLU activation with LoRA support
- **Fused QKV**: Grouped Query Attention (GQA) with LoRA
- **Ring Buffer**: Top-K adapter management
- **Q15 Gates**: Quantized router decisions
- **Deterministic Math**: Precise floating-point operations

## Architecture

### Kernel Organization
```
aos-kernel-mtl/
├── src/
│   ├── lib.rs              # Kernel API and bindings
│   ├── fused_mlp.rs        # MLP kernel implementation
│   ├── fused_qkv.rs        # QKV kernel implementation
│   ├── ring_buffer.rs      # Adapter ring buffer
│   └── math.rs             # Deterministic math utilities
├── shaders/
│   ├── fused_mlp.metal     # MLP Metal shader
│   ├── fused_qkv.metal     # QKV Metal shader
│   └── common.metal        # Shared utilities
└── tests/
    ├── kernel_tests.rs     # Kernel correctness tests
    └── performance_tests.rs # Performance benchmarks
```

### Metal Shader Structure
```metal
#include <metal_stdlib>
using namespace metal;

// Common structures
struct GqaConfig {
    uint num_attention_heads;
    uint num_key_value_heads;
    uint head_dim;
    uint kv_width;
};

struct LoraConfig {
    uint rank;
    float alpha;
    uint target_module;
};

struct RingBuffer {
    uint top_k;
    uint current_pos;
    uint adapter_indices[8];  // Max K=8
    uint16_t gates[8];        // Q15 format
};
```

## Fused MLP Kernel

### Requirements
- **Input**: `[batch_size, hidden_size]`
- **Output**: `[batch_size, hidden_size]`
- **Activation**: SwiGLU (SiLU gate, linear up)
- **LoRA**: Rank-16 adapters for `gate_proj`, `up_proj`, `down_proj`
- **Precision**: FP16 compute, int4 weights
- **Performance**: ≤ 8ms per token (p95)

### Metal Shader Implementation
```metal
kernel void fused_mlp(
    device const float* input,           // [batch_size, hidden_size]
    device const float* gate_weight,     // [hidden_size, intermediate_size]
    device const float* up_weight,       // [hidden_size, intermediate_size]
    device const float* down_weight,     // [intermediate_size, hidden_size]
    device float* output,                // [batch_size, hidden_size]
    
    // LoRA parameters
    device const float* gate_lora_a,     // [hidden_size, rank]
    device const float* gate_lora_b,     // [rank, intermediate_size]
    device const float* up_lora_a,       // [hidden_size, rank]
    device const float* up_lora_b,       // [rank, intermediate_size]
    device const float* down_lora_a,     // [intermediate_size, rank]
    device const float* down_lora_b,     // [rank, hidden_size]
    
    // Configuration
    constant LoraConfig& lora_config,
    constant RingBuffer& ring_buffer,
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint hidden_idx = gid.y;
    uint intermediate_idx = gid.z;
    
    // Load input
    float input_val = input[batch_idx * hidden_size + hidden_idx];
    
    // Compute gate projection with LoRA
    float gate_val = 0.0;
    for (uint i = 0; i < intermediate_size; i++) {
        float base_weight = gate_weight[hidden_idx * intermediate_size + i];
        float lora_delta = 0.0;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = ring_buffer.gates[k] / 32768.0; // Convert Q15 to float
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = gate_lora_a[hidden_idx * lora_config.rank + adapter_idx];
                float lora_b = gate_lora_b[adapter_idx * intermediate_size + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        gate_val += input_val * (base_weight + lora_delta);
    }
    
    // Apply SiLU activation
    float gate_activated = gate_val / (1.0 + exp(-gate_val));
    
    // Compute up projection with LoRA
    float up_val = 0.0;
    for (uint i = 0; i < intermediate_size; i++) {
        float base_weight = up_weight[hidden_idx * intermediate_size + i];
        float lora_delta = 0.0;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = ring_buffer.gates[k] / 32768.0;
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = up_lora_a[hidden_idx * lora_config.rank + adapter_idx];
                float lora_b = up_lora_b[adapter_idx * intermediate_size + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        up_val += input_val * (base_weight + lora_delta);
    }
    
    // Element-wise multiplication (SwiGLU)
    float intermediate_val = gate_activated * up_val;
    
    // Compute down projection with LoRA
    float down_val = 0.0;
    for (uint i = 0; i < hidden_size; i++) {
        float base_weight = down_weight[intermediate_idx * hidden_size + i];
        float lora_delta = 0.0;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = ring_buffer.gates[k] / 32768.0;
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = down_lora_a[intermediate_idx * lora_config.rank + adapter_idx];
                float lora_b = down_lora_b[adapter_idx * hidden_size + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        down_val += intermediate_val * (base_weight + lora_delta);
    }
    
    // Store output
    output[batch_idx * hidden_size + hidden_idx] = down_val;
}
```

### Rust API
```rust
pub struct FusedMlpKernel {
    device: Device,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    ring_buffer: RingBuffer,
}

impl FusedMlpKernel {
    pub fn new(device: &Device) -> Result<Self> {
        let command_queue = device.new_command_queue()?;
        let library = device.new_library_with_data(include_bytes!("shaders/fused_mlp.metallib"))?;
        let function = library.get_function("fused_mlp", None)?;
        let pipeline_state = device.new_compute_pipeline_state_with_function(&function)?;
        
        Ok(Self {
            device: device.clone(),
            command_queue,
            pipeline_state,
            ring_buffer: RingBuffer::new(3), // K=3
        })
    }
    
    pub fn execute(
        &self,
        input: &Buffer,
        gate_weight: &Buffer,
        up_weight: &Buffer,
        down_weight: &Buffer,
        output: &Buffer,
        lora_config: &LoraConfig,
        adapters: &[ActiveAdapter],
    ) -> Result<()> {
        // Update ring buffer with active adapters
        self.ring_buffer.update(adapters)?;
        
        let command_buffer = self.command_queue.new_command_buffer()?;
        let encoder = command_buffer.new_compute_command_encoder()?;
        
        encoder.set_compute_pipeline_state(&self.pipeline_state);
        encoder.set_buffer(0, Some(input), 0);
        encoder.set_buffer(1, Some(gate_weight), 0);
        encoder.set_buffer(2, Some(up_weight), 0);
        encoder.set_buffer(3, Some(down_weight), 0);
        encoder.set_buffer(4, Some(output), 0);
        encoder.set_buffer(5, Some(&self.ring_buffer.buffer), 0);
        encoder.set_buffer(6, Some(&lora_config.buffer), 0);
        
        // Set threadgroup size
        let threadgroup_size = MTLSize::new(16, 16, 1);
        let grid_size = MTLSize::new(
            (input.length() / 4) as u64, // FP16 = 2 bytes, 4 elements per thread
            (gate_weight.length() / 4) as u64,
            1
        );
        
        encoder.dispatch_threadgroups(grid_size, threadgroup_size);
        encoder.end_encoding();
        
        command_buffer.commit();
        command_buffer.wait_until_completed();
        
        Ok(())
    }
}
```

## Fused QKV Kernel with GQA

### Requirements
- **Input**: `[batch_size, hidden_size]`
- **Q Output**: `[batch_size, num_attention_heads, head_dim]`
- **K Output**: `[batch_size, num_key_value_heads, head_dim]`
- **V Output**: `[batch_size, num_key_value_heads, head_dim]`
- **GQA Ratio**: 8:1 (32 attention heads, 4 key-value heads)
- **LoRA**: Rank-16 adapters for `q_proj`, `k_proj`, `v_proj`
- **Performance**: ≤ 6ms per token (p95)

### Metal Shader Implementation
```metal
kernel void fused_qkv_gqa(
    device const float* input,           // [batch_size, hidden_size]
    device const float* q_weight,        // [hidden_size, hidden_size]
    device const float* k_weight,        // [hidden_size, kv_width]
    device const float* v_weight,        // [hidden_size, kv_width]
    device float* q_output,              // [batch_size, num_attention_heads, head_dim]
    device float* k_output,              // [batch_size, num_key_value_heads, head_dim]
    device float* v_output,              // [batch_size, num_key_value_heads, head_dim]
    
    // LoRA parameters
    device const float* q_lora_a,        // [hidden_size, rank]
    device const float* q_lora_b,        // [rank, hidden_size]
    device const float* k_lora_a,        // [hidden_size, rank]
    device const float* k_lora_b,        // [rank, kv_width]
    device const float* v_lora_a,        // [hidden_size, rank]
    device const float* v_lora_b,        // [rank, kv_width]
    
    // Configuration
    constant GqaConfig& gqa_config,
    constant LoraConfig& lora_config,
    constant RingBuffer& ring_buffer,
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;
    
    // Load input
    float input_val = input[batch_idx * gqa_config.hidden_size + dim_idx];
    
    // Compute Q projection with LoRA
    if (head_idx < gqa_config.num_attention_heads) {
        float q_val = 0.0;
        for (uint i = 0; i < gqa_config.hidden_size; i++) {
            float base_weight = q_weight[dim_idx * gqa_config.hidden_size + i];
            float lora_delta = 0.0;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = ring_buffer.gates[k] / 32768.0;
                
                if (adapter_idx < lora_config.rank) {
                    float lora_a = q_lora_a[dim_idx * lora_config.rank + adapter_idx];
                    float lora_b = q_lora_b[adapter_idx * gqa_config.hidden_size + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
            }
            
            q_val += input_val * (base_weight + lora_delta);
        }
        
        // Store Q output
        uint q_offset = batch_idx * gqa_config.num_attention_heads * gqa_config.head_dim +
                       head_idx * gqa_config.head_dim + dim_idx;
        q_output[q_offset] = q_val;
    }
    
    // Compute K projection with LoRA (only for key-value heads)
    if (head_idx < gqa_config.num_key_value_heads) {
        float k_val = 0.0;
        for (uint i = 0; i < gqa_config.kv_width; i++) {
            float base_weight = k_weight[dim_idx * gqa_config.kv_width + i];
            float lora_delta = 0.0;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = ring_buffer.gates[k] / 32768.0;
                
                if (adapter_idx < lora_config.rank) {
                    float lora_a = k_lora_a[dim_idx * lora_config.rank + adapter_idx];
                    float lora_b = k_lora_b[adapter_idx * gqa_config.kv_width + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
            }
            
            k_val += input_val * (base_weight + lora_delta);
        }
        
        // Store K output
        uint k_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                       head_idx * gqa_config.head_dim + dim_idx;
        k_output[k_offset] = k_val;
    }
    
    // Compute V projection with LoRA (only for key-value heads)
    if (head_idx < gqa_config.num_key_value_heads) {
        float v_val = 0.0;
        for (uint i = 0; i < gqa_config.kv_width; i++) {
            float base_weight = v_weight[dim_idx * gqa_config.kv_width + i];
            float lora_delta = 0.0;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = ring_buffer.gates[k] / 32768.0;
                
                if (adapter_idx < lora_config.rank) {
                    float lora_a = v_lora_a[dim_idx * lora_config.rank + adapter_idx];
                    float lora_b = v_lora_b[adapter_idx * gqa_config.kv_width + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
            }
            
            v_val += input_val * (base_weight + lora_delta);
        }
        
        // Store V output
        uint v_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                       head_idx * gqa_config.head_dim + dim_idx;
        v_output[v_offset] = v_val;
    }
}
```

## Ring Buffer Implementation

### Requirements
- **Top-K Management**: Efficient adapter selection
- **Q15 Gates**: Quantized router decisions
- **Memory Efficient**: Minimal overhead
- **Thread Safe**: Concurrent access support

### Rust Implementation
```rust
pub struct RingBuffer {
    top_k: usize,
    current_pos: usize,
    adapter_indices: Vec<u32>,
    gates: Vec<u16>, // Q15 format
    buffer: Buffer,
}

impl RingBuffer {
    pub fn new(top_k: usize) -> Self {
        Self {
            top_k,
            current_pos: 0,
            adapter_indices: vec![0; top_k],
            gates: vec![0; top_k],
            buffer: Buffer::new(),
        }
    }
    
    pub fn update(&mut self, adapters: &[ActiveAdapter]) -> Result<()> {
        if adapters.len() > self.top_k {
            return Err(AosError::Kernel("Too many adapters for ring buffer".to_string()));
        }
        
        for (i, adapter) in adapters.iter().enumerate() {
            self.adapter_indices[i] = adapter.id as u32;
            self.gates[i] = (adapter.gate * 32768.0) as u16; // Convert to Q15
        }
        
        // Pad remaining slots with zeros
        for i in adapters.len()..self.top_k {
            self.adapter_indices[i] = 0;
            self.gates[i] = 0;
        }
        
        // Update Metal buffer
        self.buffer.update(&self.adapter_indices, &self.gates)?;
        
        Ok(())
    }
    
    pub fn get_active_adapters(&self) -> Vec<ActiveAdapter> {
        let mut adapters = Vec::new();
        
        for i in 0..self.top_k {
            if self.adapter_indices[i] != 0 {
                adapters.push(ActiveAdapter {
                    id: self.adapter_indices[i],
                    gate: self.gates[i] as f32 / 32768.0, // Convert from Q15
                });
            }
        }
        
        adapters
    }
}
```

## Deterministic Math

### Requirements
- **Precise Operations**: No fast-math optimizations
- **Consistent Results**: Identical outputs across runs
- **Performance**: Minimal overhead
- **Validation**: Runtime checks for determinism

### Implementation
```rust
pub mod deterministic_math {
    use std::arch::aarch64::*;
    
    /// Deterministic floating-point operations
    pub struct DeterministicMath;
    
    impl DeterministicMath {
        /// Deterministic matrix multiplication
        pub fn gemm(
            a: &[f32],
            b: &[f32],
            c: &mut [f32],
            m: usize,
            n: usize,
            k: usize,
        ) -> Result<()> {
            // Disable fast-math optimizations
            unsafe {
                let old_fpcr = _mm_get_fpcr();
                _mm_set_fpcr(old_fpcr & !0x8000); // Clear fast-math bit
                
                // Perform matrix multiplication
                for i in 0..m {
                    for j in 0..n {
                        let mut sum = 0.0f32;
                        for l in 0..k {
                            sum += a[i * k + l] * b[l * n + j];
                        }
                        c[i * n + j] = sum;
                    }
                }
                
                // Restore FPCR
                _mm_set_fpcr(old_fpcr);
            }
            
            Ok(())
        }
        
        /// Deterministic activation functions
        pub fn silu(x: f32) -> f32 {
            x / (1.0 + (-x).exp())
        }
        
        pub fn gelu(x: f32) -> f32 {
            0.5 * x * (1.0 + ((2.0 / std::f32::consts::PI).sqrt() * (x + 0.044715 * x.powi(3))).tanh())
        }
        
        /// Deterministic softmax
        pub fn softmax(input: &[f32], output: &mut [f32]) -> Result<()> {
            let max_val = input.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
            let sum: f32 = input.iter().map(|&x| (x - max_val).exp()).sum();
            
            for (i, &x) in input.iter().enumerate() {
                output[i] = (x - max_val).exp() / sum;
            }
            
            Ok(())
        }
    }
}
```

## Performance Optimization

### Threadgroup Sizing
```rust
pub fn calculate_optimal_threadgroup_size(
    input_size: usize,
    output_size: usize,
    device: &Device,
) -> Result<MTLSize> {
    let max_threads_per_group = device.max_threads_per_threadgroup();
    
    // Calculate optimal dimensions
    let x_dim = (input_size / 4).min(max_threads_per_group.width as usize);
    let y_dim = (output_size / 4).min(max_threads_per_group.height as usize);
    let z_dim = 1.min(max_threads_per_group.depth as usize);
    
    Ok(MTLSize::new(x_dim as u64, y_dim as u64, z_dim as u64))
}
```

### Memory Optimization
```rust
pub struct MemoryPool {
    buffers: Vec<Buffer>,
    free_indices: Vec<usize>,
    total_size: usize,
}

impl MemoryPool {
    pub fn new(device: &Device, total_size: usize) -> Result<Self> {
        let buffer = device.new_buffer(total_size, MTLResourceOptions::StorageModeShared)?;
        
        Ok(Self {
            buffers: vec![buffer],
            free_indices: vec![0],
            total_size,
        })
    }
    
    pub fn allocate(&mut self, size: usize) -> Result<Buffer> {
        // Find free buffer of sufficient size
        for &idx in &self.free_indices {
            if self.buffers[idx].length() >= size {
                return Ok(self.buffers[idx].clone());
            }
        }
        
        // Allocate new buffer if needed
        let new_buffer = self.device.new_buffer(size, MTLResourceOptions::StorageModeShared)?;
        self.buffers.push(new_buffer);
        
        Ok(self.buffers.last().unwrap().clone())
    }
}
```

## Testing Framework

### Correctness Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fused_mlp_correctness() {
        let device = Device::system_default().unwrap();
        let kernel = FusedMlpKernel::new(&device).unwrap();
        
        // Create test data
        let input = create_test_input(&device);
        let weights = create_test_weights(&device);
        let expected_output = create_expected_output();
        
        // Execute kernel
        let output = kernel.execute(&input, &weights).unwrap();
        
        // Verify correctness
        assert_tensors_equal(&output, &expected_output, 1e-5);
    }
    
    #[test]
    fn test_gqa_correctness() {
        let device = Device::system_default().unwrap();
        let kernel = FusedQkvKernel::new(&device).unwrap();
        
        // Create GQA test data
        let input = create_gqa_test_input(&device);
        let weights = create_gqa_test_weights(&device);
        let expected_q = create_expected_q_output();
        let expected_kv = create_expected_kv_output();
        
        // Execute kernel
        let (q_output, k_output, v_output) = kernel.execute(&input, &weights).unwrap();
        
        // Verify GQA outputs
        assert_tensors_equal(&q_output, &expected_q, 1e-5);
        assert_tensors_equal(&k_output, &expected_kv, 1e-5);
        assert_tensors_equal(&v_output, &expected_kv, 1e-5);
    }
    
    #[test]
    fn test_determinism() {
        let device = Device::system_default().unwrap();
        let kernel = FusedMlpKernel::new(&device).unwrap();
        
        // Run same computation multiple times
        let mut outputs = Vec::new();
        for _ in 0..10 {
            let input = create_test_input(&device);
            let weights = create_test_weights(&device);
            let output = kernel.execute(&input, &weights).unwrap();
            outputs.push(output);
        }
        
        // Verify all outputs are identical
        for i in 1..outputs.len() {
            assert_tensors_equal(&outputs[0], &outputs[i], 0.0);
        }
    }
}
```

### Performance Tests
```rust
#[test]
fn test_performance_benchmarks() {
    let device = Device::system_default().unwrap();
    let kernel = FusedMlpKernel::new(&device).unwrap();
    
    // Benchmark MLP kernel
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let input = create_test_input(&device);
        let weights = create_test_weights(&device);
        let _output = kernel.execute(&input, &weights).unwrap();
    }
    let duration = start.elapsed();
    
    // Verify performance target
    let avg_time_per_call = duration.as_millis() / 1000;
    assert!(avg_time_per_call <= 8, "MLP kernel too slow: {}ms", avg_time_per_call);
    
    println!("MLP kernel performance: {}ms per call", avg_time_per_call);
}
```

## Integration with AdapterOS

### Kernel Loading
```rust
pub struct KernelManager {
    device: Device,
    mlp_kernel: FusedMlpKernel,
    qkv_kernel: FusedQkvKernel,
    ring_buffer: RingBuffer,
}

impl KernelManager {
    pub fn new() -> Result<Self> {
        let device = Device::system_default()
            .ok_or_else(|| AosError::Kernel("No Metal device available".to_string()))?;
        
        let mlp_kernel = FusedMlpKernel::new(&device)?;
        let qkv_kernel = FusedQkvKernel::new(&device)?;
        let ring_buffer = RingBuffer::new(3); // K=3
        
        Ok(Self {
            device,
            mlp_kernel,
            qkv_kernel,
            ring_buffer,
        })
    }
    
    pub fn load_plan(&mut self, plan: &Plan) -> Result<()> {
        // Verify kernel hashes match plan
        let mlp_hash = self.mlp_kernel.get_hash()?;
        let qkv_hash = self.qkv_kernel.get_hash()?;
        
        if mlp_hash != plan.mlp_kernel_hash || qkv_hash != plan.qkv_kernel_hash {
            return Err(AosError::DeterminismViolation(
                "Kernel hashes don't match plan".to_string()
            ));
        }
        
        // Load model weights
        self.mlp_kernel.load_weights(&plan.mlp_weights)?;
        self.qkv_kernel.load_weights(&plan.qkv_weights)?;
        
        Ok(())
    }
}
```

### Worker Integration
```rust
impl Worker {
    pub fn execute_with_metal(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        // Update ring buffer with active adapters
        let active_adapters = self.router.get_active_adapters()?;
        self.kernel_manager.ring_buffer.update(&active_adapters)?;
        
        // Execute MLP kernel
        let mlp_output = self.kernel_manager.mlp_kernel.execute(
            &input,
            &self.mlp_weights,
            &self.lora_config,
            &active_adapters,
        )?;
        
        // Execute QKV kernel
        let (q_output, k_output, v_output) = self.kernel_manager.qkv_kernel.execute(
            &mlp_output,
            &self.qkv_weights,
            &self.gqa_config,
            &self.lora_config,
            &active_adapters,
        )?;
        
        // Continue with attention computation...
        Ok(self.compute_attention(q_output, k_output, v_output)?)
    }
}
```

## Performance Targets

### Latency Requirements
- **MLP Kernel**: ≤ 8ms per token (p95)
- **QKV Kernel**: ≤ 6ms per token (p95)
- **Total Inference**: ≤ 24ms per token (p95)
- **Router Overhead**: ≤ 8% of total time

### Throughput Requirements
- **Tokens per Second**: ≥ 40 tokens/s
- **Batch Size**: 1 (streaming)
- **Context Length**: 32K tokens
- **Memory Bandwidth**: ≥ 100 GB/s

### Quality Requirements
- **Numerical Accuracy**: ≤ 1e-5 error vs CPU reference
- **Determinism**: Zero diff on identical inputs
- **Stability**: No crashes or memory leaks
- **Compatibility**: M1, M2, M3, M4 support

## Deployment Checklist

### Pre-deployment
- [ ] Kernel correctness tests pass
- [ ] Performance benchmarks meet targets
- [ ] Determinism tests pass
- [ ] Memory leak tests pass
- [ ] Integration tests pass

### Deployment
- [ ] Kernel hashes recorded in plan
- [ ] Metal shaders embedded in binary
- [ ] Performance monitoring enabled
- [ ] Error handling implemented
- [ ] Documentation updated

### Post-deployment
- [ ] Performance metrics collected
- [ ] Error rates monitored
- [ ] User feedback gathered
- [ ] Optimization opportunities identified
- [ ] Next iteration planned

## TODO: Kernel Fusion Enhancement

### High Priority

#### Integrate RoPE (Rotary Position Embeddings)
- [ ] Add RoPE computation to `fused_qkv_projection` kernel
- [ ] Implement `apply_rotary_pos_emb` function in Metal
- [ ] Handle `rope_theta` parameter from model config (currently 10000.0 for Qwen2.5-7B)
- [ ] Test with Qwen2.5-7B position embeddings up to 32K context length
- [ ] Verify deterministic output across runs

**Implementation notes:**
```metal
// RoPE formula: x * cos(θ) + rotate_half(x) * sin(θ)
// where θ = position / (rope_theta ^ (2 * dim_idx / head_dim))
kernel void apply_rope(
    device const float* q_k,           // Q or K vectors
    device float* output,              // Rotated output
    constant uint& position,           // Sequence position
    constant float& rope_theta,        // Base frequency (10000.0)
    constant uint& head_dim            // Dimension per head (128)
);
```

#### Add Deterministic Dropout
- [ ] Implement `deterministic_dropout` in Metal kernels
- [ ] Use seeded RNG with HKDF labels for reproducibility
- [ ] Integrate dropout into MLP and attention kernels
- [ ] Ensure dropout masks are consistent across runs with same seed
- [ ] Add `dropout_rate` to LoraConfig and GqaConfig

**Implementation notes:**
- Derive per-layer seeds from global seed using HKDF with labels like `dropout_layer_{N}`
- Use PCG or xorshift RNG with fixed seed for deterministic random numbers
- Cache dropout masks in ring buffer to avoid recomputation

#### Fuse Bias Terms
- [ ] Add bias parameters to fused MLP kernel (`gate_bias`, `up_bias`, `down_bias`)
- [ ] Integrate attention bias into QKV projection (`q_bias`, `k_bias`, `v_bias`)
- [ ] Update kernel signatures to include bias buffers
- [ ] Verify bias fusion doesn't impact performance (target: <2% overhead)
- [ ] Add bias to buffer layout validation

**Files to modify:**
- `metal/aos_kernels.metal` - Add bias buffers to kernel signatures
- `metal/fused_attention.metal` - Add bias to QKV projection
- `crates/mplora-kernel-mtl/src/fused_mlp.rs` - Update Rust interface
- `crates/mplora-kernel-mtl/src/fused_qkv.rs` - Update Rust interface

### Medium Priority

#### Optimize Memory Access Patterns
- [ ] Profile current memory access with Instruments (Metal System Trace)
- [ ] Implement coalesced memory access for better bandwidth utilization
- [ ] Add memory prefetching hints where beneficial (use `threadgroup_barrier`)
- [ ] Optimize threadgroup memory usage (reduce SRAM pressure)
- [ ] Target: >100 GB/s memory bandwidth on M3 Max

**Profiling workflow:**
1. Run with `xcrun xctrace record --template 'Metal System Trace'`
2. Identify memory-bound kernels (occupancy >90%, bandwidth <80%)
3. Optimize access patterns to sequential reads/writes
4. Re-profile and update baselines

#### Enhance Attention Scaling
- [ ] Make attention scaling factor configurable in GqaConfig
- [ ] Add support for different scaling strategies (sqrt, learned, constant)
- [ ] Implement scaling in the flash attention kernel (currently hardcoded as `1.0 / sqrt(head_dim)`)
- [ ] Test scaling impact on numerical stability
- [ ] Add scaling factor to telemetry events

#### Improve LoRA Integration
- [ ] Add support for different LoRA ranks per layer (currently fixed at 16)
- [ ] Implement LoRA dropout (if needed for fine-tuning support)
- [ ] Add LoRA scaling factors beyond alpha (e.g., per-adapter scaling)
- [ ] Optimize LoRA weight loading and caching (reduce buffer copies)
- [ ] Support dynamic LoRA rank adjustment based on memory pressure

### Low Priority

#### Add Kernel Profiling
- [ ] Implement detailed timing for each kernel dispatch
- [ ] Add memory bandwidth utilization metrics (read/write bytes)
- [ ] Create performance regression detection (automated CI checks)
- [ ] Add kernel execution graphs for debugging (visualize pipeline)
- [ ] Integrate with `mplora-system-metrics` for unified monitoring

#### Enhance Error Handling
- [ ] Add comprehensive error checking in Metal kernels (bounds, alignment)
- [ ] Implement graceful degradation for unsupported features (fallback to CPU)
- [ ] Add kernel validation and bounds checking (debug mode)
- [ ] Improve error messages for debugging (include buffer names, sizes)
- [ ] Add kernel panic recovery (wrap all dispatches)

#### Documentation and Testing
- [ ] Add comprehensive kernel documentation (inline comments, docstrings)
- [ ] Create unit tests for each kernel function (correctness, edge cases)
- [ ] Add integration tests with real model weights (Qwen2.5-7B)
- [ ] Document performance characteristics and limitations (memory, latency)
- [ ] Add performance profiling guide (Instruments, baselines)

### Implementation Notes

**Files to modify:**
- `metal/aos_kernels.metal` - Main kernel implementations
- `metal/fused_attention.metal` - Attention kernels
- `metal/fused_mlp.metal` - MLP kernels
- `metal/common.metal` - Shared utilities (add RoPE, dropout helpers)
- `crates/mplora-kernel-mtl/src/lib.rs` - Kernel API interface
- `crates/mplora-kernel-mtl/src/fused_mlp.rs` - MLP Rust wrapper
- `crates/mplora-kernel-mtl/src/fused_qkv.rs` - QKV Rust wrapper

**Testing approach:**
- Use deterministic test cases with known outputs (compare against PyTorch reference)
- Compare against reference implementations (HuggingFace Transformers)
- Profile with macOS Instruments (Metal System Trace, Time Profiler)
- Test on different Apple Silicon variants (M1, M2, M3, M4)
- Run regression tests before updating baselines

**Performance targets (from policy ruleset):**
- Maintain <24ms p95 latency for token generation
- Keep router overhead <8% of total inference time
- Ensure memory usage stays within 15% headroom
- Achieve >40 tokens/second throughput
- Memory bandwidth >100 GB/s on M3 Max

**Compliance requirements:**
- All changes must maintain determinism (zero-diff replay)
- Kernel hashes must be updated in manifest after changes
- Performance regression tests must pass (±8% tolerance)
- No fast-math optimizations (`#pragma clang fp contract(off)`)
- All RNG must use HKDF-derived seeds

---

## Future Enhancements

### Advanced Optimizations
- **Tensor Cores**: Utilize M-series tensor cores
- **Memory Compression**: Reduce memory bandwidth
- **Kernel Fusion**: Combine more operations
- **Dynamic Batching**: Support variable batch sizes

### New Features
- **Multi-GPU**: Support multiple M-series chips
- **Quantization**: Int8 and int4 support
- **Sparse Attention**: Implement sparse patterns
- **Custom Activations**: User-defined functions

### Research Areas
- **Novel Architectures**: Explore new model designs
- **Efficiency Improvements**: Reduce compute requirements
- **Scalability**: Support larger models
- **Innovation**: Breakthrough performance gains

---

## Build Matrix and CI Pipeline

### Supported Architectures

| Architecture | Family | Cores (GPU) | Memory | Status |
|--------------|--------|-------------|---------|--------|
| Apple M1 | 1st Gen | 7-8 | 8-16GB | Tested |
| Apple M1 Pro/Max | 1st Gen | 14-32 | 16-64GB | Tested |
| Apple M2 | 2nd Gen | 8-10 | 8-24GB | Tested |
| Apple M2 Pro/Max | 2nd Gen | 16-38 | 16-96GB | Tested |
| Apple M3 | 3rd Gen | 10 | 8-24GB | Tested |
| Apple M3 Pro/Max | 3rd Gen | 14-40 | 18-128GB | Primary |
| Apple M4 | 4th Gen | 10 | 16-32GB | Tested |

### Build Configuration

Kernels are compiled with:
- **Metal Version**: 3.1
- **SDK**: macOS (latest)
- **Optimization**: `-O3` equivalent
- **Determinism**: Fast-math disabled

### CI Workflow

The `.github/workflows/metal.yml` workflow enforces:

1. **Toolchain Validation**
   - Xcode version must match `metal/toolchain.toml`
   - Metal compiler version locked to 3.1
   - BLAKE3 hash tool required

2. **Kernel Compilation**
   - Runs `metal/ci_build.sh`
   - Compiles `.metal` → `.air` → `.metallib`
   - Computes BLAKE3 hash

3. **Hash Verification**
   - Extracts `METALLIB_HASH` constant from `mplora-kernel-mtl/src/lib.rs`
   - Compares with actual `.metallib` hash
   - **Blocks merge** on mismatch

4. **Artifact Upload**
   - Uploads `.metallib` as CI artifact
   - Retained for 30 days
   - Available for regression testing

5. **Regression Tests**
   - Runs `tests/kernel_regression.rs`
   - Compares against `metal/baselines/<arch>.json`
   - Fails if any kernel regresses >8%

### Manual Baseline Updates

When performance characteristics legitimately change:

```bash
UPDATE_BASELINES=1 cargo test --test kernel_regression
git add metal/baselines/
git commit -m "Update M3 baseline after optimization"
```

---

## Diagnostic Tools

### AOS_DETERMINISTIC_DEBUG

Enable comprehensive kernel tracing:

```bash
export AOS_DETERMINISTIC_DEBUG=1
./target/release/mplora-server
```

Output format:
```
[DEBUG] Seed: label=router, hash=a1b2c3d4...
[DEBUG] Seed: label=generator, hash=e5f6g7h8...
[DEBUG] Kernel: name=fused_mlp, params_hash=12345678...
[DEBUG] Adapter: id=5, gate=0.7531 (q15=24698)
[DEBUG] Buffer: name=input, size=16384 bytes
[DEBUG] Kernel: name=fused_qkv, params_hash=abcdef12...
```

**Security**: No tensor data is logged, only:
- HKDF labels and derived seed hashes
- Kernel names and hashed parameters
- Buffer names and sizes
- Adapter IDs and quantized gates

### aos-cli kernels --trace

Generate replayable trace from a request:

```bash
aos-cli kernels --trace request.json > trace.log
```

Use cases:
- Verify determinism across nodes
- Debug router decisions
- Reproduce specific inference runs
- Audit seed derivation chains

### aos-cli replay --diff

Compare two inference runs bit-by-bit:

```bash
aos-cli replay --diff run1.bundle run2.bundle
```

Output:
```
📊 Reproducibility Report
========================

Bundle A: checkpoint_001.bundle
Bundle B: checkpoint_002.bundle

Tokens sampled: 50
Exact matches: 48 (96%)
Bit differences: 127 bits total
Hamming distance (avg): 2.54 bits/token

Top divergences:
  Token 23: 15 bits differ
  Token 41: 12 bits differ

⚠️  Minor divergences detected (96.0% match)
```

Interpretation:
- **100% match**: Bit-for-bit identical (expected)
- **≥95% match**: Minor divergences (investigate)
- **<95% match**: Significant divergences (determinism violation)

---

## Performance Counter Semantics

### Event Schema

`kernel.profile` events conform to:

```json
{
  "ts": "2025-10-07T12:34:56.789Z",
  "device": "Apple M3 Max",
  "kernel": "fused_attention",
  "available": true,
  "counters": {
    "threads": 32768,
    "occupancy": 87,
    "mem_read": 16777216,
    "mem_write": 4194304
  }
}
```

### Counter Definitions

| Counter | Unit | Description |
|---------|------|-------------|
| `threads` | count | Total threads dispatched |
| `occupancy` | % | GPU utilization (0-100) |
| `mem_read` | bytes | Memory read from buffers |
| `mem_write` | bytes | Memory written to buffers |

### Availability

Counters require:
- macOS 11+ (Big Sur)
- M1 or newer (performance counters unsupported on Intel)
- MTLCounterSamplingPoint support

When unavailable:
- `available: false`
- All counters return `0`
- Timeseries remain continuous (no gaps)

### Collection Overhead

Performance counter sampling adds:
- **~50-100μs** per dispatch (negligible)
- **~4KB** memory per sample
- No impact on kernel execution time

Sampling is always-on in production for observability.

---

## Runtime Validation

### Buffer Layout Checks

Before every kernel dispatch, `LayoutValidator` verifies:

1. **Size Match**
   - `actual_size == expected_stride * expected_count`
   - Catches dimension mismatches

2. **Alignment**
   - Buffer address is 16-byte aligned
   - Required by Metal performance requirements

Example error:
```
Kernel layout mismatch for tensor 'qkv_weights':
  expected: stride=4, count=512, size=2048
  got: size=1024
```

**No tensor data** is included in error messages.

### Panic Recovery

Metal dispatch calls are wrapped in `catch_unwind`:

```rust
recovery.safe_dispatch(|| {
    encoder.dispatch_thread_groups(grid_size, threadgroup_size);
    Ok(())
})?;
```

On panic:
1. Device marked as `degraded`
2. Command queue destroyed
3. Error logged (no tensor data)
4. Requires explicit recovery before next dispatch

Recovery:
```rust
recovery.attempt_recovery(&device)?;
```

### Multi-GPU Selection

Static device selection via environment:

```bash
# List available GPUs
system_profiler SPDisplaysDataType | grep Chipset

# Select specific GPU
export AOS_GPU_INDEX=1
./target/release/mplora-server
```

Output:
```
Selected GPU 1: Apple M3 Max
```

---

## Testing Strategy

### Regression Tests

`tests/kernel_regression.rs`:
- Detects architecture (M1-M4)
- Loads baseline from `metal/baselines/<arch>.json`
- Runs 1000 iterations per kernel
- Asserts no regression >8%

Run manually:
```bash
cargo test --test kernel_regression -- --nocapture
```

Update baselines:
```bash
UPDATE_BASELINES=1 cargo test --test kernel_regression
```

### Layout Tests

`tests/kernel_layout.rs`:
- Positive: valid layouts pass
- Negative: size mismatches detected
- Negative: misalignment detected

### Profiling Tests

`tests/kernel_profile.rs`:
- Profiler creation succeeds
- Event structure correct
- Unavailable counters return zeros
- JSON serialization valid

### Determinism Tests

`tests/determinism_two_node.rs`:
- Metallib hash consistency
- Seed derivation determinism
- Zero-diff outputs verification

---

## Troubleshooting

### Hash Mismatch in CI

**Symptom**: CI fails with "Hash mismatch: code has X, built Y"

**Cause**: `METALLIB_HASH` constant outdated after kernel changes

**Fix**:
```bash
cd metal
bash ci_build.sh
# Copy printed hash
cd ../crates/mplora-kernel-mtl/src
# Update METALLIB_HASH in lib.rs
git commit -am "Update metallib hash after kernel changes"
```

### Performance Regression

**Symptom**: `kernel_regression` test fails with ">8% regression"

**Investigation**:
1. Check if real performance issue or environmental variance
2. Run on dedicated hardware (no background processes)
3. Compare with multiple runs

**If legitimate**:
```bash
# Update baseline
UPDATE_BASELINES=1 cargo test --test kernel_regression
```

### Device Marked Degraded

**Symptom**: "Device degraded - recovery required"

**Cause**: Kernel panic during dispatch

**Recovery**:
```rust
// In worker code
if kernels.recovery().is_degraded() {
    let device = Device::system_default()?;
    kernels.recovery_mut().attempt_recovery(&device)?;
}
```

**Prevention**: Review kernel dispatch for buffer size mismatches

---

## Performance Optimization Guide

### Baseline Targets (M3 Max)

| Metric | Target | Current |
|--------|--------|---------|
| Tokens/sec | ≥40 | 42 |
| MLP kernel | ≤8ms | 7.8ms |
| QKV kernel | ≤6ms | 5.9ms |
| Flash Attention | ≤4ms | 3.2ms |
| Router overhead | ≤8% | 8% |

### Optimization Checklist

- [ ] Threadgroup size matches hardware (16x16 or 32x32)
- [ ] Buffers aligned to 16 bytes
- [ ] No synchronous CPU-GPU transfers in hot path
- [ ] LoRA ranks kept minimal (≤32)
- [ ] K-sparse parameter tuned (K=3 optimal for most workloads)

### Profiling Workflow

1. Enable profiling: `available: true` in telemetry
2. Identify bottleneck kernels (`occupancy < 80%`)
3. Adjust threadgroup sizes
4. Re-run regression tests
5. Update baselines if improved

---

## Appendix: Event Type Reference

### kernel.profile
- **Purpose**: GPU performance metrics per dispatch
- **Frequency**: Every kernel invocation (or sampled)
- **Schema**: `KernelProfileEvent` (see above)

### adapter.vram_bytes
- **Purpose**: Per-adapter memory attribution
- **Frequency**: On load/evict
- **Schema**:
  ```json
  {
    "adapter_id": 5,
    "vram_bytes": 16777216,
    "includes_kv_cache": true
  }
  ```

### adapter.loaded / adapter.evict / adapter.reload
- **Purpose**: Adapter lifecycle tracking
- **Frequency**: On state transitions
- **Schema**: Standard event wrapper with adapter_id
