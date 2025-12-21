// AdapterOS Common Metal Functions
// Shared utilities for all Metal kernels
//
// References:
// - Metal Shading Language: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf
// - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders

#ifndef COMMON_METAL
#define COMMON_METAL

#include <metal_stdlib>
using namespace metal;

// Configuration structures
struct LoraConfig {
    uint rank;
    float alpha;
    uint target_module;
    float dropout_rate;      // Dropout rate (0.0 = no dropout)
};

struct GqaConfig {
    uint num_attention_heads;
    uint num_key_value_heads;
    uint head_dim;
    uint kv_width;
    uint hidden_size;
    float rope_theta;        // RoPE base frequency (10000.0 for Qwen)
    float attention_scale;   // Attention scaling factor (can be custom or sqrt-based)
    float dropout_rate;      // Dropout rate for attention
};

struct RingBuffer {
    uint top_k;
    uint current_pos;
    uint adapter_indices[8];  // Max K=8
    uint16_t gates[8];        // Q15 format
};

// Deterministic math functions
float deterministic_silu(float x) {
    // SiLU(x) = x * sigmoid(x) = x / (1 + exp(-x))
    return x / (1.0f + exp(-x));
}

float q15_to_float(uint16_t q15) {
    return float(q15) / 32767.0f;
}

float deterministic_gelu(float x) {
    // GELU(x) = 0.5 * x * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x^3)))
    const float sqrt_2_over_pi = 0.7978845608f;
    const float coeff = 0.044715f;
    return 0.5f * x * (1.0f + tanh(sqrt_2_over_pi * (x + coeff * x * x * x)));
}

float deterministic_softmax_sum(float x, float max_val) {
    return exp(x - max_val);
}

/// Rotary Position Embedding (RoPE) helper functions
/// Reference: https://arxiv.org/abs/2104.09864

/// Compute RoPE frequency for a given dimension
/// theta_i = rope_theta ^ (-2i / head_dim)
float rope_frequency(uint dim_idx, uint head_dim, float rope_theta) {
    float exponent = -2.0f * float(dim_idx) / float(head_dim);
    return pow(rope_theta, exponent);
}

/// Compute cos and sin for RoPE at a given position
/// Returns cos and sin in a float2
float2 rope_cos_sin(uint position, uint dim_idx, uint head_dim, float rope_theta) {
    float freq = rope_frequency(dim_idx, head_dim, rope_theta);
    float angle = float(position) / freq;
    return float2(cos(angle), sin(angle));
}

/// Apply RoPE rotation to a pair of values (even/odd dimensions)
/// x and y represent consecutive dimensions in the embedding
/// Returns rotated (x', y') where:
///   x' = x * cos(θ) - y * sin(θ)
///   y' = y * cos(θ) + x * sin(θ)
float2 apply_rope_rotation(float x, float y, float cos_theta, float sin_theta) {
    float x_rot = x * cos_theta - y * sin_theta;
    float y_rot = y * cos_theta + x * sin_theta;
    return float2(x_rot, y_rot);
}

/// Deterministic dropout using xorshift RNG
/// seed: per-layer seed derived from HKDF
/// position: token position for deterministic dropout
/// dropout_rate: probability of dropping (0.0 to 1.0)
/// Returns 0.0 if dropped, 1.0/(1-dropout_rate) if kept (inverted dropout)
float deterministic_dropout(uint seed, uint position, float dropout_rate) {
    if (dropout_rate <= 0.0f) return 1.0f;
    if (dropout_rate >= 1.0f) return 0.0f;

    // Xorshift RNG for deterministic random numbers
    uint state = seed ^ position;
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;

    // Convert to [0, 1] range
    float rand_val = float(state) / float(0xFFFFFFFF);

    // Inverted dropout: scale by 1/(1-p) when keeping
    return (rand_val >= dropout_rate) ? (1.0f / (1.0f - dropout_rate)) : 0.0f;
}

/// RMSNorm configuration
struct RmsNormConfig {
    uint hidden_size;       // Dimension of the hidden states
    float eps;              // Epsilon for numerical stability (typically 1e-6)
};

#endif // COMMON_METAL
