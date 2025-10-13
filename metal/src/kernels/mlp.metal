// AdapterOS MLP Kernel
// Fused MLP with SwiGLU activation, LoRA support, and bias
//
// Features:
// - SwiGLU activation (SiLU gate + linear up)
// - LoRA (Low-Rank Adaptation) support
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - SwiGLU: https://arxiv.org/abs/2002.05202
// - LoRA: https://arxiv.org/abs/2106.09685

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// Fused MLP kernel with SwiGLU activation, LoRA support, and bias
kernel void fused_mlp(
    constant MlpParams& params,         // All MLP parameters in a single struct
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint hidden_idx = gid.y;
    uint intermediate_idx = gid.z;
    
    // Load input value
    float input_val = params.input[batch_idx * params.lora_config.rank + hidden_idx];
    
    // Compute gate projection with LoRA
    float gate_val = 0.0f;
    for (uint i = 0; i < params.lora_config.rank; i++) {
        float base_weight = params.gate_weight[hidden_idx * params.lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < params.ring_buffer.top_k; k++) {
            uint adapter_idx = params.ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
            
            if (adapter_idx < params.lora_config.rank) {
                float lora_a = params.gate_lora_a[hidden_idx * params.lora_config.rank + adapter_idx];
                float lora_b = params.gate_lora_b[adapter_idx * params.lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        gate_val += input_val * (base_weight + lora_delta);
    }
    
    // Add gate bias if provided
    if (params.gate_bias != nullptr) {
        gate_val += params.gate_bias[intermediate_idx];
    }
    
    // Apply SiLU activation
    float gate_activated = deterministic_silu(gate_val);
    
    // Apply dropout if enabled
    if (params.lora_config.dropout_rate > 0.0f) {
        float dropout_mask = deterministic_dropout(params.dropout_seed, batch_idx * 1000 + hidden_idx, params.lora_config.dropout_rate);
        gate_activated *= dropout_mask;
    }
    
    // Compute up projection with LoRA
    float up_val = 0.0f;
    for (uint i = 0; i < params.lora_config.rank; i++) {
        float base_weight = params.up_weight[hidden_idx * params.lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < params.ring_buffer.top_k; k++) {
            uint adapter_idx = params.ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
            
            if (adapter_idx < params.lora_config.rank) {
                float lora_a = params.up_lora_a[hidden_idx * params.lora_config.rank + adapter_idx];
                float lora_b = params.up_lora_b[adapter_idx * params.lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        up_val += input_val * (base_weight + lora_delta);
    }
    
    // Add up bias if provided
    if (params.up_bias != nullptr) {
        up_val += params.up_bias[intermediate_idx];
    }
    
    // Element-wise multiplication (SwiGLU)
    float intermediate_val = gate_activated * up_val;
    
    // Compute down projection with LoRA
    float down_val = 0.0f;
    for (uint i = 0; i < params.lora_config.rank; i++) {
        float base_weight = params.down_weight[intermediate_idx * params.lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < params.ring_buffer.top_k; k++) {
            uint adapter_idx = params.ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
            
            if (adapter_idx < params.lora_config.rank) {
                float lora_a = params.down_lora_a[intermediate_idx * params.lora_config.rank + adapter_idx];
                float lora_b = params.down_lora_b[adapter_idx * params.lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        down_val += intermediate_val * (base_weight + lora_delta);
    }
    
    // Add down bias if provided
    if (params.down_bias != nullptr) {
        down_val += params.down_bias[hidden_idx];
    }
    
    // Store params.output
    params.output[batch_idx * params.lora_config.rank + hidden_idx] = down_val;
}
