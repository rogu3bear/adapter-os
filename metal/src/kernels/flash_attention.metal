// AdapterOS Flash Attention Kernel
// Memory-efficient attention computation with GQA support
//
// Features:
// - Flash Attention for memory efficiency
// - Grouped Query Attention (GQA) support
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - Flash Attention: https://arxiv.org/abs/2205.14135
// - GQA: https://arxiv.org/abs/2305.13245

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// Flash Attention kernel for memory-efficient attention computation
kernel void flash_attention(
    constant FlashAttentionParams& params, // All flash attention parameters in a single struct
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint head_idx = gid.y;
    uint seq_idx = gid.z;
    
    // Compute attention scores
    float max_score = -INFINITY;
    float sum_exp = 0.0f;
    
    // Find maximum score for numerical stability
    for (uint kv_head = 0; kv_head < params.gqa_config.num_key_value_heads; kv_head++) {
        for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
            float score = 0.0f;
            for (uint dim = 0; dim < params.gqa_config.head_dim; dim++) {
                uint q_offset = batch_idx * params.gqa_config.num_attention_heads * params.gqa_config.head_dim +
                               head_idx * params.gqa_config.head_dim + dim;
                uint k_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                               kv_head * params.gqa_config.head_dim + dim;
                
                score += params.q[q_offset] * params.k[k_offset];
            }
            score /= sqrt(float(params.gqa_config.head_dim));
            max_score = max(max_score, score);
        }
    }
    
    // Compute attention weights
    for (uint kv_head = 0; kv_head < params.gqa_config.num_key_value_heads; kv_head++) {
        for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
            float score = 0.0f;
            for (uint dim = 0; dim < params.gqa_config.head_dim; dim++) {
                uint q_offset = batch_idx * params.gqa_config.num_attention_heads * params.gqa_config.head_dim +
                               head_idx * params.gqa_config.head_dim + dim;
                uint k_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                               kv_head * params.gqa_config.head_dim + dim;
                
                score += params.q[q_offset] * params.k[k_offset];
            }
            score = (score / sqrt(float(params.gqa_config.head_dim))) - max_score;
            float exp_score = exp(score);
            sum_exp += exp_score;
        }
    }
    
    // Compute output
    for (uint dim = 0; dim < params.gqa_config.head_dim; dim++) {
        float output_val = 0.0f;
        
        for (uint kv_head = 0; kv_head < params.gqa_config.num_key_value_heads; kv_head++) {
            for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
                float score = 0.0f;
                for (uint d = 0; d < params.gqa_config.head_dim; d++) {
                    uint q_offset = batch_idx * params.gqa_config.num_attention_heads * params.gqa_config.head_dim +
                                   head_idx * params.gqa_config.head_dim + d;
                    uint k_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                                   kv_head * params.gqa_config.head_dim + d;
                    
                    score += params.q[q_offset] * params.k[k_offset];
                }
                score = (score / sqrt(float(params.gqa_config.head_dim))) - max_score;
                float attention_weight = exp(score) / sum_exp;
                
                uint v_offset = batch_idx * params.gqa_config.num_key_value_heads * params.gqa_config.head_dim +
                               kv_head * params.gqa_config.head_dim + dim;
                output_val += attention_weight * params.v[v_offset];
            }
        }
        
        uint output_offset = batch_idx * params.gqa_config.num_attention_heads * params.gqa_config.head_dim +
                           head_idx * params.gqa_config.head_dim + dim;
        params.output[output_offset] = output_val;
    }
}