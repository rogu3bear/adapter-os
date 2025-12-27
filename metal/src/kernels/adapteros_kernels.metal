// AdapterOS Modular Metal Kernels
// Production-optimized Metal kernels for Qwen2.5-7B-Instruct
//
// Features:
// - Fused MLP with SwiGLU activation and LoRA support
// - Fused QKV with Grouped Query Attention (GQA)
// - Flash Attention for memory efficiency
// - Deterministic math operations
// - Optimized memory access patterns
// - Vocabulary projection with adapter fusion
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

// Include all modular components
#include "common.metal"
#include "utils.metal"
#include "mlp.metal"
#include "attention.metal"
#include "flash_attention.metal"
#include "mplora.metal"

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
    
    // Explicit constants matching Rust set_bytes calls
    constant uint& hidden_size_ref [[buffer(3)]],
    constant uint& vocab_size_ref [[buffer(4)]],
    constant uint& batch_size_ref [[buffer(5)]],
    
    uint gid [[thread_position_in_grid]]
) {
    // 1D dispatch where each thread handles one token in the batch
    uint token_idx = gid;
    
    uint hidden_size = hidden_size_ref;
    uint vocab_size = vocab_size_ref;
    uint batch_size = batch_size_ref;
    
    // Bounds check
    if (token_idx >= batch_size) {
        return;
    }
    
    uint token_id = token_ids[token_idx];
    
    // Handle invalid tokens (out of vocab)
    if (token_id >= vocab_size) {
         // Zero out row
         for (uint i = 0; i < hidden_size; i++) {
             output[token_idx * hidden_size + i] = 0.0f;
         }
         return;
    }
    
    // Copy embedding vector
    for (uint i = 0; i < hidden_size; i++) {
        output[token_idx * hidden_size + i] = embeddings[token_id * hidden_size + i];
    }
}
