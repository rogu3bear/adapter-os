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
    uint rank;              // LoRA rank
    float alpha;            // LoRA scaling factor
    uint target_module;     // Target module identifier
    float dropout_rate;     // Dropout rate (0.0 = no dropout)
};

struct GqaConfig {
    uint num_attention_heads;   // Number of attention heads
    uint num_key_value_heads;   // Number of key-value heads (GQA)
    uint head_dim;              // Dimension of each head
    uint kv_width;              // Width of key-value projections
    uint hidden_size;           // Hidden dimension size
    float rope_theta;           // RoPE base frequency (10000.0 for Qwen)
    float attention_scale;      // Attention scaling factor (can be custom or sqrt-based)
    float dropout_rate;         // Dropout rate for attention
};

struct RingBuffer {
    uint top_k;                 // Number of active adapters (K-sparse)
    uint current_pos;           // Current position in ring buffer
    uint adapter_indices[8];    // Max K=8 adapter indices
    uint16_t gates[8];         // Q15 format gate values
    uint reserved[2];          // Padding for alignment / metadata
};

// MLP kernel parameter structures
struct MlpParams {
    // Input/output buffers
    device const float* input;          // [batch_size, hidden_size]
    device float* output;               // [batch_size, hidden_size]
    
    // Base weights
    device const float* gate_weight;    // [hidden_size, intermediate_size]
    device const float* up_weight;      // [hidden_size, intermediate_size]
    device const float* down_weight;    // [intermediate_size, hidden_size]
    
    // Biases (nullable)
    device const float* gate_bias;      // [intermediate_size]
    device const float* up_bias;        // [intermediate_size]
    device const float* down_bias;      // [hidden_size]
    
    // LoRA parameters
    device const float* gate_lora_a;    // [hidden_size, rank]
    device const float* gate_lora_b;    // [rank, intermediate_size]
    device const float* up_lora_a;      // [hidden_size, rank]
    device const float* up_lora_b;      // [rank, intermediate_size]
    device const float* down_lora_a;    // [intermediate_size, rank]
    device const float* down_lora_b;    // [rank, hidden_size]

    // Configuration
    LoraConfig lora_config;
    RingBuffer ring_buffer;
    uint dropout_seed;        // Seed for deterministic dropout
    uint hidden_size;
    uint intermediate_size;
    uint batch_size;
    uint max_adapters;
};

// Attention kernel parameter structures
struct AttentionParams {
    // Input/output buffers
    device const float* input;          // [batch_size, hidden_size]
    device float* q_output;             // [batch_size, num_attention_heads, head_dim]
    device float* k_output;             // [batch_size, num_key_value_heads, head_dim]
    device float* v_output;             // [batch_size, num_key_value_heads, head_dim]
    
    // Base weights
    device const float* q_weight;       // [hidden_size, hidden_size]
    device const float* k_weight;       // [hidden_size, kv_width]
    device const float* v_weight;       // [hidden_size, kv_width]
    
    // LoRA parameters
    device const float* q_lora_a;       // [hidden_size, rank]
    device const float* q_lora_b;       // [rank, hidden_size]
    device const float* k_lora_a;       // [hidden_size, rank]
    device const float* k_lora_b;       // [rank, kv_width]
    device const float* v_lora_a;      // [hidden_size, rank]
    device const float* v_lora_b;      // [rank, kv_width]

    // Configuration
    GqaConfig gqa_config;
    LoraConfig lora_config;
    RingBuffer ring_buffer;
    uint batch_size;
    uint max_adapters;
    uint reserved0;
    uint reserved1;
};

// Flash Attention kernel parameter structures
struct FlashAttentionParams {
    // Input/output buffers
    device const float* q;              // [batch_size, num_heads, seq_len, head_dim]
    device const float* k;              // [batch_size, num_kv_heads, seq_len, head_dim]
    device const float* v;              // [batch_size, num_kv_heads, seq_len, head_dim]
    device float* output;               // [batch_size, num_heads, seq_len, head_dim]

    // Configuration
    GqaConfig gqa_config;
    uint batch_size;
    uint sequence_length;
    uint reserved2;
    uint reserved3;
};

// Deterministic math functions
float deterministic_silu(float x) {
    // SiLU(x) = x * sigmoid(x) = x / (1 + exp(-x))
    return x / (1.0f + exp(-x));
}

float q15_to_float(uint16_t q15) {
    // Convert Q15 format to float
    return float(q15) / 32768.0f;
}

float deterministic_gelu(float x) {
    // GELU(x) = 0.5 * x * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x^3)))
    const float sqrt_2_over_pi = 0.7978845608f;
    const float coeff = 0.044715f;
    float x_cubed = x * x * x;
    float inner = sqrt_2_over_pi * (x + coeff * x_cubed);
    return 0.5f * x * (1.0f + tanh(inner));
}

float deterministic_relu(float x) {
    // ReLU(x) = max(0, x)
    return max(0.0f, x);
}

float deterministic_swish(float x) {
    // Swish(x) = x * sigmoid(x) = x / (1 + exp(-x))
    return x / (1.0f + exp(-x));
}

// Deterministic dropout function
float deterministic_dropout(uint seed, uint position, float dropout_rate) {
    // Simple deterministic dropout using position-based hashing
    uint hash = seed ^ position;
    hash = hash * 0x9e3779b9;
    hash = hash ^ (hash >> 16);
    hash = hash * 0x85ebca6b;
    hash = hash ^ (hash >> 13);
    hash = hash * 0xc2b2ae35;
    hash = hash ^ (hash >> 16);
    
    float random_val = float(hash) / 4294967296.0f; // Normalize to [0, 1)
    return (random_val > dropout_rate) ? 1.0f : 0.0f;
}

// RoPE (Rotary Position Embedding) functions
float2 apply_rope_2d(float2 q, uint position, float theta) {
    float angle = float(position) / pow(theta, float(0) / 128.0f);
    float cos_val = cos(angle);
    float sin_val = sin(angle);
    
    return float2(
        q.x * cos_val - q.y * sin_val,
        q.x * sin_val + q.y * cos_val
    );
}

// Memory access helpers
float safe_load(device const float* ptr, uint idx, float default_val = 0.0f) {
    return ptr ? ptr[idx] : default_val;
}

void safe_store(device float* ptr, uint idx, float val) {
    if (ptr) {
        ptr[idx] = val;
    }
}

// Attention scaling helpers
float compute_attention_scale(constant GqaConfig& gqa_config) {
    return gqa_config.attention_scale > 0.0f 
        ? gqa_config.attention_scale 
        : 1.0f / sqrt(float(gqa_config.head_dim));
}

// LoRA helper functions
float compute_lora_delta(
    device const float* lora_a,
    device const float* lora_b,
    constant LoraConfig& lora_config,
    constant RingBuffer& ring_buffer,
    uint input_idx,
    uint output_idx
) {
    float lora_delta = 0.0f;
    
    // Apply LoRA for active adapters
    for (uint k = 0; k < ring_buffer.top_k; k++) {
        uint adapter_idx = ring_buffer.adapter_indices[k];
        float gate_q15 = q15_to_float(ring_buffer.gates[k]);
        
        if (adapter_idx < lora_config.rank) {
            float lora_a_val = lora_a[input_idx * lora_config.rank + adapter_idx];
            float lora_b_val = lora_b[adapter_idx * lora_config.rank + output_idx];
            lora_delta += gate_q15 * lora_a_val * lora_b_val;
        }
    }
    
    return lora_delta;
}

#endif // COMMON_METAL
