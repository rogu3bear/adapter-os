// AdapterOS Metal Kernels
// Production-optimized Metal kernels for Qwen2.5-7B-Instruct
//
// Features:
// - Fused MLP with SwiGLU activation and LoRA support
// - Fused QKV with Grouped Query Attention (GQA)
// - Flash Attention for memory efficiency
// - Deterministic math operations
// - Optimized memory access patterns
//
// References:
// - SwiGLU: https://arxiv.org/abs/2002.05202
// - GQA: https://arxiv.org/abs/2305.13245
// - Flash Attention: https://arxiv.org/abs/2205.14135
// - LoRA: https://arxiv.org/abs/2106.09685
// - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

// Disable fast-math and force IEEE 754 compliance for determinism
#pragma clang fp contract(off)

#include <metal_stdlib>
using namespace metal;

#include "common.metal"

// All configuration structures and helper functions are in common.metal

// Fused MLP kernel with SwiGLU activation, LoRA support, and bias
kernel void fused_mlp(
    device const float* input,           // [batch_size, hidden_size]
    device const float* gate_weight,      // [hidden_size, intermediate_size]
    device const float* up_weight,        // [hidden_size, intermediate_size]
    device const float* down_weight,      // [intermediate_size, hidden_size]
    device const float* gate_bias,        // [intermediate_size] (nullable)
    device const float* up_bias,          // [intermediate_size] (nullable)
    device const float* down_bias,        // [hidden_size] (nullable)
    device float* output,                 // [batch_size, hidden_size]
    
    // LoRA parameters
    device const float* gate_lora_a,      // [hidden_size, rank]
    device const float* gate_lora_b,      // [rank, intermediate_size]
    device const float* up_lora_a,        // [hidden_size, rank]
    device const float* up_lora_b,        // [rank, intermediate_size]
    device const float* down_lora_a,      // [intermediate_size, rank]
    device const float* down_lora_b,      // [rank, hidden_size]
    
    // Configuration
    constant LoraConfig& lora_config,
    constant RingBuffer& ring_buffer,
    constant uint& dropout_seed,          // Seed for deterministic dropout
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    uint batch_idx = gid.x;
    uint hidden_idx = gid.y;
    uint intermediate_idx = gid.z;
    
    // Load input value
    float input_val = input[batch_idx * lora_config.rank + hidden_idx];
    
    // Compute gate projection with LoRA
    float gate_val = 0.0f;
    for (uint i = 0; i < lora_config.rank; i++) {
        float base_weight = gate_weight[hidden_idx * lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(ring_buffer.gates[k]);
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = gate_lora_a[hidden_idx * lora_config.rank + adapter_idx];
                float lora_b = gate_lora_b[adapter_idx * lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        gate_val += input_val * (base_weight + lora_delta);
    }
    
    // Add gate bias if provided
    if (gate_bias != nullptr) {
        gate_val += gate_bias[intermediate_idx];
    }
    
    // Apply SiLU activation
    float gate_activated = deterministic_silu(gate_val);
    
    // Apply dropout if enabled
    if (lora_config.dropout_rate > 0.0f) {
        float dropout_mask = deterministic_dropout(dropout_seed, batch_idx * 1000 + hidden_idx, lora_config.dropout_rate);
        gate_activated *= dropout_mask;
    }
    
    // Compute up projection with LoRA
    float up_val = 0.0f;
    for (uint i = 0; i < lora_config.rank; i++) {
        float base_weight = up_weight[hidden_idx * lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(ring_buffer.gates[k]);
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = up_lora_a[hidden_idx * lora_config.rank + adapter_idx];
                float lora_b = up_lora_b[adapter_idx * lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        up_val += input_val * (base_weight + lora_delta);
    }
    
    // Add up bias if provided
    if (up_bias != nullptr) {
        up_val += up_bias[intermediate_idx];
    }
    
    // Element-wise multiplication (SwiGLU)
    float intermediate_val = gate_activated * up_val;
    
    // Compute down projection with LoRA
    float down_val = 0.0f;
    for (uint i = 0; i < lora_config.rank; i++) {
        float base_weight = down_weight[intermediate_idx * lora_config.rank + i];
        float lora_delta = 0.0f;
        
        // Apply LoRA for active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            uint adapter_idx = ring_buffer.adapter_indices[k];
            float gate_q15 = q15_to_float(ring_buffer.gates[k]);
            
            if (adapter_idx < lora_config.rank) {
                float lora_a = down_lora_a[intermediate_idx * lora_config.rank + adapter_idx];
                float lora_b = down_lora_b[adapter_idx * lora_config.rank + i];
                lora_delta += gate_q15 * lora_a * lora_b;
            }
        }
        
        down_val += intermediate_val * (base_weight + lora_delta);
    }
    
    // Add down bias if provided
    if (down_bias != nullptr) {
        down_val += down_bias[hidden_idx];
    }
    
    // Apply final dropout if enabled
    if (lora_config.dropout_rate > 0.0f) {
        float dropout_mask = deterministic_dropout(dropout_seed + 1, batch_idx * 1000 + hidden_idx, lora_config.dropout_rate);
        down_val *= dropout_mask;
    }
    
    // Store output
    output[batch_idx * lora_config.rank + hidden_idx] = down_val;
}

// Fused QKV kernel with GQA support
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
    
    // Load input value
    float input_val = input[batch_idx * gqa_config.hidden_size + dim_idx];
    
    // Compute Q projection with LoRA (for all attention heads)
    if (head_idx < gqa_config.num_attention_heads) {
        float q_val = 0.0f;
        for (uint i = 0; i < gqa_config.hidden_size; i++) {
            float base_weight = q_weight[dim_idx * gqa_config.hidden_size + i];
            float lora_delta = 0.0f;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = q15_to_float(ring_buffer.gates[k]);
                
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
        float k_val = 0.0f;
        for (uint i = 0; i < gqa_config.kv_width; i++) {
            float base_weight = k_weight[dim_idx * gqa_config.kv_width + i];
            float lora_delta = 0.0f;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = q15_to_float(ring_buffer.gates[k]);
                
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
        float v_val = 0.0f;
        for (uint i = 0; i < gqa_config.kv_width; i++) {
            float base_weight = v_weight[dim_idx * gqa_config.kv_width + i];
            float lora_delta = 0.0f;
            
            // Apply LoRA for active adapters
            for (uint k = 0; k < ring_buffer.top_k; k++) {
                uint adapter_idx = ring_buffer.adapter_indices[k];
                float gate_q15 = q15_to_float(ring_buffer.gates[k]);
                
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

// Flash Attention kernel for memory-efficient attention computation
kernel void flash_attention(
    device const float* q,               // [batch_size, num_heads, seq_len, head_dim]
    device const float* k,               // [batch_size, num_kv_heads, seq_len, head_dim]
    device const float* v,               // [batch_size, num_kv_heads, seq_len, head_dim]
    device float* output,                // [batch_size, num_heads, seq_len, head_dim]
    
    constant GqaConfig& gqa_config,
    
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
    for (uint kv_head = 0; kv_head < gqa_config.num_key_value_heads; kv_head++) {
        for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
            float score = 0.0f;
            for (uint dim = 0; dim < gqa_config.head_dim; dim++) {
                uint q_offset = batch_idx * gqa_config.num_attention_heads * gqa_config.head_dim +
                               head_idx * gqa_config.head_dim + dim;
                uint k_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                               kv_head * gqa_config.head_dim + dim;
                
                score += q[q_offset] * k[k_offset];
            }
            score /= sqrt(float(gqa_config.head_dim));
            max_score = max(max_score, score);
        }
    }
    
    // Compute attention weights
    for (uint kv_head = 0; kv_head < gqa_config.num_key_value_heads; kv_head++) {
        for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
            float score = 0.0f;
            for (uint dim = 0; dim < gqa_config.head_dim; dim++) {
                uint q_offset = batch_idx * gqa_config.num_attention_heads * gqa_config.head_dim +
                               head_idx * gqa_config.head_dim + dim;
                uint k_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                               kv_head * gqa_config.head_dim + dim;
                
                score += q[q_offset] * k[k_offset];
            }
            score = (score / sqrt(float(gqa_config.head_dim))) - max_score;
            float exp_score = exp(score);
            sum_exp += exp_score;
        }
    }
    
    // Compute output
    for (uint dim = 0; dim < gqa_config.head_dim; dim++) {
        float output_val = 0.0f;
        
        for (uint kv_head = 0; kv_head < gqa_config.num_key_value_heads; kv_head++) {
            for (uint kv_seq = 0; kv_seq < seq_idx + 1; kv_seq++) {
                float score = 0.0f;
                for (uint d = 0; d < gqa_config.head_dim; d++) {
                    uint q_offset = batch_idx * gqa_config.num_attention_heads * gqa_config.head_dim +
                                   head_idx * gqa_config.head_dim + d;
                    uint k_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                                   kv_head * gqa_config.head_dim + d;
                    
                    score += q[q_offset] * k[k_offset];
                }
                score = (score / sqrt(float(gqa_config.head_dim))) - max_score;
                float attention_weight = exp(score) / sum_exp;
                
                uint v_offset = batch_idx * gqa_config.num_key_value_heads * gqa_config.head_dim +
                               kv_head * gqa_config.head_dim + dim;
                output_val += attention_weight * v[v_offset];
            }
        }
        
        uint output_offset = batch_idx * gqa_config.num_attention_heads * gqa_config.head_dim +
                           head_idx * gqa_config.head_dim + dim;
        output[output_offset] = output_val;
    }
}
