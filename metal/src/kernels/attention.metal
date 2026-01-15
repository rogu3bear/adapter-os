// adapterOS Attention Kernel
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
static inline float dot_base_projection(
    device const float* __restrict x,
    device const float* __restrict W,
    uint hidden_size,
    uint out_j,
    uint out_width
) {
    float acc = 0.0f;
    for (uint i = 0; i < hidden_size; ++i) {
        acc = fma(x[i], W[i * out_width + out_j], acc);
    }
    return acc;
}
kernel void fused_qkv_gqa(
    constant AttentionParams& params,   // All attention parameters in a single struct
    
    uint3 gid [[thread_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]],
    uint3 lid [[thread_position_in_threadgroup]]
) {
    const uint batch_idx = gid.x;
    const uint head_idx = gid.y;
    const uint dim_idx = gid.z; // within head_dim

    if (batch_idx >= params.batch_size) return;

    const uint hidden_size = params.gqa_config.hidden_size;
    const uint head_dim = params.gqa_config.head_dim;
    const uint kv_width = params.gqa_config.kv_width;
    const uint rank = params.lora_config.rank;

    const bool has_q_lora = (params.q_lora_a && params.q_lora_b && rank > 0);
    const bool has_k_lora = (params.k_lora_a && params.k_lora_b && rank > 0);
    const bool has_v_lora = (params.v_lora_a && params.v_lora_b && rank > 0);

    // Input vector for this token
    device const float* x = params.input + batch_idx * hidden_size;

    // Precompute x^T A per adapter for Q/K/V independently
    thread float q_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    thread float k_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    thread float v_ax[MAX_ADAPTER_SLOTS * MAX_LORA_RANK];
    if (has_q_lora) { compute_lora_ax_thread(params.q_lora_a, x, hidden_size, rank, params.ring_buffer, params.max_adapters, q_ax); }
    if (has_k_lora) { compute_lora_ax_thread(params.k_lora_a, x, hidden_size, rank, params.ring_buffer, params.max_adapters, k_ax); }
    if (has_v_lora) { compute_lora_ax_thread(params.v_lora_a, x, hidden_size, rank, params.ring_buffer, params.max_adapters, v_ax); }

    // Q projection (for full attention heads)
    if (head_idx < params.gqa_config.num_attention_heads) {
        const uint out_j = head_idx * head_dim + dim_idx; // index into hidden_size
        float q_val = dot_base_projection(x, params.q_weight, hidden_size, out_j, hidden_size);
        if (has_q_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                uint b_base = adapter_id * rank * hidden_size;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    q_val = fma(
                        q_ax[kslot * MAX_LORA_RANK + r],
                        params.q_lora_b[b_base + r * hidden_size + out_j],
                        q_val
                    );
                }
            }
        }
        const uint q_offset = batch_idx * params.gqa_config.num_attention_heads * head_dim + head_idx * head_dim + dim_idx;
        params.q_output[q_offset] = q_val;
    }

    // K projection (only for KV heads)
    if (head_idx < params.gqa_config.num_key_value_heads) {
        const uint out_j = head_idx * head_dim + dim_idx; // index into kv_width
        float k_val = dot_base_projection(x, params.k_weight, hidden_size, out_j, kv_width);
        if (has_k_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                uint b_base = adapter_id * rank * kv_width;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    k_val = fma(
                        k_ax[kslot * MAX_LORA_RANK + r],
                        params.k_lora_b[b_base + r * kv_width + out_j],
                        k_val
                    );
                }
            }
        }
        const uint k_offset = batch_idx * params.gqa_config.num_key_value_heads * head_dim + head_idx * head_dim + dim_idx;
        params.k_output[k_offset] = k_val;
    }

    // V projection (only for KV heads)
    if (head_idx < params.gqa_config.num_key_value_heads) {
        const uint out_j = head_idx * head_dim + dim_idx; // index into kv_width
        float v_val = dot_base_projection(x, params.v_weight, hidden_size, out_j, kv_width);
        if (has_v_lora) {
            const uint K = min(params.ring_buffer.top_k, (uint)MAX_ADAPTER_SLOTS);
            for (uint kslot = 0; kslot < K; ++kslot) {
                uint adapter_id = params.ring_buffer.adapter_indices[kslot];
                if (adapter_id >= params.max_adapters) { continue; }
                uint b_base = adapter_id * rank * kv_width;
                for (uint r = 0; r < rank && r < (uint)MAX_LORA_RANK; ++r) {
                    v_val = fma(
                        v_ax[kslot * MAX_LORA_RANK + r],
                        params.v_lora_b[b_base + r * kv_width + out_j],
                        v_val
                    );
                }
            }
        }
        const uint v_offset = batch_idx * params.gqa_config.num_key_value_heads * head_dim + head_idx * head_dim + dim_idx;
        params.v_output[v_offset] = v_val;
    }
}
