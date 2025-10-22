// AdapterOS Attention Kernel
// Fused QKV with Grouped Query Attention (GQA) support
//
// Features:
// - Grouped Query Attention (GQA) for memory efficiency
// - LoRA (Low-Rank Adaptation) support
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - GQA: https://arxiv.org/abs/2305.13245
// - LoRA: https://arxiv.org/abs/2106.09685

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// Fused QKV kernel with GQA support
kernel void fused_qkv_gqa(
    constant AttentionParams& params,   // All attention parameters in a single struct
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;

    if (batch_idx >= params.batch_size) {
        return;
    }

    const bool has_q_lora =
        (params.q_lora_a != nullptr) && (params.q_lora_b != nullptr) && params.lora_config.rank > 0;
    const bool has_k_lora =
        (params.k_lora_a != nullptr) && (params.k_lora_b != nullptr) && params.lora_config.rank > 0;
    const bool has_v_lora =
        (params.v_lora_a != nullptr) && (params.v_lora_b != nullptr) && params.lora_config.rank > 0;

    // Load input value
    float input_val = params.input[batch_idx * params.gqa_config.hidden_size + dim_idx];
    
    // Compute Q projection with LoRA (for all attention heads)
    if (head_idx < params.gqa_config.num_attention_heads) {
        float q_val = 0.0f;
        for (uint i = 0; i < params.gqa_config.hidden_size; i++) {
            float base_weight = params.q_weight[dim_idx * params.gqa_config.hidden_size + i];
            if (has_q_lora) {
                float lora_delta = 0.0f;
                for (uint k = 0; k < params.ring_buffer.top_k && k < params.max_adapters; k++) {
                    uint adapter_idx = params.ring_buffer.adapter_indices[k];
                    if (adapter_idx >= params.lora_config.rank) {
                        continue;
                    }
                    float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
                    float lora_a = params.q_lora_a[dim_idx * params.lora_config.rank + adapter_idx];
                    float lora_b = params.q_lora_b[adapter_idx * params.gqa_config.hidden_size + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
                base_weight += lora_delta;
            }
            q_val += input_val * base_weight;
        }
        
        // Store Q output
        uint q_offset = batch_idx * params.gqa_config.num_attention_heads * params.gqa_config.head_dim +
                       head_idx * params.gqa_config.head_dim + dim_idx;
        params.q_output[q_offset] = q_val;
    }
    
    // Compute K projection with LoRA (only for key-value heads)
    if (head_idx < params.gqa_config.num_key_value_heads) {
        float k_val = 0.0f;
        for (uint i = 0; i < params.gqa_config.kv_width; i++) {
            float base_weight = params.k_weight[dim_idx * params.gqa_config.kv_width + i];
            if (has_k_lora) {
                float lora_delta = 0.0f;
                for (uint k = 0; k < params.ring_buffer.top_k && k < params.max_adapters; k++) {
                    uint adapter_idx = params.ring_buffer.adapter_indices[k];
                    if (adapter_idx >= params.lora_config.rank) {
                        continue;
                    }
                    float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
                    float lora_a = params.k_lora_a[dim_idx * params.lora_config.rank + adapter_idx];
                    float lora_b = params.k_lora_b[adapter_idx * params.gqa_config.kv_width + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
                base_weight += lora_delta;
            }
            k_val += input_val * base_weight;
        }
        
        // Store K output
        uint k_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                       head_idx * params.gqa_config.head_dim + dim_idx;
        params.k_output[k_offset] = k_val;
    }
    
    // Compute V projection with LoRA (only for key-value heads)
    if (head_idx < params.gqa_config.num_key_value_heads) {
        float v_val = 0.0f;
        for (uint i = 0; i < params.gqa_config.kv_width; i++) {
            float base_weight = params.v_weight[dim_idx * params.gqa_config.kv_width + i];
            if (has_v_lora) {
                float lora_delta = 0.0f;
                for (uint k = 0; k < params.ring_buffer.top_k && k < params.max_adapters; k++) {
                    uint adapter_idx = params.ring_buffer.adapter_indices[k];
                    if (adapter_idx >= params.lora_config.rank) {
                        continue;
                    }
                    float gate_q15 = q15_to_float(params.ring_buffer.gates[k]);
                    float lora_a = params.v_lora_a[dim_idx * params.lora_config.rank + adapter_idx];
                    float lora_b = params.v_lora_b[adapter_idx * params.gqa_config.kv_width + i];
                    lora_delta += gate_q15 * lora_a * lora_b;
                }
                base_weight += lora_delta;
            }
            v_val += input_val * base_weight;
        }
        
        // Store V output
        uint v_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                       head_idx * params.gqa_config.head_dim + dim_idx;
        params.v_output[v_offset] = v_val;
    }
}
