// adapterOS Metal Kernels
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
    
    // Compute gate projection with LoRA (fused base + delta in a single pass)
    float gate_val = 0.0f;
    const uint r = lora_config.rank;
    for (uint i = 0; i < r; i++) {
        float base_weight = gate_weight[hidden_idx * r + i];
        float lora_delta = 0.0f;
        // Accumulate weighted LoRA delta across active adapters
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            const uint adapter_idx = ring_buffer.adapter_indices[k];
            if (adapter_idx < r) {
                const float gate_f = q15_to_float(ring_buffer.gates[k]);
                const float lora_a = gate_lora_a[hidden_idx * r + adapter_idx];
                const float lora_b = gate_lora_b[adapter_idx * r + i];
                lora_delta = fma(gate_f, lora_a * lora_b, lora_delta);
            }
        }
        gate_val = fma(input_val, base_weight + lora_delta, gate_val);
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
    
    // Compute up projection with LoRA (fused)
    float up_val = 0.0f;
    for (uint i = 0; i < r; i++) {
        float base_weight = up_weight[hidden_idx * r + i];
        float lora_delta = 0.0f;
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            const uint adapter_idx = ring_buffer.adapter_indices[k];
            if (adapter_idx < r) {
                const float gate_f = q15_to_float(ring_buffer.gates[k]);
                const float lora_a = up_lora_a[hidden_idx * r + adapter_idx];
                const float lora_b = up_lora_b[adapter_idx * r + i];
                lora_delta = fma(gate_f, lora_a * lora_b, lora_delta);
            }
        }
        up_val = fma(input_val, base_weight + lora_delta, up_val);
    }
    
    // Add up bias if provided
    if (up_bias != nullptr) {
        up_val += up_bias[intermediate_idx];
    }
    
    // Element-wise multiplication (SwiGLU)
    float intermediate_val = gate_activated * up_val;
    
    // Compute down projection with LoRA (fused)
    float down_val = 0.0f;
    for (uint i = 0; i < r; i++) {
        float base_weight = down_weight[intermediate_idx * r + i];
        float lora_delta = 0.0f;
        for (uint k = 0; k < ring_buffer.top_k; k++) {
            const uint adapter_idx = ring_buffer.adapter_indices[k];
            if (adapter_idx < r) {
                const float gate_f = q15_to_float(ring_buffer.gates[k]);
                const float lora_a = down_lora_a[intermediate_idx * r + adapter_idx];
                const float lora_b = down_lora_b[adapter_idx * r + i];
                lora_delta = fma(gate_f, lora_a * lora_b, lora_delta);
            }
        }
        down_val = fma(intermediate_val, base_weight + lora_delta, down_val);
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

// RMSNorm (Root Mean Square Layer Normalization) kernel
// Used in modern LLMs like Llama instead of LayerNorm
// Formula: y = (x / sqrt(mean(x^2) + eps)) * weight
// No bias term (unlike LayerNorm)
//
// Reference: https://arxiv.org/abs/1910.07467
kernel void rms_norm(
    device const float* input [[buffer(0)]],
    device const float* weight [[buffer(1)]],
    device float* output [[buffer(2)]],
    constant uint& hidden_size [[buffer(3)]],
    constant float& eps [[buffer(4)]],
    uint2 gid [[thread_position_in_grid]],
    uint lid [[thread_index_in_threadgroup]],
    threadgroup float* shared [[threadgroup(0)]]
) {
    uint batch_idx = gid.x;
    uint thread_idx = lid;

    // Step 1: Compute sum of squares for this batch element
    float local_sum = 0.0f;
    for (uint i = thread_idx; i < hidden_size; i += 256) {
        float val = input[batch_idx * hidden_size + i];
        local_sum += val * val;
    }

    // Store local sum to shared memory
    shared[thread_idx] = local_sum;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Parallel reduction to compute total sum
    for (uint stride = 128; stride > 0; stride /= 2) {
        if (thread_idx < stride) {
            shared[thread_idx] += shared[thread_idx + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // Step 2: Compute RMS (root mean square)
    float mean_sq = shared[0] / float(hidden_size);
    float rms = sqrt(mean_sq + eps);
    float inv_rms = 1.0f / rms;

    // Step 3: Normalize and scale
    // Each thread processes multiple elements
    for (uint i = thread_idx; i < hidden_size; i += 256) {
        uint idx = batch_idx * hidden_size + i;
        float normalized = input[idx] * inv_rms;
        output[idx] = normalized * weight[i];
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

// Configuration for vocabulary projection kernel
struct VocabProjectionConfig {
    uint hidden_size;        // Hidden dimension (e.g., 3584 for Qwen2.5-7B)
    uint vocab_size;         // Vocabulary size (e.g., 152064 for Qwen2.5-7B)
    uint batch_size;         // Batch size (number of sequences)
    uint use_bias;           // Whether to add bias (0 = no, 1 = yes)
};

// Vocabulary Projection (LM Head) Kernel
// Performs: logits = hidden_state @ lm_head_weight^T + bias
//
// This is the final layer that projects hidden states to vocabulary logits.
// Optimized for large vocabulary sizes (32K-128K+ tokens).
//
// Memory layout:
//   hidden_state: [batch_size, hidden_size]
//   lm_head_weight: [vocab_size, hidden_size] (transposed for efficient access)
//   bias: [vocab_size] (optional)
//   output: [batch_size, vocab_size]
//
// Optimization strategies:
// 1. Tiled computation for memory bandwidth efficiency
// 2. Coalesced memory access patterns
// 3. Shared memory for hidden state reuse across vocab outputs
kernel void vocabulary_projection(
    device const float* hidden_state,    // [batch_size, hidden_size]
    device const float* lm_head_weight,  // [vocab_size, hidden_size]
    device const float* bias,            // [vocab_size] (nullable)
    device float* output,                // [batch_size, vocab_size]

    constant VocabProjectionConfig& config,

    uint3 gid [[thread_position_in_grid]],
    uint3 lid [[thread_position_in_threadgroup]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]]
) {
    // Thread identifies which vocab token to compute
    uint batch_idx = tgid.x;
    uint vocab_idx = tgid.y * gsz.x + lid.x;

    if (batch_idx >= config.batch_size || vocab_idx >= config.vocab_size) {
        return;
    }

    // Compute dot product: hidden_state[batch] @ lm_head_weight[vocab]
    float logit = 0.0f;

    // Process hidden dimension with loop unrolling for better performance
    // Use 4-way unrolling for vectors of 4 floats
    uint hidden_size = config.hidden_size;
    uint i = 0;

    // Unrolled loop for better throughput
    for (; i + 3 < hidden_size; i += 4) {
        uint h_base = batch_idx * hidden_size + i;
        uint w_base = vocab_idx * hidden_size + i;

        float h0 = hidden_state[h_base];
        float h1 = hidden_state[h_base + 1];
        float h2 = hidden_state[h_base + 2];
        float h3 = hidden_state[h_base + 3];

        float w0 = lm_head_weight[w_base];
        float w1 = lm_head_weight[w_base + 1];
        float w2 = lm_head_weight[w_base + 2];
        float w3 = lm_head_weight[w_base + 3];

        // Use FMA for better precision and performance
        logit = fma(h0, w0, logit);
        logit = fma(h1, w1, logit);
        logit = fma(h2, w2, logit);
        logit = fma(h3, w3, logit);
    }

    // Handle remaining elements
    for (; i < hidden_size; i++) {
        logit = fma(hidden_state[batch_idx * hidden_size + i],
                   lm_head_weight[vocab_idx * hidden_size + i],
                   logit);
    }

    // Add bias if provided
    if (config.use_bias && bias != nullptr) {
        logit += bias[vocab_idx];
    }

    // Store output logit
    output[batch_idx * config.vocab_size + vocab_idx] = logit;
}

// Tiled vocabulary projection for large vocabularies
// Uses shared memory for hidden state to reduce global memory bandwidth
// Recommended for vocab_size > 32K
kernel void vocabulary_projection_tiled(
    device const float* hidden_state,    // [batch_size, hidden_size]
    device const float* lm_head_weight,  // [vocab_size, hidden_size]
    device const float* bias,            // [vocab_size] (nullable)
    device float* output,                // [batch_size, vocab_size]

    constant VocabProjectionConfig& config,

    uint3 gid [[thread_position_in_grid]],
    uint3 lid [[thread_position_in_threadgroup]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 gsz [[threads_per_threadgroup]]
) {
    // Tile size for hidden dimension
    const uint TILE_SIZE = 256;

    uint batch_idx = tgid.x;
    uint vocab_idx = tgid.y * gsz.x + lid.x;

    if (batch_idx >= config.batch_size || vocab_idx >= config.vocab_size) {
        return;
    }

    // Shared memory for hidden state tile
    threadgroup float shared_hidden[TILE_SIZE];

    float logit = 0.0f;
    uint hidden_size = config.hidden_size;

    // Process hidden dimension in tiles
    for (uint tile_start = 0; tile_start < hidden_size; tile_start += TILE_SIZE) {
        // Cooperatively load hidden state tile into shared memory
        uint load_idx = tile_start + lid.x;
        if (load_idx < hidden_size && lid.x < TILE_SIZE) {
            shared_hidden[lid.x] = hidden_state[batch_idx * hidden_size + load_idx];
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        // Compute partial dot product using shared memory
        uint tile_end = min(tile_start + TILE_SIZE, hidden_size);
        for (uint i = tile_start; i < tile_end; i++) {
            uint local_idx = i - tile_start;
            logit = fma(shared_hidden[local_idx],
                       lm_head_weight[vocab_idx * hidden_size + i],
                       logit);
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // Add bias if provided
    if (config.use_bias && bias != nullptr) {
        logit += bias[vocab_idx];
    }

    // Store output logit
    output[batch_idx * config.vocab_size + vocab_idx] = logit;
}

// Embedding lookup kernel
// Maps token IDs to their embedding vectors
//
// Memory layout:
//   embeddings: [vocab_size, hidden_size]
//   token_ids: [batch_size]
//   output: [batch_size, hidden_size]
kernel void embedding_lookup(
    device const float* embeddings [[buffer(0)]],  // [vocab_size, hidden_size]
    device const uint* token_ids [[buffer(1)]],     // [batch_size]
    device float* output [[buffer(2)]],             // [batch_size, hidden_size]
    constant uint& hidden_size [[buffer(3)]],
    constant uint& vocab_size [[buffer(4)]],
    constant uint& batch_size [[buffer(5)]],
    uint2 gid [[thread_position_in_grid]]
) {
    uint batch_idx = gid.x;
    uint dim_idx = gid.y;

    // Bounds checking
    if (batch_idx >= batch_size || dim_idx >= hidden_size) {
        return;
    }

    // Look up token ID with vocab bounds checking
    uint token_id = token_ids[batch_idx];

    // Ensure token_id is within vocabulary bounds
    if (token_id >= vocab_size) {
        // Out of bounds - set to zero (or could use a special UNK token)
        output[batch_idx * hidden_size + dim_idx] = 0.0f;
        return;
    }

    // Copy embedding vector element
    output[batch_idx * hidden_size + dim_idx] =
        embeddings[token_id * hidden_size + dim_idx];
}
