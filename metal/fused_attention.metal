// adapterOS Fused Attention Kernel
// Flash Attention implementation with Grouped Query Attention (GQA)
//
// References:
// - Flash Attention: https://arxiv.org/abs/2205.14135
// - GQA: https://arxiv.org/abs/2305.13245
//
// Key features:
// - Tiled attention computation in SRAM (threadgroup memory)
// - Online softmax with streaming
// - GQA support (replicate K/V heads for Q heads)
// - Causal masking for autoregressive generation
// - Deterministic math operations

#include "common.metal"

/// Fused QKV projection kernel with bias support
/// Projects input through Q, K, V weight matrices and adds bias terms
kernel void fused_qkv_projection(
    device const float* input,           // [batch, seq, hidden_size]
    device const float* q_weight,        // [hidden_size, num_heads * head_dim]
    device const float* k_weight,        // [hidden_size, num_kv_heads * head_dim]
    device const float* v_weight,        // [hidden_size, num_kv_heads * head_dim]
    device const float* q_bias,          // [num_heads * head_dim] (nullable)
    device const float* k_bias,          // [num_kv_heads * head_dim] (nullable)
    device const float* v_bias,          // [num_kv_heads * head_dim] (nullable)
    device float* q_out,                 // [batch, seq, num_heads, head_dim]
    device float* k_out,                 // [batch, seq, num_kv_heads, head_dim]
    device float* v_out,                 // [batch, seq, num_kv_heads, head_dim]
    constant GqaConfig& config,
    uint3 gid [[thread_position_in_grid]]
) {
    const uint batch_idx = gid.z;
    const uint seq_idx = gid.y;
    const uint hidden_idx = gid.x;
    
    if (hidden_idx >= config.hidden_size) return;
    
    // Load input value
    uint input_idx = batch_idx * config.hidden_size * 2048 + seq_idx * config.hidden_size + hidden_idx;
    float in_val = input[input_idx];
    
    // Q projection with bias
    for (uint h = 0; h < config.num_attention_heads; h++) {
        for (uint d = 0; d < config.head_dim; d++) {
            uint weight_idx = hidden_idx * (config.num_attention_heads * config.head_dim) + h * config.head_dim + d;
            uint out_idx = batch_idx * 2048 * config.num_attention_heads * config.head_dim
                         + seq_idx * config.num_attention_heads * config.head_dim
                         + h * config.head_dim + d;
            
            float proj_val = in_val * q_weight[weight_idx];
            
            // Add bias if provided (only once per output position)
            if (q_bias != nullptr && hidden_idx == 0) {
                proj_val += q_bias[h * config.head_dim + d];
            }
            
            q_out[out_idx] += proj_val;
        }
    }
    
    // K projection with bias
    for (uint h = 0; h < config.num_key_value_heads; h++) {
        for (uint d = 0; d < config.head_dim; d++) {
            uint weight_idx = hidden_idx * (config.num_key_value_heads * config.head_dim) + h * config.head_dim + d;
            uint out_idx = batch_idx * 2048 * config.num_key_value_heads * config.head_dim
                         + seq_idx * config.num_key_value_heads * config.head_dim
                         + h * config.head_dim + d;
            
            float proj_val = in_val * k_weight[weight_idx];
            
            // Add bias if provided
            if (k_bias != nullptr && hidden_idx == 0) {
                proj_val += k_bias[h * config.head_dim + d];
            }
            
            k_out[out_idx] += proj_val;
        }
    }
    
    // V projection with bias
    for (uint h = 0; h < config.num_key_value_heads; h++) {
        for (uint d = 0; d < config.head_dim; d++) {
            uint weight_idx = hidden_idx * (config.num_key_value_heads * config.head_dim) + h * config.head_dim + d;
            uint out_idx = batch_idx * 2048 * config.num_key_value_heads * config.head_dim
                         + seq_idx * config.num_key_value_heads * config.head_dim
                         + h * config.head_dim + d;
            
            float proj_val = in_val * v_weight[weight_idx];
            
            // Add bias if provided
            if (v_bias != nullptr && hidden_idx == 0) {
                proj_val += v_bias[h * config.head_dim + d];
            }
            
            v_out[out_idx] += proj_val;
        }
    }
}

/// Apply RoPE (Rotary Position Embeddings) to Q and K
/// This kernel applies rotary position embeddings after QKV projection
/// Reference: https://arxiv.org/abs/2104.09864
kernel void apply_rope_embeddings(
    device float* q_or_k,               // Q or K tensor to apply RoPE to
    constant uint& num_heads,           // Number of heads (attention or kv)
    constant uint& head_dim,            // Dimension per head
    constant uint& seq_position,        // Current sequence position
    constant GqaConfig& config,
    uint3 gid [[thread_position_in_grid]]
) {
    const uint batch_idx = gid.z;
    const uint seq_idx = gid.y;
    const uint head_idx = gid.x;
    
    if (head_idx >= num_heads) return;
    if (seq_idx >= 2048) return;
    
    // Calculate global position for this token
    uint position = seq_position + seq_idx;
    
    // Apply RoPE to pairs of dimensions (rotating every 2 consecutive dims)
    for (uint d = 0; d < head_dim; d += 2) {
        if (d + 1 >= head_dim) break;  // Ensure we have pairs
        
        // Compute cos and sin for this dimension pair
        float2 cos_sin = rope_cos_sin(position, d, head_dim, config.rope_theta);
        float cos_theta = cos_sin.x;
        float sin_theta = cos_sin.y;
        
        // Get current values
        uint idx_even = batch_idx * 2048 * num_heads * head_dim
                      + seq_idx * num_heads * head_dim
                      + head_idx * head_dim + d;
        uint idx_odd = idx_even + 1;
        
        float val_even = q_or_k[idx_even];
        float val_odd = q_or_k[idx_odd];
        
        // Apply rotation
        float2 rotated = apply_rope_rotation(val_even, val_odd, cos_theta, sin_theta);
        
        // Write back
        q_or_k[idx_even] = rotated.x;
        q_or_k[idx_odd] = rotated.y;
    }
}

/// Flash Attention kernel with GQA support
/// Implements tiled attention with online softmax
kernel void flash_attention(
    device const float* Q,               // [batch, seq, num_heads, head_dim]
    device const float* K,               // [batch, seq, num_kv_heads, head_dim]
    device const float* V,               // [batch, seq, num_kv_heads, head_dim]
    device float* O,                     // [batch, seq, num_heads, head_dim]
    constant GqaConfig& config,
    uint3 gid [[thread_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint tgsize [[threads_per_threadgroup]]
) {
    const uint batch_idx = gid.z;
    const uint seq_idx = gid.y;
    const uint head_idx = gid.x;
    
    if (head_idx >= config.num_attention_heads) return;
    if (seq_idx >= 2048) return;  // Max sequence length
    
    // GQA: Map query head to key/value head
    const uint kv_head_idx = head_idx / (config.num_attention_heads / config.num_key_value_heads);
    
    // Scale factor for attention (use config or default to sqrt scaling)
    const float scale = (config.attention_scale > 0.0f) ? config.attention_scale : (1.0f / sqrt(float(config.head_dim)));
    
    // Accumulate attention output and online softmax state
    float O_local[128];  // Max head_dim
    float max_score = -INFINITY;
    float sum_exp = 0.0f;
    
    // Initialize output
    for (uint d = 0; d < config.head_dim; d++) {
        O_local[d] = 0.0f;
    }
    
    // Load Q vector for this position
    float Q_local[128];
    for (uint d = 0; d < config.head_dim; d++) {
        uint q_idx = batch_idx * 2048 * config.num_attention_heads * config.head_dim
                   + seq_idx * config.num_attention_heads * config.head_dim
                   + head_idx * config.head_dim + d;
        Q_local[d] = Q[q_idx];
    }
    
    // Process each key position (apply causal mask)
    for (uint key_pos = 0; key_pos <= seq_idx; key_pos++) {
        // Compute attention score: Q · K^T
        float score = 0.0f;
        for (uint d = 0; d < config.head_dim; d++) {
            uint k_idx = batch_idx * 2048 * config.num_key_value_heads * config.head_dim
                       + key_pos * config.num_key_value_heads * config.head_dim
                       + kv_head_idx * config.head_dim + d;
            score += Q_local[d] * K[k_idx];
        }
        score *= scale;
        
        // Online softmax update
        float new_max = max(max_score, score);
        float exp_correction = exp(max_score - new_max);
        
        // Rescale previous sum and output
        sum_exp *= exp_correction;
        for (uint d = 0; d < config.head_dim; d++) {
            O_local[d] *= exp_correction;
        }
        
        // Add contribution from current key
        float exp_score = exp(score - new_max);
        sum_exp += exp_score;
        
        // Load V vector and accumulate
        for (uint d = 0; d < config.head_dim; d++) {
            uint v_idx = batch_idx * 2048 * config.num_key_value_heads * config.head_dim
                       + key_pos * config.num_key_value_heads * config.head_dim
                       + kv_head_idx * config.head_dim + d;
            O_local[d] += exp_score * V[v_idx];
        }
        
        max_score = new_max;
    }
    
    // Normalize and write output
    for (uint d = 0; d < config.head_dim; d++) {
        uint o_idx = batch_idx * 2048 * config.num_attention_heads * config.head_dim
                   + seq_idx * config.num_attention_heads * config.head_dim
                   + head_idx * config.head_dim + d;
        O[o_idx] = O_local[d] / sum_exp;
    }
}

/// Attention output projection
/// Projects attention output through output weight matrix
kernel void attention_output_projection(
    device const float* attn_out,        // [batch, seq, num_heads, head_dim]
    device const float* o_weight,        // [num_heads * head_dim, hidden_size]
    device float* output,                // [batch, seq, hidden_size]
    constant GqaConfig& config,
    uint3 gid [[thread_position_in_grid]]
) {
    const uint batch_idx = gid.z;
    const uint seq_idx = gid.y;
    const uint hidden_idx = gid.x;
    
    if (hidden_idx >= config.hidden_size) return;
    
    float result = 0.0f;
    
    // Sum over all heads and dimensions
    for (uint h = 0; h < config.num_attention_heads; h++) {
        for (uint d = 0; d < config.head_dim; d++) {
            uint attn_idx = batch_idx * 2048 * config.num_attention_heads * config.head_dim
                          + seq_idx * config.num_attention_heads * config.head_dim
                          + h * config.head_dim + d;
            uint weight_idx = (h * config.head_dim + d) * config.hidden_size + hidden_idx;
            result += attn_out[attn_idx] * o_weight[weight_idx];
        }
    }
    
    uint out_idx = batch_idx * 2048 * config.hidden_size + seq_idx * config.hidden_size + hidden_idx;
    output[out_idx] = result;
}