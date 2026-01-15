// adapterOS Flash Attention Kernel
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
    constant FlashAttentionParams& params,
    uint3 gid [[thread_position_in_grid]]
) {
    uint batch_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;

    uint num_heads = params.gqa_config.num_attention_heads;
    uint num_kv_heads = max(params.gqa_config.num_key_value_heads, 1u);
    uint head_dim = params.gqa_config.head_dim;
    uint seq_len = max(params.sequence_length, 1u);

    if (batch_idx >= params.batch_size || head_idx >= num_heads || dim_idx >= seq_len) {
        return;
    }

    // Each thread handles an entire head vector for one (batch, head, sequence) triple.
    uint query_seq = dim_idx; // Re-purpose dim_idx as sequence index via dispatch grid

    uint heads_per_kv = max(num_heads / num_kv_heads, 1u);
    uint kv_head = min(num_kv_heads - 1, head_idx / heads_per_kv);

    uint head_stride = seq_len * head_dim;
    uint q_offset = (((batch_idx * num_heads) + head_idx) * seq_len + query_seq) * head_dim;
    uint kv_base = ((batch_idx * num_kv_heads) + kv_head) * head_stride;

    device const float* q_vec = params.q + q_offset;
    device float* out_vec = params.output + q_offset;

    float scale = params.gqa_config.attention_scale > 0.0f
        ? params.gqa_config.attention_scale
        : 1.0f / sqrt((float) head_dim);

    // Find max score for numerical stability
    float max_score = -INFINITY;
    for (uint kv_seq = 0; kv_seq < seq_len; kv_seq++) {
        device const float* k_vec = params.k + kv_base + kv_seq * head_dim;
        float dot = 0.0f;
        for (uint d = 0; d < head_dim; d++) {
            dot += q_vec[d] * k_vec[d];
        }
        float scaled = dot * scale;
        max_score = max(max_score, scaled);
    }

    // Clear output accumulator
    for (uint d = 0; d < head_dim; d++) {
        out_vec[d] = 0.0f;
    }

    float denom = 0.0f;

    for (uint kv_seq = 0; kv_seq < seq_len; kv_seq++) {
        device const float* k_vec = params.k + kv_base + kv_seq * head_dim;
        device const float* v_vec = params.v + kv_base + kv_seq * head_dim;

        float dot = 0.0f;
        for (uint d = 0; d < head_dim; d++) {
            dot += q_vec[d] * k_vec[d];
        }

        float weight = exp(dot * scale - max_score);
        denom += weight;

        for (uint d = 0; d < head_dim; d++) {
            out_vec[d] += weight * v_vec[d];
        }
    }

    float inv_denom = denom > 0.0f ? 1.0f / denom : 0.0f;
    for (uint d = 0; d < head_dim; d++) {
        out_vec[d] *= inv_denom;
    }
}
