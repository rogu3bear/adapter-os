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

// Vocabulary projection kernel with adapter fusion support
kernel void vocabulary_projection(
    device const float* hidden_states [[buffer(0)]],    // [hidden_size]
    device const float* lm_head_weights [[buffer(1)]],  // [hidden_size, vocab_size]
    device float* output_logits [[buffer(2)]],          // [vocab_size]
    constant uint& hidden_size [[buffer(3)]],
    constant uint& vocab_size [[buffer(4)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= vocab_size) {
        return;
    }

    // Compute dot product: logit = sum(hidden_states[i] * lm_head_weights[i][tid])
    float logit = 0.0f;
    for (uint i = 0; i < hidden_size; i++) {
        logit += hidden_states[i] * lm_head_weights[i * vocab_size + tid];
    }

    output_logits[tid] = logit;
}
