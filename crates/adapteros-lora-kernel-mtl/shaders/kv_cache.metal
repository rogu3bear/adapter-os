//! KV cache Metal shaders for efficient autoregressive generation
//!
//! These kernels handle updating and reading from the KV cache buffers.
//! They are designed to work with GQA (Grouped Query Attention) configurations.

#include <metal_stdlib>
using namespace metal;

/// KV cache configuration passed from host
struct KVCacheParams {
    uint batch_size;        // Batch size
    uint num_kv_heads;      // Number of KV heads (for GQA)
    uint head_dim;          // Dimension per head
    uint max_seq_len;       // Maximum sequence length
    uint current_seq_pos;   // Current sequence position
    uint num_new_tokens;    // Number of new tokens to append
};

/// Update KV cache with new key/value tensors
///
/// Appends new K/V tensors to the cache at the current sequence position.
/// Layout: [batch, num_kv_heads, seq_len, head_dim]
kernel void kv_cache_update(
    device const float* new_keys [[buffer(0)]],
    device const float* new_values [[buffer(1)]],
    device float* key_cache [[buffer(2)]],
    device float* value_cache [[buffer(3)]],
    constant KVCacheParams& params [[buffer(4)]],
    uint3 gid [[thread_position_in_grid]]
) {
    // Thread grid: [num_new_tokens, num_kv_heads, head_dim]
    uint token_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;

    if (token_idx >= params.num_new_tokens ||
        head_idx >= params.num_kv_heads ||
        dim_idx >= params.head_dim) {
        return;
    }

    // Calculate offsets
    // New tensor offset: [token_idx, head_idx, dim_idx]
    uint new_offset = token_idx * params.num_kv_heads * params.head_dim +
                      head_idx * params.head_dim +
                      dim_idx;

    // Cache offset: [seq_pos + token_idx, head_idx, dim_idx]
    uint cache_seq_pos = params.current_seq_pos + token_idx;
    uint cache_offset = cache_seq_pos * params.num_kv_heads * params.head_dim +
                        head_idx * params.head_dim +
                        dim_idx;

    // Copy to cache
    key_cache[cache_offset] = new_keys[new_offset];
    value_cache[cache_offset] = new_values[new_offset];
}

/// Read KV cache slice for attention computation
///
/// Extracts K/V tensors from cache for the given sequence range.
/// Useful for sliding window attention or partial sequence retrieval.
kernel void kv_cache_read(
    device const float* key_cache [[buffer(0)]],
    device const float* value_cache [[buffer(1)]],
    device float* keys_out [[buffer(2)]],
    device float* values_out [[buffer(3)]],
    constant KVCacheParams& params [[buffer(4)]],
    uint3 gid [[thread_position_in_grid]]
) {
    // Thread grid: [current_seq_pos, num_kv_heads, head_dim]
    uint seq_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;

    if (seq_idx >= params.current_seq_pos ||
        head_idx >= params.num_kv_heads ||
        dim_idx >= params.head_dim) {
        return;
    }

    // Calculate offset
    uint offset = seq_idx * params.num_kv_heads * params.head_dim +
                  head_idx * params.head_dim +
                  dim_idx;

    // Copy from cache
    keys_out[offset] = key_cache[offset];
    values_out[offset] = value_cache[offset];
}

/// Flash attention with KV cache support
///
/// Computes attention for new query tokens against cached key/value sequence.
/// Q: [batch, num_heads, num_new_tokens, head_dim]
/// K_cache: [batch, num_kv_heads, seq_len, head_dim]
/// V_cache: [batch, num_kv_heads, seq_len, head_dim]
/// Output: [batch, num_heads, num_new_tokens, head_dim]
///
/// Supports GQA where num_heads > num_kv_heads with head sharing.
kernel void flash_attention_cached(
    device const float* query [[buffer(0)]],
    device const float* key_cache [[buffer(1)]],
    device const float* value_cache [[buffer(2)]],
    device float* output [[buffer(3)]],
    constant KVCacheParams& params [[buffer(4)]],
    constant float& scale [[buffer(5)]],          // 1/sqrt(head_dim)
    constant uint& num_query_heads [[buffer(6)]], // Number of query heads
    uint2 gid [[thread_position_in_grid]],
    uint2 tid [[thread_position_in_threadgroup]],
    uint2 tgid [[threadgroup_position_in_grid]]
) {
    // Thread grid: [num_new_tokens, num_query_heads]
    uint token_idx = gid.x;
    uint query_head_idx = gid.y;

    if (token_idx >= params.num_new_tokens || query_head_idx >= num_query_heads) {
        return;
    }

    // Map query head to KV head (for GQA)
    uint kv_head_idx = query_head_idx * params.num_kv_heads / num_query_heads;

    // Compute attention scores and output for this query position
    // Using online softmax for numerical stability

    float max_score = -INFINITY;
    float sum_exp = 0.0f;

    // First pass: compute max score
    for (uint seq_idx = 0; seq_idx < params.current_seq_pos; seq_idx++) {
        float score = 0.0f;

        // Dot product: Q[token_idx, query_head_idx, :] @ K[seq_idx, kv_head_idx, :]
        for (uint d = 0; d < params.head_dim; d++) {
            uint q_offset = token_idx * num_query_heads * params.head_dim +
                           query_head_idx * params.head_dim + d;
            uint k_offset = seq_idx * params.num_kv_heads * params.head_dim +
                           kv_head_idx * params.head_dim + d;
            score += query[q_offset] * key_cache[k_offset];
        }
        score *= scale;
        max_score = max(max_score, score);
    }

    // Second pass: compute softmax denominator and weighted sum
    float output_accum[128]; // Assuming max head_dim = 128
    for (uint d = 0; d < params.head_dim; d++) {
        output_accum[d] = 0.0f;
    }

    for (uint seq_idx = 0; seq_idx < params.current_seq_pos; seq_idx++) {
        float score = 0.0f;

        // Recompute dot product
        for (uint d = 0; d < params.head_dim; d++) {
            uint q_offset = token_idx * num_query_heads * params.head_dim +
                           query_head_idx * params.head_dim + d;
            uint k_offset = seq_idx * params.num_kv_heads * params.head_dim +
                           kv_head_idx * params.head_dim + d;
            score += query[q_offset] * key_cache[k_offset];
        }
        score *= scale;

        float exp_score = exp(score - max_score);
        sum_exp += exp_score;

        // Accumulate weighted values
        for (uint d = 0; d < params.head_dim; d++) {
            uint v_offset = seq_idx * params.num_kv_heads * params.head_dim +
                           kv_head_idx * params.head_dim + d;
            output_accum[d] += exp_score * value_cache[v_offset];
        }
    }

    // Normalize and write output
    float inv_sum = 1.0f / (sum_exp + 1e-6f);
    for (uint d = 0; d < params.head_dim; d++) {
        uint out_offset = token_idx * num_query_heads * params.head_dim +
                         query_head_idx * params.head_dim + d;
        output[out_offset] = output_accum[d] * inv_sum;
    }
}

/// Clear KV cache (reset all values to zero)
kernel void kv_cache_clear(
    device float* key_cache [[buffer(0)]],
    device float* value_cache [[buffer(1)]],
    constant uint& buffer_size [[buffer(2)]],
    uint gid [[thread_position_in_grid]]
) {
    if (gid >= buffer_size) {
        return;
    }

    key_cache[gid] = 0.0f;
    value_cache[gid] = 0.0f;
}

/// Rotate KV cache for sliding window attention
///
/// Shifts cache contents by N positions, discarding oldest entries.
/// This enables infinite context generation with fixed memory.
kernel void kv_cache_rotate(
    device float* key_cache [[buffer(0)]],
    device float* value_cache [[buffer(1)]],
    constant KVCacheParams& params [[buffer(2)]],
    constant uint& shift_amount [[buffer(3)]],
    uint3 gid [[thread_position_in_grid]]
) {
    // Thread grid: [new_seq_len, num_kv_heads, head_dim]
    uint seq_idx = gid.x;
    uint head_idx = gid.y;
    uint dim_idx = gid.z;

    uint new_seq_len = params.current_seq_pos - shift_amount;

    if (seq_idx >= new_seq_len ||
        head_idx >= params.num_kv_heads ||
        dim_idx >= params.head_dim) {
        return;
    }

    // Calculate offsets
    uint src_seq_idx = seq_idx + shift_amount;
    uint src_offset = src_seq_idx * params.num_kv_heads * params.head_dim +
                      head_idx * params.head_dim +
                      dim_idx;
    uint dst_offset = seq_idx * params.num_kv_heads * params.head_dim +
                      head_idx * params.head_dim +
                      dim_idx;

    // Shift contents
    key_cache[dst_offset] = key_cache[src_offset];
    value_cache[dst_offset] = value_cache[src_offset];
}
